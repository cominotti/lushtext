// SPDX-License-Identifier: MIT OR Apache-2.0

//! Private GObject implementation for [`super::ViewportSliceBin`].
//!
//! The public wrapper in `mod.rs` gives Rust callers a normal GTK widget type.
//! This implementation owns the GTK lifecycle details: one optional child,
//! the bin-owned adjustments handed to a `GtkScrollable` child, discovery of
//! the outer `GtkScrolledWindow`, full-height measurement, sliced allocation,
//! and the two-way adjustment synchronization with its re-entrancy guard.

use std::cell::{Cell, RefCell};
use std::sync::LazyLock;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{glib, graphene, gsk};

use crate::scroll_request::{
    ADJUSTMENT_EPSILON, ChildAnchorFacts, ChildScrollDecision, classify_child_scroll_in_frame,
    outer_scroll_request,
};
use crate::single_child::replace_child;
use crate::slice_geometry::{viewport_slice, whole_pixel_band};

/// Private widget state for `GtkLushViewportSliceBin`.
pub struct ViewportSliceBin {
    /// The single child parented by this bin.
    pub child: RefCell<Option<gtk4::Widget>>,
    /// Vertical adjustment installed on a `GtkScrollable` child.
    vadjustment: gtk4::Adjustment,
    /// Horizontal adjustment installed on a `GtkScrollable` child; never scrolls.
    hadjustment: gtk4::Adjustment,
    /// Outer scroller set explicitly through the property, if any.
    explicit_outer: RefCell<Option<glib::WeakRef<gtk4::ScrolledWindow>>>,
    /// Outer scroller currently in use (explicit or discovered).
    outer: RefCell<Option<glib::WeakRef<gtk4::ScrolledWindow>>>,
    /// Signal handlers connected to the outer scroller's vertical adjustment.
    outer_handlers: RefCell<Option<(gtk4::Adjustment, Vec<glib::SignalHandlerId>)>>,
    /// Extra content allocated above and below the viewport.
    pub overscan: Cell<f64>,
    /// True while this bin writes its own adjustment during allocation.
    allocating: Cell<bool>,
    /// The adjustment value this bin last wrote for the child. Anything else
    /// the adjustment holds is a request the child made, and only that is
    /// forwarded to the outer scroller.
    published_offset: Cell<f64>,
    /// Unclamped top of the outer viewport relative to this bin's content at
    /// the last allocation; negative when the bin starts below the viewport.
    viewport_top: Cell<f64>,
    /// The outer scroller's value `viewport_top` was measured against.
    measured_outer_value: Cell<f64>,
    /// Outer scroll delta requested by the child and not yet applied.
    pending_outer_delta: Cell<f64>,
    /// The outer value the pending batch's first delta was measured against.
    /// The idle moves the outer to this anchor plus the pending delta, so two
    /// bins that measured their requests against the same outer value do not
    /// add them up: the later idle's absolute move wins.
    pending_outer_anchor: Cell<f64>,
    /// True while an idle to apply `pending_outer_delta` is scheduled.
    outer_request_scheduled: Cell<bool>,
    /// A divergence the last allocation could not classify, as the offset the
    /// bin had published then and the value the child chose. The next
    /// allocation hands the child its own value back instead of the offset,
    /// so a request is not erased before it can be told from a settle.
    deferred: Cell<Option<DeferredDivergence>>,
    /// Vertical CSS inset (padding plus border) of the child's content box,
    /// learned from the page size the child reports after an allocation. A
    /// `GtkScrollable` works in its content box, so the geometry this bin
    /// publishes must be expressed there or the child overwrites it every
    /// frame and re-derives its value against a different page.
    content_inset: Cell<f64>,
    /// True while the child's scroll anchor was last set by a value this bin
    /// wrote (a publish that changed the value, or a write-back) rather than
    /// by a request the child made. Such an anchor may lie far from the view
    /// (ledger A7), so a page change the bin makes can move the child's value
    /// arbitrarily; see `classify_child_scroll_in_frame`.
    child_anchor_published: Cell<bool>,
    /// True once a non-zero page has shown the child's inset, so
    /// `content_inset` is exact rather than a guess of zero.
    content_inset_known: Cell<bool>,
    /// Allocations this bin has run with a child. A count that grows while
    /// nothing moves is a layout loop.
    pub(super) allocation_count: Cell<u64>,
    /// Times this bin has written its published offset back over a value the
    /// child settled on. A count that grows at rest means the child still
    /// disagrees with the geometry it is handed.
    pub(super) correction_count: Cell<u64>,
}

