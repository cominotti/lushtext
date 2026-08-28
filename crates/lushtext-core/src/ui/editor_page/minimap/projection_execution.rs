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

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{self, cairo};
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::admission::{
    cheap_minimap_availability, current_availability, minimap_analysis_request,
};
use super::policy::{
    MINIMAP_MARKER_MIN_HEIGHT, MINIMAP_MODIFIED_LINE_MARK_CAP, MINIMAP_REFRESH_DEBOUNCE,
    MINIMAP_SEARCH_MATCH_CAP, MINIMAP_TOP_CONTENT_MARGIN, MINIMAP_VIEWPORT_HORIZONTAL_OUTSET,
    MINIMAP_VIEWPORT_MIN_HEIGHT, MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS, MarkerProjectionSpace,
    MinimapAdjustmentDiagnostics, MinimapAvailability, MinimapMarker, MinimapMarkerBounds,
    MinimapMarkerKind, MinimapNativeProjectionSource, MinimapNativeSliderDiagnostics,
    MinimapProjectedBounds, MinimapProjectionSpace, MinimapTextViewRect, NativeSliderEstimateInput,
    ProjectedBoundsFit, document_height_from_line_span, fit_marker_bounds,
    fit_native_slider_to_source_map_bounds, fit_projected_bounds, gtk_f64_to_milli,
    line_bottom_in_target, line_top_in_target, marker_lane_width, marker_lane_x, marker_rgba,
    markers_from_lines, modified_line_mark_samples, native_slider_estimate_from_inputs,
    source_map_editor_height_ratio_from_heights, wide_editor_slider_offset_class,
};
use crate::config::keys;
use crate::ui::status_bar::MessageKind;
use crate::ui::window::LushtextWindow;

/// Hidden source-mark category used to keep modified lines attached to buffer edits.
pub(super) const MINIMAP_MODIFIED_MARK_CATEGORY: &str = "lushtext-minimap-modified";

impl LushtextEditorPage {
    /// Count the currently rendered markers for one semantic category.
    ///
    /// This exists mainly so widget tests can assert that bookmark, search,
    /// modified, and long-line markers appear and disappear as expected.
    #[must_use]
    pub(super) fn projected_marker_count(&self, kind: MinimapMarkerKind) -> usize {
        self.imp()
            .minimap
            .markers
            .borrow()
            .iter()
            .filter(|marker| marker.kind == kind)
            .count()
    }

