// SPDX-License-Identifier: GPL-3.0-or-later

//! Editor minimap workflow for one tab.
//!
//! This module stays in the GTK driving-adapter layer because it wires
//! `GtkSourceMap`, buffer signals, and scroll adjustments directly to the
//! editor widget tree. The logic is still kept in its own workflow file so
//! `mod.rs` and `imp.rs` do not become a mixed pile of unrelated editor
//! concerns.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{self, cairo};
use sourceview5::prelude::*;

use crate::config::keys;
use crate::ui::buffer_snapshot;
use crate::ui::status_bar::MessageKind;
use crate::ui::window::LushtextWindow;

use super::LushtextEditorPage;

/// Width reserved for the semantic marker strip painted over the map edge.
///
/// Eight pixels is enough to show four stacked marker lanes while still
/// leaving almost all of the overview map readable underneath.
const MINIMAP_MARKER_STRIP_WIDTH: i32 = 8;
/// Minimum top inset for the minimap text projection.
///
/// `GtkSourceMap` paints with a tiny font and sits inside a shell that owns its
/// own border/padding. Keeping a real text margin here prevents the first map
/// line from painting flush against a clipped top edge after width-only reflow.
const MINIMAP_TOP_CONTENT_MARGIN: i32 = 5;
/// Source-map/editor document-height ratio that identifies the wide editor state.
///
/// In that state the editor has stopped wrapping the fixture's long lines, so
/// GtkSourceMap's private slider rasterizes its top edge one row above the
/// sidebar-visible wrapped state unless we mirror GNOME Text Editor's fixed map
/// geometry and add a small slider-only correction. This is intentionally a
/// binary one-pixel correction: if future visual fixtures flap at this boundary,
/// use hysteresis or a stepped offset ladder rather than nudging the threshold.
const MINIMAP_WIDE_EDITOR_RATIO_THRESHOLD: f64 = 0.20;
/// CSS class for the wide-editor native slider top-edge correction.
const MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS: &str = "minimap-wide-editor-slider-offset";
/// Debounce for minimap marker refresh work.
///
/// This coalesces bursts from buffer edits, search updates, and resize-driven
/// adjustment changes so the main thread does not rescan the document on every
/// single notify signal.
const MINIMAP_REFRESH_DEBOUNCE: Duration = Duration::from_millis(80);
/// Line length that triggers a long-line warning marker in the minimap.
///
/// The proposal called out 120 characters explicitly, so the marker layer uses
/// the same threshold instead of trying to infer it from unrelated formatting settings.
const MINIMAP_LONG_LINE_WARNING_THRESHOLD: usize = 120;
/// Cap on search matches converted into minimap markers during one refresh.
///
/// The minimap only needs a spatial hint, not every exact hit, so we stop once
/// the marker strip is already dense enough to communicate "many matches here".
const MINIMAP_SEARCH_MATCH_CAP: usize = 2_000;
/// Maximum document size before wrapped minimap layout needs a long-line check.
///
/// Ordinary prose and source files stay below this budget, while multi-megabyte
/// minified files can make the narrow source map build a very large visual-line
/// cache when it mirrors editor word wrap.
const MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET: u64 = 2 * 1024 * 1024;
/// Long logical lines above this size make wrapped source-map layout expensive.
///
/// The threshold is high enough for normal code/log lines but catches minified
/// JSON or generated files before a 64-160px minimap explodes one line into
/// thousands of visual rows.
const MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET: usize = 8_000;
/// Minimum visual height for a semantic marker after projection.
///
/// Collapsed or sub-pixel source-map lines still need to be discoverable, but
/// this height is clamped to the rendered document content so it cannot leak
/// into the blank EOF overscroll tail.
const MINIMAP_MARKER_MIN_HEIGHT: f64 = 2.0;
/// Minimum height for the viewport highlight when the projected visible area is tiny.
///
/// Two logical pixels keeps edge anchors detectable on very short documents
/// without making the native slider look thicker than GTK's own effect.
const MINIMAP_VIEWPORT_MIN_HEIGHT: f64 = 2.0;
/// Horizontal CSS outset used by the native `GtkSourceMap` viewport slider.
///
/// This mirrors `.minimap-view slider { margin-left/right: -13px; }`. The same
/// value provides the shell's side gutters as map widget margins, so the
/// slider's outset paints inside the overlay content box and the reflow freeze
/// snapshot can cover the full rendered effect users actually see.
const MINIMAP_VIEWPORT_HORIZONTAL_OUTSET: i32 = 13;
/// Debounce that detects the end of a width-reflow burst.
///
/// Sidebar show/hide animates the editor width on every frame for roughly
/// 250ms. Wrapped document heights are asynchronous estimates while that
/// happens, so any minimap margin or scroll repair performed mid-burst reads
/// transient values and paints the native slider a few pixels off. The settle
/// delay must exceed the gap between animation frames (16-33ms) by a wide
/// margin while staying short enough that the post-reflow repair feels
/// immediate once the width stops changing.
const MINIMAP_REFLOW_SETTLE_DEBOUNCE: Duration = Duration::from_millis(150);
/// Delay before revealing the live map after a settled repair.
///
/// The margin/class writes queue GTK rendering work, and the native map's
/// private slider can take several quiet frames to repaint from the rested
/// document-height estimates after the sidebar stops consuming width. Keeping
/// the frozen native pixels over the live map during that quiet window prevents
/// a single stale native frame from leaking when the cover is removed. The
/// 800ms window is intentionally conservative; tune it down only with stream
/// frame proof because longer delays keep the minimap frozen for immediate
/// scrolls until the early-reveal path runs.
const MINIMAP_REFLOW_REVEAL_DELAY: Duration = Duration::from_millis(800);
/// Hidden source-mark category used to keep modified lines attached to buffer edits.
const MINIMAP_MODIFIED_MARK_CATEGORY: &str = "lushtext-minimap-modified";

/// Whether the minimap is currently usable for the active editor state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MinimapAvailability {
    /// The user preference currently disables the minimap everywhere.
    #[default]
    Disabled,
    /// The current document is above the minimap-supported size tier.
    TooLarge,
    /// The tab buffer was evicted and will be reloaded only when focused again.
    Evicted,
    /// The minimap is visible and fully active for this editor page.
    Visible,
}

/// Semantic marker categories painted in the minimap strip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinimapMarkerKind {
    /// Saved-file bookmarks projected from the editor's live bookmark marks.
    Bookmark,
    /// Matches for the active in-tab search session.
    Search,
    /// Lines changed since the last save or clean restore point.
    Modified,
    /// Lines that exceed the minimap long-line threshold.
    LongLine,
}

/// One normalized marker segment rendered in the minimap strip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinimapMarker {
    /// Semantic category used to pick color and lane width.
    pub kind: MinimapMarkerKind,
    /// Inclusive start line in the current buffer.
    pub start_line: u32,
    /// Inclusive end line in the current buffer.
    pub end_line: u32,
}

/// Current vertical bounds for a projected minimap marker segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinimapMarkerBounds {
    /// Semantic category used to pick color and lane width.
    pub kind: MinimapMarkerKind,
    /// Marker top in marker-strip widget coordinates.
    pub top: f64,
    /// Marker bottom in marker-strip widget coordinates.
    pub bottom: f64,
}

impl MinimapMarkerBounds {
    /// Height in marker-strip widget coordinates.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

/// Current projected bounds for a minimap visual rectangle.
///
/// Coordinates are relative to the caller-provided target widget and may
/// describe native-slider estimates, content-row anchors, or marker projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinimapProjectedBounds {
    /// Left edge in target widget coordinates.
    pub x: f64,
    /// Top edge in target widget coordinates.
    pub y: f64,
    /// Width in target widget coordinates.
    pub width: f64,
    /// Height in target widget coordinates.
    pub height: f64,
}

impl MinimapProjectedBounds {
    /// Bottom edge in target widget coordinates.
    #[must_use]
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

/// Source of the native viewport diagnostic estimate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MinimapNativeProjectionSource {
    /// Estimate derived from public `GtkTextView` geometry matching `GtkSourceMap`.
    UpstreamVisibleRectEstimate,
}

impl MinimapNativeProjectionSource {
    /// Stable serialized label for Automation1 diagnostics.
    #[must_use]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::UpstreamVisibleRectEstimate => "upstream-visible-rect-estimate",
        }
    }
}

/// Bounded text-view rectangle used by native minimap diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MinimapTextViewRect {
    /// Left coordinate in the text view's own coordinate space.
    pub x: i32,
    /// Top coordinate in the text view's own coordinate space.
    pub y: i32,
    /// Rectangle width in logical pixels.
    pub width: i32,
    /// Rectangle height in logical pixels.
    pub height: i32,
}

/// Bounded scroll adjustment summary for native minimap diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MinimapAdjustmentDiagnostics {
    /// Whether the adjustment is at its lower bound.
    pub at_lower: bool,
    /// Current adjustment value multiplied by 1000.
    pub value_milli: i64,
    /// Lower bound multiplied by 1000.
    pub lower_milli: i64,
    /// Upper bound multiplied by 1000.
    pub upper_milli: i64,
    /// Page size multiplied by 1000.
    pub page_size_milli: i64,
}

