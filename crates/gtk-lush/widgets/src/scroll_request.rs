// SPDX-License-Identifier: MIT OR Apache-2.0

//! Pure child-scroll-request translation for [`crate::ViewportSliceBin`].
//!
//! The bin hands its `GtkScrollable` child a bin-owned adjustment and writes
//! the slice offset into it on every allocation. The child may write to that
//! same adjustment to ask for a different band — `GtkListView` does exactly
//! that for `scroll_to` and keyboard-focus scrolling, from inside its own
//! allocation. The bin cannot scroll itself, so it forwards such a request to
//! the outer scroller.
//!
//! Deciding *whether the child asked at all* is the whole difficulty, and it
//! has no GTK dependency, so it lives here beside
//! [`crate::viewport_slice`] and is unit- and property-tested directly.

/// Adjustment differences below this many logical pixels are treated as noise.
pub const ADJUSTMENT_EPSILON: f64 = 0.5;

/// Decide how far the outer scroller must move for a child scroll request.
///
/// * `published_offset` is the value the bin itself last wrote into the child's
///   adjustment: the slice offset, already clamped into
///   `[0, content_height - slice_height]`.
/// * `child_value` is what the adjustment holds now.
/// * `viewport_top` is the outer viewport's top edge in the bin's content
///   coordinates. It is **negative** while other content in the same scroller
///   (a section header, say) sits above the bin, and it runs past the content
///   once the bin has scrolled by.
///
/// Returns `None` when the child did not move the adjustment, or when the band
/// it asked for is already at the viewport's top edge. Otherwise returns the
/// signed distance the outer scroller must travel so that `child_value` becomes
/// the content offset at the top of the viewport.
///
/// # Why the resting comparison is against `published_offset`
///
/// `published_offset` and `viewport_top` are different quantities, and they
/// agree only when the bin happens to start exactly at the viewport's top edge.
/// Comparing the child's value against `viewport_top` therefore reports a
/// standing request in every other position — the bin then scrolls the outer
/// window to make that phantom request true. One bin scrolls whatever sits
/// above it out of view and pins itself to the top; two bins in one scroller
/// pull in opposite directions on every allocation and oscillate forever.
/// Only a divergence from what the bin itself published is a request.
#[must_use]
pub fn outer_scroll_request(
    published_offset: f64,
    child_value: f64,
    viewport_top: f64,
) -> Option<f64> {
    if !published_offset.is_finite() || !child_value.is_finite() || !viewport_top.is_finite() {
        return None;
    }
    if (child_value - published_offset).abs() < ADJUSTMENT_EPSILON {
        return None;
    }
    let delta = child_value - viewport_top;
    (delta.abs() >= ADJUSTMENT_EPSILON).then_some(delta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bin_resting_at_its_published_offset_never_asks_to_scroll() {
        // The regression this module exists for: every one of these positions
        // is a resting bin, and the differing `viewport_top` values are the
        // ones a real scroller produces.
        for viewport_top in [-600.0, -55.0, -1.0, 0.0, 1.0, 55.0, 8_000.0] {
            assert_eq!(
                outer_scroll_request(0.0, 0.0, viewport_top),
                None,
                "a bin clamped to its content top must not scroll the outer window \
                 when the viewport top is {viewport_top}"
            );
        }
        assert_eq!(outer_scroll_request(7_335.0, 7_335.0, 14_000.0), None);
        assert_eq!(outer_scroll_request(120.0, 120.0, 120.0), None);
    }

    #[test]
    fn sub_pixel_child_drift_is_noise_rather_than_a_request() {
        assert_eq!(outer_scroll_request(100.0, 100.4, 100.0), None);
        assert_eq!(outer_scroll_request(100.0, 99.6, 100.0), None);
    }

    #[test]
    fn a_moved_child_asks_for_its_band_at_the_viewport_top() {
        // Child wants content offset 900 shown; the bin's content starts 55px
        // below the viewport top, so the outer scroller travels 955.
        assert_eq!(outer_scroll_request(0.0, 900.0, -55.0), Some(955.0));
        // Same request once the bin is aligned with the viewport.
        assert_eq!(outer_scroll_request(0.0, 900.0, 0.0), Some(900.0));
        // Scrolling back up is a negative delta.
        assert_eq!(outer_scroll_request(900.0, 0.0, 900.0), Some(-900.0));
    }

    #[test]
    fn a_request_already_at_the_viewport_top_needs_no_outer_travel() {
        // The child moved the adjustment, but to exactly the band already shown.
        assert_eq!(outer_scroll_request(0.0, 640.0, 640.0), None);
    }

    #[test]
    fn non_finite_inputs_are_ignored_instead_of_propagating() {
        assert_eq!(outer_scroll_request(f64::NAN, 10.0, 0.0), None);
        assert_eq!(outer_scroll_request(0.0, f64::INFINITY, 0.0), None);
        assert_eq!(outer_scroll_request(0.0, 10.0, f64::NEG_INFINITY), None);
    }
}
