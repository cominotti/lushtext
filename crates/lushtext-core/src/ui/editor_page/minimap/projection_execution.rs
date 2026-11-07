// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination — **execution**, for the marker and geometry projection stage order.
//!
//! The workflow's primary rendering work: recompute the semantic marker model,
//! project it and the native slider through live `GtkSourceMap` layout, keep the
//! map's wrap mode and margins aligned with the editor, and redraw the strip.
//!
//! Everything here is a **gatherer**: it reads live GTK allocation, text-iter,
//! adjustment, and style state and hands scalars to `policy`, which owns every
//! clamp, minimum, lane width, and colour. Nothing in this module decides
//! geometry; it only supplies and applies it.
//!
//! **Inversion.** Marker refresh is debounced through
//! `MinimapState::refresh_debounce` (`MINIMAP_REFRESH_DEBOUNCE`, 80ms):
//! `arm_minimap_refresh` returns immediately and control resumes in
//! `run_minimap_refresh` once the burst of edits, search updates, and adjustment
//! notifies has quietened.
//!
//! **Markers are binned per strip pixel row** (`bin-minimap-markers-per-row`
//! D2, D3, D7). The strip's draw function gathers the rendered-content band,
//! rounded outward to whole rows, and the line GTK's `line_at_y` shows at each
//! row (ledger A99), then asks each kind's index — the completed search scan
//! (A100), the modified tag (A98), the sorted long-line and bookmark lines —
//! at most once per row group plus once, through `policy`'s walker. The bins
//! are cached per draw input (map scroll value, map layout height, strip
//! height, band, marker generation), so a map scroll (A101: the strip is not
//! redrawn unless it asks) recomputes them and an unchanged frame reuses them.
//! Nothing here enumerates markers, so the cost follows the strip height.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{self, cairo};
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::admission::{
    cheap_minimap_availability, current_availability, minimap_analysis_request,
};
use super::policy::{
    MINIMAP_INTERIM_SEARCH_CHARS, MINIMAP_MARKER_MIN_HEIGHT, MINIMAP_REFRESH_DEBOUNCE,
    MINIMAP_TOP_CONTENT_MARGIN, MINIMAP_VIEWPORT_HORIZONTAL_OUTSET, MINIMAP_VIEWPORT_MIN_HEIGHT,
    MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS, MarkerRowBand, MarkerRowGroup, MarkerRowRun,
    MinimapAdjustmentDiagnostics, MinimapAvailability, MinimapMarkerBounds, MinimapMarkerKind,
    MinimapNativeProjectionSource, MinimapNativeSliderDiagnostics, MinimapProjectedBounds,
    MinimapProjectionSpace, MinimapTextViewRect, NativeSliderEstimateInput, ProjectedBoundsFit,
    document_height_from_line_span, first_sorted_line_at_or_after,
    fit_native_slider_to_source_map_bounds, fit_projected_bounds, line_bottom_in_target,
    line_top_in_target, mark_row_groups, marker_lane_width, marker_lane_x, marker_rgba,
    marker_row_groups, marker_row_runs, minimap_unavailable_message,
    native_slider_estimate_from_inputs, wide_editor_slider_offset_class,
};
use crate::config::keys;
use crate::ui::status_bar::MessageKind;
use crate::ui::window::LushtextWindow;

/// The marker kinds in drawing order (later kinds paint over earlier ones).
const DRAW_ORDER: [MinimapMarkerKind; 4] = [
    MinimapMarkerKind::Bookmark,
    MinimapMarkerKind::Search,
    MinimapMarkerKind::Modified,
    MinimapMarkerKind::LongLine,
];

/// The draw inputs the bins depend on; equal keys reuse the bins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MarkerBinGeometry {
    /// Map scroll value, in milli-pixels.
    map_value_milli: i64,
    /// Map laid-out height (`upper`), in milli-pixels: a zoom or line-count
    /// change moves the layout without moving the value.
    map_upper_milli: i64,
    /// Strip height in rows.
    strip_height: i32,
    /// Map top edge in strip rows.
    map_y: i32,
    /// Rendered-content band in strip rows.
    band: MarkerRowBand,
}

/// The per-row bins of the last draw (`MinimapState::marker_bins`).
pub(crate) struct MinimapMarkerBins {
    geometry: MarkerBinGeometry,
    marker_generation: u64,
    search_generation: u64,
    /// Whether the search runs came from a completed scan (`false` while
    /// they are kept across a rescan or absent while a scan runs).
    search_complete: bool,
    /// Drawn runs per kind, in `DRAW_ORDER`.
    runs: [Vec<MarkerRowRun>; 4],
    /// Index queries per kind in the last recomputation, in `DRAW_ORDER`.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(dead_code, reason = "evidence only")
    )]
    pub(super) queries: [usize; 4],
    /// Row groups in the last recomputation.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(dead_code, reason = "evidence only")
    )]
    pub(super) groups: usize,
    /// Band rows in the last recomputation.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(dead_code, reason = "evidence only")
    )]
    pub(super) rows: usize,
    /// Whether search bins wait for a scan to complete.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(dead_code, reason = "evidence only")
    )]
    pub(super) search_waiting: bool,
}

impl MinimapMarkerBins {
    fn runs(&self, kind: MinimapMarkerKind) -> &[MarkerRowRun] {
        &self.runs[kind_index(kind)]
    }
}

fn kind_index(kind: MinimapMarkerKind) -> usize {
    match kind {
        MinimapMarkerKind::Bookmark => 0,
        MinimapMarkerKind::Search => 1,
        MinimapMarkerKind::Modified => 2,
        MinimapMarkerKind::LongLine => 3,
    }
}

