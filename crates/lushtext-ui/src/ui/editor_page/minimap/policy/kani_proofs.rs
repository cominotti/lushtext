// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the minimap fit functions:
//! [`fit_native_slider_to_source_map_bounds`], [`fit_projected_bounds`],
//! [`expanded_to_min_height`], and [`native_slider_estimate_from_inputs`];
//! and over the integer marker row binning, [`marker_row_groups`],
//! [`mark_row_groups`] and [`marker_row_runs`] (`marker_rows_*`), on a
//! bounded domain: bands of one to three rows and documents of at most eight
//! lines, marker sets as eight-bit masks; the run growth on every whole-pixel
//! band up to 2^20 and the merge of two runs on bands up to 16 rows. `fit_marker_bounds`, its three
//! harnesses and `MarkerProjectionSpace` were retired with the binning
//! (`bin-minimap-markers-per-row` D8).
//!
//! Two domains, as in the `gtk-lush-widgets` slice-geometry precedent:
//!
//! - **No panic** is claimed for every `f64` — NaN, both infinities,
//!   subnormals — and every `i32`, and any coordinate a function returns is
//!   finite. The one exception is the private [`expanded_to_min_height`],
//!   whose callers guarantee a finite, non-empty band containing the span;
//!   it is proved on that precondition, and a `should_panic` harness keeps
//!   the inverted-band panic the precondition excludes.
//! - **Containment** is claimed on the **whole-pixel domain**: every
//!   coordinate an integer with magnitude at most 2^20, every minimum height an
//!   integer in `0..=2^16`. The **minimum-height guarantee** is proved on a
//!   smaller whole-pixel domain (magnitude at most 2^8, minimum heights in
//!   `0..=2^6`), because CBMC did not finish it on the larger one; see
//!   [`min_height_expansion_reaches_the_minimum_on_small_whole_pixels`]. There every sum,
//!   difference, midpoint (a half-integer), and half-height is exact in `f64`,
//!   so the property is a real theorem. Where a property fails for general
//!   finite `f64`, the counterexample is kept as a `should_panic` harness, so a
//!   future claim of a broader domain has to confront it.
//!
//! The analysis accumulator's byte-length shortcut, [`paragraph_may_cross`],
//! is proved sound over abstract lengths (no text, so no model of the SIMD
//! search is needed): [`skipped_line_crosses_no_threshold`] for every running
//! count, byte length, scalar count, and threshold pair, and
//! [`skipped_line_crosses_no_threshold_per_character`] steps the
//! per-character count itself for up to four scalars.
//!
//! Compiled only under `cfg(kani)` (`make kani`); ordinary builds and tests
//! never see this module. The harnesses check the functions that ship.

use super::{
    LineThresholds, MINIMAP_VIEWPORT_MIN_HEIGHT, MarkerRowBand, MarkerRowGroup, MarkerRowRun,
    MinimapProjectedBounds, MinimapProjectionSpace, NativeSliderEstimateInput, ProjectedBoundsFit,
    expanded_run, expanded_to_min_height, fit_native_slider_to_source_map_bounds,
    fit_projected_bounds, mark_row_groups, marker_row_groups, marker_row_runs,
    native_slider_estimate_from_inputs, paragraph_may_cross, push_expanded_run,
};

/// The largest coordinate magnitude of the whole-pixel domain.
const MAX_PIXEL: i32 = 1 << 20;

/// The largest minimum height of the whole-pixel domain.
const MAX_MIN_HEIGHT: i32 = 1 << 16;

/// The magnitude bound of the minimum-height harness's smaller domain.
const SMALL_PIXEL: i32 = 1 << 8;

/// The minimum-height bound of that smaller domain.
const SMALL_MIN_HEIGHT: i32 = 1 << 6;

/// A whole pixel in `range`, as the `f64` the fits take.
fn pixel_in(range: std::ops::RangeInclusive<i32>) -> f64 {
    let pixel: i32 = kani::any();
    kani::assume(range.contains(&pixel));
    f64::from(pixel)
}

