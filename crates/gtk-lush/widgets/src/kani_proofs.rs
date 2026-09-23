// SPDX-License-Identifier: MIT OR Apache-2.0

//! Kani proof harnesses over the crate's two pure `ViewportSliceBin`
//! decisions, [`viewport_slice`] and [`classify_child_scroll`].
//!
//! Compiled only under `cfg(kani)` (`make kani`); ordinary builds, tests, and
//! the stable-toolchain gates never see this module. The harnesses check the
//! functions that ship, not a copy of them.
//!
//! The geometric guarantees hold on the **whole-pixel domain** GTK actually
//! produces: integer values in the `i32` range, overscan in `0..=4096`, and a
//! reconfiguration shift that fits a `u16`. That domain is stated in the
//! rustdoc of each function and in the `gtk-lush-viewport-slice` spec. Two
//! `should_panic` harnesses keep the general-`f64` counterexamples that
//! motivated the restriction, so a future claim of a broader domain has to
//! confront them.

use crate::scroll_request::{
    ADJUSTMENT_EPSILON, ChildScrollDecision, classify_child_scroll, outer_scroll_request,
};
use crate::slice_geometry::{ViewportSlice, viewport_slice, whole_pixel_band};

/// The largest overscan the whole-pixel domain admits.
const MAX_OVERSCAN: i32 = 4096;

/// An arbitrary whole-pixel `i32` value, as an `f64`.
fn any_pixel() -> f64 {
    f64::from(kani::any::<i32>())
}

/// An arbitrary whole-pixel overscan in `0..=MAX_OVERSCAN`.
fn any_overscan() -> f64 {
    let overscan: i32 = kani::any();
    kani::assume((0..=MAX_OVERSCAN).contains(&overscan));
    f64::from(overscan)
}

/// No `f64` input — NaN, infinities, negatives, subnormals — makes either
/// decision panic. This is the only property claimed for arbitrary `f64`.
#[kani::proof]
fn no_input_panics() {
    let _ = viewport_slice(kani::any(), kani::any(), kani::any(), kani::any());
    let _ = classify_child_scroll(kani::any(), kani::any(), kani::any(), kani::any());
    let _ = outer_scroll_request(kani::any(), kani::any(), kani::any(), kani::any());
}

/// Rounding any slice to the allocated whole-pixel band never panics (the
/// `i32::clamp` in the bin's allocation) and keeps the band inside the widget.
#[kani::proof]
fn whole_pixel_band_stays_inside_the_widget() {
    let slice = ViewportSlice {
        top: kani::any(),
        height: kani::any(),
    };
    let height: i32 = kani::any();
    let (top, band) = whole_pixel_band(slice, height);
    assert!(top >= 0 && band >= 0);
    assert!(i64::from(top) + i64::from(band) <= i64::from(height.max(0)));
}

/// On whole pixels the slice lies inside the content.
#[kani::proof]
fn slice_lies_inside_content_on_whole_pixels() {
    let (top, height, content) = (any_pixel(), any_pixel(), any_pixel());
    let slice = viewport_slice(top, height, content, any_overscan());
    assert!(slice.top >= 0.0 && slice.height >= 0.0);
    assert!(slice.top + slice.height <= content.max(0.0));
}

/// On whole pixels the slice contains the visible intersection of the viewport
/// with the content.
#[kani::proof]
fn slice_covers_the_visible_intersection_on_whole_pixels() {
    let (top, height, content) = (any_pixel(), any_pixel(), any_pixel());
    let slice = viewport_slice(top, height, content, any_overscan());
    let visible_start = top.max(0.0);
    let visible_end = (top + height.max(0.0)).min(content.max(0.0));
    if visible_start < visible_end {
        assert!(slice.top <= visible_start);
        assert!(slice.top + slice.height >= visible_end);
    }
}