impl LushtextEditorPage {
    /// Count the marker runs the strip draws for one semantic category, in the
    /// band the map shows.
    ///
    /// This exists mainly so widget tests can assert that bookmark, search,
    /// modified, and long-line markers appear and disappear as expected. A
    /// marker on a line scrolled out of the map is not drawn and not counted.
    #[must_use]
    pub(super) fn projected_marker_count(&self, kind: MinimapMarkerKind) -> usize {
        self.with_marker_bins(|bins| bins.runs(kind).len())
            .unwrap_or_default()
    }

    /// Return the drawn marker runs for one semantic category, in strip
    /// coordinates.
    ///
    /// The runs are binned through the real `GtkSourceMap` layout, so widget
    /// tests can assert the marker strip follows source-map geometry instead
    /// of a hand-rolled line-count ratio.
    #[must_use]
    pub(super) fn projected_marker_bounds_for_kind(
        &self,
        kind: MinimapMarkerKind,
    ) -> Vec<MinimapMarkerBounds> {
        self.with_marker_bins(|bins| {
            bins.runs(kind)
                .iter()
                .map(|run| MinimapMarkerBounds {
                    kind,
                    top: f64::from(run.top),
                    bottom: f64::from(run.bottom),
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// Run `read` on the bins of the current draw inputs, recomputing them when
    /// an input changed; `None` while the strip or the map has no geometry.
    pub(super) fn with_marker_bins<T>(
        &self,
        read: impl FnOnce(&MinimapMarkerBins) -> T,
    ) -> Option<T> {
        let imp = self.imp();
        let source_map = imp.minimap.source_map.borrow().as_ref().cloned()?;
        let marker_strip = imp.minimap.marker_strip.borrow().as_ref().cloned()?;
        let geometry = marker_bin_geometry(&source_map, &marker_strip)?;
        let marker_generation = imp.minimap.marker_generation.get();
        let search_generation = imp.minimap.search_generation.get();
        let search_complete = self.search_scan_complete();
        let reusable = imp
            .minimap
            .marker_bins
            .borrow()
            .as_ref()
            .is_some_and(|bins| {
                bins.geometry == geometry
                    && bins.marker_generation == marker_generation
                    && bins.search_generation == search_generation
                    && bins.search_complete == search_complete
            });
        if !reusable {
            let previous = imp.minimap.marker_bins.borrow_mut().take();
            let bins = compute_marker_bins(
                self,
                &source_map,
                geometry,
                (marker_generation, search_generation, search_complete),
                previous.as_ref(),
            );
            imp.minimap.marker_bins.replace(Some(bins));
        }
        let bins = imp.minimap.marker_bins.borrow();
        bins.as_ref().map(read)
    }

    /// Whether the in-tab search has a completed scan the search index may
    /// read (`occurrences-count >= 0`, ledger A100); `false` with no search.
    pub(super) fn search_scan_complete(&self) -> bool {
        self.is_search_visible()
            && self
                .search_bar()
                .search_context()
                .is_some_and(|context| context.occurrences_count() >= 0)
    }

    /// Whether a visible search is still scanning, so search bins wait.
    pub(super) fn search_scan_pending(&self) -> bool {
        self.is_search_visible()
            && self.search_bar().search_context().is_some_and(|context| {
                context.occurrences_count() < 0 && context.regex_error().is_none()
            })
    }

    /// Invalidate the bins of every kind and redraw the strip.
    pub(super) fn note_marker_change(&self) {
        let generation = &self.imp().minimap.marker_generation;
        generation.set(generation.get().wrapping_add(1));
        self.queue_marker_strip_draw();
    }

    /// Return the native source-map viewport-highlight bounds relative to `target`.
    ///
    /// Main-thread only: this queries GTK allocation and `GtkSourceMap` line
    /// geometry. Returns `None` until the map is mounted/mapped or the
    /// projection cannot produce finite positive bounds; screenshot smoke uses
    /// this only as a crop and diagnostic hint.
    #[cfg(feature = "test-utils")]
    pub(super) fn project_viewport_bounds(
        &self,
        target: &gtk4::Widget,
    ) -> Option<MinimapProjectedBounds> {
        self.project_native_slider_diagnostics(target)
            .map(|diagnostics| diagnostics.native_slider_visible_bounds)
    }

    /// Return the first rendered map content row relative to `target`.
    ///
    /// Main-thread only: this mirrors `GtkSourceMap` line geometry for
    /// diagnostics, returning `None` until GTK has a mapped, positive allocation
    /// or no rendered content row can be projected.
    pub(super) fn project_first_content_row(
        &self,
        target: &gtk4::Widget,
    ) -> Option<MinimapProjectedBounds> {
        let source_map = self.imp().minimap.source_map.borrow().as_ref().cloned()?;
        minimap_first_content_row_bounds(&source_map, target, target.height())
    }

    /// Return native `GtkSourceMap` slider diagnostics relative to `target`.
    ///
    /// Main-thread only: this reads public `GtkTextView` geometry from the editor
    /// and bound source map. The returned estimate mirrors upstream slider inputs
    /// but remains diagnostic; screenshot pixels remain the rendered-effect oracle.
    pub(super) fn project_native_slider_diagnostics(
        &self,
        target: &gtk4::Widget,
    ) -> Option<MinimapNativeSliderDiagnostics> {
        let source_map = self.imp().minimap.source_map.borrow().as_ref().cloned()?;
        minimap_native_slider_diagnostics(self, &source_map, target, target.height())
    }

    /// Keep the source map's wrapping and text insets aligned with the editor.
    ///
    /// The minimap viewport is a visual promise about the editor, so width
    /// reflow from word wrap must be reflected in the map before its native
    /// viewport slider and our marker strip settle. The top margin is explicit
    /// because the minimap shell has its own border/padding and a flush first
    /// line is easy to clip by one pixel after adaptive shell reallocation.
    pub(super) fn sync_projection_geometry(&self) {
        if self.imp().minimap.reflow_settle.pending() {
            // A width-reflow burst is still in flight, so wrapped document
            // heights are transient estimates. Any margin derived from them
            // would move the native slider by whole pixels mid-animation. The
            // settle repair re-runs this sync once the width stops changing.
            return;
        }
        let Some(source_map) = self.imp().minimap.source_map.borrow().as_ref().cloned() else {
            return;
        };
        sync_source_map_geometry(&source_map, self.source_view());
    }

    /// Refresh minimap visibility, markers, and any one-shot availability feedback.
    pub(super) fn run_minimap_refresh(&self) {
        self.imp().minimap.refresh_pending.set(false);
        let request = minimap_analysis_request(self);
        if cheap_minimap_availability(self) == MinimapAvailability::Visible {
            self.ensure_minimap_analysis(request);
        } else {
            self.cancel_minimap_analysis(false, false);
        }
        let availability = current_availability(self);
        self.imp().minimap.availability.set(availability);
        if availability != MinimapAvailability::Reduced {
            self.imp().minimap.reduced_feedback_shown.set(false);
        }

        // Visibility and native projection are one lifecycle decision. A
        // hidden GtkSourceMap that remains bound still performs text layout,
        // so unavailable and load-suspended states retire the map instance,
        // and a visible state attaches a freshly built one. A live map's view
        // is never cleared (ledger A32).
        let projection_visible =
            availability == MinimapAvailability::Visible && !self.load_projection_suspended();
        if projection_visible {
            self.attach_native_source_map();
        } else {
            self.retire_native_source_map();
        }

        let overlay = &self.imp().minimap_overlay;
        overlay.set_visible(projection_visible);

        if !projection_visible {
            // Hidden source maps can keep stale EOF/top margins from the last
            // visible layout. Cancel pending timers first, then sync once so
            // the next visible frame cannot inherit old geometry.
            let minimap = &self.imp().minimap;
            let _ = minimap.reflow_settle.clear();
            minimap.reflow_reveal_pending.set(false);
            self.imp().overscroll.reflow_pause.borrow_mut().take();
            self.sync_projection_geometry();
            self.drop_minimap_reflow_freeze();
            self.imp().minimap.marker_bins.borrow_mut().take();
            self.note_marker_change();
            if availability != MinimapAvailability::Visible {
                self.publish_minimap_unavailable_feedback_if_needed(availability);
            }
            return;
        }

        self.sync_projection_geometry();
        self.watch_search_scan();
        self.note_marker_change();
    }

    /// Debounce marker recomputation after search, edits, or viewport changes.
    pub(super) fn arm_minimap_refresh(&self) {
        self.imp().minimap.refresh_pending.set(true);

        self.imp().minimap.refresh_debounce.schedule(
            self,
            MINIMAP_REFRESH_DEBOUNCE,
            move |editor, _| {
                editor.run_minimap_refresh();
            },
        );
    }

    /// Queue a redraw of the semantic marker strip when it exists.
    pub(super) fn queue_marker_strip_draw(&self) {
        if let Some(strip) = self.imp().minimap.marker_strip.borrow().as_ref() {
            strip.queue_draw();
        }
    }

    /// Tag the inclusive line range as modified-since-save, from the start of
    /// `start_line` to the start of the line after `end_line` (the buffer end
    /// on the last line). Tagging whole lines merges with a run already on
    /// them, so the tag holds at most two toggles per line. An empty last line
    /// has no character to tag; its terminator above is tagged instead, so
    /// the edit shows on the line above it.
    pub(super) fn record_modified_line_marks(&self, start_line: u32, end_line: u32) {
        let buffer = self.buffer();
        let mut start = text_iter_at_line_or_last(buffer.upcast_ref(), start_line);
        let mut end = end_line
            .checked_add(1)
            .and_then(|line| i32::try_from(line).ok())
            .and_then(|line| buffer.iter_at_line(line))
            .filter(|end| end.line() > start.line())
            .unwrap_or_else(|| buffer.end_iter());
        if start == end {
            if !start.backward_char() {
                return;
            }
            end = buffer.end_iter();
        }
        let tag = self.current_modified_tag();
        buffer.apply_tag(&tag, &start, &end);
    }

    /// Mark every current line as modified-since-save.
    ///
    /// Restored drafts are already different from the saved file content, so
    /// this helper lets that programmatic restore surface as minimap feedback:
    /// a fresh tag over the whole buffer, one toggle pair (10–12 µs, A3).
    pub(super) fn mark_all_lines_modified(&self) {
        self.swap_modified_tag();
        let buffer = self.buffer();
        let tag = self.current_modified_tag();
        buffer.apply_tag(&tag, &buffer.start_iter(), &buffer.end_iter());
        self.note_marker_change();
        self.arm_minimap_refresh();
    }

    /// Publish the one-shot "too large" message when this page is the active tab.
    fn publish_minimap_unavailable_feedback_if_needed(&self, availability: MinimapAvailability) {
        if availability != MinimapAvailability::Reduced
            || self.imp().minimap.reduced_feedback_shown.get()
            || !self.imp().settings.boolean(keys::SHOW_MINIMAP)
        {
            return;
        }

        let Some(root) = self
            .root()
            .and_then(|root| root.downcast::<LushtextWindow>().ok())
        else {
            return;
        };

        if !root.is_active_editor(self) {
            return;
        }

        root.publish_status_message(
            minimap_unavailable_message(self.document_scale().reduced),
            MessageKind::Warning,
        );
        self.imp().minimap.reduced_feedback_shown.set(true);
    }
}

pub(super) fn sync_source_map_geometry(
    source_map: &sourceview5::Map,
    source_view: &sourceview5::View,
) {
    if source_map.wrap_mode() != gtk4::WrapMode::None {
        source_map.set_wrap_mode(gtk4::WrapMode::None);
    }

    let top_margin = MINIMAP_TOP_CONTENT_MARGIN;
    if source_map.top_margin() != top_margin {
        source_map.set_top_margin(top_margin);
    }

    let bottom_margin = source_view.bottom_margin();
    if source_map.bottom_margin() != bottom_margin {
        source_map.set_bottom_margin(bottom_margin);
    }

    if source_map.left_margin() != 0 {
        source_map.set_left_margin(0);
    }
    if source_map.right_margin() != 0 {
        source_map.set_right_margin(0);
    }

    force_source_map_top_layout(source_map);
    sync_wide_editor_slider_offset(source_map, source_view);
}

/// Apply the pure-tested wide-editor threshold to the native slider correction class.
fn sync_wide_editor_slider_offset(source_map: &sourceview5::Map, source_view: &sourceview5::View) {
    if wide_editor_slider_offset_class(source_map_editor_document_heights(source_map, source_view))
        .is_some()
    {
        source_map.add_css_class(MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS);
    } else {
        source_map.remove_css_class(MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS);
    }
}

/// Read live GTK document heights, as `(editor, source map)`, for the tested ratio policy.
fn source_map_editor_document_heights(
    source_map: &sourceview5::Map,
    source_view: &sourceview5::View,
) -> Option<(i32, i32)> {
    let buffer = source_map.buffer();
    let end_iter = buffer.end_iter();
    let editor_document_height =
        document_height_from_line_span_of(source_view.iter_location(&end_iter))?;
    let source_map_document_height =
        document_height_from_line_span_of(source_map.iter_location(&end_iter))?;
    Some((editor_document_height, source_map_document_height))
}

/// Nudge GTK into validating the source map's first and last line geometry.
///
/// Margin and wrap writes only queue invalidation; asking for the start and end
/// iter locations makes the next margin computation read current values instead
/// of stale pre-reflow line geometry.
fn force_source_map_top_layout(source_map: &sourceview5::Map) {
    let buffer = source_map.buffer();
    let start_iter = buffer.start_iter();
    let end_iter = buffer.end_iter();
    let _ = source_map.line_yrange(&start_iter);
    let _ = source_map.iter_location(&end_iter);
}

/// Collect non-content geometry used to estimate GTK's native source-map slider.
///
/// This runs on the GTK thread and exposes only rectangles, adjustments, and
/// ratios so screenshot artifacts can explain rendered slider drift without
/// leaking document text.
fn minimap_native_slider_diagnostics(
    editor: &LushtextEditorPage,
    source_map: &sourceview5::Map,
    target: &gtk4::Widget,
    target_height: i32,
) -> Option<MinimapNativeSliderDiagnostics> {
    if target_height <= 0 || !source_map.is_mapped() || !target.is_mapped() {
        return None;
    }

    let source_map_bounds = source_map_bounds_relative_to(source_map, target)?;
    let editor_visible_rect = text_view_rect(editor.source_view().visible_rect());
    let source_map_visible_rect = text_view_rect(source_map.visible_rect());
    if editor_visible_rect.height <= 0 || source_map_visible_rect.height <= 0 {
        return None;
    }

    let buffer = source_map.buffer();
    let end_iter = buffer.end_iter();
    let source_map_end = source_map.iter_location(&end_iter);
    let editor_end = editor.source_view().iter_location(&end_iter);
    let source_map_document_height = document_height_from_line_span_of(source_map_end)?;
    let editor_document_height = document_height_from_line_span_of(editor_end)?;
    let border = source_map_border(source_map);
    // Bound once so it is visible that the estimate input and the reported
    // diagnostic describe the same border, which is the point of reporting it.
    let border_left = i32::from(border.left());
    let border_right = i32::from(border.right());

    let native_slider_estimate = native_slider_estimate_from_inputs(NativeSliderEstimateInput {
        map_x: source_map_bounds.x,
        map_y: source_map_bounds.y,
        map_width: source_map_bounds.width,
        editor_visible_y: editor_visible_rect.y,
        editor_visible_height: editor_visible_rect.height,
        editor_document_height,
        source_map_visible_y: source_map_visible_rect.y,
        source_map_document_height,
        border_left,
        border_right,
    })?;
    let native_slider_visible_bounds =
        fit_native_slider_to_source_map_bounds(native_slider_estimate, source_map_bounds)?;

    Some(MinimapNativeSliderDiagnostics {
        projection_source: MinimapNativeProjectionSource::UpstreamVisibleRectEstimate,
        source_map_bounds,
        editor_visible_rect,
        source_map_visible_rect,
        editor_document_height,
        source_map_document_height,
        border_left,
        border_right,
        source_view_vadjustment: editor
            .source_view()
            .vadjustment()
            .as_ref()
            .map(adjustment_diagnostics),
        source_map_vadjustment: source_map
            .vadjustment()
            .as_ref()
            .map(adjustment_diagnostics),
        native_slider_estimate,
        native_slider_visible_bounds,
        line_projection: minimap_viewport_bounds(editor, source_map, target, target_height),
        first_content_row: minimap_first_content_row_bounds(source_map, target, target_height),
    })
}

#[expect(
    deprecated,
    reason = "GtkSourceMap's native slider still reads CSS border from StyleContext; diagnostics mirror that upstream input"
)]
fn source_map_border(source_map: &sourceview5::Map) -> gtk4::Border {
    source_map.style_context().border()
}

pub(super) fn source_map_bounds_relative_to(
    source_map: &sourceview5::Map,
    target: &gtk4::Widget,
) -> Option<MinimapProjectedBounds> {
    let map_bounds = source_map.compute_bounds(target)?;
    let x = f64::from(map_bounds.x());
    let y = f64::from(map_bounds.y());
    let width = f64::from(map_bounds.width());
    let height = f64::from(map_bounds.height());
    if !x.is_finite() || !y.is_finite() || !width.is_finite() || !height.is_finite() {
        return None;
    }
    (width > 0.0 && height > 0.0).then_some(MinimapProjectedBounds {
        x,
        y,
        width,
        height,
    })
}

/// Unwrap a GTK iter rectangle into the scalar line span `policy` decides from.
fn document_height_from_line_span_of(rect: gtk4::gdk::Rectangle) -> Option<i32> {
    document_height_from_line_span(rect.y(), rect.height())
}

fn text_view_rect(rect: gtk4::gdk::Rectangle) -> MinimapTextViewRect {
    MinimapTextViewRect {
        x: rect.x(),
        y: rect.y(),
        width: rect.width(),
        height: rect.height(),
    }
}

/// Convert a source-map buffer y-position into source-map widget coordinates.
///
/// `policy`'s `line_top_in_target` / `line_bottom_in_target` take this conversion
/// as a closure so they stay GTK-free, and every projection below supplies the
/// same one. Keeping it named once leaves those functions reading as the geometry
/// algebra they are.
fn source_map_widget_y(source_map: &sourceview5::Map, buffer_y: i32) -> i32 {
    let (_, widget_y) =
        source_map.buffer_to_window_coords(gtk4::TextWindowType::Widget, 0, buffer_y);
    widget_y
}

fn adjustment_diagnostics(adjustment: &gtk4::Adjustment) -> MinimapAdjustmentDiagnostics {
    let value = adjustment.value();
    let lower = adjustment.lower();
    MinimapAdjustmentDiagnostics {
        at_lower: (value - lower).abs() <= 0.5,
        value_milli: gtk_f64_to_milli(value),
        lower_milli: gtk_f64_to_milli(lower),
        upper_milli: gtk_f64_to_milli(adjustment.upper()),
        page_size_milli: gtk_f64_to_milli(adjustment.page_size()),
    }
}

/// Gather the draw inputs of the bins: the map scroll, its layout height,
/// the strip height and the rendered-content band rounded outward to whole
/// rows (`bin-minimap-markers-per-row` D3).
fn marker_bin_geometry(
    source_map: &sourceview5::Map,
    marker_strip: &gtk4::DrawingArea,
) -> Option<MarkerBinGeometry> {
    let strip_height = marker_strip.height();
    let space = minimap_projection_space(source_map, marker_strip.upcast_ref(), strip_height)?;
    let band = MarkerRowBand::clamped(
        whole_pixel(space.content_top.floor()),
        whole_pixel(space.content_bottom.ceil()),
        strip_height,
    );
    let adjustment = source_map.vadjustment();
    Some(MarkerBinGeometry {
        map_value_milli: adjustment
            .as_ref()
            .map_or(0, |adj| gtk_f64_to_milli(adj.value())),
        map_upper_milli: adjustment
            .as_ref()
            .map_or(0, |adj| gtk_f64_to_milli(adj.upper())),
        strip_height,
        map_y: whole_pixel(space.map_y.round()),
        band,
    })
}

/// Recompute every kind's runs for `geometry`.
///
/// While a search scan runs, the previous search runs are kept only if they
/// came from the same query and the same geometry (an edit's rescan, ledger
/// A100); otherwise the search lane waits for the scan, because a `forward`
/// outside a completed scan scans synchronously.
fn compute_marker_bins(
    editor: &LushtextEditorPage,
    source_map: &sourceview5::Map,
    geometry: MarkerBinGeometry,
    (marker_generation, search_generation, search_complete): (u64, u64, bool),
    _previous: Option<&MinimapMarkerBins>,
) -> MinimapMarkerBins {
    let groups = row_groups(source_map, geometry);
    let mut runs: [Vec<MarkerRowRun>; 4] = Default::default();
    let mut queries = [0; 4];
    let mut search_waiting = false;
    for kind in DRAW_ORDER {
        let index = kind_index(kind);
        let marked = match kind {
            MinimapMarkerKind::Bookmark => {
                let mut lines: Vec<u32> = editor
                    .bookmark_records()
                    .into_iter()
                    .map(|bookmark| bookmark.line)
                    .collect();
                lines.sort_unstable();
                Some(mark_row_groups(&groups, |line| {
                    first_sorted_line_at_or_after(&lines, line)
                }))
            }
            MinimapMarkerKind::Search => {
                if search_complete {
                    editor.search_bar().search_context().map(|context| {
                        mark_row_groups(&groups, |line| first_search_match_line(&context, line))
                    })
                } else {
                    search_waiting = editor.search_scan_pending();
                    if search_waiting {
                        interim_search_marks(editor, &groups)
                    } else {
                        None
                    }
                }
            }
            MinimapMarkerKind::Modified => editor
                .imp()
                .minimap
                .modified
                .current
                .borrow()
                .clone()
                .map(|tag| {
                    mark_row_groups(&groups, |line| {
                        first_tagged_line(&editor.buffer(), &tag, line)
                    })
                }),
            MinimapMarkerKind::LongLine => {
                let imp = editor.imp();
                if imp
                    .settings
                    .boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE)
                {
                    let cache = imp.minimap.analysis_cache.borrow();
                    cache
                        .as_ref()
                        .filter(|cache| {
                            cache.generation == imp.minimap.analysis_generation.get()
                                && cache.markers_collected
                        })
                        .map(|cache| {
                            mark_row_groups(&groups, |line| {
                                first_sorted_line_at_or_after(&cache.result.long_line_lines, line)
                            })
                        })
                } else {
                    None
                }
            }
        };
        if let Some((marked, asked)) = marked {
            queries[index] = asked;
            runs[index] =
                marker_row_runs(&groups, &marked, geometry.band, MINIMAP_MARKER_MIN_HEIGHT);
        }
    }
    MinimapMarkerBins {
        geometry,
        marker_generation,
        search_generation,
        search_complete,
        runs,
        queries,
        groups: groups.len(),
        rows: geometry.band.rows(),
        search_waiting,
    }
}

/// Search marks for the rows the map shows while the search context's own
/// scan still runs (design D5's progressive fallback).
///
/// The scan of a large document runs behind the editor's and the map's
/// layout validation, so it can end seconds after the query, and a `forward`
/// on the live context before then scans synchronously (A100: 70-80 ms on a
/// sparse document). `main`'s capped walk paid that dispatch to show the top
/// matches after its 80 ms debounce. Instead, the text of the lines the band
/// shows, at most `MINIMAP_INTERIM_SEARCH_CHARS` characters, is copied into a
/// small private buffer searched with the same query and options, where a
/// synchronous `forward` scans only that text. The marks are an interim hint:
/// once the live scan completes the bins come from it. The copy carries no
/// tags, so text hidden in the document (folds) can match here until then.
///
/// `forward_async` on the live context was tried first: it never blocks a
/// dispatch (A102), but GtkSourceView 5.20 drops a pending request without
/// returning when the query changes, which GIO reports as a CRITICAL (A102),
/// so it cannot serve a request a keystroke may supersede.
fn interim_search_marks(
    editor: &LushtextEditorPage,
    groups: &[MarkerRowGroup],
) -> Option<(Vec<bool>, usize)> {
    let context = editor.search_bar().search_context()?;
    let settings = context.settings();
    let query = settings.search_text()?;
    let (first, last) = (groups.first()?, groups.last()?);
    let buffer = editor.buffer();
    let start = text_iter_at_line_or_last(buffer.upcast_ref(), first.line_start);
    let end = i32::try_from(last.line_end)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .filter(|end| end.line() >= start.line())
        .unwrap_or_else(|| buffer.end_iter());
    let chars = usize::try_from(end.offset().saturating_sub(start.offset())).unwrap_or(usize::MAX);
    if chars > MINIMAP_INTERIM_SEARCH_CHARS {
        return None;
    }
    let copy = sourceview5::Buffer::new(None);
    copy.set_highlight_matching_brackets(false);
    copy.set_text(&buffer.text(&start, &end, true));
    let copy_settings = sourceview5::SearchSettings::new();
    copy_settings.set_search_text(Some(&query));
    copy_settings.set_case_sensitive(settings.is_case_sensitive());
    copy_settings.set_at_word_boundaries(settings.is_at_word_boundaries());
    copy_settings.set_regex_enabled(settings.is_regex_enabled());
    copy_settings.set_wrap_around(false);
    let copy_context = sourceview5::SearchContext::new(&copy, Some(&copy_settings));
    copy_context.set_highlight(false);
    let offset = first.line_start;
    Some(mark_row_groups(groups, |line| {
        first_search_match_line(&copy_context, line.saturating_sub(offset))
            .map(|found| found.saturating_add(offset))
    }))
}

/// Read the line `line_at_y` shows at each band row and group the rows.
///
/// A line taller than one row answers for every row it covers, so the table
/// costs one `line_at_y` per line shown, not per row (ledger A99).
fn row_groups(source_map: &sourceview5::Map, geometry: MarkerBinGeometry) -> Vec<MarkerRowGroup> {
    let band = geometry.band;
    if band.rows() == 0 {
        return Vec::new();
    }
    // Strip row `r` is map widget row `r - map_y`; the conversion to buffer
    // coordinates is a fixed offset, read once.
    let map_y = geometry.map_y;
    let (_, buffer_origin) = source_map.window_to_buffer_coords(gtk4::TextWindowType::Widget, 0, 0);
    let buffer_y = |row: i32| buffer_origin.saturating_add(row.saturating_sub(map_y));
    let mut lines = Vec::with_capacity(band.rows());
    let mut row = band.lower;
    while row < band.upper {
        let (iter, line_top) = source_map.line_at_y(buffer_y(row));
        let line = u32::try_from(iter.line()).unwrap_or(0);
        let (_, height) = source_map.line_yrange(&iter);
        let covered_until = line_top.saturating_add(height.max(1));
        loop {
            lines.push(line);
            row += 1;
            if row >= band.upper || buffer_y(row) >= covered_until {
                break;
            }
        }
    }
    let (past, _) = source_map.line_at_y(buffer_y(band.upper));
    let line_past_band = u32::try_from(past.line()).unwrap_or(0);
    marker_row_groups(band, &lines, line_past_band)
}

/// The first search match line at or after `line`, from a completed scan
/// (ledger A100: microseconds there, a synchronous scan outside one).
fn first_search_match_line(context: &sourceview5::SearchContext, line: u32) -> Option<u32> {
    let buffer = context.buffer();
    let from = text_iter_at_line_or_last(buffer.upcast_ref(), line);
    if u32::try_from(from.line()).unwrap_or(0) < line {
        return None;
    }
    let (match_start, _, wrapped) = context.forward(&from)?;
    if wrapped {
        return None;
    }
    u32::try_from(match_start.line()).ok()
}

/// The first line at or after `line` that carries `tag` (ledger A98).
fn first_tagged_line(buffer: &sourceview5::Buffer, tag: &gtk4::TextTag, line: u32) -> Option<u32> {
    let mut iter = text_iter_at_line_or_last(buffer.upcast_ref(), line);
    if u32::try_from(iter.line()).unwrap_or(0) < line {
        return None;
    }
    if iter.has_tag(tag) {
        return u32::try_from(iter.line()).ok();
    }
    // From an untagged position the next toggle turns the tag on.
    if !iter.forward_to_tag_toggle(Some(tag)) || !iter.has_tag(tag) {
        return None;
    }
    u32::try_from(iter.line()).ok()
}

/// A finite GTK coordinate as a whole `i32` pixel, saturating.
fn whole_pixel(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a widget coordinate already floored, ceiled or rounded; `as` saturates at the i32 range"
    )]
    {
        value as i32
    }
}