/// Native source-map slider diagnostics exposed through Automation1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MinimapNativeSliderDiagnostics {
    /// Stable source classification for the diagnostic estimate.
    pub projection_source: MinimapNativeProjectionSource,
    /// Source map allocation in the requested target coordinate space.
    pub source_map_bounds: MinimapProjectedBounds,
    /// Editor visible rect in editor buffer coordinates.
    pub editor_visible_rect: MinimapTextViewRect,
    /// Source-map visible rect in source-map buffer coordinates.
    pub source_map_visible_rect: MinimapTextViewRect,
    /// Editor document height used by the native slider ratio.
    pub editor_document_height: i32,
    /// Source-map document height used by the native slider ratio.
    pub source_map_document_height: i32,
    /// Horizontal CSS border on the source map.
    pub border_left: i32,
    /// Horizontal CSS border on the source map.
    pub border_right: i32,
    /// Source-view vertical adjustment at snapshot time.
    pub source_view_vadjustment: Option<MinimapAdjustmentDiagnostics>,
    /// Source-map vertical adjustment at snapshot time.
    pub source_map_vadjustment: Option<MinimapAdjustmentDiagnostics>,
    /// Upstream-informed estimate of the native slider rectangle.
    pub native_slider_estimate: MinimapProjectedBounds,
    /// Native slider rectangle vertically fitted to the visible source-map allocation.
    pub native_slider_visible_bounds: MinimapProjectedBounds,
    /// Older line-projection estimate retained as explanatory contrast.
    pub line_projection: Option<MinimapProjectedBounds>,
    /// First rendered minimap content row, when projectable.
    pub first_content_row: Option<MinimapProjectedBounds>,
}

impl LushtextEditorPage {
    /// Report the current minimap availability for this editor page.
    #[must_use]
    pub fn minimap_availability(&self) -> MinimapAvailability {
        self.imp().minimap.availability.get()
    }

    /// Whether the minimap is currently visible for this editor page.
    #[must_use]
    pub fn is_minimap_visible(&self) -> bool {
        self.minimap_availability() == MinimapAvailability::Visible
    }

    /// Main-thread readiness query for queued minimap work.
    ///
    /// This reads GTK/GSettings state only; it returns false for hidden or
    /// unavailable minimap refreshes so invisible source-map work does not block
    /// Automation1 idle or visual-geometry waits.
    pub(crate) fn minimap_refresh_blocks_readiness(&self) -> bool {
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
    pub(crate) fn minimap_work_pending(&self) -> bool {
        let minimap = &self.imp().minimap;
        minimap.refresh_pending.get()
            || minimap.reflow_settle_pending.get()
            || minimap.reflow_reveal_pending.get()
    }

    /// Count the currently rendered markers for one semantic category.
    ///
    /// This exists mainly so widget tests can assert that bookmark, search,
    /// modified, and long-line markers appear and disappear as expected.
    #[must_use]
    pub fn minimap_marker_count(&self, kind: MinimapMarkerKind) -> usize {
        self.imp()
            .minimap
            .markers
            .borrow()
            .iter()
            .filter(|marker| marker.kind == kind)
            .count()
    }

    /// Test seam for the expensive long-line marker scan.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn long_line_warning_count_for_test(&self) -> usize {
        collect_long_line_warnings(self).len()
    }

    /// Return currently drawable marker bounds for one semantic category.
    ///
    /// The bounds are projected through the real `GtkSourceMap` layout, so
    /// widget tests can assert the marker strip follows source-map geometry
    /// instead of a hand-rolled line-count ratio.
    #[must_use]
    pub fn minimap_marker_bounds(&self, kind: MinimapMarkerKind) -> Vec<MinimapMarkerBounds> {
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
    pub(crate) fn minimap_viewport_bounds_relative_to(
        &self,
        target: &gtk4::Widget,
    ) -> Option<MinimapProjectedBounds> {
        self.minimap_native_slider_diagnostics_relative_to(target)
            .map(|diagnostics| diagnostics.native_slider_visible_bounds)
    }

    /// Return the first rendered map content row relative to `target`.
    ///
    /// Main-thread only: this mirrors `GtkSourceMap` line geometry for
    /// diagnostics, returning `None` until GTK has a mapped, positive allocation
    /// or no rendered content row can be projected.
    pub(crate) fn minimap_first_content_row_relative_to(
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
    pub(crate) fn minimap_native_slider_diagnostics_relative_to(
        &self,
        target: &gtk4::Widget,
    ) -> Option<MinimapNativeSliderDiagnostics> {
        let source_map = self.imp().minimap.source_map.borrow().as_ref().cloned()?;
        minimap_native_slider_diagnostics(self, &source_map, target, target.height())
    }

    /// Test seam for the viewport-highlight projection used by pixel anchors.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn minimap_viewport_bounds_for_test(&self) -> Option<MinimapProjectedBounds> {
        let source_map = self.imp().minimap.source_map.borrow().as_ref().cloned()?;
        self.minimap_viewport_bounds_relative_to(source_map.upcast_ref())
    }

    /// Test seam for the first rendered minimap content row used by pixel anchors.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn minimap_first_content_row_bounds_for_test(&self) -> Option<MinimapProjectedBounds> {
        let source_map = self.imp().minimap.source_map.borrow().as_ref().cloned()?;
        self.minimap_first_content_row_relative_to(source_map.upcast_ref())
    }

    /// Test seam for readiness coverage that needs a pending minimap refresh.
    #[cfg(feature = "test-utils")]
    pub fn mark_minimap_refresh_pending_for_test(&self) {
        self.imp().minimap.refresh_pending.set(true);
    }