#[glib::object_subclass]
impl ObjectSubclass for ViewportSliceBin {
    const NAME: &str = "GtkLushViewportSliceBin";
    type Type = super::ViewportSliceBin;
    type ParentType = gtk4::Widget;

    fn new() -> Self {
        Self {
            child: RefCell::new(None),
            vadjustment: gtk4::Adjustment::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.0),
            hadjustment: gtk4::Adjustment::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            explicit_outer: RefCell::new(None),
            outer: RefCell::new(None),
            outer_handlers: RefCell::new(None),
            overscan: Cell::new(0.0),
            allocating: Cell::new(false),
            published_offset: Cell::new(0.0),
            viewport_top: Cell::new(0.0),
            measured_outer_value: Cell::new(0.0),
            pending_outer_delta: Cell::new(0.0),
            pending_outer_anchor: Cell::new(0.0),
            outer_request_scheduled: Cell::new(false),
            deferred: Cell::new(None),
            content_inset: Cell::new(0.0),
            child_anchor_published: Cell::new(false),
            content_inset_known: Cell::new(false),
            allocation_count: Cell::new(0),
            correction_count: Cell::new(0),
        }
    }

    fn class_init(klass: &mut Self::Class) {
        klass.set_css_name("lush-viewport-slice");
    }
}

