// SPDX-License-Identifier: GPL-3.0-or-later

//! # The editor minimap workflow
//!
//! The narrow overview map down the right edge of one tab: a native
//! `GtkSourceMap` showing the whole document, a semantic marker strip painted
//! over its edge, and GTK's own viewport-highlight slider tracking where the
//! editor is looking. It appears when the user enables it or presses
//! `Ctrl+Shift+M`, and it reacts to buffer edits, search sessions, bookmarks,
//! scrolling, style changes, and — the hard case — the editor width changing
//! underneath it while the workspace sidebar animates.
//!
//! **This workflow's behavior contract is rendered pixels.** Its acceptance
//! oracle is a screenshot detector across animation frames, not an app-computed
//! rectangle: the June 2026 slider-drift regression was a case where every
//! rectangle this code computed was correct and the pixels were not. Two named
//! visual-geometry invariants, `native-minimap-highlight-anchors` and
//! `native-minimap-animation-highlight-anchors`, are required for any change to
//! this directory, and `crates/cargo-gtk-proof` plus
//! `scripts/check-visual-proof-policy.py` each key that requirement on this
//! directory's path prefix.
//!
//! This directory is a **per-workflow role home**: `ui/editor_page/` hosts eight
//! workflows and the role file names `policy.rs` and `evidence.rs` are fixed at
//! one each per workflow, so this workflow keeps its roles in a subdirectory
//! whose `mod.rs` — this file — is the facade. A prefixed `minimap_policy.rs`
//! was not available: pure policy is reached by the mutation scope through
//! `ui/**/policy.rs`, and a prefixed name would leave that scope, which is what
//! let the pre-convention hand-listed `examine_globs` entry retire.
//!
//! ## Role table
//!
//! | Module | Role | Owns |
//! | --- | --- | --- |
//! | this file | narrative facade | the entry points, this narration, and nothing else |
//! | `admission` | coordination | availability classification, analysis eligibility, generation/lifetime reservation |
//! | `analysis_execution` | coordination | the bounded cursor, the sliced scan, stale-slice rejection |
//! | `projection_execution` | coordination | marker collection, strip drawing, native geometry sync, diagnostics |
//! | `reflow_execution` | coordination | the width-reflow freeze/settle/reveal state machine |
//! | `watch` | coordination | buffer, adjustment, and search observation. **Not** the
//!   GSettings bindings: `show-minimap`, `minimap-long-line-markers-visible`, and
//!   the `minimap-width` → `width-request` mapping are bound in
//!   `ui/editor_page/imp.rs` beside the page's other preference bindings, which
//!   `.agents/rules/ui.md` requires for a pure settings-to-widget projection |
//! | `retirement` | coordination | cancellation, cursor and cache release, modified-mark clearing |
//! | `policy` | pure policy | every threshold, clamp, lane, colour, and the GTK-free content accumulator |
//! | `evidence` | evidence | `MinimapEvidence`, the one typed value observers read; `test-utils`-gated, so production reads live state directly |
//! | `test_policy` | — | the one test policy value and the single kept actuation seam, also `test-utils`-gated |
//! | `widgets` | *called presentation surface, not a role* | four read-only widget accessors read by `ui/automation.rs`, the app's D-Bus automation surface, from another workflow |
//!
//! Three `execution` modules is permitted rather than a workaround: analysis,
//! projection, and reflow are **three of this workflow's five stage orders**
//! (the other two are install and retire), each ordered and distinct, and the
//! convention lets a workflow that owns several qualify the bounded role name
//! with the stage order it serves rather than take an ill-fitting bounded name
//! for two of them.
//!
//! `MinimapState` — this workflow's widgets, markers, generations, timers, and
//! signal bag — deliberately stays in `ui/editor_page/imp.rs` as a **called
//! presentation surface**, not as a role of this workflow. It is GTK subclass
//! state whose teardown order in `dispose()` is interleaved with the editor
//! page's other subsystems, and `minimap_overlay` is a `TemplateChild` bound by
//! the Blueprint template that cannot leave `imp.rs` at all.
//!
//! ## Stages
//!
//! 1. **Install.** [`LushtextEditorPage::install_minimap`] is the only way the
//!    workflow starts. `watch` builds the native `GtkSourceMap`, the marker
//!    strip, and the render-hold overlay, then wires observation of the buffer's
//!    insert/delete/modified/changed signals, both vertical adjustments, and the
//!    in-tab search session. Every registration that can outlive its closure is
//!    tracked so `dispose()` can disconnect it.
//!
//! 2. **Decide whether the minimap may run.** `admission` classifies
//!    availability from the user preference, Focus Mode, buffer eviction, and
//!    the size tier, then decides separately whether the expensive content scan
//!    is *required*: wrapped-layout eligibility comes from an O(1) live-buffer
//!    byte estimate against an exact 2 MiB budget, never from a scan. If an
//!    accepted cache already answers the question at the current generation, no
//!    scan starts at all.
//!
//! 3. **Scan the document in bounded slices.** When a scan is required,
//!    `admission` reserves a fresh generation and the current editor lifetime,
//!    and `analysis_execution` walks the live buffer through a `GtkTextMark`
//!    cursor at most `MINIMAP_ANALYSIS_CHARS_PER_SLICE` (32 KiB) characters per
//!    turn. The document is never copied.
//!
//! 4. **Project markers and geometry.** `projection_execution` recomputes the
//!    marker model — bookmarks, search matches, modified lines, accepted
//!    long-line identities, each capped — projects it and GTK's native slider
//!    through live source-map layout, keeps the map's wrap mode and margins
//!    aligned with the editor, and redraws the strip.
//!
//! 5. **Survive the editor changing width.** `reflow_execution` owns the freeze,
//!    the settled repair, and the reveal. See below; this is the stage the
//!    workflow exists to get right.
//!
//! 6. **Give payloads back.** `retirement` cancels an in-flight scan, deletes
//!    its cursor mark, removes the sole continuation source, releases the
//!    accepted cache and retained marker identities, and clears the
//!    modified-since-save source marks.
//!
//! Alongside all of it, stage 4's modified-line marks are maintained by **four
//! other workflows** — document load, document save, local-history restore, and
//! draft restore — each of which suspends minimap edit tracking around its own
//! programmatic buffer mutation so a replacement is not recorded as a user edit.
//!
//! ## What happens between the sidebar starting to animate and the live map
//! coming back
//!
//! `AdwOverlaySplitView` allocates a new editor width on every animation frame
//! for roughly 250ms, and `GtkTextView` revalidates wrapped line heights
//! asynchronously while that happens. Any minimap margin or scroll repair
//! performed mid-burst reads a transient estimate and paints GTK's private
//! slider a few pixels off — which is exactly the drift the pixel invariants
//! detect. So:
//!
//! - the shell action calls
//!   [`LushtextEditorPage::schedule_minimap_reflow_settle_with_freeze`] *before*
//!   the transition starts, and the already-rendered native pixels are captured
//!   into stage 1's render-hold overlay — "the cover" from here on;
//! - every subsequent frame only re-arms the settle burst;
//! - 150ms after the width stops moving, the settled repair restores the user's
//!   scroll anchor, reapplies the fixed map geometry from now-rested document
//!   heights, and clears any stale source-map scroll;
//! - the live map then repaints *underneath* the cover for a conservative 800ms
//!   quiet window, because `GtkSourceMap` can still paint its first visible
//!   frame from stale private slider state;
//! - the cover is dropped exactly once, either when that window elapses or
//!   earlier if the user scrolls, which trades the conservative delay for
//!   responsiveness now that the map underneath is already correct.
//!
//! ## Why there are two ways to schedule a reflow settle
//!
//! [`LushtextEditorPage::schedule_minimap_reflow_settle_with_freeze`] is the
//! **user-action** path and captures the freeze;
//! [`LushtextEditorPage::schedule_minimap_reflow_settle`] is the **passive
//! observer** path and captures nothing. The difference is a behavior contract,
//! not an implementation detail: a passive allocation- or adjustment-derived
//! signal can fire *after* GTK has already invalidated or partially realized the
//! native map, so capturing from it would freeze an unpainted or half-realized
//! picture. Only an actor that knows a width transition is about to start can
//! capture pixels that are still the ones the user last saw.
//!
//! ## Where control leaves, and where it comes back
//!
//! Six resumption points across five stage orders. Every one re-validates
//! freshness before it does anything:
//!
//! 1. **`glib::idle_add_local`** — the analysis slice loop yields the main loop
//!    each turn and resumes in `analysis_execution`'s
//!    `run_minimap_analysis_slice`, which rejects the turn unless both the
//!    analysis generation and the editor lifetime still match, and re-checks
//!    both again after the slice and once more before publishing.
//! 2. **The marker `Debounce`** (`MINIMAP_REFRESH_DEBOUNCE`, 80ms) — edits,
//!    search updates, and adjustment notifies arrive in bursts;
//!    [`LushtextEditorPage::arm_minimap_refresh`] returns immediately and
//!    control resumes in `projection_execution`'s `run_minimap_refresh`.
//! 3. **The reflow `SettleBurst`** (`MINIMAP_REFLOW_SETTLE_DEBOUNCE`, 150ms) —
//!    resumes in `reflow_execution`'s `finish_minimap_reflow_settle`.
//! 4. **That burst handle's follow-up** (`MINIMAP_REFLOW_REVEAL_DELAY`, 800ms) —
//!    the quiet-window reveal, resuming in the same module.
//! 5. **The out-of-band early reveal** — user scrolling re-enters the same
//!    freeze machine from a *different actor* while the follow-up is still
//!    armed, through this file's
//!    [`LushtextEditorPage::reveal_minimap_reflow_freeze_for_user_scroll`].
//!    `reflow_reveal_pending` exists precisely because the reveal and the burst
//!    are separately live, and the cover must be dropped exactly once.
//! 6. **The passive `ViewportObserver`** — `ui/editor_page/overscroll.rs`
//!    re-enters stage 5 from scroll-adjustment page-size changes, without a
//!    freeze.