/// A whole-pixel coordinate with magnitude at most [`MAX_PIXEL`].
fn whole_pixel() -> f64 {
    pixel_in(-MAX_PIXEL..=MAX_PIXEL)
}

/// A whole-pixel minimum height in `0..=MAX_MIN_HEIGHT`.
fn whole_pixel_min_height() -> f64 {
    pixel_in(0..=MAX_MIN_HEIGHT)
}

/// Any finite `f64`.
fn finite() -> f64 {
    let value: f64 = kani::any();
    kani::assume(value.is_finite());
    value
}

/// Any `f64`, fractional included, inside the whole-pixel magnitude bound. The
/// `should_panic` counterexamples draw from here: nothing can overflow, so the
/// only failure Kani can report is the rounding one each harness names.
fn fractional_pixel() -> f64 {
    let value: f64 = kani::any();
    kani::assume(value.is_finite() && value.abs() <= f64::from(MAX_PIXEL));
    value
}

/// A fractional minimum height inside the whole-pixel bound.
fn fractional_min_height() -> f64 {
    let value: f64 = kani::any();
    kani::assume(value.is_finite() && (0.0..=f64::from(MAX_MIN_HEIGHT)).contains(&value));
    value
}

fn any_bounds(coordinate: fn() -> f64) -> MinimapProjectedBounds {
    MinimapProjectedBounds {
        x: coordinate(),
        y: coordinate(),
        width: coordinate(),
        height: coordinate(),
    }
}

fn any_projection_space(coordinate: fn() -> f64) -> MinimapProjectionSpace {
    MinimapProjectionSpace {
        target_height: coordinate(),
        map_x: coordinate(),
        map_y: coordinate(),
        map_width: coordinate(),
        content_top: coordinate(),
        content_bottom: coordinate(),
    }
}

fn is_finite_bounds(bounds: MinimapProjectedBounds) -> bool {
    bounds.x.is_finite()
        && bounds.y.is_finite()
        && bounds.width.is_finite()
        && bounds.height.is_finite()
}

/// The rendered-content band `[max(content_top, 0), min(content_bottom, h)]`.
fn band(content_top: f64, content_bottom: f64, height: f64) -> (f64, f64) {
    (content_top.max(0.0), content_bottom.min(height))
}

// ─── No panic, any f64 ────────────────────────────────────────────────

#[kani::proof]
fn native_slider_fit_never_panics() {
    let fitted = fit_native_slider_to_source_map_bounds(
        any_bounds(kani::any::<f64>),
        any_bounds(kani::any::<f64>),
    );
    if let Some(bounds) = fitted {
        assert!(is_finite_bounds(bounds));
    }
}

#[kani::proof]
fn projected_fit_never_panics() {
    let fitted = fit_projected_bounds(
        kani::any(),
        kani::any(),
        kani::any(),
        kani::any(),
        any_projection_space(kani::any::<f64>),
        kani::any(),
        kani::any::<ProjectedBoundsFit>(),
    );
    if let Some(bounds) = fitted {
        assert!(is_finite_bounds(bounds));
    }
}

/// `expanded_to_min_height` is private, and both callers hand it a finite,
/// non-empty band with the span already inside it; only the minimum height is
/// unchecked. On that precondition it never panics, for every `f64` minimum
/// height, and it returns finite edges inside the band.
#[kani::proof]
fn min_height_expansion_never_panics_inside_its_band() {
    let lower = finite();
    let upper = finite();
    let top = finite();
    let bottom = finite();
    kani::assume(lower < upper && lower <= top && top <= bottom && bottom <= upper);
    let (top, bottom) = expanded_to_min_height(top, bottom, lower, upper, kani::any());
    assert!(top.is_finite() && bottom.is_finite());
    assert!(lower <= top && top <= bottom && bottom <= upper);
}

/// Outside that precondition it does panic (`f64::clamp` on an inverted band),
/// which is why the precondition is stated rather than the claim widened. The
/// inputs are finite so the only failure Kani can report is that panic.
#[kani::proof]
#[kani::should_panic]
fn min_height_expansion_panics_on_an_inverted_band() {
    let lower = finite();
    let upper = finite();
    kani::assume(lower > upper);
    let _ = expanded_to_min_height(finite(), finite(), lower, upper, finite());
}

