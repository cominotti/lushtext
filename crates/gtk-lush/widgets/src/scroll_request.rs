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
//! [`crate::viewport_slice`] and is unit- and property-tested directly:
//! [`classify_child_scroll`] is the whole decision, and
//! [`outer_scroll_request`] is its request-only projection.

/// Adjustment differences below this many logical pixels are treated as noise.
pub const ADJUSTMENT_EPSILON: f64 = 0.5;

/// Classify the value a `GtkScrollable` child left in the bin-owned adjustment
/// after an allocation.
///
/// * `published_offset` is the value the bin itself last wrote into the child's
///   adjustment: the slice offset, already clamped into
///   `[0, content_height - slice_height]`.
/// * `child_value` is what the adjustment holds now.
/// * `viewport_top` is the outer viewport's top edge in the bin's content
///   coordinates. It is **negative** while other content in the same scroller
///   (a section header, say) sits above the bin, and it runs past the content
///   once the bin has scrolled by.
/// * `reconfigure_shift` is how far the child moved the adjustment's `upper` or
///   `page_size` during the allocation being classified, and zero when it left
///   them alone (see below).
///
/// The answer is a [`ChildScrollDecision`]: the child is at rest, asked for a
/// band ([`ChildScrollDecision::Request`] carries the signed distance the outer
/// scroller must travel so that `child_value` becomes the content offset at the
/// top of the viewport), settled on a value that must be written back, or made
/// a divergence this allocation cannot classify, which must be deferred.
///
/// # Domain
///
/// The function never panics for any `f64`, and for every input a resting
/// child (`child_value == published_offset`) is [`ChildScrollDecision::Rest`]
/// and a [`ChildScrollDecision::Request`] carries at least
/// [`ADJUSTMENT_EPSILON`]. **Exact landing** — `viewport_top + delta ==
/// child_value` bit for bit, which a `GtkListBase` needs because it compares
/// adjustment values exactly — is guaranteed on the **whole-pixel domain**:
/// `published_offset`, `child_value`, and `viewport_top` integer values in the
/// `i32` range and `reconfigure_shift` a whole number that fits a `u16`. All of
/// these are proved by Kani (`make kani`); exact landing is not claimed for
/// general `f64`, where Kani finds rounding counterexamples.
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
/// # Why a divergence within the correction is deferred rather than written back
///
/// The bound says a settle cannot exceed the correction. It does not say
/// everything within the correction is a settle: a child can apply a genuine
/// `scroll_to` in the very allocation in which it also corrects the geometry
/// (a list that realizes rows whose real heights differ from its estimate
/// while it moves its anchor), and a small request -- revealing a row clipped
/// by a few pixels -- then falls inside the bound too. Writing the published
/// offset back over it erases the request for good, because a `GtkListBase`
/// re-anchors on the value it is handed and drops what it had asked for. So a
/// divergence within the correction is [`ChildScrollDecision::Defer`]: the bin
/// leaves the child's value in place and allocates once more, and there, with
/// the geometry no longer moving, the ordinary rule tells a request from a
/// settle. A deferred settle is still written back, one allocation later.
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
pub fn classify_child_scroll(
    published_offset: f64,
    child_value: f64,
    viewport_top: f64,
    reconfigure_shift: f64,
) -> ChildScrollDecision {
    if !published_offset.is_finite() || !child_value.is_finite() || !viewport_top.is_finite() {
        return ChildScrollDecision::Rest;
    }
    let divergence = child_value - published_offset;
    if divergence.abs() < ADJUSTMENT_EPSILON {
        return ChildScrollDecision::Rest;
    }
    if reconfigure_shift.is_finite()
        && reconfigure_shift > 0.0
        && divergence.abs() <= reconfigure_shift + ADJUSTMENT_EPSILON
    {
        return ChildScrollDecision::Defer;
    }
    let delta = child_value - viewport_top;
    if delta.abs() >= ADJUSTMENT_EPSILON {
        ChildScrollDecision::Request(delta)
    } else {
        ChildScrollDecision::Settle
    }
}

/// What the bin knows about how its child's scroll anchor was last set, and
/// whether the allocation being classified moved the child's geometry on the
/// bin's own initiative. Crate-private: the public [`classify_child_scroll`]
/// keeps its contract; [`classify_child_scroll_in_frame`] refines it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ChildAnchorFacts {
    /// Publishing this allocation's offset changed the value the child held,
    /// so the adjustment emitted `value-changed` before the child allocated.
    pub(crate) emitted: bool,
    /// The child's anchor was last set by a value the bin wrote (a publish or
    /// a write-back), not by a request the child made itself.
    pub(crate) anchor_published: bool,
    /// Publishing this allocation changed the page or upper the child held.
    pub(crate) bin_reconfigured: bool,
}

