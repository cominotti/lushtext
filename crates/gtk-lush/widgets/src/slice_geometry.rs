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
/// The returned slice always lies inside the content and always contains the
/// intersection of the viewport with the content. When the content fits the
/// viewport, the whole content is returned at offset zero.
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

    if content_height <= viewport_height {
        return ViewportSlice {
            top: 0.0,
            height: content_height,
        };
    }

    // Chrome the host draws *above* this content eats into the viewport, so the
    // content has less of it than `viewport_height` to occupy. The child sizes
    // its own `scroll_to` and keyboard-focus decisions from the allocation it is
    // given, so a band wider than what shows makes it believe rows are on
    // screen while they sit below the fold.
    //
    // Only the top side is deducted. Shrinking the band at the bottom edge too
    // would be symmetric but wrong in practice: it collapses to nothing once
    // the content has scrolled past, and a zero-height allocation makes
    // `GtkListView` rewrite the adjustment this container owns, which reads as
    // a scroll request and starts the very fight this module exists to avoid.
    let visible_top = viewport_top.max(0.0);
    let visible_height = (viewport_height + viewport_top.min(0.0)).max(0.0);
    let height = (visible_height + 2.0 * overscan).min(content_height);
    let top = (visible_top - overscan).clamp(0.0, content_height - height);
    ViewportSlice { top, height }
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
    fn content_shorter_than_viewport_is_returned_whole() {
        let slice = viewport_slice(120.0, 665.0, 300.0, 100.0);
        assert_eq!(
            slice,
            ViewportSlice {
                top: 0.0,
                height: 300.0
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
    fn slice_clamps_at_the_bottom_of_the_content() {
        let slice = viewport_slice(11_400.0, 665.0, 11_486.0, 0.0);
        assert_eq!(slice.top, 11_486.0 - 665.0);
        assert_eq!(slice.height, 665.0);
    }

    #[test]
    fn content_scrolled_past_the_viewport_still_gets_a_full_band() {
        // Never zero: a zero-height allocation makes the child rewrite the
        // adjustment this container owns, which would read as a scroll request.
        let above = viewport_slice(20_000.0, 665.0, 11_486.0, 0.0);
        assert_eq!(above.height, 665.0);
        assert_eq!(above.top, 11_486.0 - 665.0);
        let below = viewport_slice(-20_000.0, 665.0, 11_486.0, 0.0);
        assert_eq!(below.height, 0.0);
        assert_eq!(below.top, 0.0);
    }

    #[test]
    fn non_finite_inputs_degrade_to_zero_instead_of_propagating() {
        let slice = viewport_slice(f64::NAN, f64::INFINITY, 500.0, f64::NEG_INFINITY);
        assert_eq!(slice.top, 0.0);
        assert!(slice.height.is_finite());
    }
}
