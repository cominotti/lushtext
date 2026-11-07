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
//! [`MinimapProjectionSpace`] carries the coordinate
//! spaces across the boundary between the GTK gatherers in
//! `projection_execution` and the fitting arithmetic here. The minimap mixes
//! source-map widget coordinates, marker-strip coordinates, editor buffer
//! coordinates, and caller-supplied target coordinates, several of them as bare
//! `f64`; reifying the space is what makes passing one space's value where
//! another is expected a type error instead of a silent drift.
//!
//! ## Marker row binning
//!
//! The marker strip decides per pixel row, inside the rendered-content band
//! ([`MarkerRowBand`]): [`marker_row_groups`] groups the rows by the line GTK
//! shows there, [`mark_row_groups`] asks each marker kind's index at most once
//! per group plus once, and [`marker_row_runs`] coalesces and grows the marked
//! rows into whole-pixel runs. All of it is integer arithmetic, so the cost and
//! the geometry depend on the strip height, never on the marker count
//! (`bin-minimap-markers-per-row` D2, D3; Kani `marker_rows_*`).
//!
//! ## Content analysis
//!
//! [`MinimapAnalysisPolicy`], [`MinimapAnalysisAccumulator`], and
//! [`MinimapAnalysisResult`] relocated here from `model/minimap_analysis.rs`:
//! the minimap is their single owning workflow. The accumulator is deliberately
//! independent of GTK iterator ownership so `analysis_execution` can drive it
//! one bounded slice at a time from a live buffer cursor without ever copying
//! the document.

#![deny(clippy::float_arithmetic, clippy::disallowed_methods)]

use std::time::Duration;

#[cfg(kani)]
mod kani_proofs;

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
///
/// The threshold is one fifth (0.20), held as its denominator so the comparison
/// in [`source_map_exceeds_wide_editor_ratio`] is exact integer arithmetic.
pub(super) const MINIMAP_WIDE_EDITOR_RATIO_DENOMINATOR: i64 = 5;
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
///
/// Kind: external fact; the 120-column line-length convention the minimap
/// proposal named. It bounds no work: it states a style limit users write to,
/// so it stays exact (`power-of-two-thresholds` stage 4; 128 would leave
/// 121–128-character lines unmarked).
pub(super) const MINIMAP_LONG_LINE_WARNING_THRESHOLD: usize = 120;
/// Maximum document size before wrapped minimap layout needs a long-line check.
///
/// Ordinary prose and source files stay below this budget, while multi-megabyte
/// minified files can make the narrow source map build a very large visual-line
/// cache when it mirrors editor word wrap.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub(super) const MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET: u64 = 2 * 1024 * 1024;
/// Long logical lines above this size make wrapped source-map layout expensive.
///
/// The threshold is high enough for normal code/log lines but catches minified
/// JSON or generated files before a 64-160px minimap explodes one line into
/// thousands of visual rows.
///
/// Kind: presentation cap; 8,000 rounded up to a power of two
/// (`power-of-two-thresholds` D12; its bound measured in task 9.5 M7).
pub(super) const MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET: usize = 8_192;
/// Live GTK-buffer characters inspected by one minimap analysis turn.
///
/// Kind: cap; a power of two, rounded down.
pub(super) const MINIMAP_ANALYSIS_CHARS_PER_SLICE: usize = 32 * 1024;
/// Minimum height, in whole strip rows, of a drawn semantic marker run.
///
/// Collapsed or sub-pixel source-map lines still need to be discoverable, but
/// this height is clamped to the rendered document content so it cannot leak
/// into the blank EOF overscroll tail ([`marker_row_runs`]).
///
/// Kind: out: geometry; two whole pixels keep a one-pixel line visible
/// without anti-aliasing (`bin-minimap-markers-per-row` D3).
pub(super) const MINIMAP_MARKER_MIN_HEIGHT: i32 = 2;
/// Toggles of a swapped-out modified tag removed by one retirement slice.
///
/// Removing a property-less tag costs about 0.12 µs per toggle (A3: 262,144
/// toggles in 30.6–32.4 ms; ledger A98), so 16,384 toggles (8,192 runs) is
/// about 2 ms, a quarter of `B_INTERACT_US`, leaving room for slower hosts.
///
/// Kind: cap; a power of two, rounded down (`bin-minimap-markers-per-row` D4).
pub(super) const MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES: usize = 1 << 14;
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

/// Whether the minimap is currently usable for the active editor state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MinimapAvailability {
    /// The user preference currently disables the minimap everywhere.
    #[default]
    Disabled,
    /// The document scale policy disables the minimap for this document
    /// (total size, paragraph length or batch density).
    Reduced,
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
#[cfg_attr(kani, derive(kani::Arbitrary))]
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
    #[expect(
        clippy::float_arithmetic,
        reason = "projected bounds are GTK widget coordinates, fractional under scaling"
    )]
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

/// Whether the source map is more than one fifth as tall as the editor document.
///
/// `map / editor > 1/5` exactly, as `map * 5 > editor` in `i64`, so no ratio is
/// formed; unusable (non-positive) geometry is never wide. For every pair of
/// `i32` heights this agrees with the `f64` ratio compared against `0.20` that
/// it replaced: the two could disagree only for a true ratio within a few ulps
/// (about 10^-17) of one fifth, and a ratio of two `i32` values that is not
/// exactly one fifth differs from it by at least 1 / (5 * 2^31), about 10^-10.
pub(super) fn source_map_exceeds_wide_editor_ratio(
    editor_document_height: i32,
    source_map_document_height: i32,
) -> bool {
    editor_document_height > 0
        && source_map_document_height > 0
        && i64::from(source_map_document_height) * MINIMAP_WIDE_EDITOR_RATIO_DENOMINATOR
            > i64::from(editor_document_height)
}

/// Return a finite adjustment's absolute distance from its lower bound.
///
/// GTK can expose transitional non-finite values while a newly bound source
/// map is allocating. Those values must never be passed back to `set_value`.
#[expect(
    clippy::float_arithmetic,
    reason = "GtkAdjustment values are fractional under smooth scrolling"
)]
pub(super) fn finite_adjustment_distance_from_lower(value: f64, lower: f64) -> Option<f64> {
    if !value.is_finite() || !lower.is_finite() {
        return None;
    }
    Some((value - lower).abs())
}