    /// Return currently drawable marker bounds for one semantic category.
    ///
    /// The bounds are projected through the real `GtkSourceMap` layout, so
    /// widget tests can assert the marker strip follows source-map geometry
    /// instead of a hand-rolled line-count ratio.
    #[must_use]
    pub(super) fn projected_marker_bounds_for_kind(
        &self,
        kind: MinimapMarkerKind,
    ) -> Vec<MinimapMarkerBounds> {
        let imp = self.imp();
        let Some(source_map) = imp.minimap.source_map.borrow().as_ref().cloned() else {
            return Vec::new();
        };
        let Some(marker_strip) = imp.minimap.marker_strip.borrow().as_ref().cloned() else {
            return Vec::new();
        };

        projected_minimap_marker_bounds(self, &source_map, &marker_strip, marker_strip.height())
            .into_iter()
            .filter(|bounds| bounds.kind == kind)
            .collect()
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
        if availability != MinimapAvailability::TooLarge {
            self.imp().minimap.too_large_feedback_shown.set(false);
        }

        // Visibility and native projection are one lifecycle decision. A
        // hidden GtkSourceMap that remains bound still performs text layout,
        // so unavailable and load-suspended states must detach its view.
        let projection_visible =
            availability == MinimapAvailability::Visible && !self.load_projection_suspended();
        if let Some(source_map) = self.imp().minimap.source_map.borrow().as_ref() {
            if projection_visible && source_map.view().is_none() {
                source_map.set_view(self.source_view());
            } else if !projection_visible && source_map.view().is_some() {
                source_map.set_property("view", Option::<sourceview5::View>::None);
            }
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
            self.imp().minimap.markers.borrow_mut().clear();
            self.queue_marker_strip_draw();
            if availability != MinimapAvailability::Visible {
                self.publish_minimap_unavailable_feedback_if_needed(availability);
            }
            return;
        }

        self.sync_projection_geometry();
        *self.imp().minimap.markers.borrow_mut() = collect_markers(self);
        self.queue_marker_strip_draw();
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

    /// Mark the inclusive line range as modified-since-save.
    pub(super) fn record_modified_line_marks(&self, start_line: u32, end_line: u32) {
        let mut known_lines = self.imp().minimap.modified_lines_cache.borrow_mut();
        let remaining_capacity = MINIMAP_MODIFIED_LINE_MARK_CAP.saturating_sub(known_lines.len());
        if remaining_capacity == 0 {
            return;
        }
        let mut marks = self.imp().minimap.modified_marks.borrow_mut();
        let buffer = self.buffer();

        for line in modified_line_mark_samples(start_line, end_line, remaining_capacity) {
            if !known_lines.insert(line) {
                continue;
            }
            let iter = iter_at_line_or_last(&buffer, line);
            let mark = buffer.create_source_mark(None, MINIMAP_MODIFIED_MARK_CATEGORY, &iter);
            marks.push(mark);
        }
    }

    /// Mark every current line as modified-since-save.
    ///
    /// Restored drafts are already different from the saved file content, so
    /// this helper lets that programmatic restore surface as minimap feedback.
    /// Large drafts are sampled rather than projected as one `GtkSourceMark`
    /// per line, keeping the semantic marker layer bounded.
    pub(super) fn mark_all_lines_modified(&self) {
        self.release_modified_line_marks();
        let total_lines = document_line_count(self);
        if total_lines == 0 {
            return;
        }
        self.record_modified_line_marks(0, total_lines.saturating_sub(1));
        self.arm_minimap_refresh();
    }

    /// Publish the one-shot "too large" message when this page is the active tab.
    fn publish_minimap_unavailable_feedback_if_needed(&self, availability: MinimapAvailability) {
        if availability != MinimapAvailability::TooLarge
            || self.imp().minimap.too_large_feedback_shown.get()
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
            "Minimap unavailable for this large document",
            MessageKind::Warning,
        );
        self.imp().minimap.too_large_feedback_shown.set(true);
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
    if wide_editor_slider_offset_class(source_map_editor_height_ratio(source_map, source_view))
        .is_some()
    {
        source_map.add_css_class(MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS);
    } else {
        source_map.remove_css_class(MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS);
    }
}

/// Read live GTK document heights and project them into the tested ratio helper.
fn source_map_editor_height_ratio(
    source_map: &sourceview5::Map,
    source_view: &sourceview5::View,
) -> Option<f64> {
    let buffer = source_map.buffer();
    let end_iter = buffer.end_iter();
    let editor_document_height =
        document_height_from_line_span_of(source_view.iter_location(&end_iter))?;
    let source_map_document_height =
        document_height_from_line_span_of(source_map.iter_location(&end_iter))?;
    source_map_editor_height_ratio_from_heights(editor_document_height, source_map_document_height)
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

/// Total lines in the buffer, always at least 1.
///
/// A `GtkTextBuffer` always has one line, so adding 1 to the last line index
/// cannot yield 0. `mark_all_lines_modified` keeps its zero guard as defensive
/// depth; a `.max(1)` here would instead claim the value could be 0 while the
/// arithmetic above already guarantees it cannot.
fn document_line_count(editor: &LushtextEditorPage) -> u32 {
    u32::try_from(editor.buffer().end_iter().line())
        .unwrap_or(0)
        .saturating_add(1)
}

fn collect_markers(editor: &LushtextEditorPage) -> Vec<MinimapMarker> {
    let mut markers = Vec::new();
    markers.extend(markers_from_lines(
        MinimapMarkerKind::Bookmark,
        editor
            .bookmark_records()
            .into_iter()
            .map(|bookmark| bookmark.line),
    ));
    markers.extend(markers_from_lines(
        MinimapMarkerKind::Search,
        collect_search_match_lines(editor),
    ));
    markers.extend(markers_from_lines(
        MinimapMarkerKind::Modified,
        collect_modified_lines(editor),
    ));
    if editor
        .imp()
        .settings
        .boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE)
    {
        let long_line_lines = editor
            .imp()
            .minimap
            .analysis_cache
            .borrow()
            .as_ref()
            .filter(|cache| {
                cache.generation == editor.imp().minimap.analysis_generation.get()
                    && cache.markers_collected
            })
            .map_or_else(Vec::new, |cache| cache.result.long_line_lines.clone());
        markers.extend(markers_from_lines(
            MinimapMarkerKind::LongLine,
            long_line_lines,
        ));
    }
    markers
}

fn collect_search_match_lines(editor: &LushtextEditorPage) -> Vec<u32> {
    if !editor.is_search_visible() {
        return Vec::new();
    }

    let Some(context) = editor.search_bar().search_context() else {
        return Vec::new();
    };

    let buffer = context.buffer();
    let mut iter = buffer.start_iter();
    let mut lines = std::collections::BTreeSet::new();

    for _ in 0..MINIMAP_SEARCH_MATCH_CAP {
        let Some((match_start, match_end, wrapped)) = context.forward(&iter) else {
            break;
        };
        if wrapped {
            break;
        }
        if let Ok(line) = u32::try_from(match_start.line()) {
            lines.insert(line);
        }

        if match_end == iter {
            if !iter.forward_char() {
                break;
            }
        } else {
            iter = match_end;
        }
    }

    lines.into_iter().collect()
}

fn collect_modified_lines(editor: &LushtextEditorPage) -> Vec<u32> {
    // Bound outside the loop, matching `record_modified_line_marks`: `buffer()`
    // performs a checked downcast on every call.
    let buffer = editor.buffer();
    editor
        .imp()
        .minimap
        .modified_marks
        .borrow()
        .iter()
        .filter_map(|mark| u32::try_from(buffer.iter_at_mark(mark).line()).ok())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn draw_marker_strip(
    editor: &LushtextEditorPage,
    marker_strip: &gtk4::DrawingArea,
    cr: &cairo::Context,
    width: i32,
    height: i32,
) {
    let width = f64::from(width.max(1));
    let dark = libadwaita::StyleManager::default().is_dark();
    let Some(source_map) = editor.imp().minimap.source_map.borrow().as_ref().cloned() else {
        return;
    };
    let markers = projected_minimap_marker_bounds(editor, &source_map, marker_strip, height);

    for marker in &markers {
        let lane_width = marker_lane_width(marker.kind, width);
        let x = marker_lane_x(width, lane_width);
        let (red, green, blue, alpha) = marker_rgba(marker.kind, dark);
        cr.set_source_rgba(red, green, blue, alpha);
        cr.rectangle(x, marker.top, lane_width, marker.height());
        let _ = cr.fill();
    }
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

fn projected_minimap_marker_bounds(
    editor: &LushtextEditorPage,
    source_map: &sourceview5::Map,
    marker_strip: &gtk4::DrawingArea,
    strip_height: i32,
) -> Vec<MinimapMarkerBounds> {
    let Some(space) = marker_projection_space(source_map, marker_strip, strip_height) else {
        return Vec::new();
    };

    editor
        .imp()
        .minimap
        .markers
        .borrow()
        .iter()
        .filter_map(|marker| project_marker_bounds(marker, source_map, space))
        .collect()
}

fn marker_projection_space(
    source_map: &sourceview5::Map,
    marker_strip: &gtk4::DrawingArea,
    strip_height: i32,
) -> Option<MarkerProjectionSpace> {
    let target = minimap_projection_space(source_map, marker_strip.upcast_ref(), strip_height)?;
    Some(MarkerProjectionSpace {
        strip_height: target.target_height,
        content_top: target.content_top,
        content_bottom: target.content_bottom,
        map_y_in_strip: target.map_y,
        min_height: MINIMAP_MARKER_MIN_HEIGHT,
    })
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

fn project_marker_bounds(
    marker: &MinimapMarker,
    source_map: &sourceview5::Map,
    space: MarkerProjectionSpace,
) -> Option<MinimapMarkerBounds> {
    let buffer = source_map.buffer();
    let start_iter = text_iter_at_line_or_last(&buffer, marker.start_line);
    let end_iter = text_iter_at_line_or_last(&buffer, marker.end_line);
    let (start_y, _) = source_map.line_yrange(&start_iter);
    let raw_top = line_top_in_target(space.map_y_in_strip, start_y, |buffer_y| {
        source_map_widget_y(source_map, buffer_y)
    });
    let (end_y, end_height) = source_map.line_yrange(&end_iter);
    let raw_bottom = line_bottom_in_target(space.map_y_in_strip, end_y, end_height, |buffer_y| {
        source_map_widget_y(source_map, buffer_y)
    });

    fit_marker_bounds(marker.kind, raw_top, raw_bottom, space)
}

/// Iter at `line`, clamped to the last line's **start** when `line` is past the end.
///
/// Deliberately different from `iter_at_line_or_last` below, which falls back to
/// the buffer's end position instead. The two names differ only by a prefix while
/// they disagree at exactly the edge case both advertise, so each says which.
fn text_iter_at_line_or_last(buffer: &gtk4::TextBuffer, line: u32) -> gtk4::TextIter {
    let end_iter = buffer.end_iter();
    let end_line = u32::try_from(end_iter.line()).unwrap_or_default();
    let clamped_line = line.min(end_line);

    i32::try_from(clamped_line)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .unwrap_or(end_iter)
}

/// Iter at `line`, falling back to the buffer's **end** iter when `line` is past
/// the end — not to the last line's start; see `text_iter_at_line_or_last`.
fn iter_at_line_or_last(buffer: &sourceview5::Buffer, line: u32) -> gtk4::TextIter {
    i32::try_from(line)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .unwrap_or_else(|| buffer.end_iter())
}