    /// Install the per-tab minimap widgets and signal glue.
    pub(crate) fn setup_minimap(&self) {
        let imp = self.imp();

        let source_map = self.create_native_source_map();

        let marker_strip = gtk4::DrawingArea::new();
        marker_strip.set_width_request(MINIMAP_MARKER_STRIP_WIDTH);
        marker_strip.set_halign(gtk4::Align::End);
        marker_strip.set_valign(gtk4::Align::Fill);
        marker_strip.set_vexpand(true);
        marker_strip.set_can_target(false);
        // Keep the strip over the map's right edge now that the shell gutter
        // is a map margin instead of CSS padding on the overlay.
        marker_strip.set_margin_end(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET);
        marker_strip.add_css_class("minimap-marker-strip");

        {
            let editor_weak = self.downgrade();
            marker_strip.set_draw_func(move |area, cr, width, height| {
                if let Some(editor) = editor_weak.upgrade() {
                    draw_marker_strip(&editor, area, cr, width, height);
                }
            });
        }

        imp.minimap_overlay.add_css_class("minimap-shell");
        imp.minimap_overlay.set_child(Some(&source_map));
        // Hidden freeze layer for width-reflow bursts. It sits between the map
        // and the marker strip so frozen pixels replace the native slider while
        // live semantic markers stay on top. Filling the overlay content box
        // covers the map plus both gutters where the slider's CSS outset
        // paints, and the capture viewport is sized from the same map margins
        // so `ContentFit::Fill` renders the held pixels exactly 1:1.
        let reflow_freeze_picture = gtk4::Picture::new();
        reflow_freeze_picture.set_visible(false);
        reflow_freeze_picture.set_can_target(false);
        reflow_freeze_picture.set_content_fit(gtk4::ContentFit::Fill);
        reflow_freeze_picture.set_halign(gtk4::Align::Fill);
        reflow_freeze_picture.set_valign(gtk4::Align::Fill);
        reflow_freeze_picture.add_css_class("minimap-reflow-freeze");
        imp.minimap_overlay.add_overlay(&reflow_freeze_picture);
        imp.minimap_overlay
            .set_measure_overlay(&reflow_freeze_picture, false);
        imp.minimap_overlay.add_overlay(&marker_strip);

        *imp.minimap.source_map.borrow_mut() = Some(source_map);
        *imp.minimap.reflow_freeze_picture.borrow_mut() = Some(reflow_freeze_picture);
        *imp.minimap.marker_strip.borrow_mut() = Some(marker_strip);
        self.apply_minimap_width_from_settings();

        let buffer = self.buffer();
        {
            let editor_weak = self.downgrade();
            // GtkSourceBuffer emits GObject signals for edits; store handler ids
            // so minimap observers can be disconnected when tab wiring is torn down.
            let handler_id = buffer.connect_insert_text(move |_, iter, text| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if editor.imp().minimap.tracking_suspended.get() {
                    return;
                }
                let start_line = u32::try_from(iter.line()).unwrap_or(0);
                let inserted_lines = u32::try_from(text.chars().filter(|ch| *ch == '\n').count())
                    .unwrap_or(u32::MAX);
                editor.record_modified_lines(start_line, start_line.saturating_add(inserted_lines));
                editor.schedule_minimap_refresh();
            });
            imp.minimap.insert_text_handler_id.replace(Some(handler_id));
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_delete_range(move |_, start, end| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if editor.imp().minimap.tracking_suspended.get() {
                    return;
                }
                let start_line = u32::try_from(start.line()).unwrap_or(0);
                let end_line = u32::try_from(end.line()).unwrap_or(start_line);
                editor.record_modified_lines(start_line, end_line);
                editor.schedule_minimap_refresh();
            });
            imp.minimap
                .delete_range_handler_id
                .replace(Some(handler_id));
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_modified_changed(move |buffer| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if !buffer.is_modified() {
                    editor.clear_modified_line_marks();
                }
                editor.schedule_minimap_refresh();
            });
            imp.minimap
                .modified_changed_handler_id
                .replace(Some(handler_id));
        }
        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_changed(move |_| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                editor.imp().minimap.wrapped_layout_too_large.set(None);
                if editor.imp().minimap.tracking_suspended.get() {
                    return;
                }
                editor.schedule_minimap_refresh();
            });
            imp.minimap.changed_handler_id.replace(Some(handler_id));
        }

        {
            let editor_weak = self.downgrade();
            self.search_bar().connect_search_state_changed(move || {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.schedule_minimap_refresh();
                }
            });
        }

        if let Some(vadj) = self.source_view().vadjustment() {
            let editor_weak = self.downgrade();
            vadj.connect_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.schedule_minimap_refresh();
                }
            });
        }

        self.schedule_minimap_refresh();
    }

    /// Build a native `GtkSourceMap` with LushText's stable minimap presentation.
    ///
    /// The map owns the visible viewport highlight. Keeping creation in one
    /// helper lets reflow recovery rebuild the same native widget configuration
    /// instead of restyling or drawing over the upstream effect.
    fn create_native_source_map(&self) -> sourceview5::Map {
        let source_map = sourceview5::Map::new();
        source_map.set_view(self.source_view());
        source_map.set_editable(false);
        source_map.set_cursor_visible(false);
        source_map.set_can_focus(false);
        source_map.set_wrap_mode(gtk4::WrapMode::None);
        source_map.set_show_line_numbers(false);
        source_map.set_show_line_marks(false);
        source_map.set_highlight_current_line(false);
        source_map.set_monospace(true);
        source_map.set_overflow(gtk4::Overflow::Visible);
        source_map.add_css_class("monospace");
        source_map.add_css_class("minimap-view");
        source_map.set_hexpand(true);
        source_map.set_vexpand(true);
        // The shell's side gutters are widget margins rather than CSS padding
        // so the reflow freeze picture can span the full shell content box
        // (map plus the native slider's CSS outset) without negative margins.
        source_map.set_margin_start(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET);
        source_map.set_margin_end(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET);
        sync_source_map_geometry(&source_map, self.source_view());
        source_map
    }

    /// Keep the source map's wrapping and text insets aligned with the editor.
    ///
    /// The minimap viewport is a visual promise about the editor, so width
    /// reflow from word wrap must be reflected in the map before its native
    /// viewport slider and our marker strip settle. The top margin is explicit
    /// because the minimap shell has its own border/padding and a flush first
    /// line is easy to clip by one pixel after adaptive shell reallocation.
    pub(crate) fn sync_minimap_view_geometry(&self) {
        if self.imp().minimap.reflow_settle_pending.get() {
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

    /// Compatibility helper for callers whose trigger is specifically wrap policy.
    pub(crate) fn sync_minimap_wrap_mode(&self) {
        self.sync_minimap_view_geometry();
    }

    /// Coalesce a passive width-reflow burst into one settled minimap repair.
    ///
    /// This main-thread GTK path updates the settle generation and readiness
    /// state, but it never captures a freeze because passive signals can arrive
    /// after the native map has already been invalidated.
    pub(crate) fn schedule_minimap_reflow_settle(&self) {
        self.schedule_minimap_reflow_settle_impl(false);
    }

    /// Coalesce a shell-triggered width transition and freeze the rendered map first.
    ///
    /// Shell actions call this on the GTK main thread before the split-view
    /// transition starts, so the captured cover still contains the exact
    /// previously rendered native minimap pixels.
    pub(crate) fn schedule_minimap_reflow_settle_with_freeze(&self) {
        self.schedule_minimap_reflow_settle_impl(true);
    }

    /// Coalesce a width-reflow burst into one settled minimap repair.
    ///
    /// `AdwOverlaySplitView` sidebar animation allocates a new editor width on
    /// every frame, and GtkTextView revalidates wrapped line heights
    /// asynchronously while that happens. Repair work scheduled per allocation
    /// always lands at least one frame late and reads mid-validation estimates.
    /// Action-owned shell transitions can freeze the rendered map before the
    /// first allocation frame; passive allocation observers only schedule the
    /// later repair so they never capture an unpainted or partially realized map.
    fn schedule_minimap_reflow_settle_impl(&self, freeze_rendered_map: bool) {
        let minimap = &self.imp().minimap;
        if !minimap.reflow_settle_pending.get() {
            minimap.reflow_settle_pending.set(true);
            minimap.reflow_reveal_pending.set(false);
        }
        if freeze_rendered_map {
            self.freeze_native_minimap_for_reflow();
        }

        let generation = minimap.reflow_settle_generation.get().wrapping_add(1);
        minimap.reflow_settle_generation.set(generation);
        // `_local` timers run on GTK's main loop. A weak editor reference avoids
        // keeping a closed tab alive, and the generation check discards stale
        // callbacks from superseded width bursts.
        let editor_weak = self.downgrade();
        glib::timeout_add_local_once(MINIMAP_REFLOW_SETTLE_DEBOUNCE, move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if editor.imp().minimap.reflow_settle_generation.get() != generation {
                return;
            }
            editor.finish_minimap_reflow_settle();
        });
    }

    /// Run the one-shot post-reflow repair after the editor width stops moving.
    ///
    /// The repair restores user scroll anchors, reapplies the fixed native-map
    /// geometry from settled document heights, and clears any stale source-map
    /// scroll. If a shell action captured a cover, the live map repaints under
    /// that cover before reveal; passive bursts have no cover and become ready
    /// as soon as the settled repair finishes.
    fn finish_minimap_reflow_settle(&self) {
        // Clear the pin first so the geometry sync below applies the settled margin.
        self.imp().minimap.reflow_settle_pending.set(false);

        // The rest flag was recorded from user scrolling outside the burst, so
        // a stale GTK-preserved offset during reallocation cannot suppress the
        // top anchor the user actually had before the reflow started.
        if self.imp().overscroll.v_rest_at_top.get()
            && let Some(adjustment) = self.source_view().vadjustment()
        {
            let lower = adjustment.lower();
            if (adjustment.value() - lower).abs() > 0.5 {
                adjustment.set_value(lower);
            }
        }

        self.sync_minimap_view_geometry();
        self.clamp_native_minimap_to_top_if_editor_at_top();
        self.schedule_minimap_refresh();
        self.queue_minimap_draw();
        if self.minimap_reflow_freeze_visible() {
            self.warm_live_minimap_under_reflow_freeze();
            self.imp().minimap.reflow_reveal_pending.set(true);
        } else {
            // Passive adjustment-driven bursts never captured a cover, so there
            // is no reveal window to protect after the settled repair runs.
            self.imp().minimap.reflow_reveal_pending.set(false);
            self.drop_minimap_reflow_freeze();
            return;
        }
        let generation = self.imp().minimap.reflow_settle_generation.get();
        let editor_weak = self.downgrade();
        glib::timeout_add_local_once(MINIMAP_REFLOW_REVEAL_DELAY, move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if editor.imp().minimap.reflow_settle_generation.get() != generation {
                return;
            }
            editor.imp().minimap.reflow_reveal_pending.set(false);
            editor.drop_minimap_reflow_freeze();
        });
    }

    /// Gate the post-repair reveal window on an actually visible frozen cover.
    ///
    /// Passive repairs skip capture; this helper keeps those repairs from
    /// blocking visual-readiness on a reveal delay that has nothing to reveal.
    fn minimap_reflow_freeze_visible(&self) -> bool {
        self.imp()
            .minimap
            .reflow_freeze_picture
            .borrow()
            .as_ref()
            .is_some_and(gtk4::prelude::WidgetExt::is_visible)
    }

    /// Cover the native map with its last rendered pixels while reflow settles.
    ///
    /// Shell actions arm the snapshot before the consuming width transition.
    /// Passive allocation-derived observers only schedule the settled repair,
    /// because by the time they fire GTK may already have invalidated or
    /// partially realized the native map. The capture spans the overlay content
    /// box, including the native slider's CSS outset, so the frozen picture
    /// contains the full rendered effect.
    fn freeze_native_minimap_for_reflow(&self) {
        if !self.is_minimap_visible() {
            return;
        }
        let Some(picture) = self
            .imp()
            .minimap
            .reflow_freeze_picture
            .borrow()
            .as_ref()
            .cloned()
        else {
            return;
        };
        let Some(source_map) = self.imp().minimap.source_map.borrow().as_ref().cloned() else {
            return;
        };
        // If a previous freeze is already hiding the live map, keep its original
        // pre-burst pixels; recapturing now would snapshot a transparent map.
        if picture.is_visible() && source_map.opacity() < 0.99 {
            return;
        }
        let minimap_overlay = &*self.imp().minimap_overlay;
        // The action path uses `snapshot_child()` outside the normal widget
        // snapshot vfunc, so require realized, drawable, non-empty geometry
        // before asking GTK for render nodes.
        if !source_map.is_mapped()
            || !minimap_overlay.is_mapped()
            || !source_map.is_drawable()
            || !minimap_overlay.is_drawable()
            || source_map.width() <= 0
            || source_map.height() <= 0
        {
            return;
        }
        let Some(source_map_bounds) = source_map.compute_bounds(minimap_overlay) else {
            return;
        };
        if source_map_bounds.width() <= 0.0 || source_map_bounds.height() <= 0.0 {
            return;
        }
        let width = minimap_overlay.width();
        let height = minimap_overlay.height();
        if width <= 0 || height <= 0 {
            return;
        }

        let Some(renderer) = source_map.native().and_then(|native| native.renderer()) else {
            return;
        };

        let snapshot = gtk4::Snapshot::new();
        // GTK documents `snapshot_child()` as the helper a widget normally uses
        // from its own `snapshot` vfunc. This action-primed capture is deliberate:
        // it preserves the exact native slider pixels already rendered before the
        // shell starts consuming width, while passive allocation observers only
        // schedule the later settle repair.
        minimap_overlay.snapshot_child(&source_map, &snapshot);
        #[expect(
            clippy::cast_precision_loss,
            reason = "GTK widget sizes are small logical-pixel values that fit f32 exactly"
        )]
        let viewport = gtk4::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
        let node = snapshot.to_node();

        let Some(node) = node else {
            return;
        };
        // `GtkPicture` needs a paintable, so render the captured snapshot node
        // into a texture whose viewport matches the overlay pixels users see.
        let texture = renderer.render_texture(&node, Some(&viewport));
        picture.set_paintable(Some(&texture));
        picture.set_visible(true);
        source_map.set_opacity(0.0);
        debug_assert!(picture.is_visible());
        debug_assert!(source_map.opacity() <= 0.01);
    }

    /// Let the real source map repaint while frozen pixels still cover it.
    ///
    /// `GtkSourceMap` can still paint its first visible frame from stale private
    /// slider state when it becomes visible again. Restoring opacity before the
    /// cover is removed gives GTK a few real frames to update the native map
    /// while the temporary picture keeps the user-visible effect unchanged.
    fn warm_live_minimap_under_reflow_freeze(&self) {
        let Some(picture) = self
            .imp()
            .minimap
            .reflow_freeze_picture
            .borrow()
            .as_ref()
            .cloned()
        else {
            return;
        };
        if !picture.is_visible() {
            return;
        }
        let Some(source_map) = self.imp().minimap.source_map.borrow().as_ref().cloned() else {
            return;
        };
        source_map.set_opacity(1.0);
        source_map.queue_draw();
        debug_assert!(source_map.opacity() >= 0.99);
    }

    /// Reveal the repaired live minimap early when the user scrolls during warmup.
    ///
    /// The initial settle window keeps the cover in place because the native map
    /// is still reading transient geometry. After opacity is restored under the
    /// cover, the live map is ready underneath it, so user-driven scroll
    /// should trade the conservative delay for immediate responsiveness.
    pub(crate) fn reveal_minimap_reflow_freeze_for_user_scroll(&self) {
        let minimap = &self.imp().minimap;
        if minimap.reflow_settle_pending.get() || !minimap.reflow_reveal_pending.get() {
            return;
        }

        minimap.reflow_reveal_pending.set(false);
        self.drop_minimap_reflow_freeze();
    }

    /// Keep the freeze active when the viewport height changes mid-burst.
    ///
    /// Width reflow can briefly perturb the vertical page size too. Revealing
    /// the live native map at that point leaks exactly the stale private slider
    /// frame this freeze is meant to hide, so the settled repair owns the final
    /// geometry and reveal timing.
    pub(crate) fn note_minimap_height_reflow(&self) {
        self.queue_minimap_draw();
    }

    /// Remove the frozen overlay and show the live native map again.
    ///
    /// Every exit path must restore source-map opacity before hiding and clearing
    /// the picture, or the next live minimap frame can remain invisible.
    fn drop_minimap_reflow_freeze(&self) {
        if let Some(source_map) = self.imp().minimap.source_map.borrow().as_ref() {
            source_map.set_opacity(1.0);
            debug_assert!(source_map.opacity() >= 0.99);
        }
        if let Some(picture) = self.imp().minimap.reflow_freeze_picture.borrow().as_ref() {
            picture.set_visible(false);
            picture.set_paintable(None::<&gtk4::gdk::Paintable>);
            debug_assert!(!picture.is_visible());
        }
    }

    /// Restore the bound source map's own vertical adjustment to the top edge.
    ///
    /// Upstream derives the native slider from the editor scroll position and
    /// then subtracts the source map's visible rect. If GTK preserves a stale
    /// source-map adjustment across width-only reflow, the outer allocation can
    /// be stable while the rendered slider and first map row drift by a few
    /// pixels. Returns whether a stale adjustment was actually cleared.
    fn clamp_native_minimap_to_top_if_editor_at_top(&self) -> bool {
        let Some(source_adjustment) = self.source_view().vadjustment() else {
            return false;
        };
        if (source_adjustment.value() - source_adjustment.lower()).abs() > 0.5 {
            return false;
        }

        let Some(source_map) = self.imp().minimap.source_map.borrow().as_ref().cloned() else {
            return false;
        };
        let Some(map_adjustment) = source_map.vadjustment() else {
            return false;
        };

        let lower = map_adjustment.lower();
        if (map_adjustment.value() - lower).abs() <= 0.5 {
            return false;
        }
        map_adjustment.set_value(lower);
        true
    }

    /// Refresh minimap visibility, markers, and any one-shot availability feedback.
    pub(crate) fn refresh_minimap(&self) {
        self.imp().minimap.refresh_pending.set(false);
        let availability = current_availability(self);
        self.imp().minimap.availability.set(availability);
        if availability != MinimapAvailability::TooLarge {
            self.imp().minimap.too_large_feedback_shown.set(false);
        }

        let overlay = &self.imp().minimap_overlay;
        overlay.set_visible(availability == MinimapAvailability::Visible);

        if availability != MinimapAvailability::Visible {
            // Hidden source maps can keep stale EOF/top margins from the last
            // visible layout. Cancel pending timers first, then sync once so
            // the next visible frame cannot inherit old geometry.
            let minimap = &self.imp().minimap;
            minimap
                .reflow_settle_generation
                .set(minimap.reflow_settle_generation.get().wrapping_add(1));
            minimap.reflow_settle_pending.set(false);
            minimap.reflow_reveal_pending.set(false);
            self.sync_minimap_view_geometry();
            self.drop_minimap_reflow_freeze();
            self.imp().minimap.markers.borrow_mut().clear();
            self.queue_minimap_draw();
            self.publish_minimap_unavailable_feedback_if_needed(availability);
            return;
        }

        self.apply_minimap_width_from_settings();
        self.sync_minimap_view_geometry();
        *self.imp().minimap.markers.borrow_mut() = collect_markers(self);
        self.queue_minimap_draw();
    }

    /// Debounce marker recomputation after search, edits, or viewport changes.
    pub(crate) fn schedule_minimap_refresh(&self) {
        let generation = self.imp().minimap.refresh_generation.get().wrapping_add(1);
        self.imp().minimap.refresh_generation.set(generation);
        self.imp().minimap.refresh_pending.set(true);

        let editor_weak = self.downgrade();
        // Schedule the debounced refresh on GTK's main loop. The `_local`
        // variant is required because this closure upgrades and touches GTK
        // objects, which are main-thread-only and not `Send`.
        glib::timeout_add_local_once(MINIMAP_REFRESH_DEBOUNCE, move || {
            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            if editor.imp().minimap.refresh_generation.get() != generation {
                return;
            }
            editor.refresh_minimap();
        });
    }

    /// Apply the persisted minimap width to this editor page.
    pub(crate) fn apply_minimap_width_from_settings(&self) {
        let width = self.imp().settings.int(keys::MINIMAP_WIDTH).clamp(64, 160);
        self.imp().minimap_overlay.set_width_request(width);
    }

    /// Queue a redraw of the semantic marker strip when it exists.
    pub(crate) fn queue_minimap_draw(&self) {
        if let Some(strip) = self.imp().minimap.marker_strip.borrow().as_ref() {
            strip.queue_draw();
        }
    }

    /// Temporarily suspend edit tracking while programmatic buffer mutations run.
    pub(crate) fn set_minimap_tracking_suspended(&self, suspended: bool) {
        self.imp().minimap.tracking_suspended.set(suspended);
    }

    /// Clear all modified-since-save markers for this editor.
    pub(crate) fn clear_modified_line_marks(&self) {
        let buffer = self.buffer();
        buffer.remove_source_marks(
            &buffer.start_iter(),
            &buffer.end_iter(),
            Some(MINIMAP_MODIFIED_MARK_CATEGORY),
        );
        self.imp().minimap.modified_marks.borrow_mut().clear();
        self.imp().minimap.modified_lines_cache.borrow_mut().clear();
    }

    /// Mark the inclusive line range as modified-since-save.
    pub(crate) fn record_modified_lines(&self, start_line: u32, end_line: u32) {
        let mut known_lines = self.imp().minimap.modified_lines_cache.borrow_mut();
        let mut marks = self.imp().minimap.modified_marks.borrow_mut();
        let buffer = self.buffer();

        for line in start_line..=end_line {
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
    pub(crate) fn mark_entire_buffer_modified(&self) {
        self.clear_modified_line_marks();
        let total_lines = document_line_count(self);
        if total_lines == 0 {
            return;
        }
        self.record_modified_lines(0, total_lines.saturating_sub(1));
        self.schedule_minimap_refresh();
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

fn sync_source_map_geometry(source_map: &sourceview5::Map, source_view: &sourceview5::View) {
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
        document_height_from_iter_rect(source_view.iter_location(&end_iter))?;
    let source_map_document_height =
        document_height_from_iter_rect(source_map.iter_location(&end_iter))?;
    source_map_editor_height_ratio_from_heights(editor_document_height, source_map_document_height)
}

/// Compute the source-map/editor ratio, returning `None` for unusable geometry.
fn source_map_editor_height_ratio_from_heights(
    editor_document_height: i32,
    source_map_document_height: i32,
) -> Option<f64> {
    if editor_document_height <= 0 || source_map_document_height <= 0 {
        return None;
    }
    Some(f64::from(source_map_document_height) / f64::from(editor_document_height))
}

/// Choose the CSS class that compensates the native slider at the wide-editor threshold.
fn wide_editor_slider_offset_class(ratio: Option<f64>) -> Option<&'static str> {
    ratio
        .is_some_and(|ratio| ratio.is_finite() && ratio > MINIMAP_WIDE_EDITOR_RATIO_THRESHOLD)
        .then_some(MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS)
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
    let source_map_document_height = document_height_from_iter_rect(source_map_end)?;
    let editor_document_height = document_height_from_iter_rect(editor_end)?;
    let border = source_map_border(source_map);

    let native_slider_estimate = native_slider_estimate_from_inputs(NativeSliderEstimateInput {
        map_x: source_map_bounds.x,
        map_y: source_map_bounds.y,
        map_width: source_map_bounds.width,
        editor_visible_y: editor_visible_rect.y,
        editor_visible_height: editor_visible_rect.height,
        editor_document_height,
        source_map_visible_y: source_map_visible_rect.y,
        source_map_document_height,
        border_left: i32::from(border.left()),
        border_right: i32::from(border.right()),
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
        border_left: i32::from(border.left()),
        border_right: i32::from(border.right()),
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

fn source_map_bounds_relative_to(
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

fn text_view_rect(rect: gtk4::gdk::Rectangle) -> MinimapTextViewRect {
    MinimapTextViewRect {
        x: rect.x(),
        y: rect.y(),
        width: rect.width(),
        height: rect.height(),
    }
}

fn document_height_from_iter_rect(rect: gtk4::gdk::Rectangle) -> Option<i32> {
    let height = rect.y().saturating_add(rect.height().max(0));
    (height > 0).then_some(height)
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

fn gtk_f64_to_milli(value: f64) -> i64 {
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

/// Public-geometry inputs used to mirror `GtkSourceMap`'s native slider formula.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NativeSliderEstimateInput {
    /// Source-map left edge in target coordinates.
    map_x: f64,
    /// Source-map top edge in target coordinates.
    map_y: f64,
    /// Source-map allocation width before CSS slider outset.
    map_width: f64,
    /// Editor visible rect y, including GTK's negative top-margin value at line one.
    editor_visible_y: i32,
    /// Editor visible rect height.
    editor_visible_height: i32,
    /// Editor document height used by the native slider ratio.
    editor_document_height: i32,
    /// Source-map visible rect y that GTK subtracts while drawing the slider.
    source_map_visible_y: i32,
    /// Source-map document height used by the native slider ratio.
    source_map_document_height: i32,
    /// Left CSS border removed from the usable slider width.
    border_left: i32,
    /// Right CSS border removed from the usable slider width.
    border_right: i32,
}

/// Mirror GtkSourceMap's private slider math with public geometry only.
///
/// This estimate explains where GTK should draw the native slider after
/// applying source-map scroll offset and CSS outset. It is diagnostic;
/// screenshot pixel anchors remain the authority for rendered correctness.
fn native_slider_estimate_from_inputs(
    input: NativeSliderEstimateInput,
) -> Option<MinimapProjectedBounds> {
    if !input.map_x.is_finite()
        || !input.map_y.is_finite()
        || !input.map_width.is_finite()
        || input.map_width <= 0.0
        || input.editor_visible_height <= 0
        || input.editor_document_height <= 0
        || input.source_map_document_height <= 0
    {
        return None;
    }

    let editor_document_height = f64::from(input.editor_document_height);
    let source_map_document_height = f64::from(input.source_map_document_height);
    let editor_visible_top = f64::from(input.editor_visible_y);
    let editor_visible_bottom = editor_visible_top + f64::from(input.editor_visible_height.max(1));
    let top_in_map = (editor_visible_top / editor_document_height * source_map_document_height)
        - f64::from(input.source_map_visible_y);
    let bottom_in_map = (editor_visible_bottom / editor_document_height
        * source_map_document_height)
        - f64::from(input.source_map_visible_y);
    let height = (bottom_in_map - top_in_map).max(MINIMAP_VIEWPORT_MIN_HEIGHT);
    let usable_width = (input.map_width
        - f64::from(input.border_left.max(0))
        - f64::from(input.border_right.max(0)))
    .max(1.0);

    Some(MinimapProjectedBounds {
        x: input.map_x + f64::from(input.border_left.max(0))
            - f64::from(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET),
        y: input.map_y + top_in_map,
        width: usable_width + (f64::from(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET) * 2.0),
        height,
    })
}

/// Fit the raw native slider estimate vertically into the source-map allocation.
///
/// The raw estimate intentionally mirrors `GtkSourceMap`'s private ratio math,
/// which can point outside the map when the tiny source-map document is taller
/// than the visible widget. Screenshot crops and widget tests need the visible
/// vertical part of that effect, while diagnostics keep the raw estimate
/// separately. The horizontal CSS outset is preserved because it is part of the
/// native slider effect and intentionally paints outside the map text column.
fn fit_native_slider_to_source_map_bounds(
    raw: MinimapProjectedBounds,
    source_map_bounds: MinimapProjectedBounds,
) -> Option<MinimapProjectedBounds> {
    if !raw.x.is_finite()
        || !raw.y.is_finite()
        || !raw.width.is_finite()
        || !raw.height.is_finite()
        || !source_map_bounds.y.is_finite()
        || !source_map_bounds.height.is_finite()
        || raw.width <= 0.0
        || raw.height <= 0.0
        || source_map_bounds.height <= 0.0
    {
        return None;
    }

    let lower = source_map_bounds.y;
    let upper = source_map_bounds.y + source_map_bounds.height;
    if !upper.is_finite() || upper <= lower {
        return None;
    }

    let height = raw
        .height
        .max(MINIMAP_VIEWPORT_MIN_HEIGHT)
        .min(source_map_bounds.height);
    let mut top = raw.y;
    let mut bottom = top + height;

    if top < lower {
        bottom += lower - top;
        top = lower;
    }
    if bottom > upper {
        top -= bottom - upper;
        bottom = upper;
    }

    top = top.max(lower);
    bottom = bottom.min(upper);
    (bottom > top).then_some(MinimapProjectedBounds {
        x: raw.x,
        y: top,
        width: raw.width,
        height: bottom - top,
    })
}

fn current_availability(editor: &LushtextEditorPage) -> MinimapAvailability {
    let focus_suppressed = editor.focus_mode_suppresses_minimap();
    let preference_enabled = editor.imp().settings.boolean(keys::SHOW_MINIMAP);
    let evicted = editor.is_evicted();
    let syntax_enabled = editor.size_check().syntax_enabled();
    let cheap_policy = MinimapAvailabilityPolicy {
        focus_suppressed,
        preference_enabled,
        evicted,
        syntax_enabled,
        wrapped_layout_too_large: false,
    };
    if minimap_availability_for_policy(cheap_policy) != MinimapAvailability::Visible {
        return minimap_availability_for_policy(cheap_policy);
    }

    minimap_availability_for_policy(MinimapAvailabilityPolicy {
        focus_suppressed,
        preference_enabled,
        evicted,
        syntax_enabled,
        wrapped_layout_too_large: wrapped_minimap_layout_exceeds_budget(editor),
    })
}

/// Pure availability inputs gathered from GTK state by `current_availability`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MinimapAvailabilityPolicy {
    focus_suppressed: bool,
    preference_enabled: bool,
    evicted: bool,
    syntax_enabled: bool,
    wrapped_layout_too_large: bool,
}

fn minimap_availability_for_policy(policy: MinimapAvailabilityPolicy) -> MinimapAvailability {
    if policy.focus_suppressed || !policy.preference_enabled {
        return MinimapAvailability::Disabled;
    }
    if policy.evicted {
        return MinimapAvailability::Evicted;
    }
    if !policy.syntax_enabled || policy.wrapped_layout_too_large {
        return MinimapAvailability::TooLarge;
    }
    MinimapAvailability::Visible
}

fn wrapped_minimap_layout_exceeds_budget(editor: &LushtextEditorPage) -> bool {
    if editor.source_view().wrap_mode() == gtk4::WrapMode::None {
        return false;
    }
    if let Some(cached) = editor.imp().minimap.wrapped_layout_too_large.get() {
        return cached;
    }

    let buffer = editor.buffer();
    let buffer_chars = u64::try_from(buffer.char_count()).unwrap_or(u64::MAX);
    let estimated_size = editor.file_size().unwrap_or(0).max(buffer_chars);
    if estimated_size > MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET
        && buffer_snapshot::buffer_requires_chunked_snapshot(&buffer)
    {
        editor
            .imp()
            .minimap
            .wrapped_layout_too_large
            .set(Some(true));
        return true;
    }

    if estimated_size <= MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET {
        editor
            .imp()
            .minimap
            .wrapped_layout_too_large
            .set(Some(false));
        return false;
    }

    let exceeds =
        buffer_has_line_exceeding_char_budget(&buffer, MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET);
    editor
        .imp()
        .minimap
        .wrapped_layout_too_large
        .set(Some(exceeds));
    exceeds
}

#[cfg(test)]
fn wrapped_layout_budget_exceeded(estimated_size: u64, has_extreme_line: bool) -> bool {
    estimated_size > MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET && has_extreme_line
}

fn buffer_has_line_exceeding_char_budget(
    buffer: &sourceview5::Buffer,
    line_char_budget: usize,
) -> bool {
    let mut current_line_chars = 0usize;
    let mut iter = buffer.start_iter();
    let end = buffer.end_iter();

    while iter != end {
        if iter.char() == '\n' {
            current_line_chars = 0;
        } else {
            current_line_chars = current_line_chars.saturating_add(1);
            if current_line_chars > line_char_budget {
                return true;
            }
        }

        if !iter.forward_char() {
            break;
        }
    }

    false
}

#[cfg(test)]
fn text_exceeds_line_char_budget(text: &str, line_char_budget: usize) -> bool {
    let mut current_line_chars = 0usize;

    for ch in text.chars() {
        if ch == '\n' {
            current_line_chars = 0;
        } else {
            current_line_chars = current_line_chars.saturating_add(1);
            if current_line_chars > line_char_budget {
                return true;
            }
        }
    }

    false
}

fn document_line_count(editor: &LushtextEditorPage) -> u32 {
    u32::try_from(editor.buffer().end_iter().line())
        .unwrap_or(0)
        .saturating_add(1)
        .max(1)
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
        markers.extend(markers_from_lines(
            MinimapMarkerKind::LongLine,
            collect_long_line_warnings(editor),
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
    editor
        .imp()
        .minimap
        .modified_marks
        .borrow()
        .iter()
        .filter_map(|mark| u32::try_from(editor.buffer().iter_at_mark(mark).line()).ok())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn collect_long_line_warnings(editor: &LushtextEditorPage) -> Vec<u32> {
    let buffer = editor.buffer();
    if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
        return Vec::new();
    }

    let text = buffer_snapshot::snapshot_buffer_text_direct(&buffer);
    long_line_warning_lines(&text)
}

fn long_line_warning_lines(text: &str) -> Vec<u32> {
    text.lines()
        .enumerate()
        .filter_map(|(line, text)| {
            (text.chars().count() > MINIMAP_LONG_LINE_WARNING_THRESHOLD)
                .then(|| u32::try_from(line).ok())
                .flatten()
        })
        .collect()
}

fn markers_from_lines(
    kind: MinimapMarkerKind,
    lines: impl IntoIterator<Item = u32>,
) -> Vec<MinimapMarker> {
    let mut sorted = lines.into_iter().collect::<Vec<_>>();
    sorted.sort_unstable();
    sorted.dedup();
    normalize_line_runs(kind, &sorted)
}

fn normalize_line_runs(kind: MinimapMarkerKind, lines: &[u32]) -> Vec<MinimapMarker> {
    let Some((&first, rest)) = lines.split_first() else {
        return Vec::new();
    };

    let mut markers = Vec::new();
    let mut start = first;
    let mut end = first;
    for &line in rest {
        if line == end.saturating_add(1) {
            end = line;
            continue;
        }
        markers.push(MinimapMarker {
            kind,
            start_line: start,
            end_line: end,
        });
        start = line;
        end = line;
    }
    markers.push(MinimapMarker {
        kind,
        start_line: start,
        end_line: end,
    });
    markers
}

fn draw_marker_strip(
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
    let raw_top = line_top_in_target(source_map, space.map_y, &visible_start);
    let raw_bottom = line_bottom_in_target(source_map, space.map_y, &visible_end);

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
    let raw_top = line_top_in_target(source_map, space.map_y, &start_iter);
    let raw_bottom = line_bottom_in_target(source_map, space.map_y, &start_iter);

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

/// Shared coordinate-space contract for minimap projections.
///
/// `GtkSourceMap` reports line positions in its own buffer/widget coordinates,
/// while Automation1 crops need a caller-supplied target widget coordinate
/// space. This struct keeps those spaces explicit and bounds every projection
/// to the text that the minimap actually renders, excluding the dynamic EOF tail.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MinimapProjectionSpace {
    /// Height of the target widget in the same coordinates as returned bounds.
    target_height: f64,
    /// Source-map left edge in target widget coordinates.
    map_x: f64,
    /// Source-map top edge in target widget coordinates.
    map_y: f64,
    /// Source-map width before any native-slider CSS outset is applied.
    map_width: f64,
    /// Top of the first rendered minimap line in target widget coordinates.
    content_top: f64,
    /// Bottom of the last rendered minimap line in target widget coordinates.
    content_bottom: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarkerProjectionSpace {
    strip_height: f64,
    content_top: f64,
    content_bottom: f64,
    map_y_in_strip: f64,
    min_height: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectedBoundsFit {
    /// Reject projections that land outside rendered minimap text.
    RejectOutside,
    /// Clamp projections back to rendered minimap text when the visual effect remains visible.
    ClampOutside,
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
    let content_top = line_top_in_target(source_map, map_y, &start_iter);
    let content_bottom = line_bottom_in_target(source_map, map_y, &end_iter);

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
    let raw_top = line_top_in_target(source_map, space.map_y_in_strip, &start_iter);
    let raw_bottom = line_bottom_in_target(source_map, space.map_y_in_strip, &end_iter);

    fit_marker_bounds(marker.kind, raw_top, raw_bottom, space)
}

fn text_iter_at_line_or_last(buffer: &gtk4::TextBuffer, line: u32) -> gtk4::TextIter {
    let end_iter = buffer.end_iter();
    let end_line = u32::try_from(end_iter.line()).unwrap_or_default();
    let clamped_line = line.min(end_line);

    i32::try_from(clamped_line)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .unwrap_or(end_iter)
}

fn line_top_in_target(
    source_map: &sourceview5::Map,
    map_y_in_target: f64,
    iter: &gtk4::TextIter,
) -> f64 {
    let (line_y, _) = source_map.line_yrange(iter);
    map_buffer_y_to_target_y(source_map, map_y_in_target, line_y)
}

fn line_bottom_in_target(
    source_map: &sourceview5::Map,
    map_y_in_target: f64,
    iter: &gtk4::TextIter,
) -> f64 {
    let (line_y, line_height) = source_map.line_yrange(iter);
    map_buffer_y_to_target_y(
        source_map,
        map_y_in_target,
        line_y.saturating_add(line_height.max(0)),
    )
}

fn map_buffer_y_to_target_y(
    source_map: &sourceview5::Map,
    map_y_in_target: f64,
    buffer_y: i32,
) -> f64 {
    // `buffer_to_window_coords` returns y relative to the source-map widget;
    // add the map's target-relative top edge to produce crop/anchor coordinates.
    let (_, widget_y) =
        source_map.buffer_to_window_coords(gtk4::TextWindowType::Widget, 0, buffer_y);
    map_y_in_target + f64::from(widget_y)
}

fn fit_marker_bounds(
    kind: MinimapMarkerKind,
    raw_top: f64,
    raw_bottom: f64,
    space: MarkerProjectionSpace,
) -> Option<MinimapMarkerBounds> {
    if !raw_top.is_finite()
        || !raw_bottom.is_finite()
        || !space.strip_height.is_finite()
        || !space.content_top.is_finite()
        || !space.content_bottom.is_finite()
        || space.strip_height <= 0.0
    {
        return None;
    }

    let lower = space.content_top.max(0.0);
    let upper = space.content_bottom.min(space.strip_height);
    if upper <= lower {
        return None;
    }

    let (raw_top, raw_bottom) = if raw_top <= raw_bottom {
        (raw_top, raw_bottom)
    } else {
        (raw_bottom, raw_top)
    };
    if raw_bottom < lower || raw_top > upper {
        return None;
    }

    let mut top = raw_top.max(lower);
    let mut bottom = raw_bottom.min(upper);
    if bottom < top {
        bottom = top;
    }

    let target_height = space.min_height.max(0.0).min(upper - lower);
    if bottom - top < target_height {
        // midpoint avoids `(top + bottom) / 2.0` overflow before the later clamp.
        let center = f64::midpoint(top, bottom).clamp(lower, upper);
        top = center - (target_height / 2.0);
        bottom = center + (target_height / 2.0);

        if top < lower {
            bottom += lower - top;
            top = lower;
        }
        if bottom > upper {
            top -= bottom - upper;
            bottom = upper;
        }

        top = top.max(lower);
        bottom = bottom.min(upper);
    }

    (bottom > top).then_some(MinimapMarkerBounds { kind, top, bottom })
}

/// Fit a projected minimap rectangle into rendered content bounds.
///
/// Viewport diagnostics use `ClampOutside` because GTK keeps the native slider
/// visible at document edges. Content-row diagnostics use `RejectOutside`
/// because fabricating a row would let screenshots pass without rendered text.
/// Small projections expand around their center to remain pixel-detectable.
fn fit_projected_bounds(
    x: f64,
    width: f64,
    raw_top: f64,
    raw_bottom: f64,
    space: MinimapProjectionSpace,
    min_height: f64,
    fit: ProjectedBoundsFit,
) -> Option<MinimapProjectedBounds> {
    if !x.is_finite()
        || !width.is_finite()
        || !raw_top.is_finite()
        || !raw_bottom.is_finite()
        || !space.target_height.is_finite()
        || !space.content_top.is_finite()
        || !space.content_bottom.is_finite()
        || width <= 0.0
        || space.target_height <= 0.0
    {
        return None;
    }

    let lower = space.content_top.max(0.0);
    let upper = space.content_bottom.min(space.target_height);
    if upper <= lower {
        return None;
    }

    let (raw_top, raw_bottom) = if raw_top <= raw_bottom {
        (raw_top, raw_bottom)
    } else {
        (raw_bottom, raw_top)
    };
    if fit == ProjectedBoundsFit::RejectOutside && (raw_bottom < lower || raw_top > upper) {
        return None;
    }

    let mut top = raw_top.clamp(lower, upper);
    let mut bottom = raw_bottom.clamp(lower, upper);
    if bottom < top {
        bottom = top;
    }

    let target_height = min_height.max(0.0).min(upper - lower);
    if bottom - top < target_height {
        // Use midpoint rather than `(top + bottom) / 2.0` so extreme GTK
        // coordinates cannot overflow before the final clamp.
        let center = f64::midpoint(top, bottom).clamp(lower, upper);
        top = center - (target_height / 2.0);
        bottom = center + (target_height / 2.0);

        if top < lower {
            bottom += lower - top;
            top = lower;
        }
        if bottom > upper {
            top -= bottom - upper;
            bottom = upper;
        }

        top = top.max(lower);
        bottom = bottom.min(upper);
    }

    (bottom > top).then_some(MinimapProjectedBounds {
        x,
        y: top,
        width,
        height: bottom - top,
    })
}

fn marker_lane_width(kind: MinimapMarkerKind, total_width: f64) -> f64 {
    let ratio = match kind {
        MinimapMarkerKind::Bookmark => 1.0,
        MinimapMarkerKind::Search => 0.82,
        MinimapMarkerKind::Modified => 0.64,
        MinimapMarkerKind::LongLine => 0.46,
    };
    (total_width * ratio).max(2.0)
}

fn marker_lane_x(total_width: f64, lane_width: f64) -> f64 {
    total_width - lane_width
}

fn marker_rgba(kind: MinimapMarkerKind, dark: bool) -> (f64, f64, f64, f64) {
    match (kind, dark) {
        (MinimapMarkerKind::Bookmark, false) => (0.11, 0.44, 0.85, 0.95),
        (MinimapMarkerKind::Bookmark, true) => (0.39, 0.65, 0.95, 0.95),
        (MinimapMarkerKind::Search, false) => (0.95, 0.45, 0.0, 0.92),
        (MinimapMarkerKind::Search, true) => (1.0, 0.58, 0.17, 0.92),
        (MinimapMarkerKind::Modified, false) => (0.17, 0.68, 0.42, 0.92),
        (MinimapMarkerKind::Modified, true) => (0.32, 0.84, 0.55, 0.92),
        (MinimapMarkerKind::LongLine, false) => (0.88, 0.11, 0.14, 0.92),
        (MinimapMarkerKind::LongLine, true) => (1.0, 0.46, 0.45, 0.92),
    }
}

fn iter_at_line_or_last(buffer: &sourceview5::Buffer, line: u32) -> gtk4::TextIter {
    i32::try_from(line)
        .ok()
        .and_then(|line| buffer.iter_at_line(line))
        .unwrap_or_else(|| buffer.end_iter())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimap_policy_constants_are_stable() {
        assert_eq!(MINIMAP_MARKER_STRIP_WIDTH, 8);
        assert_eq!(MINIMAP_REFRESH_DEBOUNCE, Duration::from_millis(80));
        assert_eq!(MINIMAP_LONG_LINE_WARNING_THRESHOLD, 120);
        assert_eq!(MINIMAP_SEARCH_MATCH_CAP, 2_000);
        assert_eq!(MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET, 2_097_152);
        assert_eq!(MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET, 8_000);
        assert_eq!(MINIMAP_MARKER_MIN_HEIGHT, 2.0);
        assert_eq!(MINIMAP_TOP_CONTENT_MARGIN, 5);
        assert_eq!(MINIMAP_WIDE_EDITOR_RATIO_THRESHOLD, 0.20);
        assert_eq!(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET, 13);
        assert_eq!(MINIMAP_REFLOW_SETTLE_DEBOUNCE, Duration::from_millis(150));
        assert_eq!(MINIMAP_REFLOW_REVEAL_DELAY, Duration::from_millis(800));
        assert_eq!(MINIMAP_MODIFIED_MARK_CATEGORY, "lushtext-minimap-modified");
    }

    #[test]
    fn test_source_map_editor_height_ratio_from_heights_tracks_ratio() {
        assert_eq!(
            source_map_editor_height_ratio_from_heights(1_000, 200),
            Some(0.2)
        );
        assert_eq!(
            source_map_editor_height_ratio_from_heights(6_000, 1_800),
            Some(0.3)
        );
    }

    #[test]
    fn test_source_map_editor_height_ratio_from_heights_rejects_unusable_geometry() {
        assert_eq!(source_map_editor_height_ratio_from_heights(0, 200), None);
        assert_eq!(source_map_editor_height_ratio_from_heights(1_000, 0), None);
        assert_eq!(source_map_editor_height_ratio_from_heights(-1, 200), None);
        assert_eq!(source_map_editor_height_ratio_from_heights(1_000, -1), None);
    }

    #[test]
    fn test_wide_editor_slider_offset_class_uses_strict_threshold() {
        assert_eq!(wide_editor_slider_offset_class(Some(0.199_999)), None);
        assert_eq!(wide_editor_slider_offset_class(Some(0.20)), None);
        assert_eq!(
            wide_editor_slider_offset_class(Some(0.200_001)),
            Some(MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS)
        );
    }

    #[test]
    fn test_wide_editor_slider_offset_class_rejects_missing_or_nonfinite_ratio() {
        assert_eq!(wide_editor_slider_offset_class(None), None);
        assert_eq!(wide_editor_slider_offset_class(Some(f64::NAN)), None);
        assert_eq!(wide_editor_slider_offset_class(Some(f64::INFINITY)), None);
    }

    #[test]
    fn test_marker_bounds_height_uses_bottom_minus_top() {
        let bounds = MinimapMarkerBounds {
            kind: MinimapMarkerKind::Search,
            top: 3.25,
            bottom: 11.75,
        };

        assert_eq!(bounds.height(), 8.5);
    }

    #[test]
    fn test_native_slider_estimate_subtracts_source_map_visible_offset() {
        let input = NativeSliderEstimateInput {
            map_x: 100.0,
            map_y: 52.0,
            map_width: 94.0,
            editor_visible_y: 0,
            editor_visible_height: 660,
            editor_document_height: 1320,
            source_map_visible_y: 0,
            source_map_document_height: 640,
            border_left: 0,
            border_right: 0,
        };
        let settled = native_slider_estimate_from_inputs(input)
            .expect("settled top-of-file native slider should estimate");
        let stale_map_scroll = native_slider_estimate_from_inputs(NativeSliderEstimateInput {
            source_map_visible_y: 2,
            ..input
        })
        .expect("stale source-map visible rect should still estimate");

        assert_eq!(settled.y, 52.0);
        assert_eq!(stale_map_scroll.y, 50.0);
        assert_eq!(settled.height, stale_map_scroll.height);
        assert_eq!(settled.x, 87.0);
        assert_eq!(settled.width, 120.0);
    }

    #[test]
    fn test_native_slider_visible_bounds_fit_offscreen_estimate_to_map_edge() {
        let raw = MinimapProjectedBounds {
            x: -13.0,
            y: 779.085,
            width: 120.0,
            height: 184.161,
        };
        let source_map_bounds = MinimapProjectedBounds {
            x: 0.0,
            y: 0.0,
            width: 94.0,
            height: 664.0,
        };

        let fitted = fit_native_slider_to_source_map_bounds(raw, source_map_bounds)
            .expect("offscreen native estimate should fit to the visible map edge");

        assert_eq!(fitted.x, raw.x);
        assert_eq!(fitted.width, raw.width);
        assert!((fitted.height - raw.height).abs() <= f64::EPSILON * 512.0);
        assert_eq!(fitted.bottom(), source_map_bounds.bottom());
        assert!(fitted.y >= source_map_bounds.y);
    }

    #[test]
    fn test_normalize_line_runs_merges_contiguous_lines() {
        let markers = normalize_line_runs(MinimapMarkerKind::Bookmark, &[1, 2, 3, 8, 10, 11]);
        assert_eq!(
            markers,
            vec![
                MinimapMarker {
                    kind: MinimapMarkerKind::Bookmark,
                    start_line: 1,
                    end_line: 3,
                },
                MinimapMarker {
                    kind: MinimapMarkerKind::Bookmark,
                    start_line: 8,
                    end_line: 8,
                },
                MinimapMarker {
                    kind: MinimapMarkerKind::Bookmark,
                    start_line: 10,
                    end_line: 11,
                },
            ]
        );
    }

    #[test]
    fn test_markers_from_lines_deduplicates_before_normalizing() {
        let markers = markers_from_lines(MinimapMarkerKind::Search, [4, 4, 5, 9]);
        assert_eq!(
            markers,
            vec![
                MinimapMarker {
                    kind: MinimapMarkerKind::Search,
                    start_line: 4,
                    end_line: 5,
                },
                MinimapMarker {
                    kind: MinimapMarkerKind::Search,
                    start_line: 9,
                    end_line: 9,
                },
            ]
        );
    }

    #[test]
    fn test_markers_from_lines_handles_empty_input() {
        assert!(markers_from_lines(MinimapMarkerKind::LongLine, []).is_empty());
    }

    #[test]
    fn test_buffer_line_budget_resets_on_newlines_and_uses_strict_overflow() {
        assert!(!text_exceeds_line_char_budget("abcd\nabcde\nxy", 5));
        assert!(text_exceeds_line_char_budget("abcd\nabcde\nxy", 4));

        assert!(!text_exceeds_line_char_budget("éé\né", 2));
        assert!(text_exceeds_line_char_budget("éé\né", 1));
    }

    #[test]
    fn test_buffer_line_budget_returns_false_for_empty_buffers() {
        assert!(!text_exceeds_line_char_budget("", 0));
    }

    #[test]
    fn test_minimap_availability_policy_preserves_priority_order() {
        let visible = MinimapAvailabilityPolicy {
            focus_suppressed: false,
            preference_enabled: true,
            evicted: false,
            syntax_enabled: true,
            wrapped_layout_too_large: false,
        };

        assert_eq!(
            minimap_availability_for_policy(visible),
            MinimapAvailability::Visible
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                preference_enabled: false,
                evicted: true,
                syntax_enabled: false,
                wrapped_layout_too_large: true,
                ..visible
            }),
            MinimapAvailability::Disabled,
            "the user preference should win over document state"
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                focus_suppressed: true,
                preference_enabled: true,
                evicted: true,
                ..visible
            }),
            MinimapAvailability::Disabled,
            "Focus Mode should suppress the minimap without changing the saved preference"
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                evicted: true,
                syntax_enabled: false,
                ..visible
            }),
            MinimapAvailability::Evicted,
            "evicted tabs report their reload state before size-tier feedback"
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                syntax_enabled: false,
                ..visible
            }),
            MinimapAvailability::TooLarge
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                wrapped_layout_too_large: true,
                ..visible
            }),
            MinimapAvailability::TooLarge
        );
    }

    #[test]
    fn test_wrapped_layout_budget_requires_size_above_budget_and_extreme_line() {
        assert!(!wrapped_layout_budget_exceeded(
            MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET,
            true
        ));
        assert!(!wrapped_layout_budget_exceeded(
            MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET + 1,
            false
        ));
        assert!(wrapped_layout_budget_exceeded(
            MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET + 1,
            true
        ));
    }

    #[test]
    fn test_long_line_warning_lines_use_strict_character_threshold() {
        let exact = "a".repeat(MINIMAP_LONG_LINE_WARNING_THRESHOLD);
        let too_long = "b".repeat(MINIMAP_LONG_LINE_WARNING_THRESHOLD + 1);
        let another_too_long = "c".repeat(MINIMAP_LONG_LINE_WARNING_THRESHOLD + 2);
        let text = format!("{exact}\n{too_long}\nshort\n{another_too_long}");

        assert_eq!(long_line_warning_lines(&text), vec![1, 3]);
        assert!(long_line_warning_lines("short\nalso short").is_empty());
    }

    #[test]
    fn test_fit_marker_bounds_keeps_min_height_above_eof_tail() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::Search,
            98.7,
            98.8,
            MarkerProjectionSpace {
                strip_height: 140.0,
                content_top: 10.0,
                content_bottom: 100.0,
                map_y_in_strip: 0.0,
                min_height: 8.0,
            },
        )
        .expect("bottom marker should still be drawable");

        assert_eq!(bounds.kind, MinimapMarkerKind::Search);
        assert!((bounds.top - 92.0).abs() < f64::EPSILON);
        assert!((bounds.bottom - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fit_marker_bounds_expands_collapsed_line_inside_content() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::Bookmark,
            40.0,
            40.0,
            MarkerProjectionSpace {
                strip_height: 100.0,
                content_top: 0.0,
                content_bottom: 80.0,
                map_y_in_strip: 0.0,
                min_height: 6.0,
            },
        )
        .expect("collapsed but visible line should get a minimum marker");

        assert!((bounds.top - 37.0).abs() < f64::EPSILON);
        assert!((bounds.bottom - 43.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_fit_marker_bounds_clamps_to_content_and_handles_reversed_input() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::LongLine,
            60.0,
            20.0,
            MarkerProjectionSpace {
                strip_height: 120.0,
                content_top: 25.0,
                content_bottom: 55.0,
                map_y_in_strip: 0.0,
                min_height: 2.0,
            },
        )
        .expect("partially visible reversed marker should be clamped");

        assert_eq!(bounds.kind, MinimapMarkerKind::LongLine);
        assert_eq!(bounds.top, 25.0);
        assert_eq!(bounds.bottom, 55.0);
        assert_eq!(bounds.height(), 30.0);
    }

    #[test]
    fn test_fit_marker_bounds_fills_content_when_minimum_exceeds_content_height() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::Modified,
            42.0,
            42.0,
            MarkerProjectionSpace {
                strip_height: 100.0,
                content_top: 40.0,
                content_bottom: 45.0,
                map_y_in_strip: 0.0,
                min_height: 20.0,
            },
        )
        .expect("minimum height should be capped to visible content");

        assert_eq!(bounds.top, 40.0);
        assert_eq!(bounds.bottom, 45.0);
    }

    #[test]
    fn test_fit_marker_bounds_leaves_already_tall_markers_unchanged() {
        let bounds = fit_marker_bounds(
            MinimapMarkerKind::Search,
            10.0,
            16.0,
            MarkerProjectionSpace {
                strip_height: 100.0,
                content_top: 0.0,
                content_bottom: 90.0,
                map_y_in_strip: 0.0,
                min_height: 4.0,
            },
        )
        .expect("marker taller than the minimum should stay drawable");

        assert_eq!(bounds.top, 10.0);
        assert_eq!(bounds.bottom, 16.0);
    }

    #[test]
    fn test_fit_marker_bounds_keeps_markers_that_touch_content_edges() {
        let top_bounds = fit_marker_bounds(
            MinimapMarkerKind::Bookmark,
            25.0,
            25.0,
            MarkerProjectionSpace {
                strip_height: 120.0,
                content_top: 25.0,
                content_bottom: 55.0,
                map_y_in_strip: 0.0,
                min_height: 4.0,
            },
        )
        .expect("line at content top should still be visible");
        assert_eq!(top_bounds.top, 25.0);
        assert_eq!(top_bounds.bottom, 29.0);

        let bottom_bounds = fit_marker_bounds(
            MinimapMarkerKind::Bookmark,
            55.0,
            55.0,
            MarkerProjectionSpace {
                strip_height: 120.0,
                content_top: 25.0,
                content_bottom: 55.0,
                map_y_in_strip: 0.0,
                min_height: 4.0,
            },
        )
        .expect("line at content bottom should still be visible");
        assert_eq!(bottom_bounds.top, 51.0);
        assert_eq!(bottom_bounds.bottom, 55.0);
    }

    #[test]
    fn test_fit_marker_bounds_rejects_each_non_finite_input() {
        let finite_space = MarkerProjectionSpace {
            strip_height: 100.0,
            content_top: 10.0,
            content_bottom: 80.0,
            map_y_in_strip: 0.0,
            min_height: 2.0,
        };

        assert!(
            fit_marker_bounds(MinimapMarkerKind::Search, f64::NAN, 20.0, finite_space).is_none()
        );
        assert!(
            fit_marker_bounds(MinimapMarkerKind::Search, 20.0, f64::NAN, finite_space).is_none()
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Search,
                20.0,
                21.0,
                MarkerProjectionSpace {
                    strip_height: f64::NAN,
                    ..finite_space
                },
            )
            .is_none()
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Search,
                20.0,
                21.0,
                MarkerProjectionSpace {
                    content_top: f64::NAN,
                    ..finite_space
                },
            )
            .is_none()
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Search,
                20.0,
                21.0,
                MarkerProjectionSpace {
                    content_bottom: f64::NAN,
                    ..finite_space
                },
            )
            .is_none()
        );
    }

    #[test]
    fn test_fit_marker_bounds_rejects_zero_height_when_minimum_is_zero() {
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Search,
                30.0,
                30.0,
                MarkerProjectionSpace {
                    strip_height: 100.0,
                    content_top: 0.0,
                    content_bottom: 90.0,
                    map_y_in_strip: 0.0,
                    min_height: 0.0,
                },
            )
            .is_none()
        );
    }

    #[test]
    fn test_fit_marker_bounds_rejects_unprojectable_geometry() {
        let space = MarkerProjectionSpace {
            strip_height: 100.0,
            content_top: 0.0,
            content_bottom: 80.0,
            map_y_in_strip: 0.0,
            min_height: 2.0,
        };

        assert!(
            fit_marker_bounds(MinimapMarkerKind::Modified, 90.0, 91.0, space).is_none(),
            "markers below rendered content must not be clamped into the EOF tail"
        );
        assert!(
            fit_marker_bounds(MinimapMarkerKind::Modified, f64::NAN, 10.0, space).is_none(),
            "non-finite geometry must not revive the old full-height fallback"
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Modified,
                10.0,
                11.0,
                MarkerProjectionSpace {
                    strip_height: 0.0,
                    content_top: 0.0,
                    content_bottom: 80.0,
                    map_y_in_strip: 0.0,
                    min_height: 2.0,
                },
            )
            .is_none(),
            "unallocated marker strips should not draw semantic markers"
        );
        assert!(
            fit_marker_bounds(
                MinimapMarkerKind::Modified,
                10.0,
                11.0,
                MarkerProjectionSpace {
                    strip_height: 100.0,
                    content_top: 20.0,
                    content_bottom: 20.0,
                    map_y_in_strip: 0.0,
                    min_height: 2.0,
                },
            )
            .is_none(),
            "empty rendered content should not synthesize marker bounds"
        );
    }

    #[test]
    fn test_fit_projected_bounds_clamps_viewport_to_content_edges() {
        let space = MinimapProjectionSpace {
            target_height: 100.0,
            map_x: 0.0,
            map_y: 0.0,
            map_width: 80.0,
            content_top: 10.0,
            content_bottom: 70.0,
        };

        let above = fit_projected_bounds(
            2.0,
            40.0,
            -20.0,
            -10.0,
            space,
            4.0,
            ProjectedBoundsFit::ClampOutside,
        )
        .expect("viewport above rendered content should clamp to the top edge");
        assert_eq!(above.y, 10.0);
        assert_eq!(above.height, 4.0);

        let below = fit_projected_bounds(
            2.0,
            40.0,
            90.0,
            95.0,
            space,
            4.0,
            ProjectedBoundsFit::ClampOutside,
        )
        .expect("viewport below rendered content should clamp to the bottom edge");
        assert_eq!(below.y, 66.0);
        assert_eq!(below.height, 4.0);
    }

    #[test]
    fn test_fit_projected_bounds_can_reject_outside_content() {
        let space = MinimapProjectionSpace {
            target_height: 100.0,
            map_x: 0.0,
            map_y: 0.0,
            map_width: 80.0,
            content_top: 10.0,
            content_bottom: 70.0,
        };

        assert!(
            fit_projected_bounds(
                2.0,
                40.0,
                90.0,
                95.0,
                space,
                4.0,
                ProjectedBoundsFit::RejectOutside,
            )
            .is_none()
        );
    }

    #[test]
    fn test_marker_lane_widths_are_nested_with_two_pixel_floor() {
        assert_eq!(marker_lane_width(MinimapMarkerKind::Bookmark, 10.0), 10.0);
        assert!((marker_lane_width(MinimapMarkerKind::Search, 10.0) - 8.2).abs() < 1e-12);
        assert!((marker_lane_width(MinimapMarkerKind::Modified, 10.0) - 6.4).abs() < 1e-12);
        assert!((marker_lane_width(MinimapMarkerKind::LongLine, 10.0) - 4.6).abs() < 1e-12);
        assert_eq!(marker_lane_x(10.0, 4.6), 5.4);

        for kind in [
            MinimapMarkerKind::Bookmark,
            MinimapMarkerKind::Search,
            MinimapMarkerKind::Modified,
            MinimapMarkerKind::LongLine,
        ] {
            assert_eq!(marker_lane_width(kind, 1.0), 2.0);
        }
    }

    #[test]
    fn test_marker_rgba_palette_is_stable_for_light_and_dark_modes() {
        let expected = [
            (MinimapMarkerKind::Bookmark, false, (0.11, 0.44, 0.85, 0.95)),
            (MinimapMarkerKind::Bookmark, true, (0.39, 0.65, 0.95, 0.95)),
            (MinimapMarkerKind::Search, false, (0.95, 0.45, 0.0, 0.92)),
            (MinimapMarkerKind::Search, true, (1.0, 0.58, 0.17, 0.92)),
            (MinimapMarkerKind::Modified, false, (0.17, 0.68, 0.42, 0.92)),
            (MinimapMarkerKind::Modified, true, (0.32, 0.84, 0.55, 0.92)),
            (MinimapMarkerKind::LongLine, false, (0.88, 0.11, 0.14, 0.92)),
            (MinimapMarkerKind::LongLine, true, (1.0, 0.46, 0.45, 0.92)),
        ];

        for (kind, dark, rgba) in expected {
            assert_eq!(marker_rgba(kind, dark), rgba);
        }
    }
}