/// Return the child page size that represents a fitting, non-scrollable source.
#[expect(
    clippy::float_arithmetic,
    reason = "GtkAdjustment page sizes are fractional under smooth scrolling"
)]
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
pub(super) fn wide_editor_slider_offset_class(
    document_heights: Option<(i32, i32)>,
) -> Option<&'static str> {
    document_heights
        .is_some_and(|(editor, source_map)| {
            source_map_exceeds_wide_editor_ratio(editor, source_map)
        })
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
///
/// Kani proves it never panics and never returns a non-finite coordinate for
/// any `f64` or `i32` input.
#[expect(
    clippy::float_arithmetic,
    reason = "mirrors GtkSourceMap's own f64 slider ratio; domain stated above (any f64, no panic)"
)]
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
/// Kani (`policy/kani_proofs.rs`) proves it never panics and never returns a
/// non-finite coordinate for any `f64`, and that on whole pixels (magnitude at
/// most 2^20) the result keeps `x` and `width` and lies inside the source-map
/// bounds; for general `f64` the bottom can round past the map's, kept as a
/// `should_panic` counterexample.
///
/// The raw estimate intentionally mirrors `GtkSourceMap`'s private ratio math,
/// which can point outside the map when the tiny source-map document is taller
/// than the visible widget. Screenshot crops and widget tests need the visible
/// vertical part of that effect, while diagnostics keep the raw estimate
/// separately. The horizontal CSS outset is preserved because it is part of the
/// native slider effect and intentionally paints outside the map text column.
#[expect(
    clippy::float_arithmetic,
    reason = "GTK widget coordinates are fractional; containment is proved on whole pixels up to 2^20 (rustdoc above)"
)]
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
        || !source_map_bounds.y.is_finite()
        || !source_map_bounds.height.is_finite()
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
    // Both edges are finite and inside the map, but their difference can still
    // overflow when the map is nearly `f64::MAX` tall (Kani counterexample,
    // `native_slider_fit_never_returns_an_infinite_height`).
    let height = bottom - top;
    (bottom > top && height.is_finite()).then_some(MinimapProjectedBounds {
        x: raw.x,
        y: top,
        width: raw.width,
        height,
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
    /// `DocumentScale::minimap_allowed`, decided from the paragraph profile
    /// before any map is attached. Wrapped-layout analysis no longer gates
    /// availability; it feeds markers only.
    pub(super) scale_allows_minimap: bool,
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
    if !policy.scale_allows_minimap {
        return MinimapAvailability::Reduced;
    }
    MinimapAvailability::Visible
}

/// The status feedback when the scale policy disables the minimap, naming
/// line length or size as the reason.
pub(super) fn minimap_unavailable_message(
    reduced: crate::model::document_scale::ReducedFeatures,
) -> &'static str {
    use crate::model::document_scale::{ReductionReason, ScaleFeature};
    if reduced.has_reason(ScaleFeature::Minimap, ReductionReason::LongParagraph) {
        "Minimap unavailable for this document's line lengths"
    } else {
        "Minimap unavailable for this large document"
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
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

#[expect(
    clippy::float_arithmetic,
    reason = "offsets an integer widget y by a fractional target-relative map origin"
)]
fn target_y_from_widget_y(map_y_in_target: f64, widget_y: i32) -> f64 {
    // `buffer_to_window_coords` returns y relative to the source-map widget;
    // add the map's target-relative top edge to produce crop/anchor coordinates.
    map_y_in_target + f64::from(widget_y)
}

/// Grow a projected span to the minimum visible height, without leaving content.
///
/// **Precondition:** a finite band with `lower < upper` and a span inside it,
/// which both callers guarantee; an inverted band panics in `f64::clamp`.
/// Kani proves that on this precondition it never panics for any `f64`
/// minimum height and keeps the result inside the band, and that on whole
/// pixels with magnitude at most 2^8 (minimum heights up to 2^6) the result is
/// at least the smaller of the minimum height and the band height. A unit
/// test samples the same guarantee up to 2^20; with fractional coordinates it
/// can fall a rounding error short (a kept `should_panic` counterexample).
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
#[expect(
    clippy::float_arithmetic,
    reason = "GTK widget coordinates are fractional; the minimum-height domain is stated above"
)]
#[expect(
    clippy::disallowed_methods,
    reason = "f64::midpoint of two fractional widget coordinates, overflow-free before the clamp"
)]
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

/// Fit a projected minimap rectangle into rendered content bounds.
///
/// Viewport diagnostics use `ClampOutside` because GTK keeps the native slider
/// visible at document edges. Content-row diagnostics use `RejectOutside`
/// because fabricating a row would let screenshots pass without rendered text.
/// Small projections expand around their center to remain pixel-detectable.
///
/// Kani proves it never panics or returns a non-finite coordinate for any
/// `f64`; that `RejectOutside` returns nothing for a span entirely outside the
/// band, for every finite `f64`; and that on whole pixels (magnitude at most
/// 2^20) a returned rectangle keeps `x` and `width` and lies inside the band.
/// For general `f64`, `y + height` can round above the band (a kept
/// `should_panic` counterexample). Its minimum height is
/// [`expanded_to_min_height`]'s guarantee and domain.
#[expect(
    clippy::float_arithmetic,
    reason = "GTK widget coordinates are fractional; containment is proved on whole pixels up to 2^20 (rustdoc above)"
)]
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

/// Width in whole pixels of one marker kind's lane in the right-anchored strip.
///
/// Each kind keeps a nested percentage of the strip width (bookmark 100%,
/// search 82%, modified 64%, long line 46%), rounded to the nearest whole pixel
/// with ties rounded up, and never narrower than two pixels. Rounding to the
/// nearest pixel is the integer scheme closest to the fractional lanes it
/// replaced (8 / 6.56 / 5.12 / 3.68 px on the 8 px strip become 8 / 7 / 5 / 4,
/// at most 0.44 px from each fractional edge), and it keeps each lane's edges
/// on the pixel grid so cairo fills them without anti-aliased seams. The
/// two-pixel floor applies below a 3 px strip, where it exceeds the strip just
/// as the fractional floor did.
pub(super) fn marker_lane_width(kind: MinimapMarkerKind, total_width: i32) -> i32 {
    let percent: i64 = match kind {
        MinimapMarkerKind::Bookmark => 100,
        MinimapMarkerKind::Search => 82,
        MinimapMarkerKind::Modified => 64,
        MinimapMarkerKind::LongLine => 46,
    };
    // `|total * percent| <= 2^31 * 100`, so the product cannot overflow `i64`,
    // and the rounded share lies between zero and `total`, so it fits `i32`.
    let rounded = (i64::from(total_width) * percent + 50).div_euclid(100);
    i32::try_from(rounded).unwrap_or(total_width).max(2)
}

/// Left edge in whole pixels of a lane anchored to the strip's right edge.
pub(super) fn marker_lane_x(total_width: i32, lane_width: i32) -> i32 {
    total_width.saturating_sub(lane_width)
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

/// The rendered-content band of the marker strip, in whole strip pixel rows
/// `[lower, upper)`.
///
/// The adapter rounds GTK's fractional content edges **outward** (floor the
/// top, ceil the bottom) and clamps them to the strip, so a marker on the
/// first or last rendered line is never lost; at a fractional
/// `content_bottom` a marker on the last line may cover the one pixel row
/// that line shares with the EOF tail, never a row wholly inside the tail.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub(super) struct MarkerRowBand {
    /// First strip row of rendered content.
    pub(super) lower: i32,
    /// One past the last strip row of rendered content.
    pub(super) upper: i32,
}

impl MarkerRowBand {
    /// The band from whole-pixel content edges, clamped to a strip of
    /// `strip_height` rows; empty when nothing renders.
    pub(super) fn clamped(content_top: i32, content_bottom: i32, strip_height: i32) -> Self {
        let lower = content_top.max(0);
        let upper = content_bottom.min(strip_height).max(lower);
        Self { lower, upper }
    }

    /// Rows in the band.
    pub(super) fn rows(self) -> usize {
        usize::try_from(self.upper.saturating_sub(self.lower)).unwrap_or(0)
    }
}