pub(super) fn draw_marker_strip(
    editor: &LushtextEditorPage,
    _marker_strip: &gtk4::DrawingArea,
    cr: &cairo::Context,
    width: i32,
    _height: i32,
) {
    let width = width.max(1);
    let dark = libadwaita::StyleManager::default().is_dark();
    let _ = editor.with_marker_bins(|bins| {
        for kind in DRAW_ORDER {
            let lane_width = marker_lane_width(kind, width);
            let x = marker_lane_x(width, lane_width);
            let (red, green, blue, alpha) = marker_rgba(kind, dark);
            cr.set_source_rgba(red, green, blue, alpha);
            // Whole-pixel runs and lanes become `f64` only at this cairo call,
            // so no edge is anti-aliased.
            for run in bins.runs(kind) {
                cr.rectangle(
                    f64::from(x),
                    f64::from(run.top),
                    f64::from(lane_width),
                    f64::from(run.bottom - run.top),
                );
            }
            let _ = cr.fill();
        }
    });
}

/// Project the editor viewport's visible line range through `GtkSourceMap`.
///
/// This is the older line-projection estimate kept as diagnostic contrast for
/// markers and tests. It is not the native rendered slider oracle; the native
/// estimate above mirrors GTK's visible-rect math, and screenshots decide the
/// actual rendered effect.
fn minimap_viewport_bounds(
    editor: &LushtextEditorPage,
    source_map: &sourceview5::Map,
    target: &gtk4::Widget,
    target_height: i32,
) -> Option<MinimapProjectedBounds> {
    let space = minimap_projection_space(source_map, target, target_height)?;

    let (visible_start, visible_end) = visible_editor_line_iters(editor)?;
    let (visible_start_y, _) = source_map.line_yrange(&visible_start);
    let raw_top = line_top_in_target(space.map_y, visible_start_y, |buffer_y| {
        source_map_widget_y(source_map, buffer_y)
    });
    let (visible_end_y, visible_end_height) = source_map.line_yrange(&visible_end);
    let raw_bottom =
        line_bottom_in_target(space.map_y, visible_end_y, visible_end_height, |buffer_y| {
            source_map_widget_y(source_map, buffer_y)
        });

    // The native viewport slider should stay visible at document edges, so
    // clamp off-content projections back onto the rendered minimap content.
    fit_projected_bounds(
        space.map_x - f64::from(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET),
        space.map_width + (f64::from(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET) * 2.0),
        raw_top,
        raw_bottom,
        space,
        MINIMAP_VIEWPORT_MIN_HEIGHT,
        ProjectedBoundsFit::ClampOutside,
    )
}

