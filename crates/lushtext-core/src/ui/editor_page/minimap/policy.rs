// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure minimap policy: thresholds, projection arithmetic, and content analysis.
//!
//! This is the minimap workflow's one `policy.rs`. Every decision here is a
//! LushText product decision about the minimap — the eight-pixel marker strip,
//! the four marker lanes, the thirteen-pixel native-slider CSS outset, the 0.20
//! wide-editor ratio, the exact 2 MiB wrapped-layout budget — expressed over
//! scalars so the module can hold no `gtk4`, `glib`, `gio`, `libadwaita`, or
//! `sourceview5` import. That purity is what keeps it inside the default
//! mutation scope, which reaches `ui/**/policy.rs` by convention rather than by
//! a hand-listed path; `make check-workflow-boundaries` fails naming the file
//! and the import if the line is ever crossed.
//!
//! ## Seam value objects
//!
//! [`MinimapProjectionSpace`] and [`MarkerProjectionSpace`] carry the coordinate
//! spaces across the boundary between the GTK gatherers in
//! `projection_execution` and the fitting arithmetic here. The minimap mixes
//! source-map widget coordinates, marker-strip coordinates, editor buffer
//! coordinates, and caller-supplied target coordinates, several of them as bare
//! `f64`; reifying the space is what makes passing one space's value where
//! another is expected a type error instead of a silent drift.
//!
//! ## Content analysis
//!
//! [`MinimapAnalysisPolicy`], [`MinimapAnalysisAccumulator`], and
//! [`MinimapAnalysisResult`] relocated here from `model/minimap_analysis.rs`:
//! the minimap is their single owning workflow. The accumulator is deliberately
//! independent of GTK iterator ownership so `analysis_execution` can drive it
//! one bounded slice at a time from a live buffer cursor without ever copying
//! the document.

use std::time::Duration;

/// Width reserved for the semantic marker strip painted over the map edge.
///
/// Eight pixels is enough to show four stacked marker lanes while still
/// leaving almost all of the overview map readable underneath.
pub(super) const MINIMAP_MARKER_STRIP_WIDTH: i32 = 8;
/// Minimum top inset for the minimap text projection.
///
/// `GtkSourceMap` paints with a tiny font and sits inside a shell that owns its
/// own border/padding. Keeping a real text margin here prevents the first map
/// line from painting flush against a clipped top edge after width-only reflow.
pub(super) const MINIMAP_TOP_CONTENT_MARGIN: i32 = 5;
/// Source-map/editor document-height ratio that identifies the wide editor state.
///
/// In that state the editor has stopped wrapping the fixture's long lines, so
/// GtkSourceMap's private slider rasterizes its top edge one row above the
/// sidebar-visible wrapped state unless we mirror GNOME Text Editor's fixed map
/// geometry and add a small slider-only correction. This is intentionally a
/// binary one-pixel correction: if future visual fixtures flap at this boundary,
/// use hysteresis or a stepped offset ladder rather than nudging the threshold.
pub(super) const MINIMAP_WIDE_EDITOR_RATIO_THRESHOLD: f64 = 0.20;
/// CSS class for the wide-editor native slider top-edge correction.
pub(super) const MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS: &str =
    "minimap-wide-editor-slider-offset";
/// Debounce for minimap marker refresh work.
///
/// This coalesces bursts from buffer edits, search updates, and resize-driven
/// adjustment changes so the main thread does not rescan the document on every
/// single notify signal.
pub(super) const MINIMAP_REFRESH_DEBOUNCE: Duration = Duration::from_millis(80);
/// Line length that triggers a long-line warning marker in the minimap.
///
/// The proposal called out 120 characters explicitly, so the marker layer uses
/// the same threshold instead of trying to infer it from unrelated formatting settings.
pub(super) const MINIMAP_LONG_LINE_WARNING_THRESHOLD: usize = 120;
/// Cap on search matches converted into minimap markers during one refresh.
///
/// The minimap only needs a spatial hint, not every exact hit, so we stop once
/// the marker strip is already dense enough to communicate "many matches here".
pub(super) const MINIMAP_SEARCH_MATCH_CAP: usize = 2_000;
/// Maximum document size before wrapped minimap layout needs a long-line check.
///
/// Ordinary prose and source files stay below this budget, while multi-megabyte
/// minified files can make the narrow source map build a very large visual-line
/// cache when it mirrors editor word wrap.
pub(super) const MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET: u64 = 2 * 1024 * 1024;
/// Long logical lines above this size make wrapped source-map layout expensive.
///
/// The threshold is high enough for normal code/log lines but catches minified
/// JSON or generated files before a 64-160px minimap explodes one line into
/// thousands of visual rows.
pub(super) const MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET: usize = 8_000;
/// Live GTK-buffer characters inspected by one minimap analysis turn.
pub(super) const MINIMAP_ANALYSIS_CHARS_PER_SLICE: usize = 32 * 1024;
/// Maximum retained long-line identities shared with marker projection.
pub(super) const MINIMAP_LONG_LINE_MARK_CAP: usize = 2_000;
/// Minimum visual height for a semantic marker after projection.
///
/// Collapsed or sub-pixel source-map lines still need to be discoverable, but
/// this height is clamped to the rendered document content so it cannot leak
/// into the blank EOF overscroll tail.
pub(super) const MINIMAP_MARKER_MIN_HEIGHT: f64 = 2.0;
/// Minimum height for the viewport highlight when the projected visible area is tiny.
///
/// Two logical pixels keeps edge anchors detectable on very short documents
/// without making the native slider look thicker than GTK's own effect.
pub(super) const MINIMAP_VIEWPORT_MIN_HEIGHT: f64 = 2.0;
/// Horizontal CSS outset used by the native `GtkSourceMap` viewport slider.
///
/// This mirrors `.minimap-view slider { margin-left/right: -13px; }`. The same
/// value provides the shell's side gutters as map widget margins, so the
/// slider's outset paints inside the overlay content box and the reflow freeze
/// snapshot can cover the full rendered effect users actually see.
pub(super) const MINIMAP_VIEWPORT_HORIZONTAL_OUTSET: i32 = 13;
/// Debounce that detects the end of a width-reflow burst.
///
/// Sidebar show/hide animates the editor width on every frame for roughly
/// 250ms. Wrapped document heights are asynchronous estimates while that
/// happens, so any minimap margin or scroll repair performed mid-burst reads
/// transient values and paints the native slider a few pixels off. The settle
/// delay must exceed the gap between animation frames (16-33ms) by a wide
/// margin while staying short enough that the post-reflow repair feels
/// immediate once the width stops changing.
pub(super) const MINIMAP_REFLOW_SETTLE_DEBOUNCE: Duration = Duration::from_millis(150);
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
pub(super) const MINIMAP_REFLOW_REVEAL_DELAY: Duration = Duration::from_millis(800);
/// Maximum live source marks used for the modified-line minimap layer.
pub(super) const MINIMAP_MODIFIED_LINE_MARK_CAP: usize = 2_000;

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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct MinimapAnalysisRequest {
    pub(super) wrapped_layout: bool,
    pub(super) long_line_markers: bool,
}