/// [`classify_child_scroll`] refined by where the child's anchor came from.
///
/// A divergence is written back ([`ChildScrollDecision::Settle`]) instead of
/// being forwarded or deferred when it cannot be a request:
///
/// * the publish **emitted** `value-changed` in this allocation, and a
///   `GtkListBase` drops any pending `scroll_to` on a `value-changed` (ledger
///   A9), so nothing the child does in this allocation is a request; or
/// * the child's anchor was set by a value the **bin** wrote and the bin then
///   changed the page or upper it hands the child. A `GtkListView` re-derives
///   its value from its anchor on a page change (A7), and after a host moves
///   its value that anchor's alignment is not confined to `[0, 1]`: GTK 4.22
///   was measured moving the value 4.42 px per pixel of page change, and a
///   viewport grown from 600 to 1600 px after an outer scroll of 3000 px
///   moved it 4664 px. Such a settle is unbounded by any correction the child
///   made itself (`reconfigure_shift` is zero: the bin changed the page), so
///   the magnitude rule of [`classify_child_scroll`] reads it as a request
///   and the outer scroller jumps.
///
/// An anchor the child set itself (`scroll_to`, keyboard focus) keeps the
/// ordinary rule, so a request applied in an allocation whose band height the
/// bin also changed is still forwarded.
#[must_use]
pub(crate) fn classify_child_scroll_in_frame(
    published_offset: f64,
    child_value: f64,
    viewport_top: f64,
    reconfigure_shift: f64,
    facts: ChildAnchorFacts,
) -> ChildScrollDecision {
    let decision = classify_child_scroll(
        published_offset,
        child_value,
        viewport_top,
        reconfigure_shift,
    );
    let cannot_be_a_request = facts.emitted || (facts.anchor_published && facts.bin_reconfigured);
    match decision {
        ChildScrollDecision::Request(_) | ChildScrollDecision::Defer if cannot_be_a_request => {
            ChildScrollDecision::Settle
        }
        other => other,
    }
}

/// What [`crate::ViewportSliceBin`] does with the value its child left in the
/// bin-owned adjustment after an allocation; see [`classify_child_scroll`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChildScrollDecision {
    /// The child is where the bin put it, within [`ADJUSTMENT_EPSILON`], or
    /// the inputs were not finite. Nothing to do.
    Rest,
    /// The child asked for a different band: move the outer scroller by this
    /// signed distance, which is never smaller than [`ADJUSTMENT_EPSILON`].
    Request(f64),
    /// The child settled on a value that is neither the published offset nor
    /// a request: write the published offset back.
    Settle,
    /// The divergence lies within the geometry correction the child made in
    /// this same allocation, so it may be a settle or a request. Leave the
    /// child's value alone and classify it again in an allocation whose
    /// geometry is stable.
    Defer,
}

