// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the minimap fit functions:
//! [`fit_native_slider_to_source_map_bounds`], [`fit_marker_bounds`],
//! [`fit_projected_bounds`], [`expanded_to_min_height`], and
//! [`native_slider_estimate_from_inputs`].
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
//! Compiled only under `cfg(kani)` (`make kani`); ordinary builds and tests
//! never see this module. The harnesses check the functions that ship.

use super::{
    MINIMAP_VIEWPORT_MIN_HEIGHT, MarkerProjectionSpace, MinimapMarkerKind, MinimapProjectedBounds,
    MinimapProjectionSpace, NativeSliderEstimateInput, ProjectedBoundsFit, expanded_to_min_height,
    fit_marker_bounds, fit_native_slider_to_source_map_bounds, fit_projected_bounds,
    native_slider_estimate_from_inputs,
};

/// The largest coordinate magnitude of the whole-pixel domain.
const MAX_PIXEL: i32 = 1 << 20;

/// The largest minimum height of the whole-pixel domain.
const MAX_MIN_HEIGHT: i32 = 1 << 16;

/// The magnitude bound of the minimum-height harness's smaller domain.
const SMALL_PIXEL: i32 = 1 << 8;

/// The minimum-height bound of that smaller domain.
const SMALL_MIN_HEIGHT: i32 = 1 << 6;

/// A whole-pixel coordinate with magnitude at most [`MAX_PIXEL`].
fn whole_pixel() -> f64 {
    let pixel: i32 = kani::any();
    kani::assume((-MAX_PIXEL..=MAX_PIXEL).contains(&pixel));
    f64::from(pixel)
}

/// A whole-pixel minimum height in `0..=MAX_MIN_HEIGHT`.
fn whole_pixel_min_height() -> f64 {
    let height: i32 = kani::any();
    kani::assume((0..=MAX_MIN_HEIGHT).contains(&height));
    f64::from(height)
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

fn any_kind() -> MinimapMarkerKind {
    match kani::any::<u8>() % 4 {
        0 => MinimapMarkerKind::Bookmark,
        1 => MinimapMarkerKind::Search,
        2 => MinimapMarkerKind::Modified,
        _ => MinimapMarkerKind::LongLine,
    }
}

fn any_fit() -> ProjectedBoundsFit {
    if kani::any() {
        ProjectedBoundsFit::RejectOutside
    } else {
        ProjectedBoundsFit::ClampOutside
    }
}

fn any_bounds(coordinate: fn() -> f64) -> MinimapProjectedBounds {
    MinimapProjectedBounds {
        x: coordinate(),
        y: coordinate(),
        width: coordinate(),
        height: coordinate(),
    }
}

fn any_marker_space(coordinate: fn() -> f64, min_height: fn() -> f64) -> MarkerProjectionSpace {
    MarkerProjectionSpace {
        strip_height: coordinate(),
        content_top: coordinate(),
        content_bottom: coordinate(),
        map_y_in_strip: coordinate(),
        min_height: min_height(),
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
fn marker_fit_never_panics() {
    let fitted = fit_marker_bounds(
        any_kind(),
        kani::any(),
        kani::any(),
        any_marker_space(kani::any::<f64>, kani::any::<f64>),
    );
    if let Some(bounds) = fitted {
        assert!(bounds.top.is_finite() && bounds.bottom.is_finite());
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
        any_fit(),
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

// ─── Markers ──────────────────────────────────────────────────────────

/// Fit a marker from `coordinate` inputs; the band and the fitted marker.
fn fitted_marker(
    coordinate: fn() -> f64,
    min_height: fn() -> f64,
) -> Option<(f64, f64, f64, f64, f64)> {
    let raw_top = coordinate();
    let raw_bottom = coordinate();
    let space = any_marker_space(coordinate, min_height);
    let bounds = fit_marker_bounds(any_kind(), raw_top, raw_bottom, space)?;
    let (lower, upper) = band(space.content_top, space.content_bottom, space.strip_height);
    kani::cover!(raw_top.min(raw_bottom) < lower, "clamped at the top");
    kani::cover!(raw_top.max(raw_bottom) > upper, "clamped at the bottom");
    kani::cover!(
        raw_top.max(raw_bottom) - raw_top.min(raw_bottom) < space.min_height,
        "expanded to the minimum height"
    );
    Some((lower, upper, bounds.top, bounds.bottom, space.min_height))
}

/// A fitted marker lies inside the rendered-content band, so it never reaches
/// the EOF overscroll tail. This holds for every finite `f64`: the edges are
/// clamped with `max`/`min`, which are exact.
#[kani::proof]
fn marker_bounds_stay_in_content_for_finite_f64() {
    if let Some((lower, upper, top, bottom, _)) = fitted_marker(finite, finite) {
        assert!(lower <= top && top < bottom && bottom <= upper);
    }
}

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
    let small = || {
        let pixel: i32 = kani::any();
        kani::assume((-SMALL_PIXEL..=SMALL_PIXEL).contains(&pixel));
        f64::from(pixel)
    };
    let lower = small();
    let upper = small();
    let top = small();
    let bottom = small();
    let min_height: i32 = kani::any();
    kani::assume((0..=SMALL_MIN_HEIGHT).contains(&min_height));
    let min_height = f64::from(min_height);
    kani::assume(lower < upper && lower <= top && top <= bottom && bottom <= upper);
    let (top, bottom) = expanded_to_min_height(top, bottom, lower, upper, min_height);
    kani::cover!(bottom - top > 0.0, "a non-empty result");
    assert!(lower <= top && top <= bottom && bottom <= upper);
    assert!(bottom - top >= min_height.min(upper - lower));
}

/// With fractional coordinates the centred expansion rounds, and the marker can
/// come out a hair shorter than the minimum.
#[kani::proof]
#[kani::should_panic]
fn marker_min_height_fails_for_general_f64() {
    if let Some((lower, upper, top, bottom, min_height)) =
        fitted_marker(fractional_pixel, fractional_min_height)
    {
        assert!(bottom - top >= min_height.min(upper - lower));
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
    min_height: f64,
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
    let fit = any_fit();
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
        min_height: min,
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
