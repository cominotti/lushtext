// SPDX-License-Identifier: MIT OR Apache-2.0

//! A19: `GtkViewport` freezes property notification on its adjustments for
//! the whole of its `size_allocate` and thaws it only **after** it has
//! allocated its child, while the `value-changed` that
//! `gtk_adjustment_configure` emits when it clamps the value fires
//! immediately (`gtk/gtkviewport.c`, `gtk_viewport_size_allocate`). So, in
//! the frame a viewport is resized, a handler sees:
//!
//! 1. `value-changed` (when the new page clamps the value), **before** the
//!    child is allocated;
//! 2. the child's `size_allocate`, which already reads the new `page-size`;
//! 3. `notify::page-size` and `notify::value`, still inside layout, **after**
//!    the child subtree was allocated for that frame.
//!
//! The consequence the slice bin relies on: a `queue_allocate` issued from a
//! `notify::page-size` handler for a widget inside the viewport is not served
//! in that frame. Its ancestors are still being allocated, so the request
//! never reaches the toplevel, and the widget would be drawn without a
//! current allocation (GTK prints `Trying to snapshot … without a current
//! allocation`). The probe measures the missing allocation and then keeps the
//! widget out of that frame's paint, so it never prints the warning itself.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use super::{LAYOUT_SETTLE, frame_of, join_entries, same_value};
use crate::fixtures::FixedHost;
use crate::observation::{Recorder, Stop};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The height the probe widget requests inside the viewport.
pub const CONTENT_HEIGHT: i32 = 2_000;
/// The viewport height at rest.
pub const RESTING_HEIGHT: i32 = 300;
/// The taller viewport: at the bottom, its page clamps the value.
pub const GROWN_HEIGHT: i32 = 500;
/// The shorter viewport the queue-from-notify step shrinks to.
pub const SHRUNK_HEIGHT: i32 = 400;
/// The viewport widths paired with [`RESTING_HEIGHT`], [`GROWN_HEIGHT`], and
/// [`SHRUNK_HEIGHT`]. A viewport re-allocates its child's size only when that
/// size changes, and a height change alone leaves a child taller than the
/// viewport the same size (it only moves), so each step also narrows the
/// viewport to make the viewport run the child's `size_allocate`.
pub const VIEWPORT_WIDTHS: [i32; 3] = [360, 340, 320];

/// One event the fixture logs, with the frame-clock frame it happened in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewportEvent {
    /// The outer adjustment's `value-changed`.
    ValueChanged {
        /// The frame-clock frame counter (`-1` without a frame clock).
        frame: i64,
        /// The adjustment's value in the emission.
        value: f64,
    },
    /// The probe widget's `size_allocate`.
    Allocated {
        /// The frame-clock frame counter.
        frame: i64,
        /// The adjustment's `page-size`, read inside the allocation.
        page: f64,
    },
    /// `notify::page-size` on the outer adjustment.
    PageNotified {
        /// The frame-clock frame counter.
        frame: i64,
        /// The page size in the notification.
        page: f64,
    },
    /// `notify::value` on the outer adjustment.
    ValueNotified {
        /// The frame-clock frame counter.
        frame: i64,
    },
    /// The end of one layout pass of a frame (a frame-clock `layout` handler
    /// connected after GTK's own).
    LayoutEnd {
        /// The frame-clock frame counter.
        frame: i64,
    },
}

impl ViewportEvent {
    /// The frame the event happened in.
    #[must_use]
    pub fn frame(self) -> i64 {
        match self {
            Self::ValueChanged { frame, .. }
            | Self::Allocated { frame, .. }
            | Self::PageNotified { frame, .. }
            | Self::ValueNotified { frame }
            | Self::LayoutEnd { frame } => frame,
        }
    }
}

