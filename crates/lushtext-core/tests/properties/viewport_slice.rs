// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for the GTK Lush viewport-slice geometry.
//!
//! `gtk_lush_widgets::viewport_slice` decides which band of a scrollable
//! child's content the `ViewportSliceBin` allocates. The band must stay inside
//! the content, must contain everything the outer viewport can show, and must
//! not depend on anything but its inputs.
//!
//! `gtk_lush_widgets::outer_scroll_request` decides whether the child asked to
//! see a different band and how far the outer scroller must travel to show it.
//! Its critical invariant is negative: a bin resting where it put itself must
//! never move the outer scroller, at **any** viewport position. Violating that
//! made a sidebar scroll its own section header out of view and made two
//! sections oscillate against each other forever.
//!
//! `gtk_lush_widgets::classify_child_scroll` is the whole decision
//! `outer_scroll_request` projects. Its critical invariant is the other
//! negative one: a divergence the bin cannot classify in the allocation that
//! produced it is deferred, never written back, and a deferral always ends in
//! an ordinary decision once the geometry stops moving, so a genuine request
//! made while the child reconfigured is not silently dropped.

use gtk_lush_widgets::{
    ADJUSTMENT_EPSILON, ChildScrollDecision, ViewportSlice, classify_child_scroll,
    outer_scroll_request, viewport_slice,
};
use proptest::prelude::*;

use crate::support;

/// Largest content height worth generating: far beyond any real sidebar.
const MAX_CONTENT: f64 = 1_000_000.0;
/// Largest viewport height worth generating: taller than any real monitor.
const MAX_VIEWPORT: f64 = 10_000.0;
/// Largest overscan worth generating: several viewports.
const MAX_OVERSCAN: f64 = 50_000.0;