/// Convert the editor viewport's visible y-range into text iters before map projection.
fn visible_editor_line_iters(
    editor: &LushtextEditorPage,
) -> Option<(gtk4::TextIter, gtk4::TextIter)> {
    let source_view = editor.source_view();
    let visible_rect = source_view.visible_rect();
    if visible_rect.height() <= 0 {
        return None;
    }

    let top_y = visible_rect.y().max(0);
    // Use the last visible pixel, not the first pixel below the viewport, so an
    // exact bottom alignment does not project the following line.
    let bottom_y = top_y.saturating_add(visible_rect.height().max(1).saturating_sub(1));

    // `visible_rect` is expressed in the editor view's buffer coordinates, not
    // in the source map's scaled layout. Convert those editor y-positions to
    // visible text lines first, then project the same lines through `GtkSourceMap`.
    let (start_iter, _) = source_view.line_at_y(top_y);
    let (end_iter, _) = source_view.line_at_y(bottom_y);

    Some((start_iter, end_iter))
}

/// Project the first rendered minimap text row for screenshot pixel anchors.
fn minimap_first_content_row_bounds(
    source_map: &sourceview5::Map,
    target: &gtk4::Widget,
    target_height: i32,
) -> Option<MinimapProjectedBounds> {
    let space = minimap_projection_space(source_map, target, target_height)?;

    let buffer = source_map.buffer();
    let start_iter = buffer.start_iter();
    let (start_y, start_height) = source_map.line_yrange(&start_iter);
    let raw_top = line_top_in_target(space.map_y, start_y, |buffer_y| {
        source_map_widget_y(source_map, buffer_y)
    });
    let raw_bottom = line_bottom_in_target(space.map_y, start_y, start_height, |buffer_y| {
        source_map_widget_y(source_map, buffer_y)
    });

    // Content-row anchors prove a real rendered row exists; reject outside
    // geometry instead of clamping so tests cannot pass on a synthetic edge.
    fit_projected_bounds(
        space.map_x,
        space.map_width,
        raw_top,
        raw_bottom,
        space,
        1.0,
        ProjectedBoundsFit::RejectOutside,
    )
}