impl fmt::Display for ViewportEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ValueChanged { frame, value } => {
                write!(formatter, "value-changed({value})@{frame}")
            }
            Self::Allocated { frame, page } => {
                write!(formatter, "child-allocate(page {page})@{frame}")
            }
            Self::PageNotified { frame, page } => {
                write!(formatter, "notify::page-size({page})@{frame}")
            }
            Self::ValueNotified { frame } => write!(formatter, "notify::value@{frame}"),
            Self::LayoutEnd { frame } => write!(formatter, "layout-end@{frame}"),
        }
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk4::glib;
    use gtk4::subclass::prelude::*;

    /// Hook run inside `size_allocate`.
    type AllocationHook = Box<dyn Fn()>;

    #[derive(Default)]
    pub struct AllocationProbe {
        pub allocations: Cell<u32>,
        pub on_allocate: RefCell<Option<AllocationHook>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AllocationProbe {
        const NAME: &str = "GtkLushAxiomsAllocationProbe";
        type Type = super::AllocationProbe;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for AllocationProbe {}

    impl WidgetImpl for AllocationProbe {
        fn measure(&self, orientation: gtk4::Orientation, _: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk4::Orientation::Vertical => {
                    (super::CONTENT_HEIGHT, super::CONTENT_HEIGHT, -1, -1)
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, _width: i32, _height: i32, _baseline: i32) {
            self.allocations.set(self.allocations.get() + 1);
            if let Some(hook) = self.on_allocate.borrow().as_ref() {
                hook();
            }
        }
    }
}

gtk4::glib::wrapper! {
    /// A childless widget [`CONTENT_HEIGHT`] tall that counts its
    /// allocations and runs a hook inside each.
    pub struct AllocationProbe(ObjectSubclass<imp::AllocationProbe>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl AllocationProbe {
    fn new() -> Self {
        gtk4::glib::Object::new()
    }

    /// How many times GTK has allocated the widget.
    #[must_use]
    pub fn allocations(&self) -> u32 {
        self.imp().allocations.get()
    }
}

/// A queue issued from `notify::page-size` and what became of it.
#[derive(Clone, Copy, Debug, Default)]
pub struct QueuedFromNotify {
    /// The frame the handler queued the allocation in.
    pub frame: i64,
    /// The probe widget's allocations when it queued.
    pub allocations_at_queue: u32,
    /// Its allocations at the end of that frame's last layout pass, once
    /// read.
    pub allocations_at_layout_end: Option<u32>,
    /// How many layout passes that frame ran after the queue.
    pub layout_passes: u32,
}

/// The A19 fixture: a [`FixedHost`] allocating a `GtkScrolledWindow` at a
/// chosen height; the scroller wraps an [`AllocationProbe`] in a
/// `GtkViewport`. The fixture logs the outer adjustment's signals and the
/// probe widget's allocations in the order they happen.
#[derive(Clone, Debug)]
pub struct ViewportOrder {
    /// The host whose child height is the viewport height.
    pub host: FixedHost,
    /// The scroller.
    pub scroller: gtk4::ScrolledWindow,
    /// The viewport the scroller created around the probe widget.
    pub viewport: Option<gtk4::Viewport>,
    /// The widget inside the viewport.
    pub probe: AllocationProbe,
    log: Rc<RefCell<Vec<ViewportEvent>>>,
    arm_queue: Rc<Cell<bool>>,
    queued: Rc<Cell<Option<QueuedFromNotify>>>,
    layout_handler: Rc<RefCell<Option<(gtk4::gdk::FrameClock, gtk4::glib::SignalHandlerId)>>>,
}

impl ViewportOrder {
    /// Build the fixture at [`RESTING_HEIGHT`].
    #[must_use]
    pub fn new() -> Self {
        let probe = AllocationProbe::new();
        let scroller = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .child(&probe)
            .build();
        let viewport = scroller.child().and_downcast::<gtk4::Viewport>();
        let host = FixedHost::new(&scroller, RESTING_HEIGHT);
        host.set_child_width(VIEWPORT_WIDTHS[0]);
        let fixture = Self {
            host,
            scroller,
            viewport,
            probe,
            log: Rc::default(),
            arm_queue: Rc::default(),
            queued: Rc::default(),
            layout_handler: Rc::default(),
        };
        fixture.connect_log();
        fixture
    }

    fn connect_log(&self) {
        let outer = self.scroller.vadjustment();
        let log = self.log.clone();
        let weak = self.probe.downgrade();
        outer.connect_value_changed(move |adjustment| {
            let frame = weak.upgrade().map_or(-1, |probe| frame_of(&probe));
            log.borrow_mut().push(ViewportEvent::ValueChanged {
                frame,
                value: adjustment.value(),
            });
        });
        let log = self.log.clone();
        let weak = self.probe.downgrade();
        outer.connect_notify_local(Some("value"), move |_, _| {
            let frame = weak.upgrade().map_or(-1, |probe| frame_of(&probe));
            log.borrow_mut()
                .push(ViewportEvent::ValueNotified { frame });
        });
        let log = self.log.clone();
        let weak = self.probe.downgrade();
        let arm = self.arm_queue.clone();
        let queued = self.queued.clone();
        outer.connect_notify_local(Some("page-size"), move |adjustment, _| {
            let Some(probe) = weak.upgrade() else {
                return;
            };
            let frame = frame_of(&probe);
            log.borrow_mut().push(ViewportEvent::PageNotified {
                frame,
                page: adjustment.page_size(),
            });
            if arm.replace(false) {
                queued.set(Some(QueuedFromNotify {
                    frame,
                    allocations_at_queue: probe.allocations(),
                    allocations_at_layout_end: None,
                    layout_passes: 0,
                }));
                probe.queue_allocate();
            }
        });
        let log = self.log.clone();
        let weak = self.probe.downgrade();
        let outer = self.scroller.vadjustment();
        self.probe.imp().on_allocate.replace(Some(Box::new(move || {
            let frame = weak.upgrade().map_or(-1, |probe| frame_of(&probe));
            log.borrow_mut().push(ViewportEvent::Allocated {
                frame,
                page: outer.page_size(),
            });
        })));
    }

    /// Start logging frame ends; call once the fixture is realized. Returns
    /// whether the widget has a frame clock to log them on.
    #[must_use]
    pub fn watch_layout_ends(&self) -> bool {
        let Some(clock) = self.probe.frame_clock() else {
            return false;
        };
        let log = self.log.clone();
        let queued = self.queued.clone();
        let probe = self.probe.downgrade();
        let viewport = self.viewport.clone();
        let handler = clock.connect_layout(move |clock| {
            let frame = clock.frame_counter();
            log.borrow_mut().push(ViewportEvent::LayoutEnd { frame });
            let Some(probe) = probe.upgrade() else {
                return;
            };
            if let Some(pending) = queued.get()
                && pending.frame == frame
            {
                // Every layout pass of that frame updates the reading, so it
                // covers the passes GDK re-runs within the frame; only the
                // first keeps the widget out of the paint.
                queued.set(Some(QueuedFromNotify {
                    allocations_at_layout_end: Some(probe.allocations()),
                    layout_passes: pending.layout_passes + 1,
                    ..pending
                }));
                if pending.layout_passes == 0 {
                    keep_out_of_this_paint(&probe, viewport.as_ref());
                }
            }
        });
        if let Some((old_clock, old)) = self.layout_handler.borrow_mut().replace((clock, handler)) {
            old_clock.disconnect(old);
        }
        true
    }

    /// Stop logging frame ends.
    pub fn unwatch_layout_ends(&self) {
        if let Some((clock, handler)) = self.layout_handler.borrow_mut().take() {
            clock.disconnect(handler);
        }
    }

    /// Allocate the viewport `width` × `height` from the next frame on.
    pub fn set_viewport_size(&self, width: i32, height: i32) {
        self.host.set_child_width(width);
        self.host.set_child_height(height);
    }

    /// Scroll the outer adjustment to its end.
    pub fn scroll_to_end(&self) {
        let outer = self.scroller.vadjustment();
        outer.set_value(outer.upper() - outer.page_size());
    }

    /// Queue an allocation of the probe widget from the next
    /// `notify::page-size` handler. The fixture reads the widget's
    /// allocation count at the end of that frame's layout, and then keeps the
    /// unallocated widget out of that frame's paint.
    pub fn queue_allocate_from_next_page_notify(&self) {
        self.queued.set(None);
        self.arm_queue.set(true);
    }

    /// What became of the last queue issued from `notify::page-size`.
    #[must_use]
    pub fn queued_from_notify(&self) -> Option<QueuedFromNotify> {
        self.queued.get()
    }

    /// Take the logged events.
    #[must_use]
    pub fn take_log(&self) -> Vec<ViewportEvent> {
        self.log.take()
    }

    /// The logged events so far, without taking them.
    #[must_use]
    pub fn log(&self) -> Vec<ViewportEvent> {
        self.log.borrow().clone()
    }
}

impl Default for ViewportOrder {
    fn default() -> Self {
        Self::new()
    }
}

/// Keep `probe`, which GTK left without a current allocation, from being
/// drawn in this frame: unmap it now, and in the next idle re-show it with a
/// relayout queued on the viewport, whose own pending flags are clear again,
/// so it is allocated before it is drawn. Without this GTK prints
/// `Trying to snapshot … without a current allocation` in this frame's paint.
fn keep_out_of_this_paint(probe: &AllocationProbe, viewport: Option<&gtk4::Viewport>) {
    probe.set_child_visible(false);
    let probe = probe.downgrade();
    let viewport = viewport.map(ObjectExt::downgrade);
    gtk4::glib::idle_add_local_once(move || {
        if let Some(viewport) = viewport.and_then(|weak| weak.upgrade()) {
            viewport.queue_allocate();
        }
        if let Some(probe) = probe.upgrade() {
            probe.set_child_visible(true);
        }
    });
}

/// The index of the first event satisfying `predicate`.
fn position(events: &[ViewportEvent], predicate: impl Fn(&ViewportEvent) -> bool) -> Option<usize> {
    events.iter().position(predicate)
}

fn describe(events: &[ViewportEvent]) -> String {
    join_entries(events)
}

/// Probe A19. See the module documentation.
#[must_use]
pub fn probe_a19() -> Observation {
    Recorder::run(AxiomId::new(19), |recorder| {
        let fixture = ViewportOrder::new();
        let _shown = Presented::checked(
            recorder,
            &fixture.host,
            "control: the fixture window realizes",
        )?;
        recorder.control(
            fixture.viewport.is_some(),
            "control: the scroller wraps the probe widget in a viewport",
        )?;
        recorder.control(
            fixture.watch_layout_ends(),
            "control: the probe widget has a frame clock",
        )?;
        let result = measure_order(recorder, &fixture);
        fixture.unwatch_layout_ends();
        result
    })
}

fn measure_order(recorder: &mut Recorder, fixture: &ViewportOrder) -> Result<(), Stop> {
    let outer = fixture.scroller.vadjustment();
    fixture.scroll_to_end();
    settle(LAYOUT_SETTLE);
    recorder.measure("resting_page", outer.page_size());
    recorder.measure("resting_value", outer.value());
    recorder.control(
        same_value(outer.page_size(), f64::from(RESTING_HEIGHT))
            && same_value(outer.value(), f64::from(CONTENT_HEIGHT - RESTING_HEIGHT)),
        "control: at rest the viewport shows the end of the content",
    )?;

    // (i) and (ii): grow the viewport at the end of the content, so the new
    // page clamps the value.
    let _ = fixture.take_log();
    fixture.set_viewport_size(VIEWPORT_WIDTHS[1], GROWN_HEIGHT);
    settle(LAYOUT_SETTLE);
    let grown = fixture.take_log();
    recorder.measure("grow_events", describe(&grown));
    recorder.measure("grown_page", outer.page_size());
    recorder.measure("grown_value", outer.value());
    let page = f64::from(GROWN_HEIGHT);
    let Some(allocated) = position(
        &grown,
        |event| matches!(event, ViewportEvent::Allocated { page: seen, .. } if same_value(*seen, page)),
    ) else {
        return recorder.control(false, "control: the child is allocated at the grown page");
    };
    let frame = grown[allocated].frame();
    let in_frame = |index: Option<usize>| index.filter(|&index| grown[index].frame() == frame);
    let clamped = in_frame(position(&grown, |event| {
        matches!(event, ViewportEvent::ValueChanged { value, .. }
            if same_value(*value, f64::from(CONTENT_HEIGHT - GROWN_HEIGHT)))
    }));
    let page_notified = in_frame(position(
        &grown,
        |event| matches!(event, ViewportEvent::PageNotified { page: seen, .. } if same_value(*seen, page)),
    ));
    let value_notified = in_frame(position(&grown, |event| {
        matches!(event, ViewportEvent::ValueNotified { .. })
    }));
    let layout_end = in_frame(position(&grown, |event| {
        matches!(event, ViewportEvent::LayoutEnd { .. })
    }));
    recorder.measure("grow_frame", frame);
    recorder.control(
        clamped.is_some() && page_notified.is_some() && layout_end.is_some(),
        "control: the grow clamps the value, notifies the page, and ends layout in one frame",
    )?;
    recorder.axiom(
        clamped.is_some_and(|index| index < allocated),
        "A19: value-changed from the clamp fires before the child is allocated",
    )?;
    recorder.axiom(
        page_notified.is_some_and(|index| index > allocated)
            && value_notified.is_some_and(|index| index > allocated),
        "A19: notify::page-size and notify::value arrive after the child's allocation",
    )?;
    recorder.axiom(
        page_notified
            .zip(layout_end)
            .is_some_and(|(notified, end)| notified < end),
        "A19: notify::page-size arrives inside that frame's layout phase",
    )?;

    // (iii): a queue_allocate from notify::page-size is not served in the
    // frame it was issued in.
    let _ = fixture.take_log();
    fixture.queue_allocate_from_next_page_notify();
    fixture.set_viewport_size(VIEWPORT_WIDTHS[2], SHRUNK_HEIGHT);
    settle(LAYOUT_SETTLE);
    let shrunk = fixture.take_log();
    recorder.measure("shrink_events", describe(&shrunk));
    let Some(queued) = fixture.queued_from_notify() else {
        return recorder.control(false, "control: the page-size handler queued an allocation");
    };
    recorder.measure("queue_frame", queued.frame);
    recorder.measure("allocations_at_queue", queued.allocations_at_queue);
    let Some(at_layout_end) = queued.allocations_at_layout_end else {
        return recorder.control(false, "control: the frame that queued it ended its layout");
    };
    recorder.measure("layout_passes_after_queue", queued.layout_passes);
    recorder.measure("allocations_at_layout_end", at_layout_end);
    settle(LAYOUT_SETTLE);
    recorder.measure("later_events", describe(&fixture.take_log()));
    recorder.measure("allocations_later", fixture.probe.allocations());
    recorder.axiom(
        at_layout_end == queued.allocations_at_queue,
        "A19: an allocation queued from notify::page-size is not served in that frame",
    )?;
    recorder.control(
        fixture.probe.allocations() > at_layout_end,
        "control: a relayout queued on the viewport from outside layout allocates it",
    )
}