fn intersection(
    viewport_top: f64,
    viewport_height: f64,
    content_height: f64,
) -> Option<(f64, f64)> {
    let start = viewport_top.max(0.0);
    let end = (viewport_top + viewport_height).min(content_height);
    (end > start).then_some((start, end))
}

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn slice_always_lies_inside_the_content(
        viewport_top in -MAX_CONTENT..MAX_CONTENT,
        viewport_height in 0.0..MAX_VIEWPORT,
        content_height in 0.0..MAX_CONTENT,
        overscan in 0.0..MAX_OVERSCAN,
    ) {
        let slice = viewport_slice(viewport_top, viewport_height, content_height, overscan);
        prop_assert!(slice.top >= 0.0);
        prop_assert!(slice.height >= 0.0);
        prop_assert!(slice.top + slice.height <= content_height + 1e-6);
    }

    #[test]
    fn slice_covers_the_visible_intersection(
        viewport_top in -MAX_CONTENT..MAX_CONTENT,
        viewport_height in 0.0..MAX_VIEWPORT,
        content_height in 0.0..MAX_CONTENT,
        overscan in 0.0..MAX_OVERSCAN,
    ) {
        let slice = viewport_slice(viewport_top, viewport_height, content_height, overscan);
        if let Some((start, end)) = intersection(viewport_top, viewport_height, content_height) {
            prop_assert!(slice.top <= start + 1e-6, "slice top {} above intersection start {start}", slice.top);
            prop_assert!(
                slice.top + slice.height >= end - 1e-6,
                "slice end {} below intersection end {end}",
                slice.top + slice.height
            );
        }
    }

    #[test]
    fn content_shorter_than_the_viewport_is_returned_whole(
        viewport_top in -MAX_CONTENT..MAX_CONTENT,
        viewport_height in 0.0..MAX_VIEWPORT,
        fraction in 0.0..1.0f64,
        overscan in 0.0..MAX_OVERSCAN,
    ) {
        let content_height = viewport_height * fraction;
        let slice = viewport_slice(viewport_top, viewport_height, content_height, overscan);
        prop_assert_eq!(slice, ViewportSlice { top: 0.0, height: content_height });
    }

    #[test]
    fn a_move_smaller_than_the_overscan_keeps_the_new_intersection_covered(
        viewport_top in 0.0..MAX_CONTENT,
        viewport_height in 1.0..MAX_VIEWPORT,
        content_height in 1.0..MAX_CONTENT,
        overscan in 1.0..MAX_OVERSCAN,
        move_fraction in -1.0..1.0f64,
    ) {
        let slice = viewport_slice(viewport_top, viewport_height, content_height, overscan);
        let moved_top = viewport_top + overscan * move_fraction;
        if let Some((start, end)) = intersection(moved_top, viewport_height, content_height) {
            // The previously computed slice still covers the moved viewport,
            // which is what lets a bin skip re-slicing for small moves.
            let covers = slice.top <= start + 1e-6 && slice.top + slice.height >= end - 1e-6;
            // Only guaranteed when the slice was not clamped by the content edges.
            let unclamped = slice.top > 0.0 && slice.top + slice.height < content_height;
            if unclamped {
                prop_assert!(covers, "slice {slice:?} does not cover moved viewport {start}..{end}");
            }
        }
    }

    #[test]
    fn a_resting_bin_never_scrolls_the_outer_window(
        published_offset in 0.0..MAX_CONTENT,
        viewport_top in -MAX_CONTENT..MAX_CONTENT,
        drift in -0.49..0.49f64,
    ) {
        // The bin wrote `published_offset` and the child left it there (modulo
        // sub-pixel noise). No viewport position may turn that into a request.
        prop_assert_eq!(
            outer_scroll_request(published_offset, published_offset + drift, viewport_top, 0.0),
            None
        );
    }

    #[test]
    fn a_request_lands_the_asked_for_offset_at_the_viewport_top(
        published_offset in 0.0..MAX_CONTENT,
        child_value in 0.0..MAX_CONTENT,
        viewport_top in -MAX_CONTENT..MAX_CONTENT,
    ) {
        let Some(delta) = outer_scroll_request(published_offset, child_value, viewport_top, 0.0) else {
            return Ok(());
        };
        // Scrolling the outer window by `delta` moves its top edge to exactly
        // the content offset the child asked for.
        prop_assert!(
            (viewport_top + delta - child_value).abs() < 1e-6,
            "delta {delta} from viewport_top {viewport_top} lands at {} not {child_value}",
            viewport_top + delta
        );
    }

    #[test]
    fn honouring_a_request_settles_instead_of_asking_again(
        published_offset in 0.0..MAX_CONTENT,
        child_value in 0.0..MAX_CONTENT,
        viewport_top in -MAX_CONTENT..MAX_CONTENT,
    ) {
        let Some(delta) = outer_scroll_request(published_offset, child_value, viewport_top, 0.0) else {
            return Ok(());
        };
        // After the outer scroller moves, the next allocation republishes the
        // slice offset the child now rests at. That must be the end of it:
        // a request that re-asks on every allocation is the oscillation bug.
        let settled_viewport_top = viewport_top + delta;
        prop_assert_eq!(
            outer_scroll_request(child_value, child_value, settled_viewport_top, 0.0),
            None
        );
    }

    #[test]
    fn only_movements_past_the_epsilon_are_requests(
        published_offset in 0.0..MAX_CONTENT,
        step in -MAX_CONTENT..MAX_CONTENT,
        viewport_top in -MAX_CONTENT..MAX_CONTENT,
    ) {
        let child_value = published_offset + step;
        let request = outer_scroll_request(published_offset, child_value, viewport_top, 0.0);
        if step.abs() < ADJUSTMENT_EPSILON {
            prop_assert_eq!(request, None);
        }
        // A reported request is never smaller than the epsilon either, so the
        // caller can apply it without re-checking for noise.
        if let Some(delta) = request {
            prop_assert!(delta.abs() >= ADJUSTMENT_EPSILON);
        }
    }
}

proptest! {
    /// The invariant the wheel-scroll regression violated: a child settling
    /// inside the geometry correction it just made is never forwarded, so the
    /// publish/settle cycle has a fixed point and cannot creep.
    #[test]
    fn prop_a_settle_within_the_geometry_correction_is_never_forwarded(
        published_offset in -50_000.0f64..50_000.0,
        viewport_top in -50_000.0f64..50_000.0,
        shift in 0.1f64..500.0,
        settle_fraction in -1.0f64..1.0,
    ) {
        // Any value the child lands on within its own correction is a settle.
        let child_value = published_offset + shift * settle_fraction;
        prop_assert_eq!(
            outer_scroll_request(published_offset, child_value, viewport_top, shift),
            None,
            "a settle of {} within a correction of {} must not scroll the outer window",
            child_value - published_offset,
            shift
        );
    }

    /// The converse, so the suppression cannot silently swallow real requests:
    /// past the correction, the decision is unchanged from the no-reconfigure
    /// case. This is what the focus-traversal regression would have caught.
    #[test]
    fn prop_beyond_the_correction_the_decision_is_unchanged(
        published_offset in -50_000.0f64..50_000.0,
        viewport_top in -50_000.0f64..50_000.0,
        shift in 0.1f64..500.0,
        overshoot in 1.0f64..10_000.0,
        negative in proptest::bool::ANY,
    ) {
        let beyond = shift + ADJUSTMENT_EPSILON + overshoot;
        let child_value = published_offset + if negative { -beyond } else { beyond };
        prop_assert_eq!(
            outer_scroll_request(published_offset, child_value, viewport_top, shift),
            outer_scroll_request(published_offset, child_value, viewport_top, 0.0),
            "a request beyond the correction must decide exactly as it would \
             without one"
        );
    }

    /// A child that did not touch the geometry keeps the pre-existing contract
    /// exactly, so the new parameter cannot change behaviour where it is zero.
    #[test]
    fn prop_zero_correction_preserves_the_previous_contract(
        published_offset in -50_000.0f64..50_000.0,
        child_value in -50_000.0f64..50_000.0,
        viewport_top in -50_000.0f64..50_000.0,
    ) {
        let decided = outer_scroll_request(published_offset, child_value, viewport_top, 0.0);
        if (child_value - published_offset).abs() < ADJUSTMENT_EPSILON {
            prop_assert_eq!(decided, None, "a resting bin never asks to scroll");
        } else {
            let delta = child_value - viewport_top;
            let expected = (delta.abs() >= ADJUSTMENT_EPSILON).then_some(delta);
            prop_assert_eq!(decided, expected);
        }
    }
}

