// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pure-GTK fixtures the probes and the samples share.
//!
//! A sample builds exactly the fixture its probe measures, so what a person
//! watches on screen is what CI checks. Nothing here depends on a LushText or
//! GTK Lush widget: the custom widgets are [`FixedHost`], which allocates its
//! one child at a chosen size with no transform — the only way to hand a
//! `GtkListView` an exact allocation (zero included) without a container whose
//! own policy would be under test too — and [`AllocationProbe`] (A19), a
//! childless widget that logs when it is allocated.
//!
//! The breakpoint and split-view fixtures (A14–A18) are pure Libadwaita:
//! [`BreakpointFixture`] is an `AdwBreakpointBin` that a [`FixedHost`]
//! allocates at an exact width, and [`SplitViewFixture`] an
//! `AdwOverlaySplitView` with a label on each side.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

pub use crate::probes::a03::ViewportedHost;
pub use crate::probes::a04::EmissionSites;
pub use crate::probes::a06::{INSET_CLASS, InsetStyle};
pub use crate::probes::a13::ReslicingHost;
pub use crate::probes::a17::SplitViewFixture;
pub use crate::probes::a19::{AllocationProbe, QueuedFromNotify, ViewportEvent, ViewportOrder};
pub use crate::probes::adaptive::{
    BASE_XFT_DPI, BIN_MIN_WIDTH, BreakpointFixture, ORIGINAL_LABEL, max_width_sp, set_text_scale,
    xft_dpi_for_scale,
};
pub use crate::probes::row_stride;

/// The values the A1 probe drives its fixture with, for its sample.
pub mod a01 {
    pub use crate::probes::a01::{SCROLLED_ROW, TALL_VIEWPORT};
}
/// The values the A4 probe drives its fixture with, for its sample.
pub mod a04 {
    pub use crate::probes::a04::{FAR_ROW, FOCUS_ROW};
}
/// The values the A5 probe drives its fixture with, for its sample.
pub mod a05 {
    pub use crate::probes::a05::PUBLISHED_VALUE;
}
/// The values the A7 probe drives its fixture with, for its sample.
pub mod a07 {
    pub use crate::probes::a07::{BOTTOM_ANCHORED_ROW, HOST_VALUE_ROW, PAGE_SWEEP};
}
/// The values the A9 probe drives its fixture with, for its sample.
pub mod a09 {
    pub use crate::probes::a09::{NUDGE, TARGET_ROW};
}
/// The values the A10 probe drives its fixture with, for its sample.
pub mod a10 {
    pub use crate::probes::a10::{
        ALIGNED_ROW, FOCUS_ROW, SCROLL_AWAY, SCROLL_TO_ROW, UNALIGNED_VALUE,
    };
}
/// The values the A13 probe drives its fixture with, for its sample.
pub mod a13 {
    pub use crate::probes::a13::{IDLE_MOVE, IN_LAYOUT_MOVE};
}
/// The values the A14 probe drives its fixture with, for its sample.
pub mod a14 {
    pub use crate::probes::a14::{APPLIED_LABEL, CONDITION_SP, RESCALE_WIDTH, TEXT_SCALES};
}
/// The values the A15 probe drives its fixture with, for its sample.
pub mod a15 {
    pub use crate::probes::a15::{APPLIED_LABEL, NARROW_SP, REST_WIDTH, WIDE_SP};
}
/// The values the A16 probe drives its fixture with, for its sample.
pub mod a16 {
    pub use crate::probes::a16::{
        APPLIED_LABEL, CONDITION_SP, NARROW_WIDTH, WIDE_WIDTH, WRITE_BEFORE_ADDING_THE_SETTER,
        WRITE_WHILE_APPLIED,
    };
}
/// The values the A17 probe drives its fixture with, for its sample.
pub mod a17 {
    pub use crate::probes::a17::{SCALED_TEXT, SIDEBAR_REQUEST, WINDOW_WIDTH};
}
/// The values the A18 probe drives its fixture with, for its sample.
pub mod a18 {
    pub use crate::probes::a18::{BELOW_BOTH_WIDTH, BETWEEN_WIDTH, NARROW_SP, WIDE_SP};
}

