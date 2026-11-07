// SPDX-License-Identifier: GPL-3.0-or-later

//! Evidence — the one typed value observers read for the minimap workflow.
//!
//! [`MinimapEvidence`] replaces the seven separate `*_for_test` inspection
//! functions this workflow used to expose plus the eleven-field analysis
//! snapshot. Widget tests, readiness assertions, and the Automation1 projection
//! all read this one surface; a new per-field inspection function is a
//! regression back to the shadow introspection API it replaced. When a test
//! needs a fact the surface does not carry, the field is added here.
//!
//! ## Constraints this surface owes, and where each is proved
//!
//! **Reading must not mutate.** No field here advances a counter, re-arms a
//! timer, publishes a cache, or changes the analysis generation — including the
//! four analysis metrics the surface itself reports (`slices`,
//! `chars_per_slice_high_water`, `cancellations`, `terminals`), which would make
//! the surface an observer that changes what it observes. Proved by
//! `editor_page::test_minimap_evidence_reads_do_not_advance_the_metrics_they_report`.
//!
//! **Reading must not make the toolkit create state.** The geometry fields read
//! already-realized `GtkSourceMap` layout through `compute_bounds`,
//! `line_yrange`, and `buffer_to_window_coords`. None of those materializes a
//! collection, registers a store, or starts a scan; the minimap owns no lazily
//! populated toolkit collection of the `GtkTreeListModel` kind. Unmapped or
//! unrealized widgets yield `None` rather than forcing realization.
//!
//! **Reading must be honest on a disposed widget.** GTK4 clears template
//! children in `dispose()`, before Rust's `Drop`, so
//! [`MinimapEvidence::overlay_width_request`] reads `minimap_overlay` through
//! `try_get()` and answers `None` when the child is gone. The panicking
//! accessor would turn a teardown observation into a crash. The widget-owned
//! fields already live in `RefCell<Option<..>>` and answer `false`/`None`
//! naturally. Proved by
//! `editor_page::test_minimap_evidence_reads_stay_honest_after_dispose`.
//!
//! **No field may be read from inside a mutable borrow of the state it reads.**
//! One accessor reads the whole surface through shared borrows, so
//! [`LushtextEditorPage::minimap_evidence`] computes every derived scalar and
//! drops each `Ref` **before** building the struct literal. Never add a second,
//! narrower accessor to make a nested read possible; that reintroduces the
//! scattered getters this surface replaced. Proved by
//! `editor_page::test_minimap_evidence_reads_stay_side_effect_free_across_analysis_mutation`,
//! which drives the workflow through each operation that takes such a borrow and
//! reads the surface *after* each one.
//!
//! **Aggregates are bounded and honest at zero.**
//! [`MinimapEvidence::long_line_warning_count`] holds at most one entry per
//! analysed line longer than the warning threshold, so at most 4 bytes per
//! 122 bytes of analysed text (about 3.3 % of the live buffer, with no count
//! cap), and reports `0` when no accepted marker cache exists rather than
//! pretending. The binning fields describe the last recomputation of the
//! strip's bins and are `0` before the first draw.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::policy::MinimapProjectedBounds;

