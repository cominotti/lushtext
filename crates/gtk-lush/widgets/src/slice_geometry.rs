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
//! of the real one. It has no GTK dependency so it can be unit- and
//! property-tested directly.

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
/// * `viewport_height` is the outer viewport's visible height.
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

    let height = (viewport_height + 2.0 * overscan).min(content_height);
    let top = (viewport_top - overscan).clamp(0.0, content_height - height);
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
    fn slice_clamps_at_the_top_of_the_content() {
        let slice = viewport_slice(-40.0, 665.0, 11_486.0, 100.0);
        assert_eq!(slice.top, 0.0);
        assert_eq!(slice.height, 865.0);
    }

    #[test]
    fn slice_clamps_at_the_bottom_of_the_content() {
        let slice = viewport_slice(11_400.0, 665.0, 11_486.0, 0.0);
        assert_eq!(slice.top, 11_486.0 - 665.0);
        assert_eq!(slice.height, 665.0);
    }

    #[test]
    fn non_finite_inputs_degrade_to_zero_instead_of_propagating() {
        let slice = viewport_slice(f64::NAN, f64::INFINITY, 500.0, f64::NEG_INFINITY);
        assert_eq!(slice.top, 0.0);
        assert!(slice.height.is_finite());
    }
}