mod admission;
mod analysis_execution;
#[cfg(feature = "test-utils")]
mod evidence;
mod policy;
mod projection_execution;
mod reflow_execution;
mod retirement;
#[cfg(feature = "test-utils")]
mod test_policy;
mod watch;
mod widgets;

use super::LushtextEditorPage;

pub(crate) use analysis_execution::{MinimapAnalysisCache, MinimapAnalysisSession};
#[cfg(feature = "test-utils")]
pub use evidence::MinimapEvidence;
pub(crate) use policy::{
    MinimapAdjustmentDiagnostics, MinimapNativeSliderDiagnostics, MinimapTextViewRect,
};
pub use policy::{
    MinimapAnalysisAccumulator, MinimapAnalysisPolicy, MinimapAnalysisResult, MinimapAvailability,
    MinimapMarker, MinimapMarkerBounds, MinimapMarkerKind, MinimapProjectedBounds,
};

impl LushtextEditorPage {
    /// Stage 1 — install the minimap widgets and observation for this tab.
    pub(crate) fn setup_minimap(&self) {
        self.install_minimap();
    }

    /// Report the current minimap availability for this editor page.
    #[must_use]
    pub fn minimap_availability(&self) -> MinimapAvailability {
        self.minimap_availability_state()
    }

    /// Main-thread readiness query: false for hidden or unavailable refreshes,
    /// so invisible source-map work never blocks idle or visual-geometry waits.
    pub(crate) fn minimap_refresh_blocks_readiness(&self) -> bool {
        self.minimap_refresh_readiness_block()
    }

