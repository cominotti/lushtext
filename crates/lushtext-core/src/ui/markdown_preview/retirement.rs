// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — retirement. Destroys detached render state off the GTK
//! thread, and applies backpressure while it cannot keep up.
//!
//! Detaching a rendered buffer is O(1); *freeing* it is not, so the payload is
//! retired in bounded turns and its document-sized body is handed to
//! `ui::plain_disposal` to be destroyed on a worker.
//!
//! ## The backpressure inversion
//!
//! This is the row's least obvious control flow and the one a reader must be
//! told about: when too many detached generations are outstanding,
//! `markdown_retirement_at_capacity` refuses new work, and both
//! `render_markdown_with_context` and `start_render_plan` **park**. Only
//! `retire_markdown_slice` un-parks them, through
//! `resume_deferred_markdown_work`. So the *retirement* lane restarts
//! *production* work — a resumption by a different actor than the one that
//! requested it, and one that appears in no recorded stage trace of this
//! workflow. `evidence.retirement.deferred_work_pending` makes it observable.

use std::sync::atomic::Ordering;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::services::markdown_render::MAX_MARKDOWN_SOURCE_BYTES;

use super::imp;
use super::seams::MarkdownRetirementSession;
use super::seams::{
    PendingMarkdownRender, PendingMarkdownWork, PlainRetirementTerminal, RetiredMarkdownRender,
    try_guard_markdown_source,
};
use super::{
    LushtextMarkdownPreview, MARKDOWN_RETIREMENT_CHARS_PER_TURN,
    MARKDOWN_RETIREMENT_ITEMS_PER_TURN, MAX_MARKDOWN_RETIREMENT_GENERATIONS,
    MarkdownPreviewRenderContext,
};

impl LushtextMarkdownPreview {
    pub(super) fn markdown_retirement_at_capacity(&self) -> bool {
        self.imp()
            .retirement
            .borrow()
            .as_ref()
            .is_some_and(|session| session.states.len() >= MAX_MARKDOWN_RETIREMENT_GENERATIONS)
    }

    pub(super) fn defer_markdown_work(&self, work: PendingMarkdownWork) {
        let imp = self.imp();
        if let Some(retired) = imp.deferred_work.replace(Some(work)) {
            self.retire_markdown_work(retired);
        }
        #[cfg(feature = "test-utils")]
        imp.deferred_work_high_water.set(1);
    }

    /// Coalesce repeated source updates in the retained latest allocation.
    pub(super) fn defer_markdown_render(
        &self,
        generation: u64,
        markdown: &str,
        context: MarkdownPreviewRenderContext,
    ) {
        let mut deferred = self.imp().deferred_work.borrow_mut();
        match deferred.as_mut() {
            Some(PendingMarkdownWork::Render(PendingMarkdownRender::Source(request)))
                if markdown.len() <= MAX_MARKDOWN_SOURCE_BYTES =>
            {
                request.generation = generation;
                request.source.clear();
                request.source.push_str(markdown);
                request.context = context;
                return;
            }
            Some(PendingMarkdownWork::Render(PendingMarkdownRender::SourceLimited {
                generation: retained_generation,
                source_bytes,
                context: retained_context,
            })) if markdown.len() > MAX_MARKDOWN_SOURCE_BYTES => {
                *retained_generation = generation;
                *source_bytes = markdown.len();
                *retained_context = context;
                return;
            }
            _ => {}
        }
        let request = if markdown.len() > MAX_MARKDOWN_SOURCE_BYTES {
            PendingMarkdownRender::SourceLimited {
                generation,
                source_bytes: markdown.len(),
                context,
            }
        } else if let Some(request) = try_guard_markdown_source(generation, markdown, &context) {
            PendingMarkdownRender::Source(request)
        } else {
            let retired = deferred.take();
            drop(deferred);
            if let Some(retired) = retired {
                self.retire_markdown_work(retired);
            }
            self.finish_markdown_capacity_pressure(generation, markdown.len());
            return;
        };
        let retired = deferred.replace(PendingMarkdownWork::Render(request));
        drop(deferred);
        if let Some(retired) = retired {
            self.retire_markdown_work(retired);
        }
        #[cfg(feature = "test-utils")]
        self.imp().deferred_work_high_water.set(1);
    }

    pub(super) fn cancel_deferred_markdown_work(&self) {
        if let Some(retired) = self.imp().deferred_work.take() {
            self.retire_markdown_work(retired);
        }
    }

    pub(super) fn resume_deferred_markdown_work(&self) {
        let Some(work) = self.imp().deferred_work.take() else {
            return;
        };
        if !self
            .imp()
            .render_session
            .borrow()
            .is_current(work.generation())
        {
            self.retire_markdown_work(work);
            return;
        }
        match work {
            PendingMarkdownWork::Render(request) => self.start_pending_markdown_render(request),
            PendingMarkdownWork::Projection {
                generation,
                plan,
                context,
            } => self.start_render_plan(generation, plan, context),
        }
    }