impl ObjectImpl for ViewportSliceBin {
    fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: LazyLock<Vec<glib::ParamSpec>> = LazyLock::new(|| {
            vec![
                glib::ParamSpecObject::builder::<gtk4::Widget>("child")
                    .explicit_notify()
                    .build(),
                glib::ParamSpecDouble::builder("overscan")
                    .minimum(0.0)
                    .default_value(0.0)
                    .explicit_notify()
                    .build(),
                glib::ParamSpecObject::builder::<gtk4::ScrolledWindow>("outer-scrolled-window")
                    .explicit_notify()
                    .build(),
            ]
        });
        PROPERTIES.as_ref()
    }

    fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        match pspec.name() {
            "child" => {
                let child = value
                    .get::<Option<gtk4::Widget>>()
                    .expect("child property must be a GtkWidget");
                self.set_child(child.as_ref());
            }
            "overscan" => {
                let overscan = value.get::<f64>().expect("overscan must be a double");
                if (self.overscan.get() - overscan).abs() > f64::EPSILON {
                    self.overscan.set(overscan.max(0.0));
                    self.obj().queue_allocate();
                    self.obj().notify("overscan");
                }
            }
            "outer-scrolled-window" => {
                let scroller = value
                    .get::<Option<gtk4::ScrolledWindow>>()
                    .expect("outer-scrolled-window must be a GtkScrolledWindow");
                self.explicit_outer
                    .replace(scroller.as_ref().map(ObjectExt::downgrade));
                self.rebind_outer();
                self.obj().notify("outer-scrolled-window");
            }
            name => panic!("unknown property {name}"),
        }
    }

    fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "child" => self.child.borrow().to_value(),
            "overscan" => self.overscan.get().to_value(),
            "outer-scrolled-window" => self.outer_scrolled_window().to_value(),
            name => panic!("unknown property {name}"),
        }
    }

    fn constructed(&self) {
        self.parent_constructed();
        let bin = self.obj().downgrade();
        self.vadjustment.connect_value_changed(move |adjustment| {
            if let Some(bin) = bin.upgrade() {
                bin.imp().child_adjustment_moved(adjustment.value());
            }
        });
    }

    fn dispose(&self) {
        self.disconnect_outer();
        if let Some(child) = self.child.borrow_mut().take() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ViewportSliceBin {
    fn root(&self) {
        self.parent_root();
        self.rebind_outer();
    }

    fn unroot(&self) {
        self.disconnect_outer();
        self.outer.replace(None);
        // The theme may differ under the next root; relearn the inset there.
        self.content_inset.set(0.0);
        self.content_inset_known.set(false);
        self.obj().queue_resize();
        self.parent_unroot();
    }

    fn request_mode(&self) -> gtk4::SizeRequestMode {
        self.child
            .borrow()
            .as_ref()
            .map_or(gtk4::SizeRequestMode::ConstantSize, WidgetExt::request_mode)
    }

    fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
        // Hold the borrow rather than cloning: measure runs several times per
        // layout pass and nothing below can re-enter `set_child`.
        let child = self.child.borrow();
        let Some(child) = child.as_ref().filter(|child| child.should_layout()) else {
            return (0, 0, -1, -1);
        };
        let (minimum, natural, _, _) = child.measure(orientation, for_size);
        match orientation {
            // The full content height is what the outer scroller must see so
            // its range stays exact. `GtkViewport` allocates a non-scrollable
            // child its *minimum* in the scroll direction, so inside an outer
            // scroller the bin asks for the full content as minimum too (the
            // child itself still receives only the slice). Standalone, the
            // minimum is zero so a bare host window is not forced to the
            // content height; the child then gets whatever the host allots.
            gtk4::Orientation::Vertical => {
                let content = natural.max(minimum);
                let minimum = if self.outer.borrow().is_some() {
                    content
                } else {
                    0
                };
                (minimum, content, -1, -1)
            }
            // Width follows the host, never the widest row, mirroring the
            // `propagate-natural-width = false` contract this bin replaces.
            _ => (minimum, minimum, -1, -1),
        }
    }

    fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
        let Some(child) = self.child.borrow().as_ref().cloned() else {
            return;
        };
        if !child.should_layout() {
            return;
        }

        let content_height = f64::from(height.max(0));
        let (viewport_top, viewport_height) = self.visible_band().unwrap_or((0.0, content_height));
        self.viewport_top.set(viewport_top);
        if let Some(outer) = self.outer_scrolled_window() {
            self.measured_outer_value.set(outer.vadjustment().value());
        }
        let slice = viewport_slice(
            viewport_top,
            viewport_height,
            content_height,
            self.overscan.get(),
        );
        // Never hand the child a zero page (ledger A5): once the inset is
        // known the band is at least that inset plus one pixel tall. Until a
        // page has revealed it, a full viewport's band keeps the page real, so
        // the inset is learned exactly in one allocation.
        let min_band = if self.content_inset_known.get() {
            whole_pixel_floor(self.content_inset.get()) + 1
        } else {
            whole_pixel_floor(viewport_height)
        };
        let (slice_top, slice_height) = whole_pixel_band(slice, height, min_band);

        self.allocation_count.set(self.allocation_count.get() + 1);
        self.allocating.set(true);
        // A deferred divergence survives only while the bin publishes the same
        // offset it was measured against; once the outer scroller has moved,
        // the band re-derives from it like any other scroll.
        let deferred = self.deferred.take();
        let held_value = deferred
            .filter(|held| (held.published - f64::from(slice_top)).abs() < ADJUSTMENT_EPSILON)
            .map(|held| held.child_value);
        let held_before = (
            self.vadjustment.value(),
            self.vadjustment.page_size(),
            self.vadjustment.upper(),
        );
        self.publish_slice_offset(
            f64::from(slice_top),
            content_height,
            f64::from(slice_height),
            held_value,
        );
        // What this publish did to the child: a changed value emitted
        // `value-changed`, which re-anchors the child on the value the bin
        // wrote (and drops any pending request, ledger A9); a changed page or
        // upper makes the child re-derive its value from that anchor (A7).
        let moved = |before: f64, after: f64| (after - before).abs() >= ADJUSTMENT_EPSILON;
        let emitted = moved(held_before.0, self.vadjustment.value());
        let bin_reconfigured = moved(held_before.1, self.vadjustment.page_size())
            || moved(held_before.2, self.vadjustment.upper());
        if emitted {
            self.child_anchor_published.set(true);
        }
        let transform = gsk::Transform::new()
            .translate(&graphene::Point::new(0.0, pixel_coordinate(slice_top)));
        // A `GtkScrollable` child may rewrite the geometry this bin just
        // configured -- a `GtkListView` substitutes its own content-box numbers
        // -- so capture it first: how far the geometry moved bounds how far the
        // value may have settled as a consequence (see `scroll_request`).
        let upper_before = self.vadjustment.upper();
        let page_before = self.vadjustment.page_size();
        child.allocate(width, slice_height, -1, Some(transform));
        if emitted {
            // The publish's `value-changed` reached the child before it was
            // allocated at the new value, which leaves its anchor far from the
            // view (ledger A20). Now that it is allocated there, announce the
            // value once more so it re-anchors on the row at the view's edge;
            // otherwise a later page change moves the value by the offset's
            // share of the change, and a `scroll_to` of a row taller than a
            // small band keeps that stray alignment. `allocating` still holds,
            // so this bin's own handler ignores the emission.
            self.vadjustment.emit_by_name::<()>("value-changed", &[]);
        }
        let page_shift = (self.vadjustment.page_size() - page_before).abs();
        let reconfigure_shift = (self.vadjustment.upper() - upper_before)
            .abs()
            .max(page_shift);

        // A shortened page is the child's content box: learn the inset and lay
        // out again at once so geometry the child does not correct lands this
        // frame. The `page_shift` guard keeps a zero-height slice, where the
        // page collapses for a different reason, from unlearning it.
        let learned = (f64::from(slice_height) - self.vadjustment.page_size()).max(0.0);
        let inset_changed = page_shift >= ADJUSTMENT_EPSILON
            && (learned - self.content_inset.get()).abs() >= ADJUSTMENT_EPSILON;
        if inset_changed {
            self.content_inset.set(learned);
            self.obj().queue_allocate();
        }
        // The first non-zero page reveals the inset; the band floor now
        // follows it, so lay out once more, as for a learned inset.
        if self.vadjustment.page_size() >= ADJUSTMENT_EPSILON
            && !self.content_inset_known.replace(true)
        {
            self.obj().queue_allocate();
        }

        // Only a value that differs from the one just published is a request
        // to show a different band. A value that is neither is a settle: the
        // child renders against it while this bin placed it at the published
        // offset, so it is written back here, while `allocating` still holds
        // and the bin's own handler ignores the emission. A divergence within
        // the correction the child just made could be either, so it is left
        // in place and classified by one more allocation (see
        // `scroll_request`). The learning frame skips both: its requeued pass
        // republishes anyway.
        let settled = self.vadjustment.value();
        let published = self.published_offset.get();
        let facts = ChildAnchorFacts {
            emitted,
            anchor_published: self.child_anchor_published.get(),
            bin_reconfigured,
        };
        match classify_child_scroll_in_frame(
            published,
            settled,
            viewport_top,
            reconfigure_shift,
            facts,
        ) {
            ChildScrollDecision::Request(delta) => {
                // The child moved itself: its anchor is now its own.
                self.child_anchor_published.set(false);
                self.follow_child_request(delta);
            }
            ChildScrollDecision::Settle if !inset_changed => {
                self.correction_count.set(self.correction_count.get() + 1);
                self.child_anchor_published.set(true);
                self.vadjustment.set_value(published);
            }
            ChildScrollDecision::Defer if !inset_changed => {
                self.deferred.set(Some(DeferredDivergence {
                    published,
                    child_value: settled,
                }));
                // One re-allocation per divergence: a child whose geometry
                // never stops moving keeps its value until the next
                // allocation that happens anyway, instead of looping here.
                if held_value.is_none() {
                    self.obj().queue_allocate();
                }
            }
            ChildScrollDecision::Rest
            | ChildScrollDecision::Settle
            | ChildScrollDecision::Defer => {}
        }
        self.allocating.set(false);
    }
}

