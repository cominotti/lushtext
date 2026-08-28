// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — admission. Decides whether a render may start, and with
//! what ownership of the source.
//!
//! Every entry point lands here, and the first question is never "render what"
//! but "may this source be owned at all". Three ceilings answer it: the source
//! byte limit, the retained-byte budget, and the disposal lane's capacity. A
//! source that fails any of them yields a **paused or limited preview state**
//! rather than a partial render — the large-buffer guardrail in
//! `.agents/rules/ui.md` is explicit that a secondary decoration must not trade
//! away editor responsiveness.
//!
//! This module also owns the **render session generation**: `begin_render_session`
//! advances it, and every later stage validates against it. That is what makes a
//! superseded render safe to discard at any resumption point.

use std::sync::atomic::Ordering;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;

use crate::services::markdown_render::{
    MAX_MARKDOWN_SOURCE_BYTES, markdown_render_options, plan_markdown, source_limited_markdown_plan,
};
use crate::ui::accessibility;
use crate::ui::buffer_snapshot::{BufferSnapshotHandle, BufferSnapshotPayload};

use super::policy::inline_footnote_limited_plan;
use super::policy::{InlineFootnoteLowering, lower_inline_footnotes};
use super::seams::{
    GuardedPendingMarkdownPlan, MARKDOWN_PLAN_RESERVATION_BYTES, PendingMarkdownRender,
    SnapshotMarkdownPlanOutcome,
};
use super::{
    LushtextMarkdownPreview, MARKDOWN_BACKGROUND_PLAN_THRESHOLD_BYTES, MarkdownPreviewRenderContext,
};
use crate::services::markdown_render::MarkdownRenderState;

impl LushtextMarkdownPreview {
    /// Replace the app-local note-source capture owned by this preview.
    pub(crate) fn replace_source_snapshot(&self, snapshot: Option<BufferSnapshotHandle>) {
        if let Some(previous) = self.imp().source_snapshot.replace(snapshot) {
            previous.dispose();
        }
    }

    /// Release the completed note-source capture without touching its callback.
    pub(crate) fn clear_source_snapshot(&self) {
        self.imp().source_snapshot.take();
    }

    /// Render Markdown content with no filesystem context.
    ///
    /// This keeps existing tests and call sites lightweight while the richer
    /// `render_markdown_with_context` path handles relative links and images
    /// for real Markdown files opened from the window shell.
    pub fn render_markdown(&self, markdown: &str) {
        self.render_markdown_with_context(markdown, &MarkdownPreviewRenderContext::default());
    }

    /// Plan a captured editor buffer without first coalescing its chunks on GTK.
    pub(crate) fn render_snapshot_with_context(
        &self,
        snapshot: BufferSnapshotPayload,
        context: MarkdownPreviewRenderContext,
    ) {
        self.render_snapshot_with_context_or_placeholder(snapshot, context, None);
    }

