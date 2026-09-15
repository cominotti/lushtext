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

use gtk_lush_widgets::{ADJUSTMENT_EPSILON, ViewportSlice, outer_scroll_request, viewport_slice};
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
            outer_scroll_request(published_offset, published_offset + drift, viewport_top),
            None
        );
    }

    #[test]
    fn a_request_lands_the_asked_for_offset_at_the_viewport_top(
        published_offset in 0.0..MAX_CONTENT,
        child_value in 0.0..MAX_CONTENT,
        viewport_top in -MAX_CONTENT..MAX_CONTENT,
    ) {
        let Some(delta) = outer_scroll_request(published_offset, child_value, viewport_top) else {
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
        let Some(delta) = outer_scroll_request(published_offset, child_value, viewport_top) else {
            return Ok(());
        };
        // After the outer scroller moves, the next allocation republishes the
        // slice offset the child now rests at. That must be the end of it:
        // a request that re-asks on every allocation is the oscillation bug.
        let settled_viewport_top = viewport_top + delta;
        prop_assert_eq!(
            outer_scroll_request(child_value, child_value, settled_viewport_top),
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
        let request = outer_scroll_request(published_offset, child_value, viewport_top);
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