/// Build target-relative minimap geometry shared by markers and pixel anchors.
///
/// Unmapped widgets and empty allocations are rejected because GTK cannot
/// provide stable coordinates for them. `content_top` and `content_bottom`
/// intentionally come from source-map line geometry so marker and viewport
/// projections cannot drift into blank EOF overscroll.
fn minimap_projection_space(
    source_map: &sourceview5::Map,
    target: &gtk4::Widget,
    target_height: i32,
) -> Option<MinimapProjectionSpace> {
    if target_height <= 0 || !source_map.is_mapped() || !target.is_mapped() {
        return None;
    }

    let map_bounds = source_map.compute_bounds(target)?;
    let map_x = f64::from(map_bounds.x());
    let map_y = f64::from(map_bounds.y());
    let map_width = f64::from(map_bounds.width().max(1.0));
    let buffer = source_map.buffer();
    let start_iter = buffer.start_iter();
    let end_line = u32::try_from(buffer.end_iter().line()).unwrap_or_default();
    let end_iter = text_iter_at_line_or_last(&buffer, end_line);
    let (start_y, _) = source_map.line_yrange(&start_iter);
    let content_top = line_top_in_target(map_y, start_y, |buffer_y| {
        source_map_widget_y(source_map, buffer_y)
    });
    let (end_y, end_height) = source_map.line_yrange(&end_iter);
    let content_bottom = line_bottom_in_target(map_y, end_y, end_height, |buffer_y| {
        source_map_widget_y(source_map, buffer_y)
    });

    if !map_x.is_finite()
        || !map_y.is_finite()
        || !map_width.is_finite()
        || !content_top.is_finite()
        || !content_bottom.is_finite()
        || content_bottom <= content_top
    {
        return None;
    }

    Some(MinimapProjectionSpace {
        target_height: f64::from(target_height),
        map_x,
        map_y,
        map_width,
        content_top,
        content_bottom,
    })
}