#[kani::proof]
fn native_slider_estimate_never_panics() {
    let input = NativeSliderEstimateInput {
        map_x: kani::any(),
        map_y: kani::any(),
        map_width: kani::any(),
        editor_visible_y: kani::any(),
        editor_visible_height: kani::any(),
        editor_document_height: kani::any(),
        source_map_visible_y: kani::any(),
        source_map_document_height: kani::any(),
        border_left: kani::any(),
        border_right: kani::any(),
    };
    if let Some(bounds) = native_slider_estimate_from_inputs(input) {
        assert!(is_finite_bounds(bounds));
    }
}

// ─── Minimum height ───────────────────────────────────────────────────

/// The minimum-height guarantee, proved on the helper both fits delegate it
/// to: grown from any span inside a non-empty band, the result is at least the
/// smaller of the minimum height and the band height. Both fits hand
/// [`expanded_to_min_height`] a span already clamped inside the band and
/// return its edges unchanged (the projected fit as `y` and `bottom - top`),
/// so the guarantee carries over to them.
///
/// **Domain: whole pixels with magnitude at most 2^8, minimum heights in
/// `0..=2^6`.** Over the 2^20 whole-pixel domain the containment harnesses
/// use, CBMC did not finish this property in 30 minutes (nor at 2^12 in 15),
/// either through a fit function or on the helper directly; at 2^8 it proves
/// in about 1.5 minutes. The wider domain is exercised by the unit test
/// `expanded_to_min_height_reaches_the_minimum_on_whole_pixels`.
#[kani::proof]
fn min_height_expansion_reaches_the_minimum_on_small_whole_pixels() {
    let small = || pixel_in(-SMALL_PIXEL..=SMALL_PIXEL);
    let lower = small();
    let upper = small();
    let top = small();
    let bottom = small();
    let min_height = pixel_in(0..=SMALL_MIN_HEIGHT);
    kani::assume(lower < upper && lower <= top && top <= bottom && bottom <= upper);
    let (top, bottom) = expanded_to_min_height(top, bottom, lower, upper, min_height);
    kani::cover!(bottom - top > 0.0, "a non-empty result");
    assert!(lower <= top && top <= bottom && bottom <= upper);
    assert!(bottom - top >= min_height.min(upper - lower));
}

// ─── Marker row binning ───────────────────────────────────────────────

/// Document lines the binning harnesses explore; marker sets are bitmasks
/// over them.
const MAX_LINES: u32 = 8;

/// The first set bit of `markers` at or after `from`: the index query of a
/// marker set held as a bitmask, loop-free so the solver unwinds nothing.
fn first_marker_bit(markers: u8, from: u32) -> Option<u32> {
    let masked = u32::from(markers) & !((1u32 << from.min(31)) - 1);
    (masked != 0).then(|| masked.trailing_zeros())
}

/// Whether `markers` holds a line in `[start, end)`, loop-free.
fn holds_marker_in(markers: u8, start: u32, end: u32) -> bool {
    let end = end.min(MAX_LINES + 1);
    start < end && (u32::from(markers) >> start.min(31)) & ((1u32 << (end - start)) - 1) != 0
}

/// A band of exactly `N` rows starting at a small offset, and an arbitrary
/// row table for it: entries in `0..MAX_LINES`, not assumed monotone (the
/// grouping makes them monotone by construction). Each property has one
/// harness per `N` of one to three rows (an empty band has no groups and no
/// runs by construction). **Domain: at most three rows**, a reduced bound
/// recorded in the programme record: at four rows the walk and run-builder
/// harnesses exhausted the 14 GiB address-space cap in CBMC's propositional
/// reduction. The unit property tests run the same properties over bands of
/// up to 53 rows.
fn row_table<const N: usize>() -> (MarkerRowBand, [u32; N], u32) {
    // The grouping is translation-invariant in the band's first row, so the
    // band starts at a fixed row; a symbolic start exhausted the solver's
    // memory even at one row.
    let lower: i32 = 4;
    let lines: [u32; N] = kani::any();
    for line in lines {
        kani::assume(line < MAX_LINES);
    }
    let past: u32 = kani::any();
    kani::assume(past <= MAX_LINES);
    let upper = lower + i32::try_from(N).unwrap_or(0);
    (MarkerRowBand { lower, upper }, lines, past)
}