/// Scalar minimap evidence: analysis lifecycle, pending work, and projection.
///
/// Every field is a `Copy` scalar or a small `Copy` rectangle. The surface
/// carries no document text, no marker identities, and no widget handles.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MinimapEvidence {
    /// Current editor-owned analysis generation.
    pub generation: u64,
    /// Whether one bounded cursor is active.
    pub active: bool,
    /// Whether the sole continuation source is armed.
    pub source_armed: bool,
    /// Whether accepted content evidence is cached.
    pub cache_owned: bool,
    /// Generation that published the accepted cache.
    pub cache_generation: Option<u64>,
    /// Whether the accepted cache includes marker identities.
    pub marker_cache_owned: bool,
    /// Characters represented by the accepted cache.
    pub cached_characters: u64,
    /// Analysis slices dispatched during this editor lifetime.
    pub slices: usize,
    /// Largest character count inspected by one slice.
    pub chars_per_slice_high_water: usize,
    /// Active generations cancelled before publication.
    pub cancellations: usize,
    /// Current generations that reached accepted cache publication.
    pub terminals: usize,
    /// Accepted long-line marker identities: every analysed line above the
    /// warning threshold, no count cap.
    ///
    /// `0` both when no scan has completed and when a completed scan found no
    /// long line; `marker_cache_owned` distinguishes the two.
    pub long_line_warning_count: usize,
    /// Whether the scalar wrapped-layout admission rule currently demands analysis.
    pub wrapped_layout_analysis_required: bool,
    /// Whether the native source map currently projects this editor.
    pub projection_attached: bool,
    /// Whether a native map instance currently exists (a retired map is
    /// unparented and dropped, never kept with a cleared view).
    pub map_present: bool,
    /// Native maps built so far (construction plus rebuilds).
    pub maps_built: u64,
    /// Retired maps still alive (expected 0): each one would stay bound to
    /// the document buffer and lay out every edited paragraph again.
    pub retired_maps_alive: usize,
    /// Whether a width-reflow settle burst is still pending.
    pub reflow_settle_pending: bool,
    /// Whether any queued minimap work is still pending.
    pub work_pending: bool,
    /// Band rows of the strip's last bin recomputation.
    pub binned_rows: usize,
    /// Row groups of that recomputation.
    pub row_groups: usize,
    /// Index queries of that recomputation, per kind: bookmark, search,
    /// modified, long line. Each is at most `row_groups + 1`.
    pub bookmark_queries: usize,
    /// See `bookmark_queries`.
    pub search_queries: usize,
    /// See `bookmark_queries`.
    pub modified_queries: usize,
    /// See `bookmark_queries`.
    pub long_line_queries: usize,
    /// Whether search bins wait for the search scan to complete (they are
    /// then the interim marks of the band's own text, design D5).
    pub search_waiting_for_scan: bool,
    /// Swapped-out modified tags still retiring.
    pub retiring_modified_tags: usize,
    /// Whether the retirement slice source is armed.
    pub retirement_armed: bool,
    /// Retirement slices run so far.
    pub retirement_slices: usize,
    /// Longest retirement slice so far, its own work in microseconds.
    pub retirement_slice_worst_us: u64,
    /// Native viewport-highlight bounds in source-map coordinates.
    pub viewport_bounds: Option<MinimapProjectedBounds>,
    /// First rendered minimap content row in source-map coordinates.
    pub first_content_row_bounds: Option<MinimapProjectedBounds>,
    /// Minimap shell width request, or `None` once the template child is gone.
    pub overlay_width_request: Option<i32>,
    /// Source-map allocation in minimap-shell coordinates.
    ///
    /// `None` when either widget is unmapped or the shell template child is
    /// already gone. Observers use this instead of reaching through `imp()` for
    /// the overlay, which would shape a production signature from a test.
    pub source_map_bounds_in_shell: Option<MinimapProjectedBounds>,
}