/// Iter at `line`, clamped to the last line's **start** when `line` is past the end.
fn text_iter_at_line_or_last(buffer: &gtk4::TextBuffer, line: u32) -> gtk4::TextIter {
    let end_iter = buffer.end_iter();
    let end_line = u32::try_from(end_iter.line()).unwrap_or_default();
    let clamped_line = line.min(end_line);

    i32::try_from(clamped_line)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .unwrap_or(end_iter)
}

/// Serialize a GTK adjustment value as integer milli-units for Automation1
/// diagnostics; a non-finite value serializes as 0. A boundary conversion, so
/// it lives in this adapter rather than in the whole-pixel `policy.rs`.
pub(super) fn gtk_f64_to_milli(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "GTK adjustment values are bounded logical coordinates serialized as coarse diagnostics"
    )]
    {
        (value * 1000.0).round() as i64
    }
}

#[cfg(test)]
mod milli_tests {
    use super::gtk_f64_to_milli;

    #[test]
    fn test_gtk_f64_to_milli_serializes_finite_values_and_suppresses_nonfinite() {
        assert_eq!(gtk_f64_to_milli(10.49), 10_490);
        assert_eq!(gtk_f64_to_milli(12.25), 12_250);
        assert_eq!(gtk_f64_to_milli(-1.25), -1_250);
        assert_eq!(gtk_f64_to_milli(f64::NAN), 0);
        assert_eq!(gtk_f64_to_milli(f64::INFINITY), 0);
    }
}