/// Group starts strictly increase, line ranges are contiguous and non-empty,
/// and the groups partition the band's rows: every row is in exactly one.
fn groups_partition_the_band<const N: usize>() {
    let (band, lines, past) = row_table::<N>();
    let groups = marker_row_groups(band, &lines, past);
    let mut row = band.lower;
    for (index, group) in groups.iter().enumerate() {
        assert!(group.row_start == row);
        assert!(group.row_end > group.row_start);
        assert!(group.line_end > group.line_start);
        if let Some(next) = groups.get(index + 1) {
            assert!(group.line_start < next.line_start);
            assert!(group.line_end == next.line_start);
        }
        row = group.row_end;
    }
    assert!(row == band.upper);
}

#[kani::proof]
#[kani::unwind(5)]
fn marker_rows_groups_partition_the_band_one_row() {
    groups_partition_the_band::<1>();
}

#[kani::proof]
#[kani::unwind(5)]
fn marker_rows_groups_partition_the_band_two_rows() {
    groups_partition_the_band::<2>();
}

#[kani::proof]
#[kani::unwind(6)]
fn marker_rows_groups_partition_the_band_three_rows() {
    groups_partition_the_band::<3>();
}

/// For any marker-line set, the walker marks exactly the groups whose line
/// range holds a marker line, so each in-band marker line lands in exactly
/// one group (the ranges are disjoint), and it asks the index at most
/// `groups + 1` times.
fn walk_is_sound_complete_and_bounded<const N: usize>() {
    let (band, lines, past) = row_table::<N>();
    let groups = marker_row_groups(band, &lines, past);
    let markers: u8 = kani::any();
    let (marked, queries) = mark_row_groups(&groups, |from| first_marker_bit(markers, from));
    kani::cover!(marked.contains(&true), "a marked group");
    assert!(queries <= groups.len() + 1);
    for (group, &is_marked) in groups.iter().zip(&marked) {
        assert!(is_marked == holds_marker_in(markers, group.line_start, group.line_end));
    }
}

#[kani::proof]
#[kani::unwind(5)]
fn marker_rows_walk_is_sound_complete_and_bounded_one_row() {
    walk_is_sound_complete_and_bounded::<1>();
}

#[kani::proof]
#[kani::unwind(5)]
fn marker_rows_walk_is_sound_complete_and_bounded_two_rows() {
    walk_is_sound_complete_and_bounded::<2>();
}

#[kani::proof]
#[kani::unwind(6)]
fn marker_rows_walk_is_sound_complete_and_bounded_three_rows() {
    walk_is_sound_complete_and_bounded::<3>();
}