/// Consecutive strip rows that show the same first document line.
///
/// Rows `[row_start, row_end)` show the lines `[line_start, line_end)`: a line
/// taller than one pixel spans several rows of one group, and lines no row
/// starts on (hidden, zero-height lines, ledger A99) fall inside the group
/// above them, so they are still represented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MarkerRowGroup {
    /// First strip row of the group.
    pub(super) row_start: i32,
    /// One past the group's last strip row.
    pub(super) row_end: i32,
    /// First document line the group shows.
    pub(super) line_start: u32,
    /// One past the last document line the group shows.
    pub(super) line_end: u32,
}

/// One coalesced, drawable marker rectangle of a lane, in strip rows
/// `[top, bottom)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct MarkerRowRun {
    /// First strip row of the run.
    pub(super) top: i32,
    /// One past the run's last strip row.
    pub(super) bottom: i32,
}

/// Group the band's rows by the line each one starts on.
///
/// `row_lines[i]` is the line GTK's `line_at_y` reports at strip row
/// `band.lower + i` (one entry per band row; extra entries are ignored and a
/// short table yields fewer rows), and `line_past_band` the line at the first
/// row after the band. The table is made monotone by construction (each entry
/// at least the one before it, ledger A99 says GTK's already is), so group
/// starts are strictly increasing and every row is in exactly one group. The
/// last group extends to `line_past_band`, at least one line.
pub(super) fn marker_row_groups(
    band: MarkerRowBand,
    row_lines: &[u32],
    line_past_band: u32,
) -> Vec<MarkerRowGroup> {
    let rows = band.rows().min(row_lines.len());
    // Sized once from the table (the adapter passes one entry per band row):
    // at most one group per row, so pushing never reallocates. Sizing from the
    // table's length rather than the band keeps the allocation size concrete
    // for Kani, where a symbolic size exhausted the solver's memory.
    let mut groups: Vec<MarkerRowGroup> = Vec::with_capacity(row_lines.len());
    let mut previous_line = 0;
    let mut row = band.lower;
    for &line in &row_lines[..rows] {
        let line = line.max(previous_line);
        previous_line = line;
        let next_row = row.saturating_add(1);
        match groups.last_mut() {
            Some(group) if group.line_start == line => group.row_end = next_row,
            last => {
                if let Some(group) = last {
                    group.line_end = line;
                }
                groups.push(MarkerRowGroup {
                    row_start: row,
                    row_end: next_row,
                    line_start: line,
                    line_end: line.saturating_add(1),
                });
            }
        }
        row = next_row;
    }
    if let Some(group) = groups.last_mut() {
        group.line_end = line_past_band.max(group.line_start.saturating_add(1));
    }
    groups
}

/// Mark the groups that show at least one marker line of one kind.
///
/// `first_marker_at_or_after(line)` is the kind's index query: the first
/// marker line at or after `line`, or `None`. The walk starts at the first
/// group's first line, maps each answer to its group by binary search over
/// the group starts, marks it and resumes at the next group's start, and
/// stops at an answer past the band. So it asks at most `groups + 1` times,
/// whatever the marker count, and touches only lines the strip shows.
/// Returns the marked flag of each group and the number of queries asked.
pub(super) fn mark_row_groups(
    groups: &[MarkerRowGroup],
    mut first_marker_at_or_after: impl FnMut(u32) -> Option<u32>,
) -> (Vec<bool>, usize) {
    let mut marked = vec![false; groups.len()];
    let mut queries = 0;
    let Some(first) = groups.first() else {
        return (marked, queries);
    };
    let mut next_line = first.line_start;
    loop {
        queries += 1;
        let Some(marker_line) = first_marker_at_or_after(next_line) else {
            break;
        };
        // An honest index never answers before the line asked about.
        let marker_line = marker_line.max(next_line);
        let index = groups
            .partition_point(|group| group.line_start <= marker_line)
            .saturating_sub(1);
        let Some(group) = groups.get(index) else {
            break;
        };
        if marker_line >= group.line_end {
            break;
        }
        marked[index] = true;
        let Some(next) = groups.get(index + 1) else {
            break;
        };
        next_line = next.line_start;
    }
    (marked, queries)
}

/// Coalesce the marked groups of one kind into drawable runs.
///
/// Adjacent marked rows form one run. A run shorter than `min_height` grows
/// around its centre, `(min - height) / 2` rows above and the rest (the odd
/// row) below, then is pushed back inside the band, so no marker reaches the
/// EOF overscroll tail (the integer twin of `expanded_to_min_height`). Runs
/// that then overlap or touch merge, so the result is disjoint, in increasing
/// order, and no lane pixel is painted twice. Every run lies inside the band,
/// covers every marked row, and is at least `min(min_height, band height)`
/// tall; there are at most `ceil(rows / 2)` of them.
pub(super) fn marker_row_runs(
    groups: &[MarkerRowGroup],
    marked: &[bool],
    band: MarkerRowBand,
    min_height: i32,
) -> Vec<MarkerRowRun> {
    let target = min_height.max(0).min(band.upper.saturating_sub(band.lower));
    // At most one run per marked group, so pushing never reallocates.
    let mut runs: Vec<MarkerRowRun> = Vec::with_capacity(groups.len().min(marked.len()));
    let mut pending: Option<MarkerRowRun> = None;
    for (group, &is_marked) in groups.iter().zip(marked) {
        let top = group.row_start.max(band.lower);
        let bottom = group.row_end.min(band.upper);
        if !is_marked || bottom <= top {
            continue;
        }
        pending = match pending {
            Some(run) if top <= run.bottom => Some(MarkerRowRun {
                top: run.top,
                bottom: bottom.max(run.bottom),
            }),
            Some(run) => {
                push_expanded_run(&mut runs, run, band, target);
                Some(MarkerRowRun { top, bottom })
            }
            None => Some(MarkerRowRun { top, bottom }),
        };
    }
    if let Some(run) = pending {
        push_expanded_run(&mut runs, run, band, target);
    }
    runs
}

/// Grow `run` (inside `band`) to `target` rows, at most the band height:
/// `(target - height) / 2` rows above and the rest, the odd row, below, then
/// pushed back inside the band. The integer twin of `expanded_to_min_height`;
/// Kani proves it on every whole-pixel band up to 2^20
/// (`marker_rows_expansion_*`).
pub(super) fn expanded_run(run: MarkerRowRun, band: MarkerRowBand, target: i32) -> MarkerRowRun {
    let MarkerRowRun {
        mut top,
        mut bottom,
    } = run;
    let height = bottom.saturating_sub(top);
    if height < target {
        let grow = target - height;
        top = top.saturating_sub(grow / 2);
        bottom = bottom.saturating_add(grow - grow / 2);
        if top < band.lower {
            bottom = bottom.saturating_add(band.lower - top);
            top = band.lower;
        }
        if bottom > band.upper {
            top = top.saturating_sub(bottom - band.upper);
            bottom = band.upper;
        }
        top = top.max(band.lower);
    }
    MarkerRowRun { top, bottom }
}

