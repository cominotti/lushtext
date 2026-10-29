// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the toplevel-presentation and late-composition probes (A46–A49)
//! share: a childless widget that counts its measures, allocations and
//! snapshots, and a frame-phase recorder that stamps each frame's `update`,
//! `layout`, `paint` and `after-paint` against those counts.
//!
//! GDK connects the surface's own frame-clock handlers when the surface is
//! created, before a probe can, so a probe's `layout` handler runs after GTK
//! measured and allocated the window for that frame: a count read there says
//! what the layout phase did.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

mod imp {
    use std::cell::Cell;

    use gtk4::glib;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct CountingWidget {
        pub horizontal_measures: Cell<u32>,
        pub vertical_measures: Cell<u32>,
        pub allocations: Cell<u32>,
        pub snapshots: Cell<u32>,
        pub width: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CountingWidget {
        const NAME: &str = "GtkLushAxiomsCountingWidget";
        type Type = super::CountingWidget;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for CountingWidget {}

    impl WidgetImpl for CountingWidget {
        fn measure(&self, orientation: gtk4::Orientation, _: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk4::Orientation::Horizontal {
                self.horizontal_measures
                    .set(self.horizontal_measures.get() + 1);
                let width = self.width.get();
                (width, width, -1, -1)
            } else {
                self.vertical_measures.set(self.vertical_measures.get() + 1);
                (super::COUNTING_HEIGHT, super::COUNTING_HEIGHT, -1, -1)
            }
        }

        fn size_allocate(&self, _width: i32, _height: i32, _baseline: i32) {
            self.allocations.set(self.allocations.get() + 1);
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            self.snapshots.set(self.snapshots.get() + 1);
            self.parent_snapshot(snapshot);
        }
    }
}

/// The height a [`CountingWidget`] requests.
pub const COUNTING_HEIGHT: i32 = 40;

gtk4::glib::wrapper! {
    /// A childless widget of a fixed natural size that counts how often GTK
    /// measures, allocates and snapshots it.
    pub struct CountingWidget(ObjectSubclass<imp::CountingWidget>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl CountingWidget {
    /// A widget requesting `width` × [`COUNTING_HEIGHT`].
    #[must_use]
    pub fn new(width: i32) -> Self {
        let widget: Self = gtk4::glib::Object::new();
        widget.imp().width.set(width);
        widget
    }

    /// Measures in either orientation so far.
    #[must_use]
    pub fn measures(&self) -> u32 {
        self.imp().horizontal_measures.get() + self.imp().vertical_measures.get()
    }

    /// Horizontal measures so far.
    #[must_use]
    pub fn horizontal_measures(&self) -> u32 {
        self.imp().horizontal_measures.get()
    }

    /// Allocations so far.
    #[must_use]
    pub fn allocations(&self) -> u32 {
        self.imp().allocations.get()
    }

    /// Snapshots so far.
    #[must_use]
    pub fn snapshots(&self) -> u32 {
        self.imp().snapshots.get()
    }
}

/// One frame-clock phase, stamped with a widget's measure count when the
/// probe's handler of that phase ran.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhaseStamp {
    /// The frame-clock frame counter.
    pub frame: i64,
    /// `update`, `layout`, `paint` or `after-paint`.
    pub phase: &'static str,
    /// The watched widget's measure count then.
    pub measures: u32,
}

/// Stamps every frame phase of a window's frame clock with a widget's
/// measure count, until dropped.
pub struct PhaseRecorder {
    stamps: Rc<RefCell<Vec<PhaseStamp>>>,
    clock: gtk4::gdk::FrameClock,
    handlers: Vec<gtk4::glib::SignalHandlerId>,
}

impl PhaseRecorder {
    /// Watch `clock`'s phases, stamping `widget`'s measure count.
    #[must_use]
    pub fn new(clock: &gtk4::gdk::FrameClock, widget: &CountingWidget) -> Self {
        let stamps = Rc::new(RefCell::new(Vec::new()));
        let mut handlers = Vec::new();
        let stamp = |phase: &'static str| {
            let stamps = Rc::clone(&stamps);
            let widget = widget.downgrade();
            move |clock: &gtk4::gdk::FrameClock| {
                let measures = widget.upgrade().map_or(0, |widget| widget.measures());
                stamps.borrow_mut().push(PhaseStamp {
                    frame: clock.frame_counter(),
                    phase,
                    measures,
                });
            }
        };
        handlers.push(clock.connect_update(stamp("update")));
        handlers.push(clock.connect_layout(stamp("layout")));
        handlers.push(clock.connect_paint(stamp("paint")));
        handlers.push(clock.connect_after_paint(stamp("after-paint")));
        Self {
            stamps,
            clock: clock.clone(),
            handlers,
        }
    }

    /// Every stamp so far, oldest first.
    #[must_use]
    pub fn stamps(&self) -> Vec<PhaseStamp> {
        self.stamps.borrow().clone()
    }

    /// The stamps as `phase@frame:measures` entries, for an observation.
    #[must_use]
    pub fn describe(&self) -> String {
        super::join_entries(
            self.stamps
                .borrow()
                .iter()
                .map(|stamp| format!("{}@{}:{}", stamp.phase, stamp.frame, stamp.measures)),
        )
    }
}

impl Drop for PhaseRecorder {
    fn drop(&mut self) {
        for handler in self.handlers.drain(..) {
            self.clock.disconnect(handler);
        }
    }
}

/// The first stamp of `phase` whose measure count exceeds `baseline`.
#[must_use]
pub fn first_phase_measuring(
    stamps: &[PhaseStamp],
    phase: &'static str,
    baseline: u32,
) -> Option<PhaseStamp> {
    stamps
        .iter()
        .copied()
        .find(|stamp| stamp.phase == phase && stamp.measures > baseline)
}

/// Whether some stamp of a phase *before* `phase` in the same frame already
/// saw more than `baseline` measures: the measuring happened earlier than
/// `phase`'s handlers.
#[must_use]
pub fn measured_before_phase(
    stamps: &[PhaseStamp],
    frame: i64,
    phase: &'static str,
    baseline: u32,
) -> bool {
    let order = |name: &str| match name {
        "update" => 0,
        "layout" => 1,
        "paint" => 2,
        _ => 3,
    };
    stamps.iter().any(|stamp| {
        stamp.frame == frame && order(stamp.phase) < order(phase) && stamp.measures > baseline
    })
}