/// Runs stay inside the band (no row of the EOF tail), are disjoint and in
/// increasing order, cover every marked row, are at least the smaller of the
/// minimum height and the band height, and number at most `ceil(rows / 2)`.
///
/// The groups are one row each: a taller group behaves as adjacent marked
/// one-row groups, which coalesce into the same run. Separate runs that meet
/// after growing are also proved on bands up to 16 rows by
/// `marker_rows_merge_keeps_runs_disjoint_and_ordered`, and the growth itself
/// on every whole-pixel band up to 2^20 by
/// `marker_rows_expansion_stays_in_the_band_on_whole_pixels`.
fn runs_stay_in_the_band<const N: usize>() {
    // Translation-invariant, as for the row table above.
    let lower: i32 = 4;
    let band = MarkerRowBand {
        lower,
        upper: lower + i32::try_from(N).unwrap_or(0),
    };
    let groups: Vec<MarkerRowGroup> = (0..N)
        .map(|index| {
            let row = lower + i32::try_from(index).unwrap_or(0);
            let line = u32::try_from(index).unwrap_or(0);
            MarkerRowGroup {
                row_start: row,
                row_end: row + 1,
                line_start: line,
                line_end: line + 1,
            }
        })
        .collect();
    let marked: [bool; N] = kani::any();
    let min_height: i32 = kani::any();
    kani::assume((0..=6).contains(&min_height));
    let runs = marker_row_runs(&groups, &marked, band, min_height);
    kani::cover!(
        runs.last().is_some_and(|run| run.bottom == band.upper),
        "a run against the EOF tail"
    );
    assert!(runs.len() <= band.rows().div_ceil(2));
    for (index, run) in runs.iter().enumerate() {
        assert!(band.lower <= run.top && run.top < run.bottom && run.bottom <= band.upper);
        assert!(run.bottom - run.top >= min_height.min(band.upper - band.lower));
        if let Some(next) = runs.get(index + 1) {
            assert!(run.bottom < next.top);
        }
    }
    for (group, &is_marked) in groups.iter().zip(&marked) {
        if is_marked {
            assert!(
                runs.iter()
                    .any(|run| run.top <= group.row_start && group.row_end <= run.bottom)
            );
        }
    }
}

#[kani::proof]
#[kani::unwind(6)]
fn marker_rows_runs_stay_in_the_band_at_the_minimum_height_one_row() {
    runs_stay_in_the_band::<1>();
}

#[kani::proof]
#[kani::unwind(6)]
fn marker_rows_runs_stay_in_the_band_at_the_minimum_height_two_rows() {
    runs_stay_in_the_band::<2>();
}

#[kani::proof]
#[kani::unwind(6)]
fn marker_rows_runs_stay_in_the_band_at_the_minimum_height_three_rows() {
    runs_stay_in_the_band::<3>();
}

/// The integer growth of one run, on the whole-pixel domain of the fits
/// (coordinates with magnitude at most 2^20, minimum heights up to 2^16): the
/// result stays inside the band, covers the run, and is the target height
/// exactly when the run was shorter, else unchanged.
#[kani::proof]
fn marker_rows_expansion_stays_in_the_band_on_whole_pixels() {
    let row = || {
        let value: i32 = kani::any();
        kani::assume((-MAX_PIXEL..=MAX_PIXEL).contains(&value));
        value
    };
    let band = MarkerRowBand {
        lower: row(),
        upper: row(),
    };
    let run = MarkerRowRun {
        top: row(),
        bottom: row(),
    };
    kani::assume(band.lower <= run.top && run.top < run.bottom && run.bottom <= band.upper);
    let min_height: i32 = kani::any();
    kani::assume((0..=MAX_MIN_HEIGHT).contains(&min_height));
    let target = min_height.min(band.upper - band.lower);
    let grown = expanded_run(run, band, target);
    kani::cover!(grown.top < run.top, "grown above");
    kani::cover!(
        grown.bottom == band.upper && run.bottom < band.upper,
        "pushed off the EOF tail"
    );
    assert!(band.lower <= grown.top && grown.top <= run.top);
    assert!(run.bottom <= grown.bottom && grown.bottom <= band.upper);
    let height = run.bottom - run.top;
    if height < target {
        assert!(grown.bottom - grown.top == target);
    } else {
        assert!(grown == run);
    }
}