/// Counterexample kept on purpose: for general finite `f64` the containment
/// promise fails, because near `1e260` `top + height` rounds past the content.
/// This is why the rustdoc states the whole-pixel domain.
#[kani::proof]
#[kani::should_panic]
fn slice_containment_fails_for_general_f64() {
    let (top, height, content, overscan): (f64, f64, f64, f64) =
        (kani::any(), kani::any(), kani::any(), kani::any());
    kani::assume(top.is_finite() && height.is_finite());
    kani::assume(content.is_finite() && overscan.is_finite());
    let slice = viewport_slice(top, height, content, overscan);
    assert!(slice.top >= 0.0 && slice.height >= 0.0);
    assert!(slice.top + slice.height <= content.max(0.0));
}

/// A bin whose child still holds the published offset never asks to scroll,
/// for any `f64`: the v0.7.0 resting-comparison regression, as a theorem.
#[kani::proof]
fn resting_bin_never_requests() {
    let (published, viewport_top, shift): (f64, f64, f64) = (kani::any(), kani::any(), kani::any());
    assert!(outer_scroll_request(published, published, viewport_top, shift).is_none());
    assert!(matches!(
        classify_child_scroll(published, published, viewport_top, shift),
        ChildScrollDecision::Rest
    ));
}

/// Every forwarded request travels at least [`ADJUSTMENT_EPSILON`], for any
/// `f64`.
#[kani::proof]
fn requests_are_at_least_epsilon() {
    let (published, child, viewport_top, shift): (f64, f64, f64, f64) =
        (kani::any(), kani::any(), kani::any(), kani::any());
    if let Some(delta) = outer_scroll_request(published, child, viewport_top, shift) {
        assert!(delta.abs() >= ADJUSTMENT_EPSILON);
    }
}

/// On whole pixels a honoured request lands exactly: after the outer scroller
/// travels `delta`, the viewport top equals the child's value bit for bit, so
/// the re-slice republishes the value the child already holds and emits
/// nothing (ledger axiom A9).
#[kani::proof]
fn request_lands_exactly_on_whole_pixels() {
    let (published, child, viewport_top) = (any_pixel(), any_pixel(), any_pixel());
    let shift = f64::from(kani::any::<u16>());
    if let Some(delta) = outer_scroll_request(published, child, viewport_top, shift) {
        assert!(viewport_top + delta == child);
    }
}

/// Counterexample kept on purpose: for general `f64`, `viewport_top + delta`
/// can round away from the child's value, which is why exact landing is
/// stated on whole pixels only.
#[kani::proof]
#[kani::should_panic]
fn request_landing_fails_for_general_f64() {
    let (published, child, viewport_top, shift): (f64, f64, f64, f64) =
        (kani::any(), kani::any(), kani::any(), kani::any());
    if let Some(delta) = outer_scroll_request(published, child, viewport_top, shift) {
        assert!(viewport_top + delta == child);
    }
}

/// The `ViewportSliceBin` feedback loop as a Kani step model (K4).
///
/// Each allocation calls the real [`viewport_slice`], `whole_pixel_band`, and
/// [`classify_child_scroll`], exactly as `ViewportSliceBin::size_allocate`
/// does, and applies the bin's own bookkeeping (publish, learn the inset,
/// write back, defer, request) in the same order. The child is **not** a model
/// of `GtkListView`: it is `kani::any()` restricted only by `kani::assume`
/// clauses that cite the GTK axiom ledger
/// (`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`).
///
/// Integer abstraction: every length is a whole pixel. That is sound because
/// GTK allocations are integers, `f64` add/sub is exact on them, and the
/// landing lemma is proved above on whole pixels
/// (`request_lands_exactly_on_whole_pixels`).
///
/// Envelope assumptions narrower than the ledger's statements, recorded as the
/// ledger requires:
///
/// - the child's content height is independent of the outer position and of
///   which rows are realized (A12 is only approximately true; a drifting
///   estimate is modelled separately as a reconfiguring child);
/// - a settle happens only in an allocation whose geometry the child corrected
///   (A7), and never exceeds that correction (A8).
mod slice_loop {
    use crate::scroll_request::{ChildScrollDecision, classify_child_scroll};
    use crate::slice_geometry::{viewport_slice, whole_pixel_band};

