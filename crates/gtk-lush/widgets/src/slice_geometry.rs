// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure viewport-slice geometry for [`crate::ViewportSliceBin`].
//!
//! The container hosts a `GtkScrollable` child (typically a `GtkListView`)
//! inside an outer scroller while still advertising the child's full content
//! height. `GtkListView` realizes at most `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200,
//! plus two extra items) row widgets for one visible range, so a list view
//! that is handed its whole content as viewport renders blank space after
//! roughly the two-hundredth row. This module decides which band of the
//! content the child is actually allocated, so its own viewport stays the size
//! of the real one — including the part of it that chrome above the content
//! has already taken, because the child decides for itself whether a row is
//! on screen and it can only be right if its band is. It has no GTK dependency
//! so it can be unit- and property-tested directly.

/// The smallest band a child with content is allocated: one logical pixel, so
/// content entirely off screen is never allocated zero height (ledger A5).
const MIN_BAND_HEIGHT: f64 = 1.0;

/// The band of content, in logical pixels, that the child should be allocated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportSlice {
    /// Offset of the slice from the top of the content.
    pub top: f64,
    /// Height of the slice.
    pub height: f64,
}

/// Decide which content band to allocate for a viewport.
///
/// * `viewport_top` is the outer viewport's top edge relative to the top of the
///   content; it may be negative when the content starts below the viewport.
/// * `viewport_height` is the outer viewport's visible height. The band gets
///   what is left of it after whatever the host draws above this content, so a
///   child that places a row at its own bottom edge places it at the fold.
/// * `content_height` is the child's full natural height.
/// * `overscan` is extra content allocated above and below the viewport. Zero
///   keeps the child's own viewport identical to the visible one, which lets a
///   `GtkListView`'s `scroll_to` and keyboard focus logic decide visibility
///   correctly; a positive value trades that precision for fewer row
///   realizations while scrolling.
///
/// Content the viewport shows whole is returned whole at offset zero; content
/// it shows in part gets only that part, however short the content is.
///
/// # Domain
///
/// The function never panics, for any `f64` input: NaN, infinities, and
/// negative lengths degrade to zero. Its geometric guarantees are stated on
/// the **whole-pixel domain** GTK actually produces — every argument an
/// integer value in the `i32` range, with `overscan` in `0..=4096` — where the
/// returned slice always lies inside the content (`top >= 0`, `height >= 0`,
/// `top + height <= max(content_height, 0)`) and always contains the
/// intersection of the viewport with the content. Both are proved there by
/// Kani (`make kani`). They are not claimed for arbitrary `f64`: near `1e260`
/// rounding can make `top + height` exceed the content by one ulp, a
/// counterexample Kani keeps as a `should_panic` harness.
#[must_use]
pub fn viewport_slice(
    viewport_top: f64,
    viewport_height: f64,
    content_height: f64,
    overscan: f64,
) -> ViewportSlice {
    let content_height = sanitize(content_height);
    let viewport_height = sanitize(viewport_height);
    let overscan = sanitize(overscan);
    let viewport_top = if viewport_top.is_finite() {
        viewport_top
    } else {
        0.0
    };

    // The child decides from its own allocation whether a row is on screen
    // (`scroll_to`, keyboard focus), so its band is exactly the part of the
    // content the viewport shows: chrome the host draws above this content
    // is deducted at the top, and content the viewport has scrolled past, or
    // not yet reached, at the bottom. A band wider than what shows makes the
    // child believe rows are on screen that are not, and it then never asks
    // for them: a `scroll_to` of a row inside a scrolled-past band was a
    // no-op.
    //
    // The band never collapses to nothing, though. A zero-height allocation
    // makes `GtkListView` rewrite the adjustment this container owns (ledger
    // A5), which reads as a scroll request; content entirely off screen keeps
    // a one-pixel band at the edge nearest the viewport instead, so the child
    // still sees that its rows are not visible.
    let visible_top = viewport_top.max(0.0);
    let visible_bottom = (viewport_top + viewport_height).min(content_height);
    let visible_height = (visible_bottom - visible_top).max(0.0);
    let height = (visible_height + 2.0 * overscan)
        .max(MIN_BAND_HEIGHT.min(content_height))
        .min(content_height);
    let top = (visible_top - overscan).clamp(0.0, content_height - height);
    ViewportSlice { top, height }
}

/// Round a slice to the whole-pixel band the bin allocates inside a widget of
/// `height` pixels: `(top, height)`, with `0 <= top` and
/// `top + height <= max(height, 0)` for every input, finite or not.
///
/// The band is at least `min_band` pixels tall (as far as the widget allows)
/// and stays at the slice's edge when the floor widens it. The bin passes its
/// child's learned content-box inset plus one, so the page the child works in
/// is never zero (ledger A5, A6) even where [`viewport_slice`] returns its
/// one-pixel off-screen band.
///
/// A scrollable child's vertical minimum is its content height; GTK exempts
/// `GtkScrollable` widgets from the under-allocation check for exactly this
/// reason, so the band may legitimately be smaller than `height`.
pub(crate) fn whole_pixel_band(slice: ViewportSlice, height: i32, min_band: i32) -> (i32, i32) {
    let height = height.max(0);
    let band_height = whole_pixels(slice.height)
        .max(min_band.clamp(0, height))
        .min(height);
    let band_top = whole_pixels(slice.top).clamp(0, height - band_height);
    (band_top, band_height)
}