    /// Report whether queued minimap work — refresh, analysis, or reflow — is pending.
    pub(crate) fn minimap_work_pending(&self) -> bool {
        self.minimap_work_outstanding()
    }

    /// Count the currently rendered markers for one semantic category.
    #[must_use]
    pub fn minimap_marker_count(&self, kind: MinimapMarkerKind) -> usize {
        self.projected_marker_count(kind)
    }

    /// Drawable marker bounds for one category, projected through real
    /// `GtkSourceMap` layout rather than a line-count ratio.
    #[must_use]
    pub fn minimap_marker_bounds(&self, kind: MinimapMarkerKind) -> Vec<MinimapMarkerBounds> {
        self.projected_marker_bounds_for_kind(kind)
    }

    /// Return the native source-map viewport-highlight bounds relative to `target`.
    ///
    /// Main-thread only, and diagnostic: screenshot pixels remain the oracle for
    /// the rendered effect. `None` until the map is mapped with usable geometry.
    #[cfg(feature = "test-utils")]
    pub(crate) fn minimap_viewport_bounds_relative_to(
        &self,
        target: &gtk4::Widget,
    ) -> Option<MinimapProjectedBounds> {
        self.project_viewport_bounds(target)
    }

    /// Return the first rendered map content row relative to `target`.
    pub(crate) fn minimap_first_content_row_relative_to(
        &self,
        target: &gtk4::Widget,
    ) -> Option<MinimapProjectedBounds> {
        self.project_first_content_row(target)
    }

