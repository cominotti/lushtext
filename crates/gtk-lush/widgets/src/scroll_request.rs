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
/// # Why a child's own geometry correction bounds its settle
///
/// `reconfigure_shift` is how far the child moved the adjustment's `upper` or
/// `page_size` while it was being allocated, and zero when it left them alone.
/// A `GtkListView` rewrites both constantly, replacing the content height this
/// bin configured with its own realized estimate. The value it then settles on
/// is its rendering of the band the bin asked for, re-anchored against the
/// geometry it just corrected -- not a request for a different band.
///
/// Forwarding that settle creates a loop with no fixed point. Measured on an
/// 800-row sidebar: the bin publishes 3604, the child reconfigures
/// (`upper` 15248 -> 15238, `page_size` 665 -> 655) and settles on 3605, the bin
/// forwards +1, the outer scroller moves down one pixel, the next frame
/// publishes one pixel lower, and round it goes. Each wheel tick adds one to
/// seven pixels of unrequested downward travel; about fourteen ticks accumulate
/// the 55 pixels that hide a workspace header behind the viewport edge. That is
/// the same symptom as the earlier resting-comparison defect reached by a
/// different route, which is why every test written for that one passes against
/// this.
///
/// # Why the discriminator is the shift and not the reconfigure, the magnitude, or the cause
///
/// Three simpler rules were tried against the suite and each broke something,
/// so they are recorded rather than re-attempted:
///
/// * *Suppress any reconfigured divergence.* A genuine request usually **causes**
///   a reconfigure -- moving focus to the last row realizes rows the list had
///   only estimated -- so this silenced focus traversal entirely.
/// * *Suppress divergence below the page.* The settle measured 1 to 7 pixels and
///   a focus request measured 274, but 274 is well under a 655-pixel page, so the
///   request was swallowed too. Any constant between the two is a number chosen
///   to fit one fixture.
/// * *Suppress while the outer scroller is driving.* Correct during a wheel
///   turn, but the tick where scrolling stops leaves the viewport still and the
///   residual divergence is forwarded once, which is enough to shift the header.
///
/// What bounds a settle is the correction that produced it. The child re-anchors
/// its value against geometry it moved by some amount; it cannot legitimately
/// travel further than that correction as a consequence of it. Measured: the
/// geometry moved 10 and the value settled 1 to 7. A request is not bounded that
/// way -- the focus case moved 274 against a far smaller correction. The rule
/// needs no constant, no row height, and no fixture-sized threshold.
///
/// # Why the delta is measured from the viewport top and not the published offset
///
/// The distance `child_value - published_offset` -- move the outer by exactly
/// what the child moved -- looks like the gentler answer: a row clipped by 5px
/// would cost 7px of travel instead of the 52 the chrome above the bin adds
/// here. It was tried and it breaks keyboard traversal. `GtkListBase` treats
/// every `value-changed` on its adjustment as a user scroll: it re-anchors on
/// the value and drops any pending `scroll_to`. After the outer moves by the
/// child's own delta the bin re-slices at `child_value - chrome`, publishes
/// that, and the emission wipes the child's anchor -- including a request the
/// child issued between the outer move and the re-slice. Measured: a focus
/// traversal that reached row 21 then asked for row 30 published 219 over the
/// child's 274, and row 30 was never requested again. Landing the outer where
/// the slice offset *equals* the child's value means the re-slice republishes
/// the value the child already holds, nothing is emitted, and the anchor
/// survives. That is what `child_value - viewport_top` buys, at the price of
/// scrolling chrome above the bin away on every honoured request.
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
    reconfigure_shift: f64,
) -> Option<f64> {
    if !published_offset.is_finite() || !child_value.is_finite() || !viewport_top.is_finite() {
        return None;
    }
    let divergence = child_value - published_offset;
    if reconfigure_shift.is_finite()
        && reconfigure_shift > 0.0
        && divergence.abs() <= reconfigure_shift + ADJUSTMENT_EPSILON
    {
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
                outer_scroll_request(0.0, 0.0, viewport_top, 0.0),
                None,
                "a bin clamped to its content top must not scroll the outer window \
                 when the viewport top is {viewport_top}"
            );
        }
        assert_eq!(outer_scroll_request(7_335.0, 7_335.0, 14_000.0, 0.0), None);
        assert_eq!(outer_scroll_request(120.0, 120.0, 120.0, 0.0), None);
    }

    #[test]
    fn sub_pixel_child_drift_is_noise_rather_than_a_request() {
        assert_eq!(outer_scroll_request(100.0, 100.4, 100.0, 0.0), None);
        assert_eq!(outer_scroll_request(100.0, 99.6, 100.0, 0.0), None);
    }

    #[test]
    fn a_moved_child_asks_for_its_band_at_the_viewport_top() {
        // Child wants content offset 900 shown; the bin's content starts 55px
        // below the viewport top, so the outer scroller travels 955.
        assert_eq!(outer_scroll_request(0.0, 900.0, -55.0, 0.0), Some(955.0));
        // Same request once the bin is aligned with the viewport.
        assert_eq!(outer_scroll_request(0.0, 900.0, 0.0, 0.0), Some(900.0));
        // Scrolling back up is a negative delta.
        assert_eq!(outer_scroll_request(900.0, 0.0, 900.0, 0.0), Some(-900.0));
    }

    #[test]
    fn a_small_reveal_below_chrome_still_lands_the_band_at_the_viewport_top() {
        // Measured: a row clipped by 5px made the child ask for 7 with the
        // viewport top 45px above the bin. The answer is 52, not 7; see the
        // module doc for why the gentler answer wipes pending requests.
        assert_eq!(outer_scroll_request(0.0, 7.0, -45.0, 0.0), Some(52.0));
    }

    #[test]
    fn a_request_already_at_the_viewport_top_needs_no_outer_travel() {
        // The child moved the adjustment, but to exactly the band already shown.
        assert_eq!(outer_scroll_request(0.0, 640.0, 640.0, 0.0), None);
    }

    #[test]
    fn a_settle_within_the_childs_own_geometry_correction_is_not_a_request() {
        // The measured regression: the child rewrote `upper`/`page_size` during
        // its own allocation and settled one pixel off the published offset.
        // Forwarding that is what accumulated 55px of unrequested travel.
        assert_eq!(outer_scroll_request(3604.0, 3605.0, 3604.0, 10.0), None);
        assert_eq!(outer_scroll_request(13383.0, 13388.0, 13383.0, 10.0), None);
        // Even a large divergence is deferred when the geometry moved; the next
        // allocation re-decides with a stable baseline.
        // Measured focus-traversal request: 274px while the viewport stood
        // still. Suppressing this is what broke the traversal test.
        assert_eq!(outer_scroll_request(0.0, 274.0, 0.0, 0.0), Some(274.0));
    }

    #[test]
    fn a_divergence_beyond_the_geometry_correction_is_a_request() {
        // The discriminator is the reconfigure flag and nothing else, so the
        // identical values forward when the child did not move the goalposts.
        assert_eq!(outer_scroll_request(3604.0, 3605.0, 3604.0, 0.0), Some(1.0));
        assert_eq!(outer_scroll_request(0.0, 9000.0, 0.0, 0.0), Some(9000.0));
    }

    #[test]
    fn non_finite_inputs_are_ignored_instead_of_propagating() {
        assert_eq!(outer_scroll_request(f64::NAN, 10.0, 0.0, 0.0), None);
        assert_eq!(outer_scroll_request(0.0, f64::INFINITY, 0.0, 0.0), None);
        assert_eq!(
            outer_scroll_request(0.0, 10.0, f64::NEG_INFINITY, 0.0),
            None
        );
    }
}