    /// Height of the chrome (a section header) above every bin.
    pub const HEADER: i32 = 20;
    /// Largest content height a bin's child reports.
    const MAX_CONTENT: i32 = 400;
    /// Largest CSS inset a child's content box may have (A6).
    const MAX_INSET: i32 = 10;
    /// Largest geometry correction a reconfiguring child makes (A7).
    pub const MAX_CORRECTION: i32 = 10;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Bin {
        pub content: i32,
        /// The child's real content-box inset (A6); the bin must learn it.
        pub true_inset: i32,
        pub inset: i32,
        pub published: i32,
        /// The value the child holds in the bin-owned adjustment.
        pub child: i32,
        pub deferred: Option<(i32, i32)>,
        /// Ghost: the deferred divergence was a settle, not a request.
        pub deferred_settle: bool,
        pub pending: i32,
        pub idle_scheduled: bool,
        pub needs_allocation: bool,
        /// Page and upper the adjustment holds after the child's allocation.
        pub page: i32,
        pub upper: i32,
    }

    /// How the child reacts to one allocation, chosen by Kani.
    #[derive(Clone, Copy)]
    pub struct Reaction {
        /// A `scroll_to` / focus request the child applies inside this very
        /// allocation (A4), as the value it moves to. `None` at rest.
        pub request: Option<i32>,
        /// How far the child moves its reported content height in this
        /// allocation (A7: a list re-estimating unrealized rows).
        pub upper_correction: i32,
        /// How far the child's value settles as a consequence of its own
        /// geometry correction (A7, bounded by A8).
        pub settle: i32,
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Loop<const N: usize> {
        pub outer: i32,
        pub viewport: i32,
        pub bins: [Bin; N],
    }

    fn clamp(value: i32, low: i32, high: i32) -> i32 {
        value.max(low).min(high.max(low))
    }

    impl<const N: usize> Loop<N> {
        pub fn any() -> Self {
            let viewport: i32 = kani::any();
            kani::assume((40..=200).contains(&viewport));
            let bins = [(); N].map(|()| {
                let content: i32 = kani::any();
                let true_inset: i32 = kani::any();
                kani::assume((0..=MAX_CONTENT).contains(&content));
                kani::assume((0..=MAX_INSET).contains(&true_inset));
                Bin {
                    content,
                    true_inset,
                    inset: 0,
                    published: 0,
                    child: 0,
                    deferred: None,
                    deferred_settle: false,
                    pending: 0,
                    idle_scheduled: false,
                    needs_allocation: true,
                    page: 0,
                    upper: 0,
                }
            });
            let mut model = Self {
                outer: 0,
                viewport,
                bins,
            };
            let outer: i32 = kani::any();
            kani::assume((0..=model.outer_max()).contains(&outer));
            model.outer = outer;
            model
        }

        pub fn origin(&self, index: usize) -> i32 {
            let mut origin = HEADER;
            for bin in &self.bins[..index] {
                origin += bin.content + HEADER;
            }
            origin
        }

        pub fn outer_max(&self) -> i32 {
            let mut upper = 0;
            for bin in &self.bins {
                upper += HEADER + bin.content;
            }
            (upper - self.viewport).max(0)
        }