    /// Return native `GtkSourceMap` slider diagnostics relative to `target`.
    pub(crate) fn minimap_native_slider_diagnostics_relative_to(
        &self,
        target: &gtk4::Widget,
    ) -> Option<MinimapNativeSliderDiagnostics> {
        self.project_native_slider_diagnostics(target)
    }

    /// Stage 4 — recompute availability, markers, geometry, and visibility.
    pub(crate) fn refresh_minimap(&self) {
        self.run_minimap_refresh();
    }

    /// Stage 4, deferred — coalesce a refresh burst into one debounced pass.
    pub(crate) fn schedule_minimap_refresh(&self) {
        self.arm_minimap_refresh();
    }

    /// Queue a redraw of the semantic marker strip when it exists.
    pub(crate) fn queue_minimap_draw(&self) {
        self.queue_marker_strip_draw();
    }

    /// Keep the source map's wrapping and text insets aligned with the editor.
    pub(crate) fn sync_minimap_view_geometry(&self) {
        self.sync_projection_geometry();
    }

    /// The same sync, named for callers whose trigger is specifically wrap policy.
    pub(crate) fn sync_minimap_wrap_mode(&self) {
        self.sync_projection_geometry();
    }

    /// Stage 5, passive actor — coalesce a width-reflow burst without freezing.
    ///
    /// Passive allocation- and adjustment-derived observers use this path
    /// deliberately: by the time they fire GTK may already have invalidated or
    /// partially realized the native map, so capturing from here would freeze an
    /// unpainted picture. They only schedule the settled repair.
    pub(crate) fn schedule_minimap_reflow_settle(&self) {
        self.schedule_minimap_reflow_settle_impl(false);
    }

    /// Stage 5, user actor — freeze the rendered map, then coalesce the burst.
    ///
    /// Shell actions call this on the GTK main thread *before* the split-view
    /// transition starts, so the captured cover still contains the exact
    /// previously rendered native minimap pixels. The difference from the
    /// passive path above is a behavior contract, not an optimization.
    pub(crate) fn schedule_minimap_reflow_settle_with_freeze(&self) {
        self.schedule_minimap_reflow_settle_impl(true);
    }

    /// Stage 5, out-of-band — reveal the repaired map early on user scroll.
    ///
    /// Re-enters the freeze machine from a different actor while the quiet-window
    /// follow-up is still armed; the cover is still dropped exactly once.
    pub(crate) fn reveal_minimap_reflow_freeze_for_user_scroll(&self) {
        self.reveal_repaired_minimap_early();
    }

    /// Keep the freeze active when the viewport height changes mid-burst.
    ///
    /// It keeps it by **not** entering the reveal path — only the marker strip
    /// is redrawn. Revealing here would leak exactly the stale private slider
    /// frame the freeze hides, so the settled repair owns the reveal timing.
    pub(crate) fn note_minimap_height_reflow(&self) {
        self.queue_minimap_draw();
    }

    /// Mark every current line as modified-since-save, sampled under the cap.
    pub(crate) fn mark_entire_buffer_modified(&self) {
        self.mark_all_lines_modified();
    }

    /// Stage 6 — clear all modified-since-save markers for this editor.
    pub(crate) fn clear_modified_line_marks(&self) {
        self.release_modified_line_marks();
    }

    /// Stage 6 — discard accepted content evidence after a buffer edit.
    pub(crate) fn invalidate_minimap_analysis_content(&self) {
        self.discard_minimap_analysis_content();
    }

    /// Stage 6 — discard analysis whose marker preference just changed.
    pub(crate) fn invalidate_minimap_analysis_request(&self, marker_preference_changed: bool) {
        self.discard_minimap_analysis_request(marker_preference_changed);
    }

    /// Stage 6 — retire this editor's analysis for good by advancing its lifetime.
    pub(crate) fn dispose_minimap_analysis(&self) {
        self.retire_minimap_analysis();
    }

    /// Suspend edit tracking while a programmatic buffer mutation runs.
    ///
    /// Suspends and exactly restores `imp().minimap.tracking_suspended`; the
    /// four workflows that replace buffer content depend on that exactness so a
    /// replacement is never recorded as a user edit.
    pub(crate) fn set_minimap_tracking_suspended(&self, suspended: bool) {
        self.apply_minimap_tracking_suspension(suspended);
    }

    /// Detach the native source map while a bounded buffer installation runs.
    ///
    /// There is no explicit re-attach: stage 4's refresh re-binds the view once
    /// availability says the minimap is visible again.
    pub(crate) fn suspend_minimap_projection(&self) {
        self.detach_minimap_projection();
    }
}