proptest! {
    #![proptest_config(support::property_config())]

    /// `outer_scroll_request` is exactly the request arm of the full decision,
    /// so the two can never disagree about whether to move the outer scroller.
    #[test]
    fn prop_the_request_projection_matches_the_full_decision(
        published_offset in -50_000.0f64..50_000.0,
        child_value in -50_000.0f64..50_000.0,
        viewport_top in -50_000.0f64..50_000.0,
        shift in prop_oneof![Just(0.0f64), 0.0f64..500.0],
    ) {
        let decision = classify_child_scroll(published_offset, child_value, viewport_top, shift);
        let request = outer_scroll_request(published_offset, child_value, viewport_top, shift);
        match decision {
            ChildScrollDecision::Request(delta) => prop_assert_eq!(request, Some(delta)),
            ChildScrollDecision::Rest
            | ChildScrollDecision::Settle
            | ChildScrollDecision::Defer => prop_assert_eq!(request, None),
        }
    }

    /// The swallowed-request defect: inside the correction the child just made,
    /// a divergence is neither forwarded nor overwritten. It is deferred.
    #[test]
    fn prop_a_divergence_within_the_correction_is_deferred_not_overwritten(
        published_offset in -50_000.0f64..50_000.0,
        viewport_top in -50_000.0f64..50_000.0,
        shift in 1.0f64..500.0,
        fraction in 0.0f64..1.0,
        negative in proptest::bool::ANY,
    ) {
        // Anywhere from the noise floor up to the correction's own bound.
        let magnitude = ADJUSTMENT_EPSILON + (shift - ADJUSTMENT_EPSILON) * fraction;
        let child_value = published_offset + if negative { -magnitude } else { magnitude };
        prop_assert_eq!(
            classify_child_scroll(published_offset, child_value, viewport_top, shift),
            ChildScrollDecision::Defer
        );
    }

    /// A deferral terminates: re-classified in an allocation with stable
    /// geometry, the held divergence is a request or a settle, decided by the
    /// ordinary rule, and never deferred again.
    #[test]
    fn prop_a_deferred_divergence_is_decided_once_geometry_is_stable(
        published_offset in -50_000.0f64..50_000.0,
        child_value in -50_000.0f64..50_000.0,
        viewport_top in -50_000.0f64..50_000.0,
        shift in 0.1f64..500.0,
    ) {
        if classify_child_scroll(published_offset, child_value, viewport_top, shift)
            != ChildScrollDecision::Defer
        {
            return Ok(());
        }
        let delta = child_value - viewport_top;
        let expected = if delta.abs() >= ADJUSTMENT_EPSILON {
            ChildScrollDecision::Request(delta)
        } else {
            ChildScrollDecision::Settle
        };
        prop_assert_eq!(
            classify_child_scroll(published_offset, child_value, viewport_top, 0.0),
            expected
        );
    }

    /// Without a correction there is nothing to defer against, and a resting
    /// child is at rest whatever the correction.
    #[test]
    fn prop_only_a_correction_defers_and_never_a_resting_child(
        published_offset in -50_000.0f64..50_000.0,
        child_value in -50_000.0f64..50_000.0,
        viewport_top in -50_000.0f64..50_000.0,
        shift in 0.0f64..500.0,
        drift in -0.49f64..0.49,
    ) {
        prop_assert_ne!(
            classify_child_scroll(published_offset, child_value, viewport_top, 0.0),
            ChildScrollDecision::Defer
        );
        prop_assert_eq!(
            classify_child_scroll(published_offset, published_offset + drift, viewport_top, shift),
            ChildScrollDecision::Rest
        );
    }
}
