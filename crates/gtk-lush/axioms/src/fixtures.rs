// SPDX-License-Identifier: MIT OR Apache-2.0

//! The pure-GTK fixtures the probes and the samples share.
//!
//! A sample builds exactly the fixture its probe measures, so what a person
//! watches on screen is what CI checks. Nothing here depends on a LushText or
//! GTK Lush widget: the only custom widget, [`FixedHost`], allocates its one
//! child at a chosen height with no transform. That is the only way to hand a
//! `GtkListView` an exact allocation (zero included) without a container whose
//! own policy would be under test too.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

pub use crate::probes::a03::ViewportedHost;
pub use crate::probes::a04::EmissionSites;
pub use crate::probes::a06::{INSET_CLASS, InsetStyle};
pub use crate::probes::a13::ReslicingHost;

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
/// Default size of the window a probe presents its fixture in.
pub const FIXTURE_WINDOW_SIZE: (i32, i32) = (400, 600);

/// How long a probe waits for its window to realize before calling the
/// fixture invalid. Generous: realization is scheduling-dependent.
pub(crate) const REALIZE_BUDGET: Duration = Duration::from_secs(5);

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
                child.allocate(width, self.child_height.get(), -1, None);
                self.allocating_child.set(false);
            }
        }
    }
}

gtk4::glib::wrapper! {
    /// Allocates its one child at a fixed height, and counts allocations.
    ///
    /// It measures `0 × 0` unless [`FixedHost::report_vertical_size`] gives it
    /// a vertical minimum and natural size, so by default it never passes its
    /// child's size requests on to its parent.
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

    /// The height the child is allocated.
    #[must_use]
    pub fn child_height(&self) -> i32 {
        self.imp().child_height.get()
    }

    /// Report `minimum` and `natural` as this host's vertical size request.
    pub fn report_vertical_size(&self, minimum: i32, natural: i32) {
        self.imp().reported.set(Some((minimum, natural)));
        self.queue_resize();
    }

    /// How many times GTK has allocated this host.
    #[must_use]
    pub fn allocations(&self) -> u32 {
        self.imp().allocations.get()
    }

    /// Whether the host is inside its child's `allocate` call right now.
    #[must_use]
    pub fn is_allocating_child(&self) -> bool {
        self.imp().allocating_child.get()
    }

    /// Run `hook` inside every later `size_allocate`, before the child.
    pub fn set_during_allocation(&self, hook: impl Fn() + 'static) {
        self.imp().during_allocation.replace(Some(Box::new(hook)));
    }
}

/// A `GtkListView` of `rows` labels `row 0000`, `row 0001`, …, each
/// requesting [`ROW_HEIGHT`], with no selection.
#[must_use]
pub fn numbered_list(rows: u32) -> gtk4::ListView {
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

    /// Count emissions of `value-changed` on the adjustment from now on.
    #[must_use]
    pub fn count_value_changes(&self) -> Rc<Cell<u32>> {
        count_value_changes(&self.adjustment)
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