/// The values the A19 probe drives its fixture with, for its sample.
pub mod a19 {
    pub use crate::probes::a19::{
        CONTENT_HEIGHT, GROWN_HEIGHT, RESTING_HEIGHT, SHRUNK_HEIGHT, VIEWPORT_WIDTHS,
    };
}

/// The values the A20 probe drives its fixture with, for its sample.
pub mod a20 {
    pub use crate::probes::a20::{SHRUNK_PAGE, TARGET_ROWS, reannounce_value};
}

/// Height every probe row requests, so row geometry is predictable.
///
/// GTK adds the row's own CSS padding on top, so a row is drawn taller than
/// this; probes read the real stride from the list's adjustment rather than
/// assuming it.
pub const ROW_HEIGHT: i32 = 30;
/// Rows in the probe list: far past the realized-widget cap (A1).
pub const ROWS: u32 = 400;
/// Height the host gives the list in the resting state.
pub const LIST_HEIGHT: i32 = 300;

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    /// Hook run inside `size_allocate`, before the child is allocated.
    type AllocationHook = Box<dyn Fn()>;

    #[derive(Default)]
    pub struct FixedHost {
        pub child: RefCell<Option<gtk4::Widget>>,
        pub child_height: Cell<i32>,
        pub child_width: Cell<Option<i32>>,
        pub allocations: Cell<u32>,
        pub allocating_child: Cell<bool>,
        pub reported: Cell<Option<(i32, i32)>>,
        pub during_allocation: RefCell<Option<AllocationHook>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FixedHost {
        const NAME: &str = "GtkLushAxiomsFixedHost";
        type Type = super::FixedHost;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for FixedHost {
        fn dispose(&self) {
            if let Some(child) = self.child.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for FixedHost {
        fn measure(&self, orientation: gtk4::Orientation, _: i32) -> (i32, i32, i32, i32) {
            match (orientation, self.reported.get()) {
                (gtk4::Orientation::Vertical, Some((minimum, natural))) => {
                    (minimum, natural, -1, -1)
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            self.allocations.set(self.allocations.get() + 1);
            if let Some(hook) = self.during_allocation.borrow().as_ref() {
                hook();
            }
            if let Some(child) = self.child.borrow().as_ref() {
                self.allocating_child.set(true);
                let child_width = self.child_width.get().unwrap_or(width);
                child.allocate(child_width, self.child_height.get(), -1, None);
                self.allocating_child.set(false);
            }
        }
    }
}

gtk4::glib::wrapper! {
    /// Allocates its one child at a fixed height (and, when a probe sets one,
    /// a fixed width), and counts allocations.
    ///
    /// It measures `0 × 0` unless a probe gives it a vertical minimum and
    /// natural size, so by default it never passes its child's size requests
    /// on to its parent.
    pub struct FixedHost(ObjectSubclass<imp::FixedHost>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl FixedHost {
    /// Host `child`, allocating it `child_height` pixels tall.
    #[must_use]
    pub fn new(child: &impl IsA<gtk4::Widget>, child_height: i32) -> Self {
        let host: Self = gtk4::glib::Object::new();
        child.set_parent(&host);
        host.imp().child.replace(Some(child.clone().upcast()));
        host.imp().child_height.set(child_height);
        host.set_hexpand(true);
        host.set_vexpand(true);
        host
    }

    /// Allocate the child `height` pixels tall from the next allocation on.
    pub fn set_child_height(&self, height: i32) {
        self.imp().child_height.set(height);
        self.queue_allocate();
    }

    /// Allocate the child `width` pixels wide from the next allocation on,
    /// whatever width the host itself receives.
    pub fn set_child_width(&self, width: i32) {
        self.imp().child_width.set(Some(width));
        self.queue_allocate();
    }

    /// The width the child is allocated, when a probe fixed it.
    #[must_use]
    pub fn child_width(&self) -> Option<i32> {
        self.imp().child_width.get()
    }

    /// The height the child is allocated.
    #[must_use]
    pub fn child_height(&self) -> i32 {
        self.imp().child_height.get()
    }

    /// Report `minimum` and `natural` as this host's vertical size request.
    pub(crate) fn report_vertical_size(&self, minimum: i32, natural: i32) {
        self.imp().reported.set(Some((minimum, natural)));
        self.queue_resize();
    }

    /// How many times GTK has allocated this host.
    #[must_use]
    pub fn allocations(&self) -> u32 {
        self.imp().allocations.get()
    }

    /// Whether the host is inside its child's `allocate` call right now.
    pub(crate) fn is_allocating_child(&self) -> bool {
        self.imp().allocating_child.get()
    }

    /// Run `hook` inside every later `size_allocate`, before the child.
    pub(crate) fn set_during_allocation(&self, hook: impl Fn() + 'static) {
        self.imp().during_allocation.replace(Some(Box::new(hook)));
    }
}

/// A `GtkListView` of `rows` labels `row 0000`, `row 0001`, …, each
/// requesting [`ROW_HEIGHT`], with no selection.
fn numbered_list(rows: u32) -> gtk4::ListView {
    let strings: Vec<String> = (0..rows).map(|index| format!("row {index:04}")).collect();
    let model = gtk4::StringList::new(&strings.iter().map(String::as_str).collect::<Vec<_>>());
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::new(None);
        label.set_height_request(ROW_HEIGHT);
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|object| object.string().to_string())
            .unwrap_or_default();
        if let Some(label) = item.child().and_downcast::<gtk4::Label>() {
            label.set_text(&text);
        }
    });
    gtk4::ListView::new(Some(gtk4::NoSelection::new(Some(model))), Some(factory))
}

/// A probe list inside a [`FixedHost`], holding an adjustment the fixture
/// owns: the role a `GtkScrolledWindow` or a slice bin plays.
#[derive(Clone, Debug)]
pub struct HostedList {
    /// The host that allocates the list.
    pub host: FixedHost,
    /// The list under observation.
    pub list: gtk4::ListView,
    /// The vertical adjustment the host owns and the list works in.
    pub adjustment: gtk4::Adjustment,
}

impl HostedList {
    /// Build the fixture: [`ROWS`] rows allocated [`LIST_HEIGHT`] tall.
    #[must_use]
    pub fn new() -> Self {
        let list = numbered_list(ROWS);
        let adjustment = gtk4::Adjustment::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.0);
        list.set_vadjustment(Some(&adjustment));
        let host = FixedHost::new(&list, LIST_HEIGHT);
        Self {
            host,
            list,
            adjustment,
        }
    }
}

impl Default for HostedList {
    fn default() -> Self {
        Self::new()
    }
}

/// Count emissions of `value-changed` on `adjustment` from now on.
#[must_use]
pub fn count_value_changes(adjustment: &gtk4::Adjustment) -> Rc<Cell<u32>> {
    let count = Rc::new(Cell::new(0));
    let counter = count.clone();
    adjustment.connect_value_changed(move |_| counter.set(counter.get() + 1));
    count
}

/// The row widgets `list` has realized, in drawing order.
#[must_use]
pub fn realized_rows(list: &gtk4::ListView) -> Vec<gtk4::Widget> {
    let mut rows = Vec::new();
    let mut child = list.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.css_name() == "row" {
            rows.push(widget);
        }
    }
    rows
}

/// The index a numbered row widget shows (`row 0042` → `42`).
#[must_use]
pub fn row_index(row: &gtk4::Widget) -> Option<u32> {
    let label = row.first_child().and_downcast::<gtk4::Label>()?;
    label.text().strip_prefix("row ")?.parse().ok()
}

/// The lowest and highest row index among `rows`.
#[must_use]
pub fn row_index_range(rows: &[gtk4::Widget]) -> Option<(u32, u32)> {
    let indices = rows.iter().filter_map(row_index);
    indices.fold(None, |range, index| match range {
        None => Some((index, index)),
        Some((low, high)) => Some((low.min(index), high.max(index))),
    })
}