/// Appending a run after an earlier one (the coalescing loop guarantees a
/// gap of at least one row between them) leaves the runs disjoint, in
/// increasing order and inside the band, each at least the target height,
/// and every row of both covered: they stay apart, or they met after growing
/// and merged.
#[kani::proof]
#[kani::unwind(4)]
fn marker_rows_merge_keeps_runs_disjoint_and_ordered() {
    let small = || {
        let value: i32 = kani::any();
        kani::assume((0..=16).contains(&value));
        value
    };
    let band = MarkerRowBand {
        lower: small(),
        upper: small(),
    };
    let first = MarkerRowRun {
        top: small(),
        bottom: small(),
    };
    let second = MarkerRowRun {
        top: small(),
        bottom: small(),
    };
    kani::assume(band.lower <= first.top && first.top < first.bottom);
    kani::assume(first.bottom < second.top && second.top < second.bottom);
    kani::assume(second.bottom <= band.upper);
    let min_height: i32 = kani::any();
    kani::assume((0..=8).contains(&min_height));
    let target = min_height.min(band.upper - band.lower);
    let mut runs = Vec::with_capacity(2);
    push_expanded_run(&mut runs, first, band, target);
    push_expanded_run(&mut runs, second, band, target);
    kani::cover!(runs.len() == 1, "merged");
    kani::cover!(runs.len() == 2, "kept apart");
    assert!(!runs.is_empty() && runs.len() <= 2);
    for (index, run) in runs.iter().enumerate() {
        assert!(band.lower <= run.top && run.top < run.bottom && run.bottom <= band.upper);
        assert!(run.bottom - run.top >= target);
        if let Some(next) = runs.get(index + 1) {
            assert!(run.bottom < next.top);
        }
    }
    for original in [first, second] {
        assert!(
            runs.iter()
                .any(|run| run.top <= original.top && original.bottom <= run.bottom)
        );
    }
}

// ─── Projected rectangles ─────────────────────────────────────────────

/// The inputs and result of one projected fit.
struct ProjectedFit {
    x: f64,
    width: f64,
    raw_top: f64,
    raw_bottom: f64,
    lower: f64,
    upper: f64,
    fit: ProjectedBoundsFit,
    fitted: Option<MinimapProjectedBounds>,
}

fn projected_fit(coordinate: fn() -> f64, min_height: fn() -> f64) -> ProjectedFit {
    let x = coordinate();
    let width = coordinate();
    let raw_top = coordinate();
    let raw_bottom = coordinate();
    let space = any_projection_space(coordinate);
    let min = min_height();
    let fit: ProjectedBoundsFit = kani::any();
    let fitted = fit_projected_bounds(x, width, raw_top, raw_bottom, space, min, fit);
    let (lower, upper) = band(space.content_top, space.content_bottom, space.target_height);
    if fitted.is_some() {
        kani::cover!(raw_top.min(raw_bottom) < lower, "clamped at the top");
        kani::cover!(raw_top.max(raw_bottom) > upper, "clamped at the bottom");
        kani::cover!(
            raw_top.max(raw_bottom) - raw_top.min(raw_bottom) < min,
            "expanded to the minimum height"
        );
    }
    ProjectedFit {
        x,
        width,
        raw_top,
        raw_bottom,
        lower,
        upper,
        fit,
        fitted,
    }
}

/// `RejectOutside` returns nothing for a span entirely outside the band. Only
/// comparisons decide it, so this holds for every finite `f64`.
#[kani::proof]
fn projected_fit_rejects_a_span_outside_the_band() {
    let fit = projected_fit(finite, finite);
    if fit.fit == ProjectedBoundsFit::RejectOutside
        && (fit.raw_top.max(fit.raw_bottom) < fit.lower
            || fit.raw_top.min(fit.raw_bottom) > fit.upper)
    {
        assert!(fit.fitted.is_none());
    }
}

/// A fitted projected rectangle keeps its horizontal position and width and
/// lies inside the rendered-content band, `lower <= y < y + height <= upper`,
/// on whole pixels.
#[kani::proof]
fn projected_bounds_stay_in_content_on_whole_pixels() {
    let fit = projected_fit(whole_pixel, whole_pixel_min_height);
    if let Some(bounds) = fit.fitted {
        assert!(bounds.x == fit.x && bounds.width == fit.width);
        assert!(fit.lower <= bounds.y && bounds.height > 0.0 && bounds.bottom() <= fit.upper);
    }
}

/// With fractional coordinates `y + height` can round above the band.
#[kani::proof]
#[kani::should_panic]
fn projected_containment_fails_for_general_f64() {
    let fit = projected_fit(fractional_pixel, fractional_min_height);
    if let Some(bounds) = fit.fitted {
        assert!(bounds.bottom() <= fit.upper);
    }
}