impl MinimapAnalysisRequest {
    pub(super) fn required(self) -> bool {
        self.wrapped_layout || self.long_line_markers
    }
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

/// Compute the source-map/editor ratio, returning `None` for unusable geometry.
pub(super) fn source_map_editor_height_ratio_from_heights(
    editor_document_height: i32,
    source_map_document_height: i32,
) -> Option<f64> {
    if editor_document_height <= 0 || source_map_document_height <= 0 {
        return None;
    }
    Some(f64::from(source_map_document_height) / f64::from(editor_document_height))
}

/// Return a finite adjustment's absolute distance from its lower bound.
///
/// GTK can expose transitional non-finite values while a newly bound source
/// map is allocating. Those values must never be passed back to `set_value`.
pub(super) fn finite_adjustment_distance_from_lower(value: f64, lower: f64) -> Option<f64> {
    if !value.is_finite() || !lower.is_finite() {
        return None;
    }
    Some((value - lower).abs())
}

/// Return the child page size that represents a fitting, non-scrollable source.
pub(super) fn fitting_source_map_page_size(
    source_upper: f64,
    source_page_size: f64,
    map_upper: f64,
    map_page_size: f64,
) -> Option<f64> {
    if !source_upper.is_finite()
        || !source_page_size.is_finite()
        || !map_upper.is_finite()
        || !map_page_size.is_finite()
        || (source_upper - source_page_size).abs() > f64::EPSILON
        || map_page_size >= map_upper
    {
        return None;
    }
    Some(map_upper)
}

/// Choose the CSS class that compensates the native slider at the wide-editor threshold.
pub(super) fn wide_editor_slider_offset_class(ratio: Option<f64>) -> Option<&'static str> {
    ratio
        .is_some_and(|ratio| ratio.is_finite() && ratio > MINIMAP_WIDE_EDITOR_RATIO_THRESHOLD)
        .then_some(MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS)
}

/// Derive the rendered document height from one line's vertical span.
///
/// The last line's top offset plus its own height is the document height a
/// native-slider ratio needs. A non-positive result means the caller's geometry
/// is not yet usable, which is a rejection rather than a zero.
pub(super) fn document_height_from_line_span(line_y: i32, line_height: i32) -> Option<i32> {
    let height = line_y.saturating_add(line_height.max(0));
    (height > 0).then_some(height)
}

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

/// Public-geometry inputs used to mirror `GtkSourceMap`'s native slider formula.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct NativeSliderEstimateInput {
    /// Source-map left edge in target coordinates.
    pub(super) map_x: f64,
    /// Source-map top edge in target coordinates.
    pub(super) map_y: f64,
    /// Source-map allocation width before CSS slider outset.
    pub(super) map_width: f64,
    /// Editor visible rect y, including GTK's negative top-margin value at line one.
    pub(super) editor_visible_y: i32,
    /// Editor visible rect height.
    pub(super) editor_visible_height: i32,
    /// Editor document height used by the native slider ratio.
    pub(super) editor_document_height: i32,
    /// Source-map visible rect y that GTK subtracts while drawing the slider.
    pub(super) source_map_visible_y: i32,
    /// Source-map document height used by the native slider ratio.
    pub(super) source_map_document_height: i32,
    /// Left CSS border removed from the usable slider width.
    pub(super) border_left: i32,
    /// Right CSS border removed from the usable slider width.
    pub(super) border_right: i32,
}