    pub(super) fn retire_guarded_markdown<T: Send + 'static>(
        &self,
        retired: crate::ui::plain_disposal::DisposalOwned<T>,
    ) {
        let imp = self.imp();
        imp.plain_retirement_jobs.fetch_add(1, Ordering::AcqRel);
        let pending = imp
            .plain_retirement_pending
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        imp.plain_retirement_pending_high_water
            .fetch_max(pending, Ordering::AcqRel);
        let terminal = PlainRetirementTerminal(imp.plain_retirement_pending.clone());
        drop(retired.with_disposal_terminal(move || {
            drop(terminal);
        }));
    }

    pub(super) fn retire_markdown_work(&self, work: PendingMarkdownWork) {
        match work {
            PendingMarkdownWork::Render(PendingMarkdownRender::Source(request)) => {
                self.retire_guarded_markdown(request);
            }
            PendingMarkdownWork::Render(PendingMarkdownRender::SourceLimited { .. }) => {}
            PendingMarkdownWork::Projection { plan, .. } => {
                self.retire_guarded_markdown(plan);
            }
        }
    }

    /// Detach the visible buffer in O(1) and retire its payload in bounded turns.
    pub(super) fn clear_rendered_state(&self, allow_escape_generation: bool) {
        let imp = self.imp();
        let retirement_len = imp
            .retirement
            .borrow()
            .as_ref()
            .map_or(0, |session| session.states.len());
        if allow_escape_generation
            && retirement_len >= MAX_MARKDOWN_RETIREMENT_GENERATIONS.saturating_add(1)
        {
            // The first terminal transition may consume one escape generation.
            // Further terminal updates own only the small current buffer, so
            // reuse it instead of growing detached GTK generations without bound.
            debug_assert!(imp.rendered_embeds.borrow().is_empty());
            debug_assert!(imp.text_link_targets.borrow().is_empty());
            imp.text_view.buffer().set_text("");
            self.advance_rendered_embed_generation();
            return;
        }
        let old_buffer = imp.text_view.buffer();
        let new_buffer = gtk4::TextBuffer::new(None::<&gtk4::TextTagTable>);
        imp::create_or_update_tags(&new_buffer, libadwaita::StyleManager::default().is_dark());
        imp.text_view.set_buffer(Some(&new_buffer));

        let retired = RetiredMarkdownRender {
            buffer: old_buffer,
            embeds: std::mem::take(&mut *imp.rendered_embeds.borrow_mut()).into(),
            links: std::mem::take(&mut *imp.text_link_targets.borrow_mut()).into(),
        };
        if !retired.is_empty() {
            let mut retirement = imp.retirement.borrow_mut();
            let session = retirement.get_or_insert_with(MarkdownRetirementSession::default);
            debug_assert!(
                session.states.len()
                    < MAX_MARKDOWN_RETIREMENT_GENERATIONS + usize::from(allow_escape_generation)
            );
            session.states.push_back(retired);
            #[cfg(feature = "test-utils")]
            imp.retirement_generations_high_water.set(
                imp.retirement_generations_high_water
                    .get()
                    .max(session.states.len()),
            );
            drop(retirement);
            self.arm_markdown_retirement();
        }
        self.advance_rendered_embed_generation();
    }

    /// Keep exactly one idle source draining detached render state.
    pub(super) fn arm_markdown_retirement(&self) {
        let imp = self.imp();
        if imp.retirement_armed.replace(true) {
            return;
        }
        let preview_weak = self.downgrade();
        glib::idle_add_local(move || {
            let Some(preview) = preview_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if preview.retire_markdown_slice() {
                glib::ControlFlow::Continue
            } else {
                preview.imp().retirement_armed.set(false);
                glib::ControlFlow::Break
            }
        });
    }

    /// Release a bounded amount of detached text, widgets, and links.
    pub(super) fn retire_markdown_slice(&self) -> bool {
        let imp = self.imp();
        let mut session_slot = imp.retirement.borrow_mut();
        let Some(session) = session_slot.as_mut() else {
            return false;
        };
        let Some(state) = session.states.front_mut() else {
            session_slot.take();
            return false;
        };

        let mut retired_items = 0usize;
        while retired_items < MARKDOWN_RETIREMENT_ITEMS_PER_TURN {
            let widget = state.embeds.pop_front().map(|embed| embed.widget);
            if let Some(widget) = widget {
                if widget.parent().as_ref() == Some(imp.text_view.upcast_ref()) {
                    imp.text_view.remove(&widget);
                }
                retired_items += 1;
                continue;
            }
            if state.links.pop_front().is_some() {
                retired_items += 1;
                continue;
            }
            break;
        }

        let char_count = usize::try_from(state.buffer.char_count()).unwrap_or_default();
        let retired_chars = char_count.min(MARKDOWN_RETIREMENT_CHARS_PER_TURN);
        if retired_chars > 0 {
            let mut end = state.buffer.end_iter();
            let start_offset = end
                .offset()
                .saturating_sub(i32::try_from(retired_chars).unwrap_or(i32::MAX));
            let mut start = state.buffer.iter_at_offset(start_offset);
            state.buffer.delete(&mut start, &mut end);
        }

        #[cfg(feature = "test-utils")]
        {
            imp.retirement_chars_high_water
                .set(imp.retirement_chars_high_water.get().max(retired_chars));
            imp.retirement_items_high_water
                .set(imp.retirement_items_high_water.get().max(retired_items));
        }

        if state.is_empty() {
            session.states.pop_front();
        }
        let should_resume = session.states.len() < MAX_MARKDOWN_RETIREMENT_GENERATIONS
            && imp.deferred_work.borrow().is_some();
        if session.states.is_empty() {
            session_slot.take();
        }
        let retirement_pending = session_slot.is_some();
        drop(session_slot);
        if should_resume {
            self.resume_deferred_markdown_work();
        }
        retirement_pending || self.imp().retirement.borrow().is_some()
    }
}