// ─── Native slider ────────────────────────────────────────────────────

/// Fit a native slider from `coordinate` inputs.
fn fitted_native_slider(
    coordinate: fn() -> f64,
) -> Option<(
    MinimapProjectedBounds,
    MinimapProjectedBounds,
    MinimapProjectedBounds,
)> {
    let raw = any_bounds(coordinate);
    let map = any_bounds(coordinate);
    let fitted = fit_native_slider_to_source_map_bounds(raw, map)?;
    kani::cover!(raw.y < map.y, "clamped at the top");
    kani::cover!(
        raw.y + raw.height > map.y + map.height,
        "clamped at the bottom"
    );
    kani::cover!(raw.height < MINIMAP_VIEWPORT_MIN_HEIGHT, "expanded");
    Some((raw, map, fitted))
}

/// The fitted native slider keeps its horizontal position and width and lies
/// vertically inside the source-map bounds, on whole pixels.
#[kani::proof]
fn native_slider_stays_in_the_source_map_on_whole_pixels() {
    if let Some((raw, map, fitted)) = fitted_native_slider(whole_pixel) {
        assert!(fitted.x == raw.x && fitted.width == raw.width);
        assert!(fitted.height > 0.0);
        assert!(map.y <= fitted.y && fitted.bottom() <= map.y + map.height);
    }
}

/// With fractional coordinates the fitted slider's bottom can round past the
/// source map's.
#[kani::proof]
#[kani::should_panic]
fn native_slider_containment_fails_for_general_f64() {
    if let Some((_, map, fitted)) = fitted_native_slider(fractional_pixel) {
        assert!(fitted.bottom() <= map.y + map.height);
    }
}

/// Any line content of `content_bytes` bytes holds `chars` scalars, at most
/// one per byte and at least one when it is not empty; and any threshold
/// pair, each in play or not.
fn line_extension() -> (usize, usize, usize, LineThresholds) {
    let running: usize = kani::any();
    let content_bytes: usize = kani::any();
    let chars: usize = kani::any();
    kani::assume(chars <= content_bytes);
    kani::assume(content_bytes == 0 || chars >= 1);
    let thresholds = LineThresholds {
        wrapped: kani::any(),
        warning: kani::any(),
    };
    (running, content_bytes, chars, thresholds)
}

/// When the shortcut skips the count, the line's largest running count,
/// `running + chars` (saturating; the count only grows), stays at or below
/// every threshold in play, so per-character inspection crosses neither.
#[kani::proof]
#[kani::unwind(3)]
fn skipped_line_crosses_no_threshold() {
    let (running, content_bytes, chars, thresholds) = line_extension();
    if !paragraph_may_cross(running, content_bytes, thresholds) {
        let reached = running.saturating_add(chars);
        for threshold in [thresholds.wrapped, thresholds.warning]
            .into_iter()
            .flatten()
        {
            assert!(reached <= threshold);
        }
        kani::cover!(
            chars > 0 && thresholds.wrapped.is_some(),
            "skipped a counted line"
        );
    }
    kani::cover!(
        paragraph_may_cross(running, content_bytes, thresholds),
        "the count is needed"
    );
}

/// The same, stepping `inspect_char`'s count one scalar at a time (up to four
/// scalars, so the loop unwinds), checking every intermediate count.
#[kani::proof]
#[kani::unwind(5)]
fn skipped_line_crosses_no_threshold_per_character() {
    let (running, content_bytes, chars, thresholds) = line_extension();
    kani::assume(chars <= 4);
    if !paragraph_may_cross(running, content_bytes, thresholds) {
        let mut count = running;
        for _ in 0..chars {
            count = count.saturating_add(1);
            for threshold in [thresholds.wrapped, thresholds.warning]
                .into_iter()
                .flatten()
            {
                assert!(count <= threshold);
            }
        }
        kani::cover!(chars == 4, "four scalars skipped");
    }
}