/// A child value the bin left in place because the allocation that produced
/// it also moved the child's geometry; see `ViewportSliceBin::deferred`.
#[derive(Clone, Copy)]
struct DeferredDivergence {
    /// The offset the bin had published in that allocation.
    published: f64,
    /// The value the child chose in it.
    child_value: f64,
}

/// The smallest whole number of pixels covering `inset`, which is a learned
/// CSS inset and so small and non-negative.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a CSS inset is a few pixels; the saturating cast is the intent"
)]
fn whole_pixel_floor(inset: f64) -> i32 {
    inset.ceil().max(0.0) as i32
}

/// Convert a whole-pixel offset to the `f32` graphene coordinate space.
#[expect(
    clippy::cast_precision_loss,
    reason = "widget offsets stay far below f32's exact-integer range"
)]
fn pixel_coordinate(value: i32) -> f32 {
    value as f32
}

impl ViewportSliceBin {
    pub(super) fn set_child(&self, child: Option<&gtk4::Widget>) {
        replace_child(
            &self.child,
            self.obj().upcast_ref(),
            child,
            |old_child| {
                if let Some(scrollable) = old_child.dynamic_cast_ref::<gtk4::Scrollable>() {
                    scrollable.set_vadjustment(None::<&gtk4::Adjustment>);
                    scrollable.set_hadjustment(None::<&gtk4::Adjustment>);
                }
                // A new child has its own inset; relearn from its first page.
                // Its anchor is its own until the bin writes a value into it.
                self.content_inset.set(0.0);
                self.content_inset_known.set(false);
                self.child_anchor_published.set(false);
            },
            |new_child| {
                if let Some(scrollable) = new_child.dynamic_cast_ref::<gtk4::Scrollable>() {
                    scrollable.set_vadjustment(Some(&self.vadjustment));
                    scrollable.set_hadjustment(Some(&self.hadjustment));
                }
            },
        );
    }