    /// Plan a captured buffer on a worker and preserve an editor-specific empty state.
    pub(crate) fn render_snapshot_with_context_or_placeholder(
        &self,
        snapshot: BufferSnapshotPayload,
        context: MarkdownPreviewRenderContext,
        empty_placeholder: Option<&'static str>,
    ) {
        let generation = self.begin_render_session();
        self.show_pending_markdown_plan();
        spawn_blocking_then(
            (self.downgrade(), context),
            move || {
                let source = snapshot.into_guarded_string_on_worker();
                if empty_placeholder.is_some() && source.trim().is_empty() {
                    drop(source);
                    return SnapshotMarkdownPlanOutcome::Empty;
                }
                let source_bytes = source.len();
                if source_bytes > MAX_MARKDOWN_SOURCE_BYTES {
                    drop(source);
                    return SnapshotMarkdownPlanOutcome::SourceLimited { source_bytes };
                }
                let reservation = if let Some(source_weight) = source.reservation_weight() {
                    crate::ui::plain_disposal::try_reserve_replacement_for_gtk(
                        MARKDOWN_PLAN_RESERVATION_BYTES,
                        source_weight,
                    )
                } else {
                    crate::ui::plain_disposal::try_reserve_for_gtk(MARKDOWN_PLAN_RESERVATION_BYTES)
                };
                let Some(reservation) = reservation else {
                    drop(source);
                    return SnapshotMarkdownPlanOutcome::CapacityUnavailable { source_bytes };
                };
                let source = source.into_inner_on_worker();
                let plan = match lower_inline_footnotes(&source, markdown_render_options()) {
                    InlineFootnoteLowering::Lowered(lowered) => plan_markdown(&lowered),
                    InlineFootnoteLowering::Unchanged => plan_markdown(&source),
                    InlineFootnoteLowering::Limited => inline_footnote_limited_plan(source.len()),
                    InlineFootnoteLowering::Cancelled => source_limited_markdown_plan(source.len()),
                };
                SnapshotMarkdownPlanOutcome::Planned(reservation.own(plan))
            },
            move |(preview_weak, context), outcome| {
                if let Some(preview) = preview_weak.upgrade() {
                    match outcome {
                        SnapshotMarkdownPlanOutcome::Planned(plan) => {
                            preview.start_render_plan(generation, plan, context);
                        }
                        SnapshotMarkdownPlanOutcome::Empty => {
                            if let Some(description) = empty_placeholder {
                                preview.show_content_placeholder(description);
                            }
                        }
                        SnapshotMarkdownPlanOutcome::SourceLimited { source_bytes } => {
                            preview.start_render_plan(
                                generation,
                                crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                                    source_limited_markdown_plan(source_bytes),
                                ),
                                context,
                            );
                        }
                        SnapshotMarkdownPlanOutcome::CapacityUnavailable { source_bytes } => {
                            if preview.imp().render_session.borrow().is_current(generation) {
                                preview.finish_markdown_capacity_pressure(generation, source_bytes);
                            }
                        }
                    }
                }
            },
        );
    }

    /// Render Markdown content into the text view, replacing any previous content.
    ///
    /// Switches to content mode (text view visible, placeholder hidden). The
    /// renderer walks pulldown-cmark's event stream and maps text blocks to
    /// `GtkTextTag`s while buffering tables and image blocks into anchored GTK
    /// widgets when plain text is not expressive enough.
    ///
    /// # Panics
    ///
    /// Panics if the parser reports the end of a buffered table or image while
    /// the internal buffered state is missing.
    pub fn render_markdown_with_context(
        &self,
        markdown: &str,
        context: &MarkdownPreviewRenderContext,
    ) {
        let generation = self.begin_render_session();
        let context = context.clone();

        if self.markdown_retirement_at_capacity() {
            self.defer_markdown_render(generation, markdown, context);
            return;
        }

        self.render_markdown_generation(generation, markdown, context);
    }

    pub(super) fn render_markdown_generation(
        &self,
        generation: u64,
        markdown: &str,
        context: MarkdownPreviewRenderContext,
    ) {
        if markdown.len() > MAX_MARKDOWN_SOURCE_BYTES {
            self.start_render_plan(
                generation,
                crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                    source_limited_markdown_plan(markdown.len()),
                ),
                context,
            );
        } else if markdown.len() > MARKDOWN_BACKGROUND_PLAN_THRESHOLD_BYTES {
            self.enqueue_markdown_plan(generation, markdown, context);
        } else {
            let Some(reservation) =
                crate::ui::plain_disposal::try_reserve_for_gtk(MARKDOWN_PLAN_RESERVATION_BYTES)
            else {
                self.finish_markdown_capacity_pressure(generation, markdown.len());
                return;
            };
            let plan = match lower_inline_footnotes(markdown, markdown_render_options()) {
                InlineFootnoteLowering::Lowered(lowered) => plan_markdown(&lowered),
                InlineFootnoteLowering::Unchanged => plan_markdown(markdown),
                InlineFootnoteLowering::Limited => inline_footnote_limited_plan(markdown.len()),
                InlineFootnoteLowering::Cancelled => return,
            };
            self.start_render_plan(generation, reservation.own(plan), context);
        }
    }

    pub(super) fn show_markdown_memory_pressure(&self) {
        const MESSAGE: &str = "Markdown preview paused while memory pressure clears.";
        self.show_content_view();
        let buffer = self.imp().text_view.buffer();
        let already_visible = usize::try_from(buffer.char_count()).ok()
            == Some(MESSAGE.chars().count())
            && buffer.text(&buffer.start_iter(), &buffer.end_iter(), true) == MESSAGE;
        if !already_visible {
            self.clear_rendered_state(false);
            self.imp().text_view.buffer().set_text(MESSAGE);
        }
        accessibility::set_description(
            &*self.imp().text_view,
            "Markdown preview deferred by bounded plain-data capacity",
        );
    }

    pub(super) fn finish_markdown_capacity_pressure(&self, generation: u64, source_bytes: usize) {
        debug_assert!(source_bytes <= MAX_MARKDOWN_SOURCE_BYTES);
        self.show_markdown_memory_pressure();
        self.imp()
            .render_session
            .borrow_mut()
            .transition(generation, MarkdownRenderState::Failed);
    }

    pub(super) fn start_pending_markdown_render(&self, request: PendingMarkdownRender) {
        match request {
            PendingMarkdownRender::Source(request) => {
                self.render_owned_markdown_generation(request);
            }
            PendingMarkdownRender::SourceLimited {
                generation,
                source_bytes,
                context,
            } => self.start_render_plan(
                generation,
                crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                    source_limited_markdown_plan(source_bytes),
                ),
                context,
            ),
        }
    }

    pub(super) fn render_owned_markdown_generation(&self, request: GuardedPendingMarkdownPlan) {
        let source_bytes = request.source.len();
        if source_bytes > MAX_MARKDOWN_SOURCE_BYTES {
            let generation = request.generation;
            let context = request.context.clone();
            self.retire_guarded_markdown(request);
            self.start_render_plan(
                generation,
                crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                    source_limited_markdown_plan(source_bytes),
                ),
                context,
            );
        } else {
            self.show_pending_markdown_plan();
            self.enqueue_owned_markdown_plan(request);
        }
    }

    pub(super) fn show_pending_markdown_plan(&self) {
        self.show_content_view();
        self.clear_rendered_state(false);
        self.imp()
            .text_view
            .buffer()
            .set_text("Rendering Markdown preview…");
        accessibility::set_description(
            &*self.imp().text_view,
            "Markdown preview rendering is pending",
        );
    }

    /// Invalidate older work and open one new generation-owned render session.
    pub(super) fn begin_render_session(&self) -> u64 {
        let imp = self.imp();
        if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
            cancel.store(true, Ordering::Release);
        }
        self.cancel_queued_image_work();
        let generation = imp.render_session.borrow_mut().begin();
        imp.render_generation.store(generation, Ordering::Release);
        #[cfg(feature = "test-utils")]
        {
            imp.projection_dispatch_count.set(0);
            imp.projection_high_water_events.set(0);
            imp.image_admission.borrow_mut().reset_high_water();
        }
        generation
    }
}