/// Round a logical-pixel length to whole pixels for GTK allocation.
#[expect(
    clippy::cast_possible_truncation,
    reason = "saturating float-to-int casts are the intent; the band is then clamped to a widget height"
)]
fn whole_pixels(value: f64) -> i32 {
    value.round().max(0.0) as i32
}

fn sanitize(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_shown_whole_is_returned_whole() {
        let slice = viewport_slice(-120.0, 665.0, 300.0, 100.0);
        assert_eq!(
            slice,
            ViewportSlice {
                top: 0.0,
                height: 300.0
            }
        );
    }

    #[test]
    fn short_content_shown_in_part_only_gets_the_part_that_shows() {
        // A 300px section whose first 120px have scrolled above the viewport.
        let slice = viewport_slice(120.0, 665.0, 300.0, 0.0);
        assert_eq!(
            slice,
            ViewportSlice {
                top: 120.0,
                height: 180.0
            }
        );
    }

    #[test]
    fn zero_content_yields_an_empty_slice() {
        assert_eq!(
            viewport_slice(50.0, 400.0, 0.0, 0.0),
            ViewportSlice {
                top: 0.0,
                height: 0.0
            }
        );
    }

    #[test]
    fn middle_of_the_content_slices_exactly_the_viewport_without_overscan() {
        let slice = viewport_slice(8_000.0, 665.0, 11_486.0, 0.0);
        assert_eq!(
            slice,
            ViewportSlice {
                top: 8_000.0,
                height: 665.0
            }
        );
    }

    #[test]
    fn overscan_widens_the_slice_on_both_sides() {
        let slice = viewport_slice(8_000.0, 665.0, 11_486.0, 665.0);
        assert_eq!(slice.top, 7_335.0);
        assert_eq!(slice.height, 1_995.0);
    }

    #[test]
    fn a_content_start_below_the_viewport_only_gets_the_visible_remainder() {
        // 40px of the viewport are spent on whatever the host draws above this
        // content, so 625px of it show; the overscan adds 100 on each side and
        // the top one is clamped away at the content edge.
        let slice = viewport_slice(-40.0, 665.0, 11_486.0, 100.0);
        assert_eq!(slice.top, 0.0);
        assert_eq!(slice.height, 825.0);
    }

    #[test]
    fn a_content_end_inside_the_viewport_only_gets_the_visible_remainder() {
        // The viewport shows the last 86px of the content and whatever the
        // host draws below it; the band is those 86px, not a full viewport
        // reaching 579px above the viewport top.
        let slice = viewport_slice(11_400.0, 665.0, 11_486.0, 0.0);
        assert_eq!(slice.top, 11_400.0);
        assert_eq!(slice.height, 86.0);
    }

    #[test]
    fn content_off_screen_keeps_a_one_pixel_band_at_its_nearest_edge() {
        // Never zero: a zero-height allocation makes the child rewrite the
        // adjustment this container owns, which would read as a scroll request.
        let above = viewport_slice(20_000.0, 665.0, 11_486.0, 0.0);
        assert_eq!(above.height, 1.0);
        assert_eq!(above.top, 11_486.0 - 1.0);
        let below = viewport_slice(-20_000.0, 665.0, 11_486.0, 0.0);
        assert_eq!(below.height, 1.0);
        assert_eq!(below.top, 0.0);
    }

    #[test]
    fn whole_pixel_band_rounds_and_stays_inside_the_widget() {
        let band = |top, height, widget| whole_pixel_band(ViewportSlice { top, height }, widget, 0);
        assert_eq!(band(10.4, 99.6, 500), (10, 100));
        assert_eq!(band(480.0, 100.0, 500), (400, 100));
        assert_eq!(band(0.0, 900.0, 500), (0, 500));
        assert_eq!(band(f64::NAN, f64::INFINITY, 500), (0, 500));
        assert_eq!(band(-5.0, -5.0, -10), (0, 0));
    }

    #[test]
    fn the_band_floor_keeps_an_off_screen_band_at_its_edge() {
        let band = |top, height, floor| whole_pixel_band(ViewportSlice { top, height }, 500, floor);
        // A one-pixel band at the content end widens upward to the floor.
        assert_eq!(band(499.0, 1.0, 11), (489, 11));
        // At the content start it widens downward.
        assert_eq!(band(0.0, 1.0, 11), (0, 11));
        // A floor never exceeds the widget, and a band above it is untouched.
        assert_eq!(band(0.0, 1.0, 900), (0, 500));
        assert_eq!(band(100.0, 300.0, 11), (100, 300));
    }

    #[test]
    fn non_finite_inputs_degrade_to_zero_instead_of_propagating() {
        let slice = viewport_slice(f64::NAN, f64::INFINITY, 500.0, f64::NEG_INFINITY);
        assert_eq!(slice.top, 0.0);
        assert!(slice.height.is_finite());
    }
}