/// Grow `run` to `target` rows inside the band and append it to `runs`,
/// merging it with the runs it now overlaps or touches. `target` is at most
/// the band height, so the pushes never leave the band.
fn push_expanded_run(
    runs: &mut Vec<MarkerRowRun>,
    run: MarkerRowRun,
    band: MarkerRowBand,
    target: i32,
) {
    let MarkerRowRun {
        mut top,
        mut bottom,
    } = expanded_run(run, band, target);
    while let Some(last) = runs.last().copied()
        && top <= last.bottom
    {
        top = top.min(last.top);
        bottom = bottom.max(last.bottom);
        runs.pop();
    }
    runs.push(MarkerRowRun { top, bottom });
}

/// The first value in `sorted` at or after `line`: the index query of a kind
/// held as sorted line numbers (bookmarks, long lines).
pub(super) fn first_sorted_line_at_or_after(sorted: &[u32], line: u32) -> Option<u32> {
    sorted
        .get(sorted.partition_point(|&value| value < line))
        .copied()
}

#[cfg(test)]
mod marker_row_tests {
    use proptest::prelude::*;

    use super::*;

    /// Every line a group shows, by brute force over the model's lines.
    fn model_marked(groups: &[MarkerRowGroup], markers: &[u32]) -> Vec<bool> {
        groups
            .iter()
            .map(|group| {
                markers
                    .iter()
                    .any(|&line| (group.line_start..group.line_end).contains(&line))
            })
            .collect()
    }

    fn walk(groups: &[MarkerRowGroup], markers: &[u32]) -> (Vec<bool>, usize) {
        let mut sorted = markers.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        mark_row_groups(groups, |line| first_sorted_line_at_or_after(&sorted, line))
    }

    /// A monotone row table from per-row line steps.
    fn table(first: u32, steps: &[u8]) -> Vec<u32> {
        let mut line = first;
        steps
            .iter()
            .map(|&step| {
                line += u32::from(step);
                line
            })
            .collect()
    }

    #[test]
    fn groups_follow_a_monotone_row_table() {
        let band = MarkerRowBand {
            lower: 10,
            upper: 16,
        };
        // Line 3 spans rows 10-12, line 4 row 13, lines 5-7 (6 and 7 hidden)
        // row 14, line 8 row 15; line 9 starts after the band.
        let groups = marker_row_groups(band, &[3, 3, 3, 4, 5, 8], 9);
        assert_eq!(
            groups,
            vec![
                MarkerRowGroup {
                    row_start: 10,
                    row_end: 13,
                    line_start: 3,
                    line_end: 4
                },
                MarkerRowGroup {
                    row_start: 13,
                    row_end: 14,
                    line_start: 4,
                    line_end: 5
                },
                MarkerRowGroup {
                    row_start: 14,
                    row_end: 15,
                    line_start: 5,
                    line_end: 8
                },
                MarkerRowGroup {
                    row_start: 15,
                    row_end: 16,
                    line_start: 8,
                    line_end: 9
                },
            ]
        );
    }

    #[test]
    fn a_decreasing_table_entry_is_held_at_the_previous_line() {
        let band = MarkerRowBand { lower: 0, upper: 3 };
        let groups = marker_row_groups(band, &[5, 4, 6], 6);
        assert_eq!(groups.len(), 2);
        assert_eq!((groups[0].row_start, groups[0].row_end), (0, 2));
        assert_eq!((groups[1].line_start, groups[1].line_end), (6, 7));
    }

    #[test]
    fn the_last_group_reaches_the_line_past_the_band() {
        let band = MarkerRowBand { lower: 0, upper: 2 };
        let groups = marker_row_groups(band, &[0, 1], 4);
        assert_eq!(groups[1].line_end, 4);
        let groups = marker_row_groups(band, &[0, 1], 1);
        assert_eq!(groups[1].line_end, 2, "at least the group's own line");
    }

    #[test]
    fn an_empty_band_has_no_groups_and_asks_nothing() {
        let groups = marker_row_groups(MarkerRowBand::default(), &[1, 2], 3);
        assert!(groups.is_empty());
        assert_eq!(walk(&groups, &[1, 2]), (Vec::new(), 0));
    }

    #[test]
    fn hidden_lines_inside_one_group_mark_it() {
        let band = MarkerRowBand { lower: 0, upper: 3 };
        let groups = marker_row_groups(band, &[0, 1, 9], 10);
        let (marked, _) = walk(&groups, &[5]);
        assert_eq!(marked, vec![false, true, false]);
    }

    #[test]
    fn markers_past_the_band_are_ignored_and_stop_the_walk() {
        let band = MarkerRowBand { lower: 0, upper: 3 };
        let groups = marker_row_groups(band, &[10, 11, 12], 13);
        let (marked, queries) = walk(&groups, &[2, 13, 14, 500]);
        assert_eq!(marked, vec![false; 3]);
        assert_eq!(queries, 1);
    }

    #[test]
    fn the_walk_asks_at_most_once_per_group_plus_once() {
        let band = MarkerRowBand { lower: 0, upper: 4 };
        let groups = marker_row_groups(band, &[0, 1, 2, 3], 4);
        let every_line: Vec<u32> = (0..4).collect();
        assert_eq!(walk(&groups, &every_line), (vec![true; 4], 4));
        assert_eq!(walk(&groups, &[1]), (vec![false, true, false, false], 2));
    }

    #[test]
    fn the_band_clamps_to_the_strip() {
        assert_eq!(
            MarkerRowBand::clamped(-3, 40, 30),
            MarkerRowBand {
                lower: 0,
                upper: 30
            }
        );
        assert_eq!(
            MarkerRowBand::clamped(5, 4, 30),
            MarkerRowBand { lower: 5, upper: 5 }
        );
        assert_eq!(MarkerRowBand::clamped(5, 4, 30).rows(), 0);
        assert_eq!(MarkerRowBand::clamped(5, 9, 30).rows(), 4);
    }

    fn runs_of(rows: &[i32], band: MarkerRowBand, min_height: i32) -> Vec<(i32, i32)> {
        let groups: Vec<MarkerRowGroup> = (band.lower..band.upper)
            .map(|row| MarkerRowGroup {
                row_start: row,
                row_end: row + 1,
                line_start: u32::try_from(row - band.lower).unwrap_or(0),
                line_end: u32::try_from(row - band.lower).unwrap_or(0) + 1,
            })
            .collect();
        let marked: Vec<bool> = groups
            .iter()
            .map(|group| rows.contains(&group.row_start))
            .collect();
        marker_row_runs(&groups, &marked, band, min_height)
            .into_iter()
            .map(|run| (run.top, run.bottom))
            .collect()
    }

    #[test]
    fn adjacent_rows_coalesce_into_one_run() {
        let band = MarkerRowBand {
            lower: 0,
            upper: 20,
        };
        assert_eq!(runs_of(&[3, 4, 5, 9], band, 0), vec![(3, 6), (9, 10)]);
    }

    #[test]
    fn a_one_row_run_grows_with_the_odd_row_below() {
        let band = MarkerRowBand {
            lower: 0,
            upper: 20,
        };
        assert_eq!(runs_of(&[9], band, 2), vec![(9, 11)]);
        assert_eq!(runs_of(&[9], band, 3), vec![(8, 11)]);
    }

    #[test]
    fn a_run_at_the_band_edges_is_pushed_inside_never_below_content() {
        let band = MarkerRowBand {
            lower: 4,
            upper: 10,
        };
        assert_eq!(
            runs_of(&[9], band, 2),
            vec![(8, 10)],
            "EOF tail: never past upper"
        );
        assert_eq!(runs_of(&[4], band, 3), vec![(4, 7)]);
        assert_eq!(runs_of(&[6], band, 50), vec![(4, 10)], "at most the band");
    }

