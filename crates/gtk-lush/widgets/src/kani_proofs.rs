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
    let min_band: i32 = kani::any();
    let (top, band) = whole_pixel_band(slice, height, min_band);
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
/// The child's value follows its scroll anchor (ledger A7): the row edge it
/// keeps at a fixed fraction of its page, so the value is
/// `anchor − (anchor − value₀) × page / page₀`. A `value-changed` the child
/// sees before it is allocated at the new value (the bin's publish) leaves
/// that anchor stray, at any row of the content (A20), and a re-derivation
/// against another page then lands anywhere; one it sees after (the bin's
/// re-announcement, a write-back) or a request (A4) anchors it on a row edge
/// inside its view. An emitting publish drops any pending request (A9).
///
/// Envelope assumptions narrower than the ledger's statements, recorded as the
/// ledger requires:
///
/// - the child's content height is independent of the outer position and of
///   which rows are realized (A12 is only approximately true; a drifting
///   estimate is modelled separately as a reconfiguring child, whose own
///   settle stays within its correction, A8);
/// - a held (deferred) settle handed back with nothing moving re-anchors on the
///   published offset (A8's second half, modelled literally);
/// - re-derivation truncates toward zero where GTK rounds a `double`; the
///   difference is at most one pixel and only on a page change.
mod slice_loop {
    use crate::scroll_request::{
        ChildAnchorFacts, ChildScrollDecision, classify_child_scroll,
        classify_child_scroll_in_frame,
    };
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
        /// A non-zero page has shown the inset (`content_inset_known`).
        pub inset_known: bool,
        pub published: i32,
        /// The value the child holds in the bin-owned adjustment.
        pub child: i32,
        pub deferred: Option<(i32, i32)>,
        /// Ghost: the deferred divergence was a settle, not a request.
        pub deferred_settle: bool,
        pub pending: i32,
        /// The outer value the pending batch's first delta was measured
        /// against (design D2): the idle sets `request_anchor + pending`, so
        /// requests from several bins measured against one outer value do not
        /// add up.
        pub request_anchor: i32,
        pub idle_scheduled: bool,
        pub needs_allocation: bool,
        /// Page and upper the adjustment holds after the child's allocation.
        pub page: i32,
        pub upper: i32,
        /// Ghost of the child's scroll anchor (A7): the row edge it keeps at a
        /// fixed fraction of its page, the value it held when anchored, and
        /// the page it was anchored at. The child re-derives its value as
        /// `anchor_pos - (anchor_pos - anchor_value) * page / anchor_page`.
        pub anchor_pos: i32,
        pub anchor_value: i32,
        pub anchor_page: i32,
        /// Ghost: the anchor's alignment is unconfined (A20: set by a
        /// `value-changed` the child saw before it was allocated at that
        /// value), so re-deriving it against another page may land anywhere.
        pub anchor_stray: bool,
        /// The bin's own record that the anchor was last set by a value it
        /// wrote (`ViewportSliceBin::child_anchor_published`).
        pub anchor_published: bool,
    }

    /// How the child reacts to one allocation, chosen by Kani. Every
    /// nondeterministic choice the child makes in an allocation is a field,
    /// so an allocation is a function of the model and its reaction (which
    /// the bin-independence harness L3 relies on).
    #[derive(Clone, Copy)]
    pub struct Reaction {
        /// A `scroll_to` / focus request the child applies inside this very
        /// allocation (A4), as the value it moves to. `None` at rest.
        pub request: Option<i32>,
        /// How far the child moves its reported content height in this
        /// allocation (A12: a list re-estimating unrealized rows).
        pub upper_correction: i32,
        /// How far the child's value settles as a consequence of its own
        /// geometry correction (bounded by A8).
        pub settle: i32,
        /// The row a publish's `value-changed` anchors the child on (A20:
        /// anywhere in the content).
        pub stray_anchor: i32,
        /// Where a stray anchor re-derives the value on a page change (A7:
        /// alignment unconfined).
        pub stray_value: i32,
        /// The row edge, inside the view, that a request or a post-allocation
        /// `value-changed` anchors the child on (A7, A20), as an offset from
        /// the view's top; two, for the two announcements an allocation can
        /// make (the re-announcement after an emitting publish, then a
        /// write-back).
        pub view_anchors: [i32; 2],
    }

    impl Reaction {
        /// A reaction with the given request and estimate correction, every
        /// other choice left to Kani.
        pub fn with(request: Option<i32>, upper_correction: i32, settle: i32) -> Self {
            Self {
                request,
                upper_correction,
                settle,
                stray_anchor: kani::any(),
                stray_value: kani::any(),
                view_anchors: [kani::any(), kani::any()],
            }
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Loop<const N: usize> {
        pub outer: i32,
        pub viewport: i32,
        pub bins: [Bin; N],
        /// Classify with the anchor facts (`classify_child_scroll_in_frame`,
        /// what the bin ships) or with the magnitude rule alone
        /// (`classify_child_scroll`, the pre-fix bin), for the pinned
        /// counterexample.
        pub anchor_facts: bool,
        /// Content above the first bin's header and below the last bin (the
        /// bin-independence harnesses place one bin at any origin in any
        /// outer range; zero everywhere else).
        pub above: i32,
        pub below: i32,
        /// Ghost: how many times the outer value was written (L2).
        pub outer_writes: u32,
    }

    fn clamp(value: i32, low: i32, high: i32) -> i32 {
        value.max(low).min(high.max(low))
    }

    /// A20: a `value-changed` the child sees once it is allocated at `value`
    /// anchors it on the row edge inside its view, alignment within [0, 1].
    fn reanchor_in_view(bin: &mut Bin, value: i32, page: i32, offset: i32) {
        kani::assume((0..=page).contains(&offset));
        let row = value + offset;
        bin.anchor_pos = row;
        bin.anchor_value = value;
        bin.anchor_page = page;
        bin.anchor_stray = false;
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
                // A6: the child's content box lies inside what it measures, so
                // its reported height is at least its inset.
                kani::assume(content >= true_inset);
                Bin {
                    content,
                    true_inset,
                    inset: 0,
                    inset_known: false,
                    published: 0,
                    child: 0,
                    deferred: None,
                    deferred_settle: false,
                    pending: 0,
                    request_anchor: 0,
                    idle_scheduled: false,
                    needs_allocation: true,
                    page: 0,
                    upper: 0,
                    // A fresh list is anchored on its first row at alignment 0.
                    anchor_pos: 0,
                    anchor_value: 0,
                    anchor_page: 0,
                    anchor_stray: false,
                    anchor_published: false,
                }
            });
            let mut model = Self {
                outer: 0,
                viewport,
                bins,
                anchor_facts: true,
                above: 0,
                below: 0,
                outer_writes: 0,
            };
            let outer: i32 = kani::any();
            kani::assume((0..=model.outer_max()).contains(&outer));
            model.outer = outer;
            model
        }

        pub fn origin(&self, index: usize) -> i32 {
            let mut origin = self.above + HEADER;
            for bin in &self.bins[..index] {
                origin += bin.content + HEADER;
            }
            origin
        }

        pub fn outer_max(&self) -> i32 {
            let mut upper = self.above + self.below;
            for bin in &self.bins {
                upper += HEADER + bin.content;
            }
            (upper - self.viewport).max(0)
        }

        /// One `size_allocate` of bin `index`, mirroring the widget.
        pub fn allocate(&mut self, index: usize, reaction: Reaction) -> ChildScrollDecision {
            let outer = self.outer;
            let anchor_facts = self.anchor_facts;
            let viewport_top = outer - self.origin(index);
            let bin = &mut self.bins[index];
            bin.needs_allocation = false;
            let slice = viewport_slice(
                f64::from(viewport_top),
                f64::from(self.viewport),
                f64::from(bin.content),
                0.0,
            );
            let min_band = if bin.inset_known {
                bin.inset + 1
            } else {
                self.viewport
            };
            let (top, height) = whole_pixel_band(slice, bin.content, min_band);
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
            // What the publish did to the child. A changed value emits
            // `value-changed`: the child re-anchors on the value the bin wrote,
            // at a row anywhere in its content (A7: after a host moves the
            // value the anchor's alignment is not confined to [0, 1]), and any
            // pending `scroll_to` is dropped (A9), so no request follows in
            // this allocation. A changed page or upper makes the child
            // re-derive its value from that anchor (A7).
            let emitted = configured != bin.child;
            let bin_reconfigured = page != bin.page || upper != bin.upper;
            if emitted {
                // A20: announced before the child is allocated at the new
                // value, the anchor is stray.
                let anchor = reaction.stray_anchor;
                kani::assume((0..=bin.content).contains(&anchor));
                bin.anchor_pos = anchor;
                bin.anchor_value = configured;
                bin.anchor_page = page;
                bin.anchor_stray = true;
                bin.anchor_published = true;
                kani::assume(reaction.request.is_none());
            }
            // The child's allocation: it works in its content box (A6) and
            // rewrites page and upper there, may correct its content estimate
            // (A12), re-derives its value from its anchor against the corrected
            // geometry (A7), and applies any pending request inside this
            // allocation (A4).
            let child_page = (height - bin.true_inset).max(0);
            let child_upper =
                (bin.content - bin.true_inset + reaction.upper_correction).max(child_page);
            let page_shift = (child_page - page).abs();
            let reconfigure_shift = (child_upper - upper).abs().max(page_shift);
            // A8, for the child's own estimate correction only: moving its
            // rows moves its value by no more than the correction.
            kani::assume(reaction.settle.abs() <= reconfigure_shift);
            // A7: the value on the line through the anchor. At the page the
            // anchor was set at, or with the anchor row at the view's edge
            // (alignment 0), the line is flat. Otherwise a published anchor's
            // alignment is unconfined, so the value may land anywhere in the
            // child's range (clamped below); an anchor the child set itself
            // follows the line exactly.
            let derived = if child_page == bin.anchor_page || bin.anchor_pos == bin.anchor_value {
                bin.anchor_value
            } else if bin.anchor_stray {
                let anywhere = reaction.stray_value;
                kani::assume((-MAX_CONTENT..=2 * MAX_CONTENT).contains(&anywhere));
                anywhere
            } else {
                bin.anchor_pos
                    - (bin.anchor_pos - bin.anchor_value) * child_page / bin.anchor_page.max(1)
            };
            let child_value = match reaction.request {
                Some(target) => {
                    // A4: applied inside this allocation; A7 (after
                    // `scroll_to` the anchor is the target row's edge inside
                    // the view, alignment within [0, 1]).
                    let target = clamp(target, 0, child_upper - child_page);
                    kani::assume((0..=child_page).contains(&reaction.view_anchors[0]));
                    // A7: a row above the view is anchored by its top edge at
                    // the view's top (alignment 0, `gtk_list_base_compute_scroll_align`),
                    // so only a request moving the view down can leave a
                    // fractional alignment.
                    if target < bin.child {
                        kani::assume(reaction.view_anchors[0] == 0);
                    }
                    let row = target + reaction.view_anchors[0];
                    bin.anchor_pos = row;
                    bin.anchor_value = target;
                    bin.anchor_page = child_page;
                    bin.anchor_stray = false;
                    target
                }
                // A8: a settle does not survive into a stable-geometry frame —
                // handed its settled value back with nothing moving, the child
                // re-anchors on the offset its rows are drawn at.
                None if held_settle && reconfigure_shift == 0 => {
                    let value = clamp(bin.published, 0, child_upper - child_page);
                    bin.anchor_value = value;
                    bin.anchor_page = child_page;
                    value
                }
                None => {
                    // The anchor's line now passes through the value just
                    // derived at this page, so an allocation at the same page
                    // derives it again (GTK re-derives deterministically); an
                    // estimate correction moves the anchor row with it.
                    bin.anchor_pos += reaction.settle;
                    bin.anchor_value = derived + reaction.settle;
                    bin.anchor_page = child_page;
                    clamp(derived + reaction.settle, 0, child_upper - child_page)
                }
            };
            bin.page = child_page;
            bin.upper = child_upper;
            if emitted {
                // The bin announces the value again now that the child is
                // allocated at it (`ViewportSliceBin::size_allocate`): A20
                // re-anchors on the row at the view's edge, alignment within
                // [0, 1].
                reanchor_in_view(bin, child_value, child_page, reaction.view_anchors[0]);
            }
            let learned = (height - child_page).max(0);
            let inset_changed = page_shift >= 1 && learned != bin.inset;
            if inset_changed {
                bin.inset = learned;
                bin.needs_allocation = true;
            }
            if child_page >= 1 && !bin.inset_known {
                bin.inset_known = true;
                bin.needs_allocation = true;
            }
            let facts = ChildAnchorFacts {
                emitted,
                anchor_published: bin.anchor_published,
                bin_reconfigured,
            };
            let decision = if anchor_facts {
                classify_child_scroll_in_frame(
                    f64::from(bin.published),
                    f64::from(child_value),
                    f64::from(viewport_top),
                    f64::from(reconfigure_shift),
                    facts,
                )
            } else {
                classify_child_scroll(
                    f64::from(bin.published),
                    f64::from(child_value),
                    f64::from(viewport_top),
                    f64::from(reconfigure_shift),
                )
            };
            bin.child = child_value;
            match decision {
                ChildScrollDecision::Request(delta) => {
                    // The child moved itself: its anchor is its own.
                    bin.anchor_published = false;
                    // Exact on whole pixels (the landing lemma).
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "a whole-pixel difference of i32 values"
                    )]
                    let delta = delta as i32;
                    // A new batch records the outer value this delta was
                    // measured against; a batch already pending keeps its
                    // anchor and accumulates (within-bin semantics unchanged).
                    if !bin.idle_scheduled {
                        bin.request_anchor = outer;
                    }
                    bin.pending += delta;
                    bin.idle_scheduled = true;
                }
                ChildScrollDecision::Settle if !inset_changed => {
                    // The write-back emits `value-changed` after the child's
                    // allocation: it re-anchors on the published offset, at
                    // the row at the view's edge (A20), and the bin records a
                    // published anchor.
                    bin.child = bin.published;
                    let published = bin.published;
                    reanchor_in_view(bin, published, child_page, reaction.view_anchors[1]);
                    bin.anchor_published = true;
                }
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
        /// (A13: never from inside layout), as an absolute move to the batch's
        /// anchor plus its accumulated delta. `set_value` clamps and emits only
        /// on a change (A11); a change re-allocates every bin.
        pub fn run_idles(&mut self) {
            for index in 0..N {
                if !self.bins[index].idle_scheduled {
                    continue;
                }
                self.bins[index].idle_scheduled = false;
                let delta = core::mem::take(&mut self.bins[index].pending);
                let target = self.bins[index].request_anchor + delta;
                let moved = clamp(target, 0, self.outer_max());
                if moved != self.outer {
                    self.outer = moved;
                    self.outer_writes += 1;
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
        Reaction::with(None, 0, 0)
    }

    /// A child with no input whose geometry may be corrected in any
    /// allocation (A7, A12), settling within that correction (A8).
    pub fn reconfiguring_child(_: usize) -> Reaction {
        let upper_correction: i32 = kani::any();
        let settle: i32 = kani::any();
        kani::assume((-MAX_CORRECTION..=MAX_CORRECTION).contains(&upper_correction));
        kani::assume((-MAX_CORRECTION..=MAX_CORRECTION).contains(&settle));
        Reaction::with(None, upper_correction, settle)
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
    let min_band = if bin.inset_known {
        bin.inset + 1
    } else {
        model.viewport
    };
    whole_pixel_band(slice, bin.content, min_band).0
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

/// Whether bin `index`'s band landed on `target`, or still shows the row the
/// request anchored the child on (A7: when the band height changes after the
/// request lands, the child re-derives its value to keep that row at its
/// fraction of the page, and the bin forwards that too), or stopped at the end
/// of the bin's content (A11 on the child's own range: the rows asked for are
/// then all on screen), or the outer scroller is clamped at an end (A11) — and
/// the child holds that band offset, so no `value-changed` erases it (A9).
fn request_is_honoured<const N: usize>(
    model: &slice_loop::Loop<N>,
    index: usize,
    target: i32,
) -> bool {
    let bin = model.bins[index];
    let top = band_top(model, index);
    let clamped = model.outer == 0 || model.outer == model.outer_max();
    let at_content_end = top == bin.upper - bin.page;
    let anchor_row_shown =
        !bin.anchor_published && (top..=top + bin.page).contains(&bin.anchor_pos);
    (top == target || anchor_row_shown || at_content_end || clamped) && bin.child == top
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
    model.frame(|allocated| {
        slice_loop::Reaction::with((allocated == index).then_some(target), 0, 0)
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
    model.frame(|_| slice_loop::Reaction::with(Some(target), 0, 0));
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

/// Two bins request in the same frame (N4, design D1 of
/// `extend-closed-loop-geometry-verification`): after rest, both children
/// apply a request inside the same frame's allocations (A4), before either
/// forwarding idle has run, so both deltas are measured against the same
/// outer value. GLib runs same-priority idles in FIFO order, so bin 1's idle
/// (scheduled last) applies last; "last applied wins": once allocations stop,
/// bin 1's request is honoured or clamped, the outer is never the sum of both
/// deltas, the loop is at rest, and every child holds its band offset.
fn assert_simultaneous_requests_do_not_add_up() {
    let mut model = slice_loop::Loop::<2>::any();
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    let outer_before = model.outer;
    let mut targets = [0; 2];
    for (index, target) in targets.iter_mut().enumerate() {
        let bin = model.bins[index];
        *target = kani::any();
        // A5: only a list that shows rows can request.
        kani::assume(bin.page > 0);
        kani::assume((0..=bin.upper - bin.page).contains(target));
        kani::assume((*target - bin.child).abs() >= 1);
        // The child applies the request inside an allocation (A4).
        model.bins[index].needs_allocation = true;
    }
    let deltas = [0, 1].map(|index| targets[index] - (outer_before - model.origin(index)));
    model.frame(|allocated| slice_loop::Reaction::with(Some(targets[allocated]), 0, 0));
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    let summed = outer_before + deltas[0] + deltas[1];
    let clamped = model.outer == 0 || model.outer == model.outer_max();
    assert!(
        clamped || deltas[0] == 0 || model.outer != summed,
        "two requests measured against the same outer value must not add up"
    );
    assert!(
        request_is_honoured(&model, 1, targets[1]),
        "the last-applied request must land exactly, or be clamped, and its child must hold \
         the band offset"
    );
    assert!(model.at_rest(), "simultaneous requests must come to rest");
    for index in 0..2 {
        assert!(
            model.bins[index].child == band_top(&model, index),
            "every child must hold its band offset once the requests rest"
        );
    }
}

/// A counterexample on the pre-fix model (the outer landed at
/// `outer₀ + d_A + d_B`); proved once the idle applies `anchor + pending`.
#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_two_simultaneous_requests_do_not_add_up() {
    assert_simultaneous_requests_do_not_add_up();
}

/// After rest, the outer viewport's height changes (a window resize or
/// maximize, 40..=200 px either way) and every bin re-allocates; returns
/// whether, two frames later, the outer scroller is where it was, the loop is
/// at rest, and every child holds its band offset. With `anchor_facts` the
/// bin classifies as it ships (`classify_child_scroll_in_frame`); without,
/// by the magnitude rule alone.
fn viewport_resize_leaves_the_outer_alone<const N: usize>(anchor_facts: bool) -> bool {
    let mut model = slice_loop::Loop::<N>::any();
    model.anchor_facts = anchor_facts;
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    let outer = model.outer;
    let viewport: i32 = kani::any();
    kani::assume((40..=200).contains(&viewport) && viewport != model.viewport);
    model.viewport = viewport;
    // A11: the outer scroller clamps into its new range; that is not the
    // bin's doing, so compare against the clamped value.
    let outer = outer.min(model.outer_max());
    model.outer = outer;
    for bin in &mut model.bins {
        bin.needs_allocation = true;
    }
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    let rests = model.outer == outer && model.at_rest();
    rests && (0..N).all(|index| model.bins[index].child == band_top(&model, index))
}

/// A viewport height change after an outer scroll leaves the outer scroller
/// alone (the settle a published anchor makes on a page change, A7, is
/// written back instead of forwarded).
#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_viewport_resize_leaves_the_outer_alone() {
    assert!(viewport_resize_leaves_the_outer_alone::<1>(true));
}

/// Counterexample kept on purpose: classified by the magnitude rule alone,
/// the settle a published anchor makes when the viewport height changes is
/// forwarded as a request and the outer scroller jumps. This is the pre-fix
/// bin; real GTK reproduces it (4664 px on maximize in the adoption lab).
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(4)]
fn slice_loop_forwarding_a_published_anchor_settle_moves_the_outer() {
    assert!(viewport_resize_leaves_the_outer_alone::<1>(false));
}

/// The **resize-coincident request residual**, kept on purpose: a child
/// request applied in the same allocation in which the viewport height
/// changes, while the child's anchor is one the bin published, cannot be told
/// from the settle the page change causes, so it is written back and erased
/// (A9). It needs a `scroll_to` in the very frame of a resize after an outer
/// scroll; recorded in the programme record.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(4)]
fn slice_loop_request_coinciding_with_a_resize_is_erased() {
    let mut model = slice_loop::Loop::<1>::any();
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    let viewport: i32 = kani::any();
    kani::assume((40..=200).contains(&viewport) && viewport != model.viewport);
    model.viewport = viewport;
    let clamped_outer = model.outer.min(model.outer_max());
    model.outer = clamped_outer;
    let target: i32 = kani::any();
    let bin = model.bins[0];
    kani::assume(bin.page > 0);
    kani::assume((0..=bin.upper - bin.page).contains(&target));
    kani::assume((target - bin.child).abs() >= 1);
    model.bins[0].needs_allocation = true;
    model.frame(|_| slice_loop::Reaction::with(Some(target), 0, 0));
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    assert!(request_is_honoured(&model, 0, target));
}

// --- Bin independence (unbounded attempt 2, design D9) -----------------------
//
// L1 frame locality, L2 single writer, L3 locality of the decision, L4 one bin
// under any origin and any outer jump, and L5 the pairwise simultaneous-request
// harness above. The composition argument is written in the programme record
// (`docs/next/formal-verification.md`, phase 3) and names these harnesses.

/// Largest amount of content placed above the one bin of an L4 harness.
const MAX_ABOVE: i32 = 400;
/// Largest amount of content placed below it.
const MAX_BELOW: i32 = 400;

/// One bin anywhere in an outer range: arbitrary content above and below it
/// (other bins, headers), and an arbitrary resting outer value.
fn one_bin_anywhere() -> slice_loop::Loop<1> {
    let mut model = slice_loop::Loop::<1>::any();
    let (above, below): (i32, i32) = (kani::any(), kani::any());
    kani::assume((0..=MAX_ABOVE).contains(&above) && (0..=MAX_BELOW).contains(&below));
    model.above = above;
    model.below = below;
    let outer: i32 = kani::any();
    kani::assume((0..=model.outer_max()).contains(&outer));
    model.outer = outer;
    model
}

/// L4: one bin at any origin in any outer range rests within two frames,
/// leaves the outer alone, and its child holds its band offset.
#[kani::proof]
#[kani::unwind(4)]
fn slice_bin_rests_from_any_origin() {
    let mut model = one_bin_anywhere();
    let outer = model.outer;
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    assert!(model.at_rest() && model.outer == outer);
    assert!(model.bins[0].child == band_top(&model, 0));
}

/// L4: after rest, an arbitrary outer jump (what another bin's request or the
/// user does to it) re-slices the bin, which rests again within two frames at
/// the jumped value with its child on its band offset.
#[kani::proof]
#[kani::unwind(4)]
fn slice_bin_rests_after_any_outer_jump() {
    let mut model = one_bin_anywhere();
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    let jumped: i32 = kani::any();
    kani::assume((0..=model.outer_max()).contains(&jumped));
    model.outer = jumped;
    model.bins[0].needs_allocation = true;
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    assert!(model.at_rest() && model.outer == jumped);
    assert!(model.bins[0].child == band_top(&model, 0));
}

/// L4: one bin at any origin honours a request (as `assert_request_is_honoured`
/// does for the fixed layouts).
#[kani::proof]
#[kani::unwind(4)]
fn slice_bin_honours_a_request_from_any_origin() {
    let mut model = one_bin_anywhere();
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    let target: i32 = kani::any();
    let bin = model.bins[0];
    kani::assume(bin.page > 0);
    kani::assume((0..=bin.upper - bin.page).contains(&target));
    kani::assume((target - bin.child).abs() >= 1);
    model.bins[0].needs_allocation = true;
    model.frame(|_| slice_loop::Reaction::with(Some(target), 0, 0));
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    assert!(request_is_honoured(&model, 0, target));
    assert!(model.at_rest());
}

/// Two bins after any two frames (rest, or on their way to it) and an
/// arbitrary outer value: the states L1–L3 quantify over.
fn two_bins_anywhere() -> slice_loop::Loop<2> {
    let mut model = slice_loop::Loop::<2>::any();
    model.frame(slice_loop::reconfiguring_child);
    let outer: i32 = kani::any();
    kani::assume((0..=model.outer_max()).contains(&outer));
    model.outer = outer;
    model
}

/// L1 and L2: allocating one bin changes neither the other bin nor the outer
/// value (only the idle writes it: L2's ghost counter stays put).
#[kani::proof]
#[kani::unwind(4)]
fn slice_bin_allocation_touches_only_its_own_bin() {
    let mut model = two_bins_anywhere();
    let index: usize = kani::any();
    kani::assume(index < 2);
    let before = model;
    model.allocate(index, slice_loop::reconfiguring_child(index));
    let other = 1 - index;
    assert!(
        model.bins[other] == before.bins[other],
        "L1: another bin changed"
    );
    assert!(
        model.outer == before.outer,
        "L2: an allocation wrote the outer"
    );
    assert!(
        model.outer_writes == before.outer_writes,
        "L2: an allocation wrote the outer"
    );
}

/// L3: bin 1's allocation depends only on its own state, its viewport top
/// (`outer - origin`), and the viewport height: a model whose first bin has
/// different content, and whose outer value differs by the same amount as the
/// origin, gives bin 1 the same state (its request anchor, an absolute outer
/// value, shifts by exactly that amount).
#[kani::proof]
#[kani::unwind(4)]
fn slice_bin_decision_depends_only_on_its_own_inputs() {
    let a = two_bins_anywhere();
    let mut b = a;
    let content: i32 = kani::any();
    kani::assume((0..=a.bins[0].content + MAX_BELOW).contains(&content));
    b.bins[0] = slice_loop::Loop::<2>::any().bins[0];
    b.bins[0].content = content;
    let shift = b.origin(1) - a.origin(1);
    b.outer = a.outer + shift;
    kani::assume((0..=b.outer_max()).contains(&b.outer));
    let reaction = slice_loop::reconfiguring_child(1);
    let (mut a, mut b) = (a, b);
    a.allocate(1, reaction);
    b.allocate(1, reaction);
    let (mut left, mut right) = (a.bins[1], b.bins[1]);
    assert!(right.request_anchor - left.request_anchor == shift || !left.idle_scheduled);
    left.request_anchor = 0;
    right.request_anchor = 0;
    assert!(
        left == right,
        "L3: bin 1 depends on something besides its own inputs"
    );
}