        /// One `size_allocate` of bin `index`, mirroring the widget.
        pub fn allocate(&mut self, index: usize, reaction: Reaction) -> ChildScrollDecision {
            let viewport_top = self.outer - self.origin(index);
            let bin = &mut self.bins[index];
            bin.needs_allocation = false;
            let slice = viewport_slice(
                f64::from(viewport_top),
                f64::from(self.viewport),
                f64::from(bin.content),
                0.0,
            );
            let (top, height) = whole_pixel_band(slice, bin.content);
            let held = bin
                .deferred
                .take()
                .filter(|(published, _)| *published == top)
                .map(|(_, value)| value);
            let held_settle = held.is_some() && bin.deferred_settle;
            bin.deferred_settle = false;
            // publish_slice_offset: content-box page/upper, clamped (A11).
            let page = (height - bin.inset).max(0);
            let upper = (bin.content - bin.inset).max(page);
            let configured = clamp(held.unwrap_or(top), 0, upper - page);
            bin.published = if held.is_some() {
                clamp(top, 0, upper - page)
            } else {
                configured
            };
            // The child's allocation: it works in its content box (A6) and
            // rewrites page and upper there, may correct its content estimate
            // (A7), re-derives its value against the corrected geometry, and
            // applies any pending request inside this allocation (A4).
            let child_page = (height - bin.true_inset).max(0);
            let child_upper =
                (bin.content - bin.true_inset + reaction.upper_correction).max(child_page);
            let page_shift = (child_page - page).abs();
            let reconfigure_shift = (child_upper - upper).abs().max(page_shift);
            // A8: a settle is bounded by the correction that caused it.
            kani::assume(reaction.settle.abs() <= reconfigure_shift);
            let child_value = match reaction.request {
                Some(target) => clamp(target, 0, child_upper - child_page),
                // A8: a settle does not survive into a stable-geometry frame —
                // handed its settled value back with nothing moving, the child
                // re-anchors on the offset its rows are drawn at.
                None if held_settle && reconfigure_shift == 0 => {
                    clamp(bin.published, 0, child_upper - child_page)
                }
                None => clamp(configured + reaction.settle, 0, child_upper - child_page),
            };
            bin.page = child_page;
            bin.upper = child_upper;
            let learned = (height - child_page).max(0);
            let inset_changed = page_shift >= 1 && learned != bin.inset;
            if inset_changed {
                bin.inset = learned;
                bin.needs_allocation = true;
            }
            let decision = classify_child_scroll(
                f64::from(bin.published),
                f64::from(child_value),
                f64::from(viewport_top),
                f64::from(reconfigure_shift),
            );
            bin.child = child_value;
            match decision {
                ChildScrollDecision::Request(delta) => {
                    // Exact on whole pixels (the landing lemma).
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "a whole-pixel difference of i32 values"
                    )]
                    let delta = delta as i32;
                    bin.pending += delta;
                    bin.idle_scheduled = true;
                }
                ChildScrollDecision::Settle if !inset_changed => bin.child = bin.published,
                ChildScrollDecision::Defer if !inset_changed => {
                    bin.deferred = Some((bin.published, child_value));
                    // A re-deferred value keeps what it was when first held.
                    bin.deferred_settle = if held.is_some() {
                        held_settle
                    } else {
                        reaction.request.is_none()
                    };
                    if held.is_none() {
                        bin.needs_allocation = true;
                    }
                }
                ChildScrollDecision::Rest
                | ChildScrollDecision::Settle
                | ChildScrollDecision::Defer => {}
            }
            decision
        }

        /// The idle that applies accumulated requests to the outer scroller
        /// (A13: never from inside layout). `set_value` clamps and emits only
        /// on a change (A11); a change re-allocates every bin.
        pub fn run_idles(&mut self) {
            for index in 0..N {
                if !self.bins[index].idle_scheduled {
                    continue;
                }
                self.bins[index].idle_scheduled = false;
                let delta = core::mem::take(&mut self.bins[index].pending);
                let moved = clamp(self.outer + delta, 0, self.outer_max());
                if moved != self.outer {
                    self.outer = moved;
                    for bin in &mut self.bins {
                        bin.needs_allocation = true;
                    }
                }
            }
        }

        /// One frame: allocate every bin that queued an allocation, then idle.
        pub fn frame(&mut self, mut reaction: impl FnMut(usize) -> Reaction) {
            for index in 0..N {
                if self.bins[index].needs_allocation {
                    let reaction = reaction(index);
                    self.allocate(index, reaction);
                }
            }
            self.run_idles();
        }

        pub fn at_rest(&self) -> bool {
            self.bins
                .iter()
                .all(|bin| !bin.needs_allocation && !bin.idle_scheduled && bin.deferred.is_none())
        }
    }

    /// A child with no input and stable geometry: it only rewrites page and
    /// upper in its content box (A6), never settles (A8: no settle in a
    /// stable-geometry frame), and requests nothing (A4).
    pub fn resting_child(_: usize) -> Reaction {
        Reaction {
            request: None,
            upper_correction: 0,
            settle: 0,
        }
    }

    /// A child with no input whose geometry may be corrected in any
    /// allocation (A7, A12), settling within that correction (A8).
    pub fn reconfiguring_child(_: usize) -> Reaction {
        let upper_correction: i32 = kani::any();
        let settle: i32 = kani::any();
        kani::assume((-MAX_CORRECTION..=MAX_CORRECTION).contains(&upper_correction));
        kani::assume((-MAX_CORRECTION..=MAX_CORRECTION).contains(&settle));
        Reaction {
            request: None,
            upper_correction,
            settle,
        }
    }
}