    pub(super) fn outer_scrolled_window(&self) -> Option<gtk4::ScrolledWindow> {
        self.outer
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
    }

    /// Resolve the outer scroller (explicit first, then nearest ancestor) and
    /// follow its vertical adjustment.
    fn rebind_outer(&self) {
        self.disconnect_outer();
        let explicit = self
            .explicit_outer
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade);
        let outer = explicit.or_else(|| {
            self.obj()
                .ancestor(gtk4::ScrolledWindow::static_type())
                .and_downcast::<gtk4::ScrolledWindow>()
        });
        self.outer.replace(outer.as_ref().map(ObjectExt::downgrade));
        let Some(outer) = outer else {
            return;
        };

        let adjustment = outer.vadjustment();
        let bin = self.obj().downgrade();
        let on_value = adjustment.connect_value_changed({
            let bin = bin.clone();
            move |_| {
                if let Some(bin) = bin.upgrade() {
                    bin.queue_allocate();
                }
            }
        });
        let on_page = adjustment.connect_page_size_notify(move |_| {
            // `GtkViewport` thaws its adjustments' notifications only after it
            // has allocated its child, so this runs inside layout, after this
            // bin's allocation for the frame (ledger A19). Queued here, the
            // allocation would be pending while the frame is drawn, which GTK
            // reports as a snapshot without a current allocation; re-slice
            // from an idle instead, after the frame.
            let bin = bin.clone();
            glib::idle_add_local_once(move || {
                if let Some(bin) = bin.upgrade() {
                    bin.queue_allocate();
                }
            });
        });
        self.outer_handlers
            .replace(Some((adjustment, vec![on_value, on_page])));
        self.obj().queue_resize();
    }

    fn disconnect_outer(&self) {
        if let Some((adjustment, handlers)) = self.outer_handlers.borrow_mut().take() {
            for handler in handlers {
                adjustment.disconnect(handler);
            }
        }
    }

    /// The outer viewport band relative to this bin's content, when an outer
    /// scroller is mapped and the bin's position inside it can be computed.
    fn visible_band(&self) -> Option<(f64, f64)> {
        let outer = self.outer_scrolled_window()?;
        if !outer.is_mapped() {
            return None;
        }
        let origin = self.obj().compute_point(&outer, &graphene::Point::zero())?;
        let viewport_height = f64::from(outer.height().max(0));
        if viewport_height <= 0.0 {
            return None;
        }
        let viewport_top = -f64::from(origin.y());
        Some((viewport_top, viewport_height))
    }

    /// Write the slice offset the child should rest at and remember it as the
    /// baseline that tells this bin's own writes apart from the child's.
    ///
    /// With `held_value`, the adjustment keeps that value instead: a deferred
    /// divergence the child must still hold when it is allocated again, since
    /// a `GtkListBase` re-anchors on any value it is handed and would drop the
    /// request being deferred. The baseline is the offset all the same.
    fn publish_slice_offset(
        &self,
        offset: f64,
        content_height: f64,
        slice_height: f64,
        held_value: Option<f64>,
    ) {
        // Upper and page are expressed in the child's content box; the value
        // is not offset, because a row at child content `y` is drawn at
        // `offset + inset_top + (y - value)` and this bin's own frame (its
        // measured natural height) already contains the inset.
        let inset = self.content_inset.get();
        let page = (slice_height - inset).max(0.0);
        let upper = (content_height - inset).max(page);
        self.vadjustment.configure(
            held_value.unwrap_or(offset),
            0.0,
            upper,
            self.vadjustment.step_increment(),
            page,
            page,
        );
        // Read back rather than storing `offset`: `configure` clamps to
        // `[lower, upper - page_size]`, and a baseline that disagrees with the
        // adjustment by even a pixel is read as a child request forever after.
        // A held value is not the baseline, so clamp the offset the same way.
        let published = match held_value {
            Some(_) => offset.clamp(0.0, upper - page),
            None => self.vadjustment.value(),
        };
        self.published_offset.set(published);
    }

    /// The child moved its own adjustment outside an allocation (for example a
    /// deferred `scroll_to`): treat it like an in-allocation request.
    fn child_adjustment_moved(&self, value: f64) {
        if self.allocating.get() {
            return;
        }
        // Outside an allocation there is no reconfigure in flight: the child
        // moved the value on its own, which is the deferred `scroll_to` case.
        // Its newer value supersedes any divergence still held for it.
        self.deferred.set(None);
        if let Some(delta) = outer_scroll_request(
            self.published_offset.get(),
            value,
            self.viewport_top.get(),
            0.0,
        ) {
            self.child_anchor_published.set(false);
            self.follow_child_request(delta);
        }
    }

    /// Translate a child-originated scroll request into an outer scroll so the
    /// requested band enters the real viewport; the next allocation re-derives
    /// the slice from the outer position.
    ///
    /// `delta` is measured against the unclamped viewport top, not the clamped
    /// slice offset: at the top of the content the slice starts at zero while
    /// the viewport may start above the bin (a section header, say), and that
    /// gap is part of the distance the outer scroller has to travel so that the
    /// re-slice republishes exactly the value the child chose (see
    /// `scroll_request` for why anything less wipes the child's pending
    /// requests).
    ///
    /// The request usually arrives from inside a layout pass (the child applies
    /// `scroll_to` in its own allocation). Moving the outer adjustment there
    /// does not reliably schedule a further layout, so the delta is accumulated
    /// and applied from an idle, where the outer scroller's own `value-changed`
    /// path re-slices this bin like any user scroll. `Adjustment::set_value`
    /// clamps to `[lower, upper - page_size]` and emits nothing for an
    /// unchanged value, so a request the outer scroller cannot honour ends
    /// there instead of re-queuing an allocation.
    ///
    /// The idle's move is **anchored**: the first request of a batch records
    /// the outer value its `viewport_top` was measured against, and the idle
    /// sets `anchor + pending` rather than adding `pending` to whatever the
    /// outer holds by then. Several bins sharing one outer scroller measure
    /// their requests in the same frame against the same outer value; adding
    /// each delta to the value the previous bin's idle already moved would
    /// land the outer at the sum, where no request is honoured. Anchored, the
    /// later idle's absolute move wins, the superseded bin re-slices from the
    /// new value like any scroll, and its dropped `scroll_to` (ledger A9) is
    /// not re-issued. Requests one bin accumulates before its idle runs keep
    /// adding up within the batch: replacing that with the latest request
    /// regressed keyboard traversal. A user scroll made between a request and
    /// its idle (less than one frame) is overridden rather than added to.
    fn follow_child_request(&self, delta: f64) {
        if self.outer.borrow().is_none() {
            return;
        }
        if !self.outer_request_scheduled.get() {
            self.pending_outer_anchor
                .set(self.measured_outer_value.get());
        }
        self.pending_outer_delta
            .set(self.pending_outer_delta.get() + delta);
        if self.outer_request_scheduled.replace(true) {
            return;
        }
        let bin = self.obj().downgrade();
        glib::idle_add_local_once(move || {
            let Some(bin) = bin.upgrade() else {
                return;
            };
            let imp = bin.imp();
            imp.outer_request_scheduled.set(false);
            let delta = imp.pending_outer_delta.replace(0.0);
            let Some(outer) = imp.outer_scrolled_window() else {
                return;
            };
            let anchor = imp.pending_outer_anchor.get();
            outer.vadjustment().set_value(anchor + delta);
        });
    }
}