/// Mirror GtkSourceMap's private slider math with public geometry only.
///
/// This estimate explains where GTK should draw the native slider after
/// applying source-map scroll offset and CSS outset. It is diagnostic;
/// screenshot pixel anchors remain the authority for rendered correctness.
pub(super) fn native_slider_estimate_from_inputs(
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
pub(super) fn fit_native_slider_to_source_map_bounds(
    raw: MinimapProjectedBounds,
    source_map_bounds: MinimapProjectedBounds,
) -> Option<MinimapProjectedBounds> {
    if !raw.x.is_finite()
        || !raw.y.is_finite()
        || !raw.width.is_finite()
        || !raw.height.is_finite()
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

pub(super) fn wrapped_layout_analysis_required_for_bytes(
    wrapping: bool,
    estimated_bytes: u64,
) -> bool {
    wrapping && estimated_bytes > MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET
}

/// Pure availability inputs gathered from GTK state by `current_availability`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MinimapAvailabilityPolicy {
    pub(super) focus_suppressed: bool,
    pub(super) preference_enabled: bool,
    pub(super) evicted: bool,
    pub(super) syntax_enabled: bool,
    pub(super) wrapped_layout_too_large: bool,
}

pub(super) fn minimap_availability_for_policy(
    policy: MinimapAvailabilityPolicy,
) -> MinimapAvailability {
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

#[cfg(test)]
fn wrapped_layout_budget_exceeded(estimated_size: u64, has_extreme_line: bool) -> bool {
    estimated_size > MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET && has_extreme_line
}

#[cfg(test)]
fn text_exceeds_line_char_budget(text: &str, line_char_budget: usize) -> bool {
    let policy = MinimapAnalysisPolicy {
        warning_line_chars: usize::MAX,
        wrapped_line_chars: line_char_budget,
        marker_limit: 0,
    };
    let mut analysis = MinimapAnalysisAccumulator::new(policy, false);
    analysis.inspect_slice(text.chars(), usize::MAX);
    analysis.wrapped_layout_too_large()
}

pub(super) fn modified_line_mark_samples(start_line: u32, end_line: u32, cap: usize) -> Vec<u32> {
    if cap == 0 || start_line > end_line {
        return Vec::new();
    }
    let total = u64::from(end_line - start_line) + 1;
    let cap = u64::try_from(cap).unwrap_or(u64::MAX);
    if total <= cap {
        return (start_line..=end_line).collect();
    }

    let sample_count = cap.max(1);
    let span = total - 1;
    let denominator = sample_count.saturating_sub(1).max(1);
    let mut samples = Vec::with_capacity(usize::try_from(sample_count).unwrap_or(usize::MAX));
    for index in 0..sample_count {
        let offset = (index * span) / denominator;
        let line = start_line.saturating_add(u32::try_from(offset).unwrap_or(u32::MAX));
        if samples.last().copied() != Some(line) {
            samples.push(line);
        }
    }
    samples
}

#[cfg(test)]
fn long_line_warning_lines(text: &str) -> Vec<u32> {
    let policy = MinimapAnalysisPolicy {
        warning_line_chars: MINIMAP_LONG_LINE_WARNING_THRESHOLD,
        wrapped_line_chars: usize::MAX,
        marker_limit: usize::MAX,
    };
    let mut analysis = MinimapAnalysisAccumulator::new(policy, true);
    analysis.inspect_slice(text.chars(), usize::MAX);
    analysis.finish().long_line_lines
}

pub(super) fn markers_from_lines(
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

/// Shared coordinate-space contract for minimap projections.
///
/// `GtkSourceMap` reports line positions in its own buffer/widget coordinates,
/// while Automation1 crops need a caller-supplied target widget coordinate
/// space. This struct keeps those spaces explicit and bounds every projection
/// to the text that the minimap actually renders, excluding the dynamic EOF tail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MinimapProjectionSpace {
    /// Height of the target widget in the same coordinates as returned bounds.
    pub(super) target_height: f64,
    /// Source-map left edge in target widget coordinates.
    pub(super) map_x: f64,
    /// Source-map top edge in target widget coordinates.
    pub(super) map_y: f64,
    /// Source-map width before any native-slider CSS outset is applied.
    pub(super) map_width: f64,
    /// Top of the first rendered minimap line in target widget coordinates.
    pub(super) content_top: f64,
    /// Bottom of the last rendered minimap line in target widget coordinates.
    pub(super) content_bottom: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MarkerProjectionSpace {
    pub(super) strip_height: f64,
    pub(super) content_top: f64,
    pub(super) content_bottom: f64,
    pub(super) map_y_in_strip: f64,
    pub(super) min_height: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProjectedBoundsFit {
    /// Reject projections that land outside rendered minimap text.
    RejectOutside,
    /// Clamp projections back to rendered minimap text when the visual effect remains visible.
    ClampOutside,
}

pub(super) fn line_top_in_target(
    map_y_in_target: f64,
    line_y: i32,
    widget_y_for_buffer_y: impl FnOnce(i32) -> i32,
) -> f64 {
    target_y_from_widget_y(map_y_in_target, widget_y_for_buffer_y(line_y))
}

pub(super) fn line_bottom_in_target(
    map_y_in_target: f64,
    line_y: i32,
    line_height: i32,
    widget_y_for_buffer_y: impl FnOnce(i32) -> i32,
) -> f64 {
    let bottom_y = line_y.saturating_add(line_height.max(0));
    target_y_from_widget_y(map_y_in_target, widget_y_for_buffer_y(bottom_y))
}

fn target_y_from_widget_y(map_y_in_target: f64, widget_y: i32) -> f64 {
    // `buffer_to_window_coords` returns y relative to the source-map widget;
    // add the map's target-relative top edge to produce crop/anchor coordinates.
    map_y_in_target + f64::from(widget_y)
}

/// Grow a projected span to the minimum visible height, without leaving content.
///
/// Both minimap fitting functions need the same expansion, and they needed it
/// **identically** — this was one verbatim-duplicated block before it was
/// extracted. Keeping it in one place is also what makes its three genuinely
/// equivalent boundary mutants excludable by name: a symbol-scoped exclusion on
/// either caller would have swallowed caught mutants from their unrelated
/// reject/accept guards.
///
/// Expansion is centred on the existing span, then pushed back inside both
/// edges, then clamped. That final clamp is what keeps a minimum larger than the
/// rendered content span from bleeding into the blank EOF overscroll: such a
/// span ends up exactly as tall as the content, never taller.
fn expanded_to_min_height(
    top: f64,
    bottom: f64,
    lower: f64,
    upper: f64,
    min_height: f64,
) -> (f64, f64) {
    // Deliberately **not** capped at the content span. An earlier revision wrote
    // `.min(upper - lower)` here, and survivor triage showed that cap is dead
    // code: the trailing clamp already bounds the result, and whenever the cap
    // would bind, the expansion fills the whole span either way. Removing it
    // deletes an equivalent mutant instead of excluding one.
    let target_height = min_height.max(0.0);
    if bottom - top >= target_height {
        return (top, bottom);
    }

    // midpoint avoids `(top + bottom) / 2.0` overflow before the later clamp.
    let center = f64::midpoint(top, bottom).clamp(lower, upper);
    let mut top = center - (target_height / 2.0);
    let mut bottom = center + (target_height / 2.0);

    if top < lower {
        bottom += lower - top;
        top = lower;
    }
    if bottom > upper {
        top -= bottom - upper;
        bottom = upper;
    }

    (top.max(lower), bottom.min(upper))
}

pub(super) fn fit_marker_bounds(
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

    let top = raw_top.max(lower);
    let bottom = raw_bottom.min(upper).max(top);

    let (top, bottom) = expanded_to_min_height(top, bottom, lower, upper, space.min_height);

    (bottom > top).then_some(MinimapMarkerBounds { kind, top, bottom })
}

/// Fit a projected minimap rectangle into rendered content bounds.
///
/// Viewport diagnostics use `ClampOutside` because GTK keeps the native slider
/// visible at document edges. Content-row diagnostics use `RejectOutside`
/// because fabricating a row would let screenshots pass without rendered text.
/// Small projections expand around their center to remain pixel-detectable.
pub(super) fn fit_projected_bounds(
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

    let top = raw_top.clamp(lower, upper);
    let bottom = raw_bottom.clamp(lower, upper);

    let (top, bottom) = expanded_to_min_height(top, bottom, lower, upper, min_height);

    (bottom > top).then_some(MinimapProjectedBounds {
        x,
        y: top,
        width,
        height: bottom - top,
    })
}

pub(super) fn marker_lane_width(kind: MinimapMarkerKind, total_width: f64) -> f64 {
    let ratio = match kind {
        MinimapMarkerKind::Bookmark => 1.0,
        MinimapMarkerKind::Search => 0.82,
        MinimapMarkerKind::Modified => 0.64,
        MinimapMarkerKind::LongLine => 0.46,
    };
    (total_width * ratio).max(2.0)
}

pub(super) fn marker_lane_x(total_width: f64, lane_width: f64) -> f64 {
    total_width - lane_width
}

pub(super) fn marker_rgba(kind: MinimapMarkerKind, dark: bool) -> (f64, f64, f64, f64) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimap_policy_constants_are_stable() {
        assert_eq!(MINIMAP_MARKER_STRIP_WIDTH, 8);
        assert_eq!(MINIMAP_REFRESH_DEBOUNCE, Duration::from_millis(80));
        assert_eq!(MINIMAP_LONG_LINE_WARNING_THRESHOLD, 120);
        assert_eq!(MINIMAP_SEARCH_MATCH_CAP, 2_000);
        assert_eq!(MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET, 2 * 1024 * 1024);
        assert_eq!(MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET, 8_000);
        assert_eq!(MINIMAP_ANALYSIS_CHARS_PER_SLICE, 32 * 1024);
        assert_eq!(MINIMAP_LONG_LINE_MARK_CAP, 2_000);
        assert_eq!(MINIMAP_MARKER_MIN_HEIGHT, 2.0);
        assert_eq!(MINIMAP_TOP_CONTENT_MARGIN, 5);
        assert_eq!(MINIMAP_WIDE_EDITOR_RATIO_THRESHOLD, 0.20);
        assert_eq!(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET, 13);
        assert_eq!(MINIMAP_REFLOW_SETTLE_DEBOUNCE, Duration::from_millis(150));
        assert_eq!(MINIMAP_REFLOW_REVEAL_DELAY, Duration::from_millis(800));
        // Owned by `projection_execution`, which creates the marks; asserted
        // here so every minimap constant's stability lives in one test.
        assert_eq!(
            super::super::projection_execution::MINIMAP_MODIFIED_MARK_CATEGORY,
            "lushtext-minimap-modified"
        );
        assert_eq!(MINIMAP_MODIFIED_LINE_MARK_CAP, 2_000);
    }

    #[test]
    fn test_modified_line_mark_samples_cover_large_ranges_with_a_cap() {
        let samples = modified_line_mark_samples(0, 9_999, 2_000);

        assert_eq!(samples.len(), 2_000);
        assert_eq!(samples.first().copied(), Some(0));
        assert_eq!(samples.last().copied(), Some(9_999));
        assert!(samples.windows(2).all(|window| window[0] < window[1]));
    }

    #[test]
    fn test_modified_line_mark_samples_reject_empty_or_reversed_ranges() {
        assert!(modified_line_mark_samples(4, 4, 0).is_empty());
        assert!(modified_line_mark_samples(5, 4, 1).is_empty());
        assert_eq!(modified_line_mark_samples(7, 7, 1), vec![7]);
    }

    #[test]
    fn test_modified_line_mark_samples_preserve_full_small_ranges_and_sample_edges() {
        assert_eq!(modified_line_mark_samples(5, 7, 10), vec![5, 6, 7]);
        assert_eq!(modified_line_mark_samples(10, 14, 3), vec![10, 12, 14]);
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
    fn test_adjustment_distance_rejects_nonfinite_transition_values() {
        assert_eq!(finite_adjustment_distance_from_lower(4.5, 1.0), Some(3.5));
        assert_eq!(finite_adjustment_distance_from_lower(f64::NAN, 0.0), None);
        assert_eq!(finite_adjustment_distance_from_lower(0.0, f64::NAN), None);
        assert_eq!(
            finite_adjustment_distance_from_lower(f64::INFINITY, 0.0),
            None
        );
    }

    #[test]
    fn test_fitting_source_map_page_size_closes_only_zero_source_range() {
        assert_eq!(
            fitting_source_map_page_size(733.0, 733.0, 645.0, 0.0),
            Some(645.0)
        );
        assert_eq!(fitting_source_map_page_size(900.0, 733.0, 645.0, 0.0), None);
        assert_eq!(
            fitting_source_map_page_size(733.0, 733.0, 645.0, 645.0),
            None
        );
        assert_eq!(
            fitting_source_map_page_size(f64::NAN, 733.0, 645.0, 0.0),
            None
        );
    }

    /// Survivor triage: this predicate came out of the GTK adapter with no unit
    /// assertion of its own, so all three of its mutants lived. Cover the full
    /// truth table — `-> true` dies on the neither case, `-> false` dies on
    /// either single case, and `|| -> &&` dies on both single cases.
    #[test]
    fn test_analysis_is_required_when_either_consumer_asks_for_it() {
        let request = |wrapped_layout, long_line_markers| MinimapAnalysisRequest {
            wrapped_layout,
            long_line_markers,
        };

        assert!(
            !request(false, false).required(),
            "neither wrapped layout nor markers wants a scan"
        );
        assert!(
            request(true, false).required(),
            "wrapped-layout eligibility alone requires the scan"
        );
        assert!(
            request(false, true).required(),
            "long-line markers alone require the scan"
        );
        assert!(request(true, true).required());
    }

    /// Survivor triage: the existing test proved one non-finite input and the
    /// two clearly-fitting cases, which left the later guards and the strict
    /// epsilon comparison unexercised. Each input is made non-finite on its own
    /// so a `||` collapsed to `&&` cannot short-circuit past it.
    #[test]
    fn test_fitting_source_map_page_size_rejects_each_nonfinite_input() {
        for (source_upper, source_page_size, map_upper, map_page_size) in [
            (f64::NAN, 733.0, 645.0, 0.0),
            (733.0, f64::NAN, 645.0, 0.0),
            (733.0, 733.0, f64::INFINITY, 0.0),
            (733.0, 733.0, 645.0, f64::NAN),
        ] {
            assert_eq!(
                fitting_source_map_page_size(
                    source_upper,
                    source_page_size,
                    map_upper,
                    map_page_size
                ),
                None,
                "one non-finite input must reject on its own"
            );
        }
    }

    /// Survivor triage: the source-range comparison is **strictly** greater than
    /// `EPSILON`, so a difference of exactly one epsilon still describes a
    /// fitting, non-scrollable source. `>=` would reject that legitimate state
    /// and leave `GtkSourceMap` dividing by a zero range.
    #[test]
    fn test_fitting_source_map_page_size_accepts_an_exact_epsilon_difference() {
        assert_eq!(
            fitting_source_map_page_size(1.0 + f64::EPSILON, 1.0, 645.0, 0.0),
            Some(645.0)
        );
        assert_eq!(
            fitting_source_map_page_size(1.0 + (f64::EPSILON * 4.0), 1.0, 645.0, 0.0),
            None,
            "a difference above one epsilon is a scrollable source"
        );
        assert_eq!(
            fitting_source_map_page_size(733.0, 733.0, 645.0, 644.0),
            Some(645.0),
            "a map page size strictly below its upper still needs closing"
        );
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
    fn test_projected_bounds_bottom_uses_top_plus_height() {
        let bounds = MinimapProjectedBounds {
            x: 4.0,
            y: 12.5,
            width: 64.0,
            height: 7.25,
        };

        assert_eq!(bounds.bottom(), 19.75);
    }

    #[test]
    fn test_native_projection_source_label_is_stable() {
        assert_eq!(
            MinimapNativeProjectionSource::UpstreamVisibleRectEstimate.as_str(),
            "upstream-visible-rect-estimate"
        );
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
        assert_eq!(settled.height, 320.0);
        assert_eq!(settled.x, 87.0);
        assert_eq!(settled.width, 120.0);

        let mid_document = native_slider_estimate_from_inputs(NativeSliderEstimateInput {
            editor_visible_y: 330,
            editor_visible_height: 330,
            ..input
        })
        .expect("mid-document native slider should preserve scaled top and height");
        assert_eq!(mid_document.y, 212.0);
        assert_eq!(mid_document.height, 160.0);

        let bordered = native_slider_estimate_from_inputs(NativeSliderEstimateInput {
            border_left: 2,
            border_right: 3,
            ..input
        })
        .expect("native slider should account for CSS borders");
        assert_eq!(bordered.x, 89.0);
        assert_eq!(bordered.width, 115.0);
    }

    #[test]
    fn test_native_slider_estimate_rejects_each_unusable_input() {
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

        assert!(native_slider_estimate_from_inputs(input).is_some());
        assert!(
            native_slider_estimate_from_inputs(NativeSliderEstimateInput {
                map_x: f64::NAN,
                ..input
            })
            .is_none()
        );
        assert!(
            native_slider_estimate_from_inputs(NativeSliderEstimateInput {
                map_y: f64::NAN,
                ..input
            })
            .is_none()
        );
        assert!(
            native_slider_estimate_from_inputs(NativeSliderEstimateInput {
                map_width: 0.0,
                ..input
            })
            .is_none()
        );
        assert!(
            native_slider_estimate_from_inputs(NativeSliderEstimateInput {
                editor_visible_height: 0,
                ..input
            })
            .is_none()
        );
        assert!(
            native_slider_estimate_from_inputs(NativeSliderEstimateInput {
                editor_document_height: 0,
                ..input
            })
            .is_none()
        );
        assert!(
            native_slider_estimate_from_inputs(NativeSliderEstimateInput {
                source_map_document_height: 0,
                ..input
            })
            .is_none()
        );
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

        let above = MinimapProjectedBounds { y: -20.0, ..raw };
        let fitted_above = fit_native_slider_to_source_map_bounds(above, source_map_bounds)
            .expect("offscreen native estimate should fit to the top map edge");
        assert_eq!(fitted_above.y, source_map_bounds.y);
        assert_eq!(fitted_above.height, above.height);
    }

    #[test]
    fn test_native_slider_visible_bounds_rejects_unusable_geometry() {
        let raw = MinimapProjectedBounds {
            x: -13.0,
            y: 10.0,
            width: 120.0,
            height: 184.0,
        };
        let source_map_bounds = MinimapProjectedBounds {
            x: 0.0,
            y: 0.0,
            width: 94.0,
            height: 664.0,
        };

        assert!(fit_native_slider_to_source_map_bounds(raw, source_map_bounds).is_some());
        assert!(
            fit_native_slider_to_source_map_bounds(
                MinimapProjectedBounds { width: 0.0, ..raw },
                source_map_bounds,
            )
            .is_none()
        );
        assert!(
            fit_native_slider_to_source_map_bounds(
                MinimapProjectedBounds { height: 0.0, ..raw },
                source_map_bounds,
            )
            .is_none()
        );
        assert!(
            fit_native_slider_to_source_map_bounds(
                MinimapProjectedBounds { x: f64::NAN, ..raw },
                source_map_bounds,
            )
            .is_none()
        );
        assert!(
            fit_native_slider_to_source_map_bounds(
                MinimapProjectedBounds { y: f64::NAN, ..raw },
                source_map_bounds,
            )
            .is_none()
        );
        assert!(
            fit_native_slider_to_source_map_bounds(
                MinimapProjectedBounds {
                    width: f64::NAN,
                    ..raw
                },
                source_map_bounds,
            )
            .is_none()
        );
        assert!(
            fit_native_slider_to_source_map_bounds(
                MinimapProjectedBounds {
                    height: f64::NAN,
                    ..raw
                },
                source_map_bounds,
            )
            .is_none()
        );
        assert!(
            fit_native_slider_to_source_map_bounds(
                raw,
                MinimapProjectedBounds {
                    height: 0.0,
                    ..source_map_bounds
                },
            )
            .is_none()
        );
        assert!(
            fit_native_slider_to_source_map_bounds(
                raw,
                MinimapProjectedBounds {
                    y: f64::NAN,
                    ..source_map_bounds
                },
            )
            .is_none()
        );
        assert!(
            fit_native_slider_to_source_map_bounds(
                raw,
                MinimapProjectedBounds {
                    height: f64::NAN,
                    ..source_map_bounds
                },
            )
            .is_none()
        );
    }

    #[test]
    fn test_document_height_from_line_span_uses_y_plus_nonnegative_height() {
        assert_eq!(document_height_from_line_span(12, 8), Some(20));
        assert_eq!(document_height_from_line_span(12, -8), Some(12));
        assert_eq!(document_height_from_line_span(0, 0), None);
        assert_eq!(document_height_from_line_span(-1, 1), None);
    }

    #[test]
    fn test_gtk_f64_to_milli_serializes_finite_values_and_suppresses_nonfinite() {
        assert_eq!(gtk_f64_to_milli(10.49), 10_490);
        assert_eq!(gtk_f64_to_milli(12.25), 12_250);
        assert_eq!(gtk_f64_to_milli(-1.25), -1_250);
        assert_eq!(gtk_f64_to_milli(f64::NAN), 0);
        assert_eq!(gtk_f64_to_milli(f64::INFINITY), 0);
    }

    #[test]
    fn test_line_top_in_target_offsets_projected_source_map_line() {
        let projected = line_top_in_target(12.5, 48, |buffer_y| {
            assert_eq!(buffer_y, 48);
            30
        });

        assert_eq!(projected, 42.5);
    }

    #[test]
    fn test_line_bottom_in_target_uses_nonnegative_line_height_before_projection() {
        let projected = line_bottom_in_target(12.5, 48, 7, |buffer_y| {
            assert_eq!(buffer_y, 55);
            30
        });
        let collapsed = line_bottom_in_target(12.5, 48, -7, |buffer_y| {
            assert_eq!(buffer_y, 48);
            30
        });

        assert_eq!(projected, 42.5);
        assert_eq!(collapsed, 42.5);
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
    fn test_wrapped_layout_analysis_uses_strict_scalar_byte_threshold() {
        assert!(!wrapped_layout_analysis_required_for_bytes(false, u64::MAX));
        assert!(!wrapped_layout_analysis_required_for_bytes(
            true,
            MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET
        ));
        assert!(wrapped_layout_analysis_required_for_bytes(
            true,
            MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET + 1
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
    fn test_fit_projected_bounds_rejects_each_unusable_input() {
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
                20.0,
                30.0,
                space,
                4.0,
                ProjectedBoundsFit::ClampOutside,
            )
            .is_some()
        );

        let cases = [
            (f64::NAN, 40.0, 20.0, 30.0, space),
            (2.0, f64::NAN, 20.0, 30.0, space),
            (2.0, 40.0, f64::NAN, 30.0, space),
            (2.0, 40.0, 20.0, f64::NAN, space),
            (
                2.0,
                40.0,
                20.0,
                30.0,
                MinimapProjectionSpace {
                    target_height: f64::NAN,
                    ..space
                },
            ),
            (
                2.0,
                40.0,
                20.0,
                30.0,
                MinimapProjectionSpace {
                    content_top: f64::NAN,
                    ..space
                },
            ),
            (
                2.0,
                40.0,
                20.0,
                30.0,
                MinimapProjectionSpace {
                    content_bottom: f64::NAN,
                    ..space
                },
            ),
            (2.0, 0.0, 20.0, 30.0, space),
            (
                2.0,
                40.0,
                20.0,
                30.0,
                MinimapProjectionSpace {
                    target_height: 0.0,
                    ..space
                },
            ),
            (
                2.0,
                40.0,
                20.0,
                30.0,
                MinimapProjectionSpace {
                    content_top: 70.0,
                    content_bottom: 10.0,
                    ..space
                },
            ),
        ];

        for (x, width, raw_top, raw_bottom, case_space) in cases {
            assert!(
                fit_projected_bounds(
                    x,
                    width,
                    raw_top,
                    raw_bottom,
                    case_space,
                    4.0,
                    ProjectedBoundsFit::ClampOutside,
                )
                .is_none()
            );
        }

        // Infinities can otherwise clamp into plausible-looking bounds, so
        // keep them separate from the NaN cases that may collapse naturally.
        let infinite_cases = [
            ("x", f64::INFINITY, 40.0, 20.0, 30.0, space),
            ("width", 2.0, f64::INFINITY, 20.0, 30.0, space),
            ("raw_top", 2.0, 40.0, f64::NEG_INFINITY, 30.0, space),
            ("raw_bottom", 2.0, 40.0, 20.0, f64::INFINITY, space),
            (
                "target_height",
                2.0,
                40.0,
                20.0,
                30.0,
                MinimapProjectionSpace {
                    target_height: f64::INFINITY,
                    ..space
                },
            ),
            (
                "content_top",
                2.0,
                40.0,
                20.0,
                30.0,
                MinimapProjectionSpace {
                    content_top: f64::NEG_INFINITY,
                    ..space
                },
            ),
            (
                "content_bottom",
                2.0,
                40.0,
                20.0,
                30.0,
                MinimapProjectionSpace {
                    content_bottom: f64::INFINITY,
                    ..space
                },
            ),
        ];

        for (label, x, width, raw_top, raw_bottom, case_space) in infinite_cases {
            assert!(
                fit_projected_bounds(
                    x,
                    width,
                    raw_top,
                    raw_bottom,
                    case_space,
                    4.0,
                    ProjectedBoundsFit::ClampOutside,
                )
                .is_none(),
                "{label} infinity should be rejected before projection fitting"
            );
        }
    }

    #[test]
    fn test_fit_projected_bounds_rejects_outside_but_keeps_touching_edges() {
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
                0.0,
                9.9,
                space,
                4.0,
                ProjectedBoundsFit::RejectOutside,
            )
            .is_none()
        );
        assert!(
            fit_projected_bounds(
                2.0,
                40.0,
                70.1,
                80.0,
                space,
                4.0,
                ProjectedBoundsFit::RejectOutside,
            )
            .is_none()
        );

        let top_edge = fit_projected_bounds(
            2.0,
            40.0,
            0.0,
            10.0,
            space,
            4.0,
            ProjectedBoundsFit::RejectOutside,
        )
        .expect("touching top content edge should remain projectable");
        assert_eq!(top_edge.y, 10.0);
        assert_eq!(top_edge.height, 4.0);

        let bottom_edge = fit_projected_bounds(
            2.0,
            40.0,
            70.0,
            80.0,
            space,
            4.0,
            ProjectedBoundsFit::RejectOutside,
        )
        .expect("touching bottom content edge should remain projectable");
        assert_eq!(bottom_edge.y, 66.0);
        assert_eq!(bottom_edge.height, 4.0);
    }

    #[test]
    fn test_fit_projected_bounds_min_height_uses_content_span_cap() {
        let space = MinimapProjectionSpace {
            target_height: 100.0,
            map_x: 0.0,
            map_y: 0.0,
            map_width: 80.0,
            content_top: 10.0,
            content_bottom: 70.0,
        };

        let full_height = fit_projected_bounds(
            2.0,
            40.0,
            30.0,
            31.0,
            space,
            100.0,
            ProjectedBoundsFit::ClampOutside,
        )
        .expect("minimum height should cap at rendered content span");
        assert_eq!(full_height.y, 10.0);
        assert_eq!(full_height.height, 60.0);

        let tall_enough = fit_projected_bounds(
            2.0,
            40.0,
            20.0,
            40.0,
            space,
            4.0,
            ProjectedBoundsFit::ClampOutside,
        )
        .expect("already tall projection should stay unchanged");
        assert_eq!(tall_enough.y, 20.0);
        assert_eq!(tall_enough.height, 20.0);

        let reversed_edges = fit_projected_bounds(
            2.0,
            40.0,
            40.0,
            20.0,
            space,
            4.0,
            ProjectedBoundsFit::ClampOutside,
        )
        .expect("reversed raw edges should be normalized before fitting");
        assert_eq!(reversed_edges.y, 20.0);
        assert_eq!(reversed_edges.height, 20.0);

        assert!(
            fit_projected_bounds(
                2.0,
                40.0,
                30.0,
                30.0,
                space,
                0.0,
                ProjectedBoundsFit::ClampOutside,
            )
            .is_none(),
            "zero-height projection with no minimum stays invisible"
        );
    }

    /// Survivor triage: the content-span cap is `upper - lower`, and the four
    /// stale `line:column` exclusions had stopped protecting it. The previous cap
    /// test used `content_top = 10.0` with a 60px span, where `upper - lower` and
    /// `upper + lower` both exceed the minimum, so the mutant survived. A span
    /// **smaller** than the minimum with a non-zero lower edge separates them:
    /// the real cap is 1px, the mutated cap would be 121px.
    #[test]
    fn test_fit_projected_bounds_span_cap_subtracts_rather_than_adds_the_content_edges() {
        let space = MinimapProjectionSpace {
            target_height: 100.0,
            map_x: 0.0,
            map_y: 0.0,
            map_width: 80.0,
            content_top: 60.0,
            content_bottom: 61.0,
        };

        let capped = fit_projected_bounds(
            2.0,
            40.0,
            60.0,
            60.0,
            space,
            8.0,
            ProjectedBoundsFit::ClampOutside,
        )
        .expect("a one-pixel content span is still projectable");
        assert_eq!(
            capped.height, 1.0,
            "the minimum height must cap at the content span, never grow past it"
        );
        assert_eq!(capped.y, 60.0);
        assert!(
            capped.bottom() <= 61.0,
            "an expanded projection must not leave rendered content"
        );
    }

    /// Direct coverage of the extracted shared expansion, so its arithmetic is
    /// asserted at the definition rather than only through two callers.
    #[test]
    fn test_expanded_to_min_height_centres_then_pushes_inside_both_edges() {
        // Already tall enough: returned unchanged, no centring.
        assert_eq!(
            expanded_to_min_height(20.0, 30.0, 0.0, 100.0, 4.0),
            (20.0, 30.0)
        );

        // Collapsed span inside the content: expands around its own centre.
        assert_eq!(
            expanded_to_min_height(50.0, 50.0, 0.0, 100.0, 4.0),
            (48.0, 52.0)
        );

        // Against the top edge: the whole expansion is pushed down, not clipped.
        assert_eq!(
            expanded_to_min_height(0.0, 0.0, 0.0, 100.0, 4.0),
            (0.0, 4.0)
        );

        // Against the bottom edge: pushed up by the same rule.
        assert_eq!(
            expanded_to_min_height(100.0, 100.0, 0.0, 100.0, 4.0),
            (96.0, 100.0)
        );

        // Content thinner than the minimum: the result fills the content span
        // exactly rather than growing past it, from the trailing clamp alone.
        assert_eq!(
            expanded_to_min_height(60.5, 60.5, 60.0, 61.0, 8.0),
            (60.0, 61.0)
        );

        // A zero minimum asks for nothing and must not move the span.
        assert_eq!(
            expanded_to_min_height(10.0, 10.0, 0.0, 100.0, 0.0),
            (10.0, 10.0)
        );

        // A negative minimum is floored at zero rather than inverting the span.
        assert_eq!(
            expanded_to_min_height(10.0, 12.0, 0.0, 100.0, -5.0),
            (10.0, 12.0)
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

/// Character and retained-marker policy for one minimap analysis generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinimapAnalysisPolicy {
    /// Strict character count above which a line receives a warning marker.
    pub warning_line_chars: usize,
    /// Strict character count above which wrapped minimap layout is rejected.
    pub wrapped_line_chars: usize,
    /// Maximum warning-line identities retained for projection.
    pub marker_limit: usize,
}

/// Accepted GTK-free result from one complete content generation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MinimapAnalysisResult {
    /// Whether any logical line exceeded the wrapped-layout character budget.
    pub wrapped_layout_too_large: bool,
    /// Bounded warning-line identities in source order.
    pub long_line_lines: Vec<u32>,
    /// Characters examined across every bounded slice.
    pub characters_examined: u64,
    /// Logical lines reached by the complete scan.
    pub lines_examined: u64,
}

/// Incremental logical-line accumulator independent of GTK iterator ownership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MinimapAnalysisAccumulator {
    policy: MinimapAnalysisPolicy,
    collect_markers: bool,
    current_line: u32,
    current_line_chars: usize,
    current_line_marked: bool,
    wrapped_layout_too_large: bool,
    long_line_lines: Vec<u32>,
    characters_examined: u64,
}

impl MinimapAnalysisAccumulator {
    /// Start one content scan, optionally retaining long-line marker identities.
    #[must_use]
    pub fn new(policy: MinimapAnalysisPolicy, collect_markers: bool) -> Self {
        Self {
            policy,
            collect_markers,
            current_line: 0,
            current_line_chars: 0,
            current_line_marked: false,
            wrapped_layout_too_large: false,
            long_line_lines: Vec::with_capacity(policy.marker_limit.min(64)),
            characters_examined: 0,
        }
    }

    /// Inspect at most `character_limit` scalars from one caller-owned iterator.
    pub fn inspect_slice(
        &mut self,
        characters: impl IntoIterator<Item = char>,
        character_limit: usize,
    ) -> usize {
        let mut inspected = 0usize;
        for character in characters.into_iter().take(character_limit) {
            self.inspect_char(character);
            inspected = inspected.saturating_add(1);
        }
        inspected
    }

    /// Inspect one scalar from a GTK-owned cursor without retaining GTK state.
    pub fn inspect_char(&mut self, character: char) {
        self.characters_examined = self.characters_examined.saturating_add(1);
        if character == '\n' {
            self.current_line = self.current_line.saturating_add(1);
            self.current_line_chars = 0;
            self.current_line_marked = false;
            return;
        }

        self.current_line_chars = self.current_line_chars.saturating_add(1);
        if self.current_line_chars > self.policy.wrapped_line_chars {
            self.wrapped_layout_too_large = true;
        }
        if self.collect_markers
            && !self.current_line_marked
            && self.current_line_chars > self.policy.warning_line_chars
        {
            self.current_line_marked = true;
            if self.long_line_lines.len() < self.policy.marker_limit {
                self.long_line_lines.push(self.current_line);
            }
        }
    }

    /// Return whether layout evidence already found an extreme logical line.
    #[must_use]
    pub fn wrapped_layout_too_large(&self) -> bool {
        self.wrapped_layout_too_large
    }

    /// Return the number of examined characters.
    #[must_use]
    pub fn characters_examined(&self) -> u64 {
        self.characters_examined
    }

    /// Finish a complete scan and transfer its bounded accepted evidence.
    #[must_use]
    pub fn finish(self) -> MinimapAnalysisResult {
        MinimapAnalysisResult {
            wrapped_layout_too_large: self.wrapped_layout_too_large,
            long_line_lines: self.long_line_lines,
            characters_examined: self.characters_examined,
            lines_examined: u64::from(self.current_line).saturating_add(1),
        }
    }
}

#[cfg(test)]
mod analysis_tests {
    use super::*;

    const POLICY: MinimapAnalysisPolicy = MinimapAnalysisPolicy {
        warning_line_chars: 4,
        wrapped_line_chars: 8,
        marker_limit: 2,
    };

    #[test]
    fn slices_preserve_line_state_and_strict_thresholds() {
        let mut analysis = MinimapAnalysisAccumulator::new(POLICY, true);
        assert_eq!(analysis.inspect_slice("short\nlo".chars(), 8), 8);
        assert_eq!(analysis.inspect_slice("ng-line\nend".chars(), 64), 11);
        let result = analysis.finish();

        assert!(result.wrapped_layout_too_large);
        assert_eq!(result.long_line_lines, vec![0, 1]);
        assert_eq!(result.characters_examined, 19);
        assert_eq!(result.lines_examined, 3);
    }

    /// Survivor triage for the two mutants this module carried in from
    /// `model/minimap_analysis.rs`: `characters_examined` had no assertion of its
    /// own, because every test read the count off the finished result instead.
    /// The accumulator's running total is what the sliced GTK scan reports mid
    /// scan, so it is worth asserting directly and at more than one value.
    #[test]
    fn running_character_count_is_observable_before_the_scan_finishes() {
        let mut analysis = MinimapAnalysisAccumulator::new(POLICY, true);
        assert_eq!(analysis.characters_examined(), 0);

        analysis.inspect_char('a');
        assert_eq!(analysis.characters_examined(), 1);

        assert_eq!(analysis.inspect_slice("bcde\nfg".chars(), 7), 7);
        assert_eq!(
            analysis.characters_examined(),
            8,
            "newlines are examined characters too"
        );

        // The running total must agree with the terminal one it feeds.
        assert_eq!(analysis.clone().finish().characters_examined, 8);
    }

    #[test]
    fn marker_cap_does_not_stop_wrapped_layout_evidence() {
        let mut analysis = MinimapAnalysisAccumulator::new(POLICY, true);
        analysis.inspect_slice("12345\n67890\nabcdefghijkl\n".chars(), usize::MAX);
        let result = analysis.finish();

        assert_eq!(result.long_line_lines, vec![0, 1]);
        assert!(result.wrapped_layout_too_large);
        assert_eq!(result.lines_examined, 4);
    }

    #[test]
    fn marker_disabled_scan_retains_only_shared_layout_evidence() {
        let mut analysis = MinimapAnalysisAccumulator::new(POLICY, false);
        analysis.inspect_slice("abcdefghijkl".chars(), usize::MAX);
        let result = analysis.finish();

        assert!(result.wrapped_layout_too_large);
        assert!(result.long_line_lines.is_empty());
    }

    #[test]
    fn many_short_lines_require_multiple_bounded_caller_slices() {
        let text = "x\n".repeat(10_000);
        let mut analysis = MinimapAnalysisAccumulator::new(POLICY, true);
        let mut characters = text.chars();
        let mut slices = 0usize;
        loop {
            let inspected = analysis.inspect_slice(characters.by_ref(), 257);
            if inspected == 0 {
                break;
            }
            assert!(inspected <= 257);
            slices = slices.saturating_add(1);
        }
        let result = analysis.finish();

        assert!(slices > 1);
        assert_eq!(result.characters_examined, 20_000);
        assert!(!result.wrapped_layout_too_large);
        assert!(result.long_line_lines.is_empty());
    }
}