/// Where bin `index` of `model` is allocated its band, as the widget computes it.
fn band_top<const N: usize>(model: &slice_loop::Loop<N>, index: usize) -> i32 {
    let bin = model.bins[index];
    let slice = viewport_slice(
        f64::from(model.outer - model.origin(index)),
        f64::from(model.viewport),
        f64::from(bin.content),
        0.0,
    );
    whole_pixel_band(slice, bin.content).0
}

/// Fixed point at rest, no oscillation, and render fidelity for `N` bins: with
/// no input and a stable child, the loop reaches rest within two frames, the
/// outer scroller never moves, a further re-allocation changes nothing, and
/// every child holds exactly its band's offset, so a row at content `y` is
/// drawn at `y` in the bin's frame.
fn assert_loop_rests<const N: usize>() {
    let mut model = slice_loop::Loop::<N>::any();
    let outer = model.outer;
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    assert!(model.at_rest(), "the loop must rest within two frames");
    assert!(
        model.outer == outer,
        "a resting loop must not move the outer scroller"
    );
    for index in 0..N {
        assert!(
            model.bins[index].child == band_top(&model, index),
            "render fidelity: the child must hold its band offset at rest"
        );
    }
    // A fixed point: re-allocating everything again changes nothing.
    let rested = model;
    for bin in &mut model.bins {
        bin.needs_allocation = true;
    }
    model.frame(slice_loop::resting_child);
    assert!(model == rested, "a rested loop must be a fixed point");
}

#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_rests_with_one_bin() {
    assert_loop_rests::<1>();
}

#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_rests_with_two_bins() {
    assert_loop_rests::<2>();
}

#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_rests_with_three_bins() {
    assert_loop_rests::<3>();
}

/// Whether bin `index`'s band landed on `target` — or stopped at the end of the
/// bin's content (A11 on the child's own range: the rows asked for are then
/// all on screen), or the outer scroller is clamped at an end (A11) — and the
/// child holds that band offset, so no `value-changed` erases it (A9).
fn request_is_honoured<const N: usize>(
    model: &slice_loop::Loop<N>,
    index: usize,
    target: i32,
) -> bool {
    let top = band_top(model, index);
    let clamped = model.outer == 0 || model.outer == model.outer_max();
    let at_content_end = top == model.bins[index].upper - model.bins[index].page;
    (top == target || at_content_end || clamped) && model.bins[index].child == top
}

/// Request fidelity and bounded liveness: after rest, a child request beyond
/// epsilon is honoured — its bin's band lands exactly on the requested value
/// and the child keeps it, so no `value-changed` erases it (A9) — or the outer
/// scroller is clamped at an end (A11), within two further frames.
fn assert_request_is_honoured<const N: usize>() {
    let mut model = slice_loop::Loop::<N>::any();
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    let index: usize = kani::any();
    kani::assume(index < N);
    let target: i32 = kani::any();
    let bin = model.bins[index];
    // A5: a list allocated a zero-height band rewrites its adjustment rather
    // than requesting anything; only a list that shows rows can request.
    kani::assume(bin.page > 0);
    kani::assume((0..=bin.upper - bin.page).contains(&target));
    kani::assume((target - bin.child).abs() >= 1);
    // The child applies the request inside an allocation (A4).
    model.bins[index].needs_allocation = true;
    model.frame(|allocated| slice_loop::Reaction {
        request: (allocated == index).then_some(target),
        upper_correction: 0,
        settle: 0,
    });
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    assert!(
        request_is_honoured(&model, index, target),
        "a request must land exactly, or be clamped at the content end or an end of the \
         outer range, and the child must then hold the band offset"
    );
    assert!(model.at_rest(), "an honoured request must come to rest");
}

