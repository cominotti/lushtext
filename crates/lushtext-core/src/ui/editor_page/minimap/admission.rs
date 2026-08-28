// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination — **admission**, for the availability and analysis stage order.
//!
//! The gate that decides whether the minimap may be shown at all and whether
//! the expensive bounded content scan may start. It gathers live GTK and
//! GSettings state, hands it to `policy` as scalars, and — when analysis is
//! genuinely required and no accepted cache already answers the question —
//! reserves the analysis generation and lifetime up front before
//! `analysis_execution` spends a single slice.
//!
//! The budget it protects is the O(1) live-buffer byte estimate: wrapped-layout
//! eligibility is decided from an estimate, never from a scan, so a 2 MiB
//! document costs one comparison rather than a walk.

use glib::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::analysis_execution::{MinimapAnalysisAccumulator, MinimapAnalysisSession};
use super::policy::{
    MINIMAP_LONG_LINE_MARK_CAP, MINIMAP_LONG_LINE_WARNING_THRESHOLD,
    MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET, MinimapAnalysisPolicy, MinimapAnalysisRequest,
    MinimapAvailability, MinimapAvailabilityPolicy, minimap_availability_for_policy,
    wrapped_layout_analysis_required_for_bytes,
};
use crate::config::keys;

impl LushtextEditorPage {
    /// Report the current minimap availability for this editor page.
    #[must_use]
    pub(super) fn minimap_availability_state(&self) -> MinimapAvailability {
        self.imp().minimap.availability.get()
    }

    /// Whether the minimap is currently visible for this editor page.
    #[must_use]
    pub fn is_minimap_visible(&self) -> bool {
        self.minimap_availability_state() == MinimapAvailability::Visible
    }

    /// Main-thread readiness query for queued minimap work.
    ///
    /// This reads GTK/GSettings state only; it returns false for hidden or
    /// unavailable minimap refreshes so invisible source-map work does not block
    /// Automation1 idle or visual-geometry waits.
    pub(super) fn minimap_refresh_readiness_block(&self) -> bool {
        let imp = self.imp();
        if !self.minimap_work_pending() {
            return false;
        }

        self.is_minimap_visible()
            || (imp.settings.boolean(keys::SHOW_MINIMAP)
                && !self.focus_mode_suppresses_minimap()
                && !self.is_evicted()
                && self.size_check().syntax_enabled())
    }

    /// Report whether queued minimap work is still pending.
    ///
    /// This covers both the debounced marker refresh and a pending width-reflow
    /// settle/reveal repair, so visual proof captures cannot race a frozen or
    /// not-yet-repaired native slider.
    pub(super) fn minimap_work_outstanding(&self) -> bool {
        let minimap = &self.imp().minimap;
        minimap.refresh_pending.get()
            || minimap.analysis_session.borrow().is_some()
            || minimap.reflow_settle.pending()
            || minimap.reflow_reveal_pending.get()
    }

    pub(super) fn ensure_minimap_analysis(&self, request: MinimapAnalysisRequest) {
        if !request.required() {
            self.cancel_minimap_analysis(false, false);
            return;
        }

        let imp = self.imp();
        let minimap = &imp.minimap;
        let current_generation = minimap.analysis_generation.get();
        let cache_satisfies = minimap
            .analysis_cache
            .borrow()
            .as_ref()
            .is_some_and(|cache| {
                cache.generation == current_generation
                    && (!request.long_line_markers || cache.markers_collected)
            });
        if cache_satisfies {
            self.cancel_minimap_analysis(false, false);
            return;
        }

        let active_matches = minimap
            .analysis_session
            .borrow()
            .as_ref()
            .is_some_and(|session| session.request == request);
        if active_matches {
            return;
        }
        self.cancel_minimap_analysis(false, false);

        // Reserve a fresh generation: cancellation above already advanced it, so
        // read it again rather than incrementing the stale value read on entry.
        let generation = minimap.analysis_generation.get().wrapping_add(1);
        minimap.analysis_generation.set(generation);
        let lifetime = minimap.analysis_lifetime.get();
        let buffer = self.buffer();
        let cursor_mark = buffer.create_mark(None, &buffer.start_iter(), true);
        let policy = MinimapAnalysisPolicy {
            warning_line_chars: MINIMAP_LONG_LINE_WARNING_THRESHOLD,
            wrapped_line_chars: MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET,
            marker_limit: MINIMAP_LONG_LINE_MARK_CAP,
        };
        minimap
            .analysis_session
            .replace(Some(MinimapAnalysisSession {
                generation,
                lifetime,
                request,
                buffer,
                cursor_mark,
                accumulator: MinimapAnalysisAccumulator::new(policy, request.long_line_markers),
            }));

        let editor_weak = self.downgrade();
        let source_id = glib::idle_add_local(move || {
            let Some(editor) = editor_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            editor.run_minimap_analysis_slice(generation, lifetime)
        });
        minimap.analysis_source_id.replace(Some(source_id));
    }
}

pub(super) fn current_availability(editor: &LushtextEditorPage) -> MinimapAvailability {
    let cheap = cheap_minimap_availability(editor);
    if cheap != MinimapAvailability::Visible {
        return cheap;
    }
    let wrapped_layout_too_large = wrapped_layout_analysis_required(editor)
        && editor
            .imp()
            .minimap
            .analysis_cache
            .borrow()
            .as_ref()
            .is_some_and(|cache| {
                cache.generation == editor.imp().minimap.analysis_generation.get()
                    && cache.result.wrapped_layout_too_large
            });
    minimap_availability_for_policy(MinimapAvailabilityPolicy {
        focus_suppressed: false,
        preference_enabled: true,
        evicted: false,
        syntax_enabled: true,
        wrapped_layout_too_large,
    })
}

pub(super) fn cheap_minimap_availability(editor: &LushtextEditorPage) -> MinimapAvailability {
    let focus_suppressed = editor.focus_mode_suppresses_minimap();
    let preference_enabled = editor.imp().settings.boolean(keys::SHOW_MINIMAP);
    let evicted = editor.is_evicted();
    let syntax_enabled = editor.size_check().syntax_enabled();
    minimap_availability_for_policy(MinimapAvailabilityPolicy {
        focus_suppressed,
        preference_enabled,
        evicted,
        syntax_enabled,
        wrapped_layout_too_large: false,
    })
}

pub(super) fn minimap_analysis_request(editor: &LushtextEditorPage) -> MinimapAnalysisRequest {
    MinimapAnalysisRequest {
        wrapped_layout: wrapped_layout_analysis_required(editor),
        long_line_markers: editor
            .imp()
            .settings
            .boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE),
    }
}

pub(super) fn wrapped_layout_analysis_required(editor: &LushtextEditorPage) -> bool {
    wrapped_layout_analysis_required_for_bytes(
        editor.source_view().wrap_mode() != gtk4::WrapMode::None,
        editor.estimated_live_buffer_bytes(),
    )
}