#[cfg(feature = "test-utils")]
impl LushtextEditorPage {
    /// Read the whole minimap evidence surface in one side-effect-free pass.
    ///
    /// Every derived scalar is computed and every `Ref` dropped before the
    /// struct literal is built, because one accessor reading the whole surface
    /// through shared borrows would otherwise panic if any field were read from
    /// inside a mutable borrow of the same state.
    #[must_use]
    pub fn minimap_evidence(&self) -> MinimapEvidence {
        let imp = self.imp();
        let minimap = &imp.minimap;

        let generation = minimap.analysis_generation.get();
        let active = minimap.analysis_session.borrow().is_some();
        let source_armed = minimap.analysis_source_id.borrow().is_some();

        let (cache_owned, cache_generation, marker_cache_owned, cached_characters, long_lines) = {
            let cache = minimap.analysis_cache.borrow();
            let entry = cache.as_ref();
            (
                entry.is_some(),
                entry.map(|cache| cache.generation),
                entry.is_some_and(|cache| cache.markers_collected),
                entry.map_or(0, |cache| cache.result.characters_examined),
                entry
                    .filter(|cache| cache.markers_collected)
                    .map_or(0, |cache| cache.result.long_line_lines.len()),
            )
        };

        let slices = minimap.analysis_slices.get();
        let chars_per_slice_high_water = minimap.analysis_chars_per_slice_high_water.get();
        let cancellations = minimap.analysis_cancellations.get();
        let terminals = minimap.analysis_terminals.get();

        let reflow_settle_pending = minimap.reflow_settle.pending();
        let work_pending = self.minimap_work_outstanding();
        let (binned_rows, row_groups, queries, search_waiting_for_scan) = minimap
            .marker_bins
            .borrow()
            .as_ref()
            .map_or((0, 0, [0; 4], false), |bins| {
                (bins.rows, bins.groups, bins.queries, bins.search_waiting)
            });
        let retiring_modified_tags = minimap.modified.retiring.borrow().len();
        let retirement_armed = minimap.modified.retirement_source.borrow().is_some();
        let retirement_slices = minimap.modified.retirement_slices.get();
        let retirement_slice_worst_us = minimap.modified.retirement_slice_worst_us.get();
        // `wrapped_layout_analysis_required` reads the source view's wrap mode
        // and live byte estimate, both of which reach the panicking
        // `TemplateChild` accessor. GTK4 clears template children in
        // `dispose()` before Rust's `Drop`, so probe with `try_get()` first and
        // answer `false` once the view is gone rather than crashing a teardown
        // observation. This exact defect was found by the disposal proof below,
        // which is why the constraint is proved and not asserted.
        let source_view_live: Option<sourceview5::View> = imp.source_view.try_get();
        let wrapped_layout_analysis_required =
            source_view_live.is_some() && super::admission::wrapped_layout_analysis_required(self);

        let source_map = minimap.source_map.borrow().as_ref().cloned();
        let projection_attached = source_map
            .as_ref()
            .is_some_and(|source_map| source_map.view().is_some());
        let viewport_bounds = source_map.as_ref().and_then(|source_map| {
            self.minimap_viewport_bounds_relative_to(source_map.upcast_ref())
        });
        let first_content_row_bounds = source_map.as_ref().and_then(|source_map| {
            self.minimap_first_content_row_relative_to(source_map.upcast_ref())
        });

        // `try_get()` rather than the panicking accessor: GTK4 clears template
        // children in `dispose()` before Rust's `Drop`, and a teardown
        // observation must produce an honest answer instead of a crash.
        let overlay = imp.minimap_overlay.try_get();
        let overlay_width_request = overlay
            .as_ref()
            .map(gtk4::prelude::WidgetExt::width_request);
        let source_map_bounds_in_shell =
            source_map
                .as_ref()
                .zip(overlay.as_ref())
                .and_then(|(source_map, overlay)| {
                    super::projection_execution::source_map_bounds_relative_to(
                        source_map,
                        overlay.upcast_ref(),
                    )
                });

        MinimapEvidence {
            generation,
            active,
            source_armed,
            cache_owned,
            cache_generation,
            marker_cache_owned,
            cached_characters,
            slices,
            chars_per_slice_high_water,
            cancellations,
            terminals,
            long_line_warning_count: long_lines,
            wrapped_layout_analysis_required,
            projection_attached,
            map_present: source_map.is_some(),
            maps_built: minimap.maps_built.get(),
            retired_maps_alive: minimap
                .retired_maps
                .borrow()
                .iter()
                .filter(|map| map.upgrade().is_some())
                .count(),
            reflow_settle_pending,
            work_pending,
            binned_rows,
            row_groups,
            bookmark_queries: queries[0],
            search_queries: queries[1],
            modified_queries: queries[2],
            long_line_queries: queries[3],
            search_waiting_for_scan,
            retiring_modified_tags,
            retirement_armed,
            retirement_slices,
            retirement_slice_worst_us,
            viewport_bounds,
            first_content_row_bounds,
            overlay_width_request,
            source_map_bounds_in_shell,
        }
    }
}