#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_honours_a_request_with_one_bin() {
    assert_request_is_honoured::<1>();
}

#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_honours_a_request_with_two_bins() {
    assert_request_is_honoured::<2>();
}

/// A request the child applies in the very first allocation, while the bin is
/// still learning its content-box inset (A6), diverging from the published
/// offset by at most the inset (`within_the_inset`) or by more than the inset
/// plus one; returns whether it was honoured ([`request_is_honoured`]).
fn learning_frame_request_is_honoured(within_the_inset: bool) -> bool {
    let mut model = slice_loop::Loop::<1>::any();
    let target: i32 = kani::any();
    kani::assume(model.bins[0].true_inset > 0);
    kani::assume(target >= 0 && target <= model.bins[0].content);
    model.frame(|_| slice_loop::Reaction {
        request: Some(target),
        upper_correction: 0,
        settle: 0,
    });
    let bin = model.bins[0];
    // A5: only a list that shows rows can request.
    kani::assume(bin.page > 0);
    let divergence = (bin.child - bin.published).abs();
    kani::assume(divergence >= 1);
    if within_the_inset {
        kani::assume(divergence <= bin.true_inset);
    } else {
        kani::assume(divergence > bin.true_inset + 1);
    }
    let requested = bin.child;
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    request_is_honoured(&model, 0, requested)
}

/// A learning-frame request larger than the inset being learned is honoured.
#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_learning_frame_request_beyond_the_inset_is_honoured() {
    assert!(learning_frame_request_is_honoured(false));
}

/// The **learning-frame residual**, kept on purpose as a counterexample: a
/// request within the inset the bin is learning, made in that very first
/// allocation, is indistinguishable there from the learning-frame settle
/// (ledger A8: measured ~3px on a `navigation-sidebar` list), which the bin must
/// not forward (it would travel `child - viewport_top`, ~58px, and hide the
/// header on first show). The learning frame therefore republishes, and such a
/// request is erased (A9). Recorded in the programme record as an accepted
/// residual; this harness fails the day a change makes it reachable-and-honoured
/// or widens it.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(4)]
fn slice_loop_learning_frame_request_within_the_inset_is_erased() {
    assert!(learning_frame_request_is_honoured(true));
}

/// Run `reconfiguring` frames in which the child may correct its geometry and
/// settle within the correction (A7, A8), then two stable frames; returns
/// whether the outer scroller stayed where it was and the loop went quiet (no
/// allocation queued, no idle scheduled).
fn reconfiguring_child_leaves_the_outer_alone(reconfiguring: usize) -> bool {
    let mut model = slice_loop::Loop::<1>::any();
    let outer = model.outer;
    for _ in 0..reconfiguring {
        model.frame(slice_loop::reconfiguring_child);
    }
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    let quiet = model
        .bins
        .iter()
        .all(|bin| !bin.needs_allocation && !bin.idle_scheduled);
    model.outer == outer && quiet
}

/// One allocation in which the child corrects its geometry and settles within
/// the correction (A7, A8), with no input, never moves the outer scroller, and
/// the loop is quiet two stable frames later.
#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_one_reconfiguring_allocation_leaves_the_outer_alone() {
    assert!(reconfiguring_child_leaves_the_outer_alone(1));
}

/// Counterexample kept on purpose: **two consecutive** reconfiguring
/// allocations, each settling within its own correction (A8), can add up to a
/// divergence larger than the second correction; the bin then reads it as a
/// request and travels `child - viewport_top`, scrolling chrome above the bin
/// away (found by Kani, K4). No real consumer has produced even one settle
/// (A8's phase-0 evidence: 0 settles and no `Defer` across 48 instrumented
/// tests), so this is recorded as an envelope residual in the programme record,
/// with its candidate fix, rather than changed blind.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(4)]
fn slice_loop_two_reconfiguring_allocations_can_move_the_outer() {
    assert!(reconfiguring_child_leaves_the_outer_alone(2));
}
