// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — execution, qualified by stage order (projection).
//!
//! Turns an immutable plan into GTK text tags and anchored widgets, **at most
//! one batch per main-loop turn**. The per-turn ceiling is the point: a long
//! document renders progressively instead of freezing the frame, and the
//! projection yields with its cross-turn state reified in
//! `MarkdownProjectionContinuation` so the next turn resumes where the last
//! stopped rather than re-deriving position from the buffer.
//!
//! Resumption point: each queued batch turn. Every one revalidates the render
//! session generation first, so a superseded projection stops without
//! publishing. A `ContinuationBreach` is the case where cross-turn state no
//! longer describes the buffer; it fails the render loudly rather than
//! projecting onto a document it cannot account for.

use std::sync::atomic::Ordering;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use std::collections::VecDeque;

use super::code_blocks::CodeBlockTheme;
use crate::services::markdown_render::MarkdownRenderState;

use crate::services::markdown_render::MarkdownEventBatch;
use crate::ui::accessibility;

use super::continuation::{ContinuationBreach, MarkdownProjectionContinuation};
use super::seams::{
    GuardedMarkdownPlan, GuardedMarkdownProjection, MarkdownProjectionTerminal,
    PendingMarkdownWork, simplified_render_description,
};
use super::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};

impl LushtextMarkdownPreview {
    /// Accept a current immutable plan and project at most one batch per GTK turn.
    pub(super) fn start_render_plan(
        &self,
        generation: u64,
        plan: GuardedMarkdownPlan,
        context: MarkdownPreviewRenderContext,
    ) {
        if !self.imp().render_session.borrow().is_current(generation) {
            self.retire_guarded_markdown(plan);
            return;
        }
        if self.markdown_retirement_at_capacity() {
            self.defer_markdown_work(PendingMarkdownWork::Projection {
                generation,
                plan,
                context,
            });
            return;
        }
        self.show_content_view();
        self.clear_rendered_state(false);
        self.imp()
            .render_session
            .borrow_mut()
            .transition(generation, MarkdownRenderState::Projecting);

        let user_visible_omissions = plan.user_visible_omissions();
        // One palette per generation instead of one per projection turn. It stays
        // outside the guarded projection payload because a GtkSourceView style
        // scheme is not `Send` and that payload is retired off the GTK thread.
        let code_block_theme =
            CodeBlockTheme::from_settings(&gtk4::gio::Settings::new(crate::config::APP_ID));
        let mut projection =
            Some(
                plan.map_preserving_reservation(|plan| GuardedMarkdownProjection {
                    batches: VecDeque::from(plan.batches),
                    limit: plan.limit,
                    user_visible_omissions,
                    continuation: MarkdownProjectionContinuation::new(),
                }),
            );
        // Preserve immediate small-document rendering while the planner's
        // event-and-byte ceilings bound this initial GTK turn exactly like
        // every deferred projection slice.
        if let Some(breach) = self.project_next_batch(
            generation,
            projection.as_deref_mut(),
            &context,
            &code_block_theme,
        ) {
            // A refused batch leaves the continuation holding an in-flight embed
            // buffer, so this takes the same accounted retirement path the
            // deferred slice uses: dropping it here would skip the pending
            // counter `render_pending()` consults and let readiness report
            // settled while a document-sized payload is still queued.
            if let Some(retired) = projection.take() {
                self.retire_guarded_markdown(retired);
            }
            self.finish_render_breach(generation, breach);
            return;
        }
        if projection
            .as_ref()
            .is_none_or(|projection| projection.batches.is_empty())
        {
            let terminal = projection
                .as_deref()
                .map(GuardedMarkdownProjection::terminal);
            drop(projection.take());
            self.finish_render_plan(generation, terminal.unwrap_or_default());
            return;
        }
        accessibility::set_description(
            &*self.imp().text_view,
            "Markdown preview projection is pending",
        );

        let preview_weak = self.downgrade();
        glib::idle_add_local(move || {
            let Some(preview) = preview_weak.upgrade() else {
                drop(projection.take());
                return glib::ControlFlow::Break;
            };
            if !preview.imp().render_session.borrow().is_current(generation) {
                if let Some(retired) = projection.take() {
                    preview.retire_guarded_markdown(retired);
                }
                return glib::ControlFlow::Break;
            }
            if let Some(breach) = preview.project_next_batch(
                generation,
                projection.as_deref_mut(),
                &context,
                &code_block_theme,
            ) {
                if let Some(retired) = projection.take() {
                    preview.retire_guarded_markdown(retired);
                }
                preview.finish_render_breach(generation, breach);
                return glib::ControlFlow::Break;
            }
            if projection
                .as_ref()
                .is_none_or(|projection| projection.batches.is_empty())
            {
                let terminal = projection
                    .as_deref()
                    .map(GuardedMarkdownProjection::terminal);
                drop(projection.take());
                preview.finish_render_plan(generation, terminal.unwrap_or_default());
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    /// Apply at most one batch into the generation-owned continuation.
    ///
    /// Returns the breach that made the batch unusable, if any: the continuation
    /// validates each batch's expected structure before applying it, so a
    /// mis-chained carry becomes an explicit terminal instead of corrupted
    /// rendered content.
    pub(super) fn project_next_batch(
        &self,
        generation: u64,
        projection: Option<&mut GuardedMarkdownProjection>,
        context: &MarkdownPreviewRenderContext,
        code_block_theme: &CodeBlockTheme,
    ) -> Option<ContinuationBreach> {
        let projection = projection?;
        let batch = projection.batches.pop_front()?;
        self.apply_render_batch(generation, &batch);
        projection
            .continuation
            .apply_batch(self, batch, context, code_block_theme)
            .err()
    }

    /// Record direct slice evidence before applying a current batch.
    pub(super) fn apply_render_batch(&self, generation: u64, batch: &MarkdownEventBatch) {
        debug_assert!(self.imp().render_session.borrow().is_current(generation));
        #[cfg(not(feature = "test-utils"))]
        let _ = batch;
        #[cfg(feature = "test-utils")]
        {
            let imp = self.imp();
            imp.projection_dispatch_count
                .set(imp.projection_dispatch_count.get().wrapping_add(1));
            imp.projection_high_water_events
                .set(imp.projection_high_water_events.get().max(batch.len()));
        }
    }

    /// Publish the terminal this generation's projection reached.
    ///
    /// Three terminals stay distinguishable here. A global budget still stops
    /// planning and publishes `Limited` with that budget's own copy. A plan that
    /// reached the end of the document but replaced named units with markers
    /// publishes `Simplified` and reports the count once, rather than announcing
    /// each marker as it is projected. Everything else publishes `Complete`.
    pub(super) fn finish_render_plan(&self, generation: u64, terminal: MarkdownProjectionTerminal) {
        let imp = self.imp();
        if !imp.render_session.borrow().is_current(generation) {
            return;
        }
        let (state, description) = match terminal.limit {
            Some(limit) => (
                MarkdownRenderState::Limited,
                Some(limit.description().to_string()),
            ),
            None if terminal.user_visible_omissions > 0 => (
                MarkdownRenderState::Simplified,
                Some(simplified_render_description(
                    terminal.user_visible_omissions,
                )),
            ),
            None => (MarkdownRenderState::Complete, None),
        };
        match description {
            Some(description) => {
                let buffer = imp.text_view.buffer();
                let mut end = buffer.end_iter();
                if end.offset() > 0 {
                    buffer.insert(&mut end, "\n\n");
                }
                buffer.insert(&mut end, &description);
                accessibility::set_description(&*imp.text_view, &description);
            }
            None => accessibility::set_description(&*imp.text_view, "Rendered Markdown preview"),
        }
        imp.render_session
            .borrow_mut()
            .transition(generation, state);
        self.queue_code_block_width_refresh();
    }

    /// Publish the explicit terminal for a batch the continuation refused.
    pub(super) fn finish_render_breach(&self, generation: u64, breach: ContinuationBreach) {
        let imp = self.imp();
        if !imp.render_session.borrow().is_current(generation) {
            return;
        }
        let description = breach.description();
        let buffer = imp.text_view.buffer();
        let mut end = buffer.end_iter();
        if end.offset() > 0 {
            buffer.insert(&mut end, "\n\n");
        }
        buffer.insert(&mut end, description);
        accessibility::set_description(&*imp.text_view, description);
        imp.render_session
            .borrow_mut()
            .transition(generation, MarkdownRenderState::Failed);
        self.queue_code_block_width_refresh();
    }

    /// Cancel current planning/projection work without retaining stale payloads.
    pub(super) fn cancel_render_session(&self) {
        self.cancel_pending_markdown_planning();
        self.cancel_deferred_markdown_work();
        self.cancel_queued_image_work();
        let generation = self.imp().render_session.borrow_mut().cancel();
        self.imp()
            .render_generation
            .store(generation, Ordering::Release);
    }
}