/// Decide how far the outer scroller must move for a child scroll request.
///
/// Returns the distance of a [`ChildScrollDecision::Request`] and `None` for
/// every other decision, so `None` does not mean the value may be overwritten:
/// a deferred divergence must be left in place (see [`classify_child_scroll`],
/// which carries the arguments' full contract and the whole-pixel domain on
/// which a returned distance lands exactly).
#[must_use]
pub fn outer_scroll_request(
    published_offset: f64,
    child_value: f64,
    viewport_top: f64,
    reconfigure_shift: f64,
) -> Option<f64> {
    match classify_child_scroll(
        published_offset,
        child_value,
        viewport_top,
        reconfigure_shift,
    ) {
        ChildScrollDecision::Request(delta) => Some(delta),
        ChildScrollDecision::Rest | ChildScrollDecision::Settle | ChildScrollDecision::Defer => {
            None
        }
    }
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
        // Measured: a row clipped by 5px asked for 7 with the viewport top
        // 45px above the bin; the answer is 52 (see the module doc).
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
        // Measured focus-traversal request: 274px while the geometry stood
        // still. Suppressing this is what broke the traversal test.
        assert_eq!(outer_scroll_request(0.0, 274.0, 0.0, 0.0), Some(274.0));
    }

    #[test]
    fn a_divergence_within_the_correction_is_deferred_not_written_back() {
        // The same measured settles are not overwritten in the reconfiguring
        // allocation: they are left for one that can tell them apart.
        assert_eq!(
            classify_child_scroll(3604.0, 3605.0, 3604.0, 10.0),
            ChildScrollDecision::Defer
        );
        assert_eq!(
            classify_child_scroll(13383.0, 13388.0, 13383.0, 10.0),
            ChildScrollDecision::Defer
        );
        // The case the deferral exists for: a row clipped by 7px is revealed
        // by a `scroll_to` applied in the allocation that also discovered
        // 400px of content. Writing the published offset back erased it.
        assert_eq!(
            classify_child_scroll(1953.0, 1960.0, 1953.0, 400.0),
            ChildScrollDecision::Defer
        );
    }

    #[test]
    fn a_deferred_divergence_is_decided_by_the_ordinary_rule_once_geometry_is_stable() {
        // The request survives the deferral and is honoured one allocation
        // later, from the same published offset.
        assert_eq!(
            classify_child_scroll(1953.0, 1960.0, 1953.0, 0.0),
            ChildScrollDecision::Request(7.0)
        );
        // A deferred value that is already at the viewport top asks for no
        // travel, so it is a settle after all and is written back then.
        assert_eq!(
            classify_child_scroll(0.0, 640.0, 640.0, 0.0),
            ChildScrollDecision::Settle
        );
    }

    #[test]
    fn rest_request_and_settle_keep_their_meaning_without_a_correction() {
        assert_eq!(
            classify_child_scroll(120.0, 120.3, 55.0, 0.0),
            ChildScrollDecision::Rest
        );
        assert_eq!(
            classify_child_scroll(0.0, 900.0, -55.0, 0.0),
            ChildScrollDecision::Request(955.0)
        );
        assert_eq!(
            classify_child_scroll(0.0, 640.0, 640.0, 0.0),
            ChildScrollDecision::Settle
        );
        // A correction does not turn a resting child into a deferral.
        assert_eq!(
            classify_child_scroll(120.0, 120.3, 55.0, 10.0),
            ChildScrollDecision::Rest
        );
        assert_eq!(
            classify_child_scroll(f64::NAN, 10.0, 0.0, 10.0),
            ChildScrollDecision::Rest
        );
    }

    #[test]
    fn the_learning_frame_settle_is_routed_to_correction_not_forwarding() {
        // The bin learns a padded child's content-box inset from the first
        // allocation, and on that frame the child still rewrites the page by
        // the inset and settles a few pixels off. Classified as a request this
        // would forward `child - viewport_top`, ~55px at the top: the header
        // scrolled away on first show. The bound keeps it from forwarding; the
        // bin's learning frame then republishes the offset in any case.
        assert_eq!(outer_scroll_request(0.0, 3.0, -55.0, 10.0), None);
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

    #[test]
    fn a_divergence_after_an_emitting_publish_is_written_back() {
        let facts = ChildAnchorFacts {
            emitted: true,
            ..ChildAnchorFacts::default()
        };
        assert_eq!(
            classify_child_scroll_in_frame(2946.0, 7610.0, 2946.0, 0.0, facts),
            ChildScrollDecision::Settle
        );
        assert_eq!(
            classify_child_scroll_in_frame(2946.0, 2950.0, 2946.0, 10.0, facts),
            ChildScrollDecision::Settle
        );
    }

    #[test]
    fn a_page_change_under_a_published_anchor_is_a_settle_not_a_request() {
        let facts = ChildAnchorFacts {
            emitted: false,
            anchor_published: true,
            bin_reconfigured: true,
        };
        // Measured: a 600 -> 1600 px viewport after a 3000 px outer scroll.
        assert_eq!(
            classify_child_scroll_in_frame(2946.0, 7610.0, 2946.0, 0.0, facts),
            ChildScrollDecision::Settle
        );
    }

    #[test]
    fn a_request_anchored_by_the_child_keeps_the_ordinary_rule() {
        for facts in [
            ChildAnchorFacts::default(),
            ChildAnchorFacts {
                bin_reconfigured: true,
                ..ChildAnchorFacts::default()
            },
            ChildAnchorFacts {
                anchor_published: true,
                ..ChildAnchorFacts::default()
            },
        ] {
            assert_eq!(
                classify_child_scroll_in_frame(274.0, 602.0, 274.0, 0.0, facts),
                classify_child_scroll(274.0, 602.0, 274.0, 0.0)
            );
            assert_eq!(
                classify_child_scroll_in_frame(274.0, 274.0, 274.0, 0.0, facts),
                ChildScrollDecision::Rest
            );
        }
    }
}
