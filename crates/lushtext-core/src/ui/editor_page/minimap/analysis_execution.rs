// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination — **execution**, for the sliced content-analysis stage order.
//!
//! The forward phases of the bounded GTK-iterator scan. One
//! [`MinimapAnalysisSession`] owns a live buffer cursor as a `GtkTextMark`, and
//! each turn inspects at most `MINIMAP_ANALYSIS_CHARS_PER_SLICE` characters
//! before yielding the main loop. The document is never copied: the accumulator
//! in `policy` takes one `char` at a time straight off the cursor.
//!
//! This module is one of three stage-order-qualified `execution` roles in this
//! workflow — analysis, projection, and reflow are three distinct ordered stage
//! orders, and the convention permits qualifying the bounded role name with the
//! stage order it serves rather than taking an ill-fitting name for two of them.
//!
//! **Inversion.** The scan is driven by `glib::idle_add_local`; control leaves
//! at the end of each turn and resumes in `run_minimap_analysis_slice`, which
//! re-checks both the analysis generation and the editor lifetime before doing
//! anything and again before publishing. A stale slice publishes nothing.

use glib::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::policy::{MINIMAP_ANALYSIS_CHARS_PER_SLICE, MinimapAnalysisRequest};

pub(super) use super::policy::{MinimapAnalysisAccumulator, MinimapAnalysisResult};

/// Accepted content evidence reused by layout availability and marker projection.
pub(crate) struct MinimapAnalysisCache {
    pub(super) generation: u64,
    pub(super) markers_collected: bool,
    pub(super) result: MinimapAnalysisResult,
}

/// One bounded GTK iterator cursor owned by the current editor generation.
pub(crate) struct MinimapAnalysisSession {
    pub(super) generation: u64,
    pub(super) lifetime: u64,
    pub(super) request: MinimapAnalysisRequest,
    pub(super) buffer: sourceview5::Buffer,
    pub(super) cursor_mark: gtk4::TextMark,
    pub(super) accumulator: MinimapAnalysisAccumulator,
}

impl LushtextEditorPage {
    /// Whether a dispatched slice still speaks for this editor's live analysis.
    ///
    /// Both halves matter and neither implies the other: the generation retires
    /// one superseded request, while the lifetime retires every request this
    /// editor will ever make. The slice loop re-asks before it starts a turn,
    /// again after the turn, and once more before publishing, so this predicate
    /// is named rather than spelled out at each of those three points.
    fn minimap_analysis_is_current(&self, generation: u64, lifetime: u64) -> bool {
        let minimap = &self.imp().minimap;
        minimap.analysis_generation.get() == generation
            && minimap.analysis_lifetime.get() == lifetime
    }

    pub(super) fn run_minimap_analysis_slice(
        &self,
        generation: u64,
        lifetime: u64,
    ) -> glib::ControlFlow {
        let imp = self.imp();
        if !self.minimap_analysis_is_current(generation, lifetime) {
            return glib::ControlFlow::Break;
        }
        let Some(mut session) = imp.minimap.analysis_session.take() else {
            imp.minimap.analysis_source_id.take();
            return glib::ControlFlow::Break;
        };
        if session.generation != generation
            || session.lifetime != lifetime
            || session.buffer != self.buffer()
        {
            session.buffer.delete_mark(&session.cursor_mark);
            imp.minimap.analysis_source_id.take();
            return glib::ControlFlow::Break;
        }

        let mut iter = session.buffer.iter_at_mark(&session.cursor_mark);
        let end = session.buffer.end_iter();
        let mut inspected = 0usize;
        while iter != end && inspected < MINIMAP_ANALYSIS_CHARS_PER_SLICE {
            session.accumulator.inspect_char(iter.char());
            inspected = inspected.saturating_add(1);
            if !iter.forward_char() {
                break;
            }
        }
        session.buffer.move_mark(&session.cursor_mark, &iter);
        #[cfg(feature = "test-utils")]
        {
            imp.minimap
                .analysis_slices
                .set(imp.minimap.analysis_slices.get().saturating_add(1));
            imp.minimap.analysis_chars_per_slice_high_water.set(
                imp.minimap
                    .analysis_chars_per_slice_high_water
                    .get()
                    .max(inspected),
            );
            if let Some(hook) = imp.minimap.analysis_after_slice_hook.take() {
                hook();
            }
        }
        if !self.minimap_analysis_is_current(generation, lifetime) {
            session.buffer.delete_mark(&session.cursor_mark);
            imp.minimap.analysis_source_id.take();
            return glib::ControlFlow::Break;
        }

        let complete = iter == end
            || (session.request.wrapped_layout
                && !session.request.long_line_markers
                && session.accumulator.wrapped_layout_too_large());
        if !complete {
            imp.minimap.analysis_session.replace(Some(session));
            return glib::ControlFlow::Continue;
        }

        session.buffer.delete_mark(&session.cursor_mark);
        imp.minimap.analysis_source_id.take();
        if !self.minimap_analysis_is_current(generation, lifetime) {
            return glib::ControlFlow::Break;
        }
        imp.minimap
            .analysis_cache
            .replace(Some(MinimapAnalysisCache {
                generation,
                markers_collected: session.request.long_line_markers,
                result: session.accumulator.finish(),
            }));
        #[cfg(feature = "test-utils")]
        imp.minimap
            .analysis_terminals
            .set(imp.minimap.analysis_terminals.get().saturating_add(1));
        self.run_minimap_refresh();
        glib::ControlFlow::Break
    }
}