    #[test]
    fn runs_that_meet_after_growing_merge() {
        let band = MarkerRowBand {
            lower: 0,
            upper: 20,
        };
        // Rows 5 and 7 grow to [5, 7) and [7, 9): they touch, so one run.
        assert_eq!(runs_of(&[5, 7], band, 2), vec![(5, 9)]);
        assert_eq!(runs_of(&[5, 9], band, 2), vec![(5, 7), (9, 11)]);
    }

    #[test]
    fn sorted_line_index_answers_the_first_line_at_or_after() {
        let sorted = [2, 5, 9];
        assert_eq!(first_sorted_line_at_or_after(&sorted, 0), Some(2));
        assert_eq!(first_sorted_line_at_or_after(&sorted, 5), Some(5));
        assert_eq!(first_sorted_line_at_or_after(&sorted, 6), Some(9));
        assert_eq!(first_sorted_line_at_or_after(&sorted, 10), None);
    }

    /// The integer expansion against the `f64` one it twins, on whole pixels
    /// sampled up to 2^20 (the fit's containment domain).
    #[test]
    fn integer_expansion_agrees_with_expanded_to_min_height_on_whole_pixels() {
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = |bound: i32| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let span = u64::try_from(bound).unwrap_or(1).max(1);
            i32::try_from((state >> 33) % span).unwrap_or(0)
        };
        for _ in 0..20_000 {
            let lower = next(1 << 20);
            let upper = lower + 1 + next(1 << 10);
            let top = lower + next(upper - lower);
            let bottom = top + 1 + next((upper - top).min(4));
            let bottom = bottom.min(upper);
            let min_height = next(1 << 6);
            let band = MarkerRowBand { lower, upper };
            let group = MarkerRowGroup {
                row_start: top,
                row_end: bottom,
                line_start: 0,
                line_end: 1,
            };
            let runs = marker_row_runs(&[group], &[true], band, min_height);
            let (float_top, float_bottom) = expanded_to_min_height(
                f64::from(top),
                f64::from(bottom),
                f64::from(lower),
                f64::from(upper),
                f64::from(min_height),
            );
            let run = runs[0];
            let height = run.bottom - run.top;
            assert!(lower <= run.top && run.bottom <= upper);
            assert!(run.top <= top && bottom <= run.bottom);
            assert!(height >= min_height.min(upper - lower));
            // The float form centres on a half pixel; the integer one puts
            // the odd row below. Both have the same height and differ by at
            // most half a pixel at each edge.
            assert_eq!(
                f64::from(height),
                (float_bottom - float_top).max(f64::from(bottom - top))
            );
            assert!((f64::from(run.top) - float_top).abs() <= 0.5);
        }
    }

    fn row_table() -> impl Strategy<Value = (i32, Vec<u32>, u32)> {
        (
            0i32..40,
            0u32..6,
            proptest::collection::vec(0u8..4, 0..48),
            0u32..4,
        )
            .prop_map(|(lower, first, steps, past)| {
                let lines = table(first, &steps);
                let last = lines.last().copied().unwrap_or(first);
                (lower, lines, last + past)
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4_000))]

        #[test]
        fn groups_partition_the_band(
            (lower, lines, past) in row_table(),
        ) {
            let band = MarkerRowBand { lower, upper: lower + i32::try_from(lines.len()).unwrap_or(0) };
            let groups = marker_row_groups(band, &lines, past);
            let mut row = band.lower;
            for window in groups.windows(2) {
                prop_assert!(window[0].line_start < window[1].line_start);
                prop_assert_eq!(window[0].line_end, window[1].line_start);
            }
            for group in &groups {
                prop_assert_eq!(group.row_start, row);
                prop_assert!(group.row_end > group.row_start);
                prop_assert!(group.line_end > group.line_start);
                row = group.row_end;
            }
            prop_assert_eq!(row, band.upper);
        }

        #[test]
        fn the_walk_is_sound_complete_and_bounded(
            (lower, lines, past) in row_table(),
            markers in proptest::collection::vec(0u32..240, 0..64),
        ) {
            let band = MarkerRowBand { lower, upper: lower + i32::try_from(lines.len()).unwrap_or(0) };
            let groups = marker_row_groups(band, &lines, past);
            let (marked, queries) = walk(&groups, &markers);
            prop_assert_eq!(&marked, &model_marked(&groups, &markers));
            prop_assert!(queries <= groups.len() + 1);
        }

        #[test]
        fn runs_cover_marks_inside_the_band(
            (lower, lines, past) in row_table(),
            markers in proptest::collection::vec(0u32..240, 0..64),
            min_height in 0i32..8,
            extra_band in 0i32..6,
        ) {
            let band = MarkerRowBand {
                lower,
                upper: lower + i32::try_from(lines.len()).unwrap_or(0) + extra_band,
            };
            let groups = marker_row_groups(band, &lines, past);
            let (marked, _) = walk(&groups, &markers);
            let runs = marker_row_runs(&groups, &marked, band, min_height);
            let rows = band.rows();
            prop_assert!(runs.len() <= rows.div_ceil(2));
            for window in runs.windows(2) {
                prop_assert!(window[0].bottom < window[1].top);
            }
            for run in &runs {
                prop_assert!(band.lower <= run.top && run.bottom <= band.upper);
                prop_assert!(run.bottom - run.top >= min_height.min(band.upper - band.lower));
            }
            for (group, &is_marked) in groups.iter().zip(&marked) {
                if is_marked {
                    for row in group.row_start..group.row_end {
                        prop_assert!(runs.iter().any(|run| run.top <= row && row < run.bottom));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kani's `native_slider_fit_never_panics` counterexample: with a
    /// source map near `f64::MAX` tall, pushing the slider back inside the map
    /// made `bottom - top` overflow, so the fit returned an infinite height.
    #[test]
    fn native_slider_fit_never_returns_an_infinite_height() {
        let raw = MinimapProjectedBounds {
            x: 0.0,
            y: f64::from_bits(0xfd7d_3ebf_8000_0000),
            width: 1.0,
            height: f64::MAX,
        };
        let source_map = MinimapProjectedBounds {
            x: 0.0,
            y: f64::from_bits(0xff78_0000_0014_b9c0),
            width: 1.0,
            height: f64::MAX,
        };
        if let Some(fitted) = fit_native_slider_to_source_map_bounds(raw, source_map) {
            assert!(
                fitted.y.is_finite() && fitted.height.is_finite(),
                "{fitted:?}"
            );
        }
    }

    /// A source map whose bottom edge overflows to infinity is rejected, not
    /// fitted into an unbounded range. The proof-strength lane found this
    /// guard unconstrained by every harness and test (the harnesses stay far
    /// below `f64::MAX`).
    #[test]
    fn native_slider_fit_rejects_a_source_map_whose_bottom_overflows() {
        // Both of the map's fields are finite, but `y + height` is not; a
        // slider inside it would otherwise be fitted against an unbounded range.
        let raw = MinimapProjectedBounds {
            x: 0.0,
            y: f64::MAX / 2.0,
            width: 8.0,
            height: f64::MAX / 4.0,
        };
        let source_map = MinimapProjectedBounds {
            x: 0.0,
            y: f64::MAX / 2.0,
            width: 8.0,
            height: f64::MAX,
        };
        assert_eq!(
            fit_native_slider_to_source_map_bounds(raw, source_map),
            None
        );
    }

    /// The wider-domain companion of the Kani harness
    /// `min_height_expansion_reaches_the_minimum_on_small_whole_pixels`, which
    /// CBMC can finish only up to 2^8: sampled whole pixels up to 2^20 and
    /// minimum heights up to 2^16.
    #[test]
    fn expanded_to_min_height_reaches_the_minimum_on_whole_pixels() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move |bound: i64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let span = u64::try_from(2 * bound + 1).expect("a positive span");
            let offset = i64::try_from(state % span).expect("below the span");
            f64::from(i32::try_from(offset - bound).expect("within the pixel bound"))
        };
        for _ in 0..200_000 {
            let mut edges = [next(1 << 20), next(1 << 20), next(1 << 20), next(1 << 20)];
            edges.sort_by(f64::total_cmp);
            let [lower, top, bottom, upper] = edges;
            if lower >= upper {
                continue;
            }
            let min_height = next(1 << 16).abs();
            let (fitted_top, fitted_bottom) =
                expanded_to_min_height(top, bottom, lower, upper, min_height);
            assert!(lower <= fitted_top && fitted_top <= fitted_bottom && fitted_bottom <= upper);
            assert!(
                fitted_bottom - fitted_top >= min_height.min(upper - lower),
                "{top} {bottom} in {lower}..{upper}, minimum {min_height}"
            );
        }
    }

    #[test]
    fn test_minimap_policy_constants_are_stable() {
        assert_eq!(MINIMAP_MARKER_STRIP_WIDTH, 8);
        assert_eq!(MINIMAP_REFRESH_DEBOUNCE, Duration::from_millis(80));
        assert_eq!(MINIMAP_LONG_LINE_WARNING_THRESHOLD, 120);
        assert_eq!(MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET, 2 * 1024 * 1024);
        assert_eq!(MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET, 8_192);
        assert_eq!(MINIMAP_ANALYSIS_CHARS_PER_SLICE, 32 * 1024);
        assert_eq!(MINIMAP_MARKER_MIN_HEIGHT, 2);
        assert_eq!(MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES, 1 << 14);
        assert_eq!(MINIMAP_TOP_CONTENT_MARGIN, 5);
        assert_eq!(MINIMAP_WIDE_EDITOR_RATIO_DENOMINATOR, 5);
        assert_eq!(MINIMAP_VIEWPORT_HORIZONTAL_OUTSET, 13);
        assert_eq!(MINIMAP_REFLOW_SETTLE_DEBOUNCE, Duration::from_millis(150));
        assert_eq!(MINIMAP_REFLOW_REVEAL_DELAY, Duration::from_millis(800));
    }

    #[test]
    fn test_source_map_exceeds_wide_editor_ratio_is_strict() {
        // Exactly one fifth is not wide; one pixel more is.
        assert!(!source_map_exceeds_wide_editor_ratio(1_000, 200));
        assert!(source_map_exceeds_wide_editor_ratio(1_000, 201));
        assert!(!source_map_exceeds_wide_editor_ratio(1_001, 200));
        assert!(source_map_exceeds_wide_editor_ratio(6_000, 1_800));
        // No overflow at the extremes.
        assert!(source_map_exceeds_wide_editor_ratio(i32::MAX, i32::MAX));
        assert!(!source_map_exceeds_wide_editor_ratio(i32::MAX, 1));
    }

    #[test]
    fn test_source_map_exceeds_wide_editor_ratio_rejects_unusable_geometry() {
        assert!(!source_map_exceeds_wide_editor_ratio(0, 200));
        assert!(!source_map_exceeds_wide_editor_ratio(1_000, 0));
        assert!(!source_map_exceeds_wide_editor_ratio(-1, 200));
        assert!(!source_map_exceeds_wide_editor_ratio(1_000, -1));
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
        assert_eq!(
            wide_editor_slider_offset_class(Some((1_000_000, 199_999))),
            None
        );
        assert_eq!(
            wide_editor_slider_offset_class(Some((1_000_000, 200_000))),
            None
        );
        assert_eq!(
            wide_editor_slider_offset_class(Some((1_000_000, 200_001))),
            Some(MINIMAP_WIDE_EDITOR_SLIDER_OFFSET_CLASS)
        );
    }

    #[test]
    fn test_wide_editor_slider_offset_class_rejects_missing_or_unusable_heights() {
        assert_eq!(wide_editor_slider_offset_class(None), None);
        assert_eq!(wide_editor_slider_offset_class(Some((0, 200))), None);
        assert_eq!(wide_editor_slider_offset_class(Some((1_000, 0))), None);
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
    fn test_buffer_line_budget_resets_on_newlines_and_uses_strict_overflow() {
        assert!(!text_exceeds_line_char_budget("abcd\nabcde\nxy", 5));
        assert!(text_exceeds_line_char_budget("abcd\nabcde\nxy", 4));

        assert!(!text_exceeds_line_char_budget("éé\né", 2));
        assert!(text_exceeds_line_char_budget("éé\né", 1));
    }

    #[test]
    fn test_analysis_lines_follow_gtk_paragraph_delimiters() {
        let policy = MinimapAnalysisPolicy {
            warning_line_chars: 2,
            wrapped_line_chars: usize::MAX,
            marker_limit: 16,
        };
        let mut analysis = MinimapAnalysisAccumulator::new(policy, true);
        analysis.inspect_slice("aaa\rb\r\nccc\u{2029}dddd".chars(), usize::MAX);
        let result = analysis.finish();
        assert_eq!(result.long_line_lines, vec![0, 2, 3]);
        assert_eq!(result.lines_examined, 4);
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
            scale_allows_minimap: true,
        };

        assert_eq!(
            minimap_availability_for_policy(visible),
            MinimapAvailability::Visible
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                preference_enabled: false,
                evicted: true,
                scale_allows_minimap: false,
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
                scale_allows_minimap: false,
                ..visible
            }),
            MinimapAvailability::Evicted,
            "evicted tabs report their reload state before size-tier feedback"
        );
        assert_eq!(
            minimap_availability_for_policy(MinimapAvailabilityPolicy {
                scale_allows_minimap: false,
                ..visible
            }),
            MinimapAvailability::Reduced
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

    /// The marker follows the 120-column convention the minimap proposal
    /// named: a 121- or 128-character line breaks it and is marked.
    #[test]
    fn long_line_markers_follow_the_120_column_convention() {
        let text = format!(
            "{}\n{}\n{}",
            "a".repeat(120),
            "b".repeat(121),
            "c".repeat(128)
        );
        assert_eq!(long_line_warning_lines(&text), vec![1, 2]);
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
        // The shipped 8 px strip: 8 / 6.56 / 5.12 / 3.68 px round to 8 / 7 / 5 / 4.
        let strip = MINIMAP_MARKER_STRIP_WIDTH;
        let expected = [
            (MinimapMarkerKind::Bookmark, 8, 0),
            (MinimapMarkerKind::Search, 7, 1),
            (MinimapMarkerKind::Modified, 5, 3),
            (MinimapMarkerKind::LongLine, 4, 4),
        ];
        for (kind, width, x) in expected {
            assert_eq!(marker_lane_width(kind, strip), width, "{kind:?}");
            assert_eq!(marker_lane_x(strip, width), x, "{kind:?}");
        }

        assert_eq!(marker_lane_width(MinimapMarkerKind::Bookmark, 10), 10);
        assert_eq!(marker_lane_width(MinimapMarkerKind::Search, 10), 8);
        assert_eq!(marker_lane_width(MinimapMarkerKind::Modified, 10), 6);
        assert_eq!(marker_lane_width(MinimapMarkerKind::LongLine, 10), 5);
        assert_eq!(marker_lane_x(10, 5), 5);
        // A tie rounds up: 25 * 82% = 20.5 px.
        assert_eq!(marker_lane_width(MinimapMarkerKind::Search, 25), 21);

        for kind in [
            MinimapMarkerKind::Bookmark,
            MinimapMarkerKind::Search,
            MinimapMarkerKind::Modified,
            MinimapMarkerKind::LongLine,
        ] {
            assert_eq!(marker_lane_width(kind, 1), 2);
            assert_eq!(marker_lane_width(kind, i32::MIN), 2);
            assert!(marker_lane_width(kind, i32::MAX) > 2);
        }
    }

    #[test]
    fn test_marker_lanes_round_to_nearest_and_stay_nested_inside_the_strip() {
        let kinds = [
            (MinimapMarkerKind::Bookmark, 100i64),
            (MinimapMarkerKind::Search, 82),
            (MinimapMarkerKind::Modified, 64),
            (MinimapMarkerKind::LongLine, 46),
        ];
        // From 4 px up every share rounds to at least two pixels, so the floor
        // never overrides the nearest pixel (at 3 px the long-line lane's
        // 1.38 px is floored to 2, covered above).
        for strip in 4..=4096i32 {
            let mut previous = strip;
            for (kind, percent) in kinds {
                let width = marker_lane_width(kind, strip);
                let x = marker_lane_x(strip, width);
                // Nearest whole pixel to `strip * percent / 100`, in hundredths.
                let error = (i64::from(width) * 100 - i64::from(strip) * percent).abs();
                assert!(error <= 50, "{kind:?} at {strip}: {width}");
                assert!((2..=strip).contains(&width), "{kind:?} at {strip}: {width}");
                assert!(width <= previous, "{kind:?} at {strip} is not nested");
                assert_eq!(
                    x + width,
                    strip,
                    "{kind:?} at {strip} leaves the right edge"
                );
                assert!(x >= 0, "{kind:?} at {strip}: x {x}");
                previous = width;
            }
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

impl MinimapAnalysisPolicy {
    /// The policy every production analysis generation uses.
    pub const PRODUCTION: Self = Self {
        warning_line_chars: MINIMAP_LONG_LINE_WARNING_THRESHOLD,
        wrapped_line_chars: MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET,
        marker_limit: usize::MAX,
    };
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

/// The per-line thresholds a line can still cross; `None` for one no longer
/// in play (its verdict is already set, or markers are not collected or this
/// line is already marked).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineThresholds {
    /// [`MinimapAnalysisPolicy::wrapped_line_chars`] while in play.
    pub wrapped: Option<usize>,
    /// [`MinimapAnalysisPolicy::warning_line_chars`] while in play.
    pub warning: Option<usize>,
}

/// Whether a line at `running` characters, extended by content of
/// `content_bytes` UTF-8 bytes, may cross a threshold in play, so its exact
/// character count is needed.
///
/// A scalar is at least one byte, so content of `b` bytes holds at most `b`
/// scalars and a line reaches at most `running + b` (saturating). When that
/// stays at or below every threshold in play, no scalar of the content
/// crosses one and the count can be skipped. Kani proves this for every
/// running count, byte length, scalar count no larger than it, and threshold
/// pair (`kani_proofs::skipped_line_crosses_no_threshold`).
#[must_use]
pub fn paragraph_may_cross(
    running: usize,
    content_bytes: usize,
    thresholds: LineThresholds,
) -> bool {
    let reach = running.saturating_add(content_bytes);
    [thresholds.wrapped, thresholds.warning]
        .into_iter()
        .flatten()
        .any(|threshold| reach > threshold)
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
    /// The previous scalar was `\r`, so a `\n` now completes that CRLF.
    previous_was_cr: bool,
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
            previous_was_cr: false,
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

    /// Inspect every scalar of `text`, in order: the slice form of
    /// [`inspect_char`](Self::inspect_char), its reference.
    ///
    /// The resulting state equals calling `inspect_char` for each character
    /// of `text`, for every text, every policy, and every split of a document
    /// into consecutive calls (a `\r` ending one call and a `\n` starting the
    /// next are one CRLF). Outside Kani, the line breaks come from
    /// [`next_paragraph_break`](crate::model::paragraph_boundary::next_paragraph_break)
    /// (a SIMD skip), the characters from the
    /// standard library's word-at-a-time count, and a closed line whose byte
    /// length cannot carry it past a threshold still in play
    /// ([`paragraph_may_cross`]) is not counted at all: the break resets the
    /// only fields the count reaches. Under Kani it is the per-character loop,
    /// because the fast path's `count_scalars` exists only outside Kani; no
    /// harness checks this function, so its equivalence with `inspect_char`
    /// rests on the property test `inspect_str_equals_inspect_char` and the
    /// unit tests below.
    #[cfg(kani)]
    pub fn inspect_str(&mut self, text: &str) {
        for character in text.chars() {
            self.inspect_char(character);
        }
    }

    /// See the `cfg(kani)` form above for the contract.
    #[cfg(not(kani))]
    pub fn inspect_str(&mut self, text: &str) {
        // Imported here, not at the top of the module, so the module's
        // line-anchored mutation exclusions (`.cargo/mutants.toml`) keep
        // their line numbers.
        use crate::model::paragraph_boundary::{
            ParagraphDelimiter, count_scalars, next_paragraph_break,
        };

        let bytes = text.as_bytes();
        if bytes.is_empty() {
            return;
        }
        self.characters_examined = self.characters_examined.saturating_add(count_scalars(text));
        let mut from = 0;
        if self.previous_was_cr && bytes[0] == b'\n' {
            // The `\n` of a CRLF split across calls: its line already closed.
            from = 1;
        }
        self.previous_was_cr = false;
        while from < bytes.len() {
            let Some(found) = next_paragraph_break(text, from) else {
                self.extend_line(&text[from..]);
                return;
            };
            let content = &text[from..found.content_end];
            if paragraph_may_cross(
                self.current_line_chars,
                content.len(),
                self.thresholds_in_play(),
            ) {
                self.extend_line(content);
            }
            self.close_line();
            from = found.next_start;
            // A `\r` ending the call may be the first half of a CRLF.
            self.previous_was_cr = found.delimiter == ParagraphDelimiter::Cr && from == bytes.len();
        }
    }

    /// The thresholds the current line can still cross: the wrapped-layout
    /// one until its verdict is set, the marker one while markers are
    /// collected and this line is unmarked.
    #[cfg(not(kani))]
    fn thresholds_in_play(&self) -> LineThresholds {
        LineThresholds {
            wrapped: (!self.wrapped_layout_too_large).then_some(self.policy.wrapped_line_chars),
            warning: (self.collect_markers && !self.current_line_marked)
                .then_some(self.policy.warning_line_chars),
        }
    }

    /// Add `content`'s scalars (no delimiter among them, at least one) to the
    /// current line, recording a crossing as `inspect_char` would.
    #[cfg(not(kani))]
    fn extend_line(&mut self, content: &str) {
        if content.is_empty() {
            return;
        }
        // The running count only grows, so a threshold is crossed by one of
        // these scalars exactly when it is crossed by the last; the marker
        // records the line, which none of them changes.
        let chars = usize::try_from(crate::model::paragraph_boundary::count_scalars(content))
            .unwrap_or(usize::MAX);
        self.current_line_chars = self.current_line_chars.saturating_add(chars);
        self.check_thresholds();
    }

    /// Close the current line at a delimiter.
    fn close_line(&mut self) {
        self.current_line = self.current_line.saturating_add(1);
        self.current_line_chars = 0;
        self.current_line_marked = false;
    }

    /// Record the crossings the current line's running count has reached.
    fn check_thresholds(&mut self) {
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

    /// Inspect one scalar from a GTK-owned cursor without retaining GTK state.
    pub fn inspect_char(&mut self, character: char) {
        self.characters_examined = self.characters_examined.saturating_add(1);
        // GTK paragraph delimiters (`model::paragraph_boundary`): `\n`, `\r`,
        // `\r\n` as one, and U+2029, so CR-only and U+2029 documents get the
        // same line identities as the buffer's lines.
        let completes_crlf = character == '\n' && self.previous_was_cr;
        self.previous_was_cr = character == '\r';
        if completes_crlf {
            return;
        }
        if matches!(character, '\n' | '\r' | '\u{2029}') {
            self.close_line();
            return;
        }

        self.current_line_chars = self.current_line_chars.saturating_add(1);
        self.check_thresholds();
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

    /// Long-line markers and the wrapped-layout verdict of `lines` lines of
    /// `chars` characters each, under the production policy.
    fn production_analysis(lines: usize, chars: usize) -> MinimapAnalysisResult {
        let line = "x".repeat(chars) + "\n";
        let mut analysis = MinimapAnalysisAccumulator::new(MinimapAnalysisPolicy::PRODUCTION, true);
        for _ in 0..lines {
            analysis.inspect_slice(line.chars(), usize::MAX);
        }
        analysis.finish()
    }

    /// No cap (`bin-minimap-markers-per-row` D6): every line above the
    /// warning threshold is retained, past `main`'s former 2,048, and the
    /// wrapped-layout budget is unchanged.
    #[test]
    fn production_analysis_keeps_every_long_line() {
        assert_eq!(production_analysis(2_049, 200).long_line_lines.len(), 2_049);
        let many = production_analysis(4_096, 130);
        assert_eq!(many.long_line_lines.len(), 4_096);
        assert_eq!(many.long_line_lines.last().copied(), Some(4_095));
        assert_eq!(MinimapAnalysisPolicy::PRODUCTION.marker_limit, usize::MAX);
        assert!(!production_analysis(1, 8_192).wrapped_layout_too_large);
        assert!(production_analysis(1, 8_193).wrapped_layout_too_large);
    }

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

    /// `inspect_char` over every character of every part, in order.
    fn per_character(
        policy: MinimapAnalysisPolicy,
        collect: bool,
        parts: &[&str],
    ) -> MinimapAnalysisAccumulator {
        let mut analysis = MinimapAnalysisAccumulator::new(policy, collect);
        for part in parts {
            for character in part.chars() {
                analysis.inspect_char(character);
            }
        }
        analysis
    }

    fn by_slices(
        policy: MinimapAnalysisPolicy,
        collect: bool,
        parts: &[&str],
    ) -> MinimapAnalysisAccumulator {
        let mut analysis = MinimapAnalysisAccumulator::new(policy, collect);
        for part in parts {
            analysis.inspect_str(part);
        }
        analysis
    }

    #[test]
    fn inspect_str_equals_inspect_char_for_every_two_way_split_and_policy() {
        let text = "ab\r\ncdéf\r\rgh😀ij\u{2029}k\n\u{2028}lmnopqrs\r";
        for warning in 0..6 {
            for wrapped in 0..9 {
                for marker_limit in 0..3 {
                    let policy = MinimapAnalysisPolicy {
                        warning_line_chars: warning,
                        wrapped_line_chars: wrapped,
                        marker_limit,
                    };
                    for collect in [false, true] {
                        let whole = per_character(policy, collect, &[text]);
                        assert_eq!(by_slices(policy, collect, &[text]), whole);
                        for cut in (0..=text.len()).filter(|&cut| text.is_char_boundary(cut)) {
                            let (head, tail) = text.split_at(cut);
                            assert_eq!(
                                by_slices(policy, collect, &[head, tail]),
                                whole,
                                "{policy:?} {collect} cut {cut}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn crlf_split_across_slices_is_one_line_break() {
        let split = by_slices(POLICY, true, &["ab\r", "\ncd"]);
        assert_eq!(split, per_character(POLICY, true, &["ab\r\ncd"]));
        let result = split.finish();
        assert_eq!(result.lines_examined, 2);
        assert_eq!(result.characters_examined, 6);
    }

    #[test]
    fn thresholds_crossed_inside_a_long_multibyte_line_mark_it_once() {
        let line = format!("abc\n{}\nend", "é日😀".repeat(5));
        let analysis = by_slices(POLICY, true, &[&line]);
        assert_eq!(analysis, per_character(POLICY, true, &[&line]));
        let result = analysis.finish();
        assert_eq!(result.long_line_lines, vec![1]);
        assert!(result.wrapped_layout_too_large);
    }

    #[test]
    fn inspect_str_saturates_like_inspect_char() {
        let mut near_max = MinimapAnalysisAccumulator::new(POLICY, true);
        near_max.current_line = u32::MAX - 1;
        near_max.current_line_chars = usize::MAX - 2;
        near_max.characters_examined = u64::MAX - 3;
        let text = "abcd\n\r\nxy\u{2029}\rzzzzzzzzz";
        let mut sliced = near_max.clone();
        sliced.inspect_str(text);
        let mut reference = near_max;
        for character in text.chars() {
            reference.inspect_char(character);
        }
        assert_eq!(sliced, reference);
        assert_eq!(sliced.current_line, u32::MAX);
        assert_eq!(sliced.characters_examined, u64::MAX);
    }

    #[test]
    fn a_line_shorter_than_every_threshold_in_bytes_is_not_counted() {
        let in_play = LineThresholds {
            wrapped: Some(8),
            warning: Some(4),
        };
        assert!(!paragraph_may_cross(1, 3, in_play));
        assert!(paragraph_may_cross(1, 4, in_play));
        assert!(!paragraph_may_cross(
            1,
            4,
            LineThresholds {
                warning: None,
                ..in_play
            }
        ));
        assert!(paragraph_may_cross(usize::MAX, 1, in_play));
        assert!(!paragraph_may_cross(
            usize::MAX,
            1,
            LineThresholds {
                wrapped: None,
                warning: None
            }
        ));
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