#[cfg(test)]
mod wide_editor_ratio_tests {
    use proptest::prelude::*;

    use super::super::policy::source_map_exceeds_wide_editor_ratio;

    /// The `f64` form the exact comparison replaced (the ratio of the two
    /// heights against `0.20`), kept as the reference here because the policy
    /// module denies the float arithmetic it needs.
    fn legacy_exceeds(editor_document_height: i32, source_map_document_height: i32) -> bool {
        if editor_document_height <= 0 || source_map_document_height <= 0 {
            return false;
        }
        let ratio = f64::from(source_map_document_height) / f64::from(editor_document_height);
        ratio.is_finite() && ratio > 0.20
    }

    #[test]
    fn exact_comparison_matches_the_f64_ratio_densely() {
        for editor in -2..=3_000 {
            for source_map in -2..=1_000 {
                assert_eq!(
                    source_map_exceeds_wide_editor_ratio(editor, source_map),
                    legacy_exceeds(editor, source_map),
                    "editor {editor}, source map {source_map}"
                );
            }
        }
        // Every exact one-fifth pair and its neighbours, across the i32 range.
        for source_map in (1..=i32::MAX / 5).step_by(4_093) {
            let editor = source_map * 5;
            for (e, m) in [
                (editor, source_map),
                (editor - 1, source_map),
                (editor + 1, source_map),
            ] {
                assert_eq!(
                    source_map_exceeds_wide_editor_ratio(e, m),
                    legacy_exceeds(e, m),
                    "editor {e}, source map {m}"
                );
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20_000))]

        #[test]
        fn exact_comparison_matches_the_f64_ratio_everywhere(
            editor in any::<i32>(),
            source_map in any::<i32>(),
        ) {
            prop_assert_eq!(
                source_map_exceeds_wide_editor_ratio(editor, source_map),
                legacy_exceeds(editor, source_map)
            );
        }
    }
}
