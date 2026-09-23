// SPDX-License-Identifier: GPL-3.0-or-later

//! Isolated probes for the GTK axiom ledger.
//!
//! Each test pins one axiom of
//! `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`
//! against real GTK, with the smallest pure-GTK fixture that shows it and no
//! LushText or GTK Lush widget involved. The designs and verification envelopes
//! listed in the ledger rely on these behaviours, so a probe that fails after a
//! toolkit update is an **axiom change**: revisit the ledger entry and every
//! dependent design before resolving it, rather than adjusting the probe.
//!
//! The one fixture widget, [`fixed_host::FixedHost`], allocates its single
//! child at a chosen height with no transform. That is the only way to hand a
//! `GtkListView` an exact allocation (zero included) without a container whose
//! own policy would be under test too.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use crate::common::{
    ensure_gtk_init, flush_after_delay, numbered_label_list, present_window, test_application,
};
use gtk4::prelude::*;

/// Height every probe row requests, so row geometry is predictable.
const ROW_HEIGHT: i32 = 30;
/// Rows in the probe list: far past the realized-widget cap (A1).
const ROWS: u32 = 400;
/// Height the host gives the list in the resting state.
const LIST_HEIGHT: i32 = 300;

mod fixed_host {
    use std::cell::{Cell, RefCell};

    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    mod imp {
        use super::{Cell, RefCell, glib};
        use gtk4::prelude::*;
        use gtk4::subclass::prelude::*;

        /// Hook run inside `size_allocate`, before the child is allocated.
        type AllocationHook = Box<dyn Fn()>;

        #[derive(Default)]
        pub struct FixedHost {
            pub child: RefCell<Option<gtk4::Widget>>,
            pub child_height: Cell<i32>,
            pub allocations: Cell<u32>,
            pub during_allocation: RefCell<Option<AllocationHook>>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for FixedHost {
            const NAME: &str = "LushtextTestAxiomFixedHost";
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
            fn measure(&self, _: gtk4::Orientation, _: i32) -> (i32, i32, i32, i32) {
                (0, 0, -1, -1)
            }

            fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
                self.allocations.set(self.allocations.get() + 1);
                if let Some(hook) = self.during_allocation.borrow().as_ref() {
                    hook();
                }
                if let Some(child) = self.child.borrow().as_ref() {
                    child.allocate(width, self.child_height.get(), -1, None);
                }
            }
        }
    }

    glib::wrapper! {
        /// Allocates its one child at a fixed height, and counts allocations.
        pub struct FixedHost(ObjectSubclass<imp::FixedHost>)
            @extends gtk4::Widget,
            @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
    }

    impl FixedHost {
        pub fn new(child: &impl IsA<gtk4::Widget>, child_height: i32) -> Self {
            let host: Self = glib::Object::new();
            child.set_parent(&host);
            host.imp().child.replace(Some(child.clone().upcast()));
            host.imp().child_height.set(child_height);
            host.set_hexpand(true);
            host.set_vexpand(true);
            host
        }

        pub fn set_child_height(&self, height: i32) {
            self.imp().child_height.set(height);
            self.queue_allocate();
        }

        pub fn allocations(&self) -> u32 {
            self.imp().allocations.get()
        }

        /// Run `hook` inside every later `size_allocate`, before the child.
        pub fn set_during_allocation(&self, hook: impl Fn() + 'static) {
            self.imp().during_allocation.replace(Some(Box::new(hook)));
        }
    }
}

fn probe_list() -> gtk4::ListView {
    numbered_label_list(
        ROWS,
        |model| gtk4::NoSelection::new(Some(model)).upcast(),
        |label| label.set_height_request(ROW_HEIGHT),
    )
}

/// A probe list inside a [`fixed_host::FixedHost`], holding an adjustment the
/// test owns (the role a `GtkScrolledWindow` or a slice bin plays).
struct HostedList {
    window: libadwaita::ApplicationWindow,
    host: fixed_host::FixedHost,
    list: gtk4::ListView,
    adjustment: gtk4::Adjustment,
}

impl HostedList {
    fn present() -> Self {
        ensure_gtk_init();
        let list = probe_list();
        let adjustment = gtk4::Adjustment::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.0);
        list.set_vadjustment(Some(&adjustment));
        let host = fixed_host::FixedHost::new(&list, LIST_HEIGHT);
        let window = present_content(&host);
        let hosted = Self {
            window,
            host,
            list,
            adjustment,
        };
        assert!(
            hosted.adjustment.upper() > f64::from(LIST_HEIGHT),
            "the probe list must report more content than it is allocated"
        );
        hosted
    }

    /// Emissions of `value-changed` on the list's adjustment from now on.
    fn count_value_changes(&self) -> Rc<Cell<u32>> {
        let count = Rc::new(Cell::new(0));
        let counter = count.clone();
        self.adjustment
            .connect_value_changed(move |_| counter.set(counter.get() + 1));
        count
    }
}

fn present_content(content: &impl IsA<gtk4::Widget>) -> libadwaita::ApplicationWindow {
    ensure_gtk_init();
    let app = test_application();
    let window = libadwaita::ApplicationWindow::builder()
        .application(&app)
        .default_width(400)
        .default_height(600)
        .content(content)
        .build();
    present_window(&window);
    flush_after_delay(Duration::from_millis(200));
    window
}

/// A5: a zero-height allocation makes a `GtkListView` rewrite the host's
/// adjustment — its value as well as its page — although nothing asked it to
/// scroll. This is why `viewport_slice` never shrinks the band at the bottom
/// edge: a host that read the rewrite as a request would scroll on its own.
#[test]
fn test_axiom_a5_zero_height_allocation_rewrites_the_hosts_adjustment() {
    let hosted = HostedList::present();
    let published = 1_200.0;
    hosted.adjustment.set_value(published);
    flush_after_delay(Duration::from_millis(200));
    // Control: at a positive height nothing rewrites the published value.
    hosted.host.queue_allocate();
    flush_after_delay(Duration::from_millis(200));
    assert!(
        (hosted.adjustment.value() - published).abs() < f64::EPSILON,
        "control: a resting list at {LIST_HEIGHT}px must keep the value {published}; saw {}",
        hosted.adjustment.value()
    );

    let emissions = hosted.count_value_changes();
    hosted.host.set_child_height(0);
    flush_after_delay(Duration::from_millis(200));
    assert!(
        hosted.adjustment.page_size().abs() < f64::EPSILON,
        "A5: the zero-height list must publish a zero page; saw {}",
        hosted.adjustment.page_size()
    );
    assert!(
        (hosted.adjustment.value() - published).abs() >= 1.0 && emissions.get() > 0,
        "A5: the zero-height list must rewrite the host's value {published} and emit \
         value-changed; saw value {} after {} emission(s)",
        hosted.adjustment.value(),
        emissions.get()
    );
    drop(hosted.window);
}

/// A9: a `GtkListBase` drops a pending `scroll_to` on any `value-changed` of
/// its adjustment. This is why a honoured slice-bin request must land where the
/// re-slice republishes exactly the child's value: any emission in between
/// erases requests the child made meanwhile.
#[test]
fn test_axiom_a9_value_changed_drops_a_pending_scroll_to() {
    let target = 200;
    let target_top = f64::from(target) * f64::from(ROW_HEIGHT);
    let reach = target_top + f64::from(ROW_HEIGHT) - f64::from(LIST_HEIGHT);

    // Control: with no intervening emission the request is applied.
    let honoured = HostedList::present();
    honoured
        .list
        .scroll_to(target, gtk4::ListScrollFlags::NONE, None);
    flush_after_delay(Duration::from_millis(300));
    assert!(
        honoured.adjustment.value() >= reach - 1.0,
        "control: scroll_to({target}) must bring the row into view (value >= {reach}); saw {}",
        honoured.adjustment.value()
    );
    drop(honoured.window);

    let dropped = HostedList::present();
    dropped
        .list
        .scroll_to(target, gtk4::ListScrollFlags::NONE, None);
    let nudged = dropped.adjustment.value() + 10.0;
    dropped.adjustment.set_value(nudged);
    flush_after_delay(Duration::from_millis(300));
    assert!(
        (dropped.adjustment.value() - nudged).abs() < 1.0,
        "A9: a value-changed before the next allocation must drop the pending \
         scroll_to({target}); the list stayed at {nudged} expected, saw {}",
        dropped.adjustment.value()
    );
    drop(dropped.window);
}

/// A11: `configure` and `set_value` clamp the value into
/// `[lower, upper - page_size]`, and a `set_value` that does not change the
/// value emits nothing. The slice bin reads its published offset back after
/// `configure` because of the first half, and an unhonourable outer request
/// ends quietly because of the second.
#[test]
fn test_axiom_a11_adjustments_clamp_and_skip_unchanged_values() {
    ensure_gtk_init();
    let adjustment = gtk4::Adjustment::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.0);
    let emissions = Rc::new(Cell::new(0u32));
    let counter = emissions.clone();
    adjustment.connect_value_changed(move |_| counter.set(counter.get() + 1));

    adjustment.configure(500.0, 0.0, 300.0, 1.0, 100.0, 100.0);
    assert!(
        (adjustment.value() - 200.0).abs() < f64::EPSILON,
        "A11: configure must clamp 500 to upper - page = 200; saw {}",
        adjustment.value()
    );
    adjustment.set_value(-5.0);
    assert!(
        adjustment.value().abs() < f64::EPSILON,
        "A11: set_value must clamp -5 to lower = 0; saw {}",
        adjustment.value()
    );
    adjustment.set_value(1_000.0);
    assert!(
        (adjustment.value() - 200.0).abs() < f64::EPSILON,
        "A11: set_value must clamp 1000 to upper - page = 200; saw {}",
        adjustment.value()
    );
    let before = emissions.get();
    adjustment.set_value(200.0);
    adjustment.set_value(5_000.0);
    assert_eq!(
        emissions.get(),
        before,
        "A11: a set_value that leaves the (clamped) value unchanged must emit nothing"
    );
    adjustment.set_value(150.0);
    assert_eq!(
        emissions.get(),
        before + 1,
        "control: a set_value that changes the value emits exactly once"
    );
}

/// A13: moving a scroller's adjustment from inside layout does not reliably
/// schedule a relayout of a widget that re-slices on that adjustment's
/// `value-changed`: its own `queue_allocate` from the emission is lost because
/// it is already being allocated. This is why the slice bin applies a child's
/// request to the outer scroller from an idle rather than inside allocation.
#[test]
fn test_axiom_a13_an_in_layout_scroll_does_not_schedule_a_relayout() {
    ensure_gtk_init();
    let label = gtk4::Label::new(Some("tall content"));
    let host = fixed_host::FixedHost::new(&label, 4_000);
    host.set_size_request(-1, 4_000);
    let scroller = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .child(&host)
        .build();
    let window = present_content(&scroller);
    let outer = scroller.vadjustment();
    // Re-slice on every outer move, as the slice bin does.
    let weak_host = host.downgrade();
    outer.connect_value_changed(move |_| {
        if let Some(host) = weak_host.upgrade() {
            host.queue_allocate();
        }
    });

    // Control: the same move from outside layout re-allocates the host.
    let before_control = host.allocations();
    outer.set_value(250.0);
    flush_after_delay(Duration::from_millis(300));
    assert!(
        host.allocations() > before_control,
        "control: an outer move outside layout must re-allocate the host"
    );

    let moved = Rc::new(Cell::new(false));
    let move_once = moved.clone();
    let scroll = outer.clone();
    host.set_during_allocation(move || {
        if !move_once.replace(true) {
            scroll.set_value(500.0);
        }
    });
    host.queue_allocate();
    flush_after_delay(Duration::from_millis(300));
    assert!(moved.get(), "the in-layout move must have run");
    let after_move = host.allocations();
    flush_after_delay(Duration::from_millis(300));
    assert!(
        (outer.value() - 500.0).abs() < f64::EPSILON,
        "the in-layout move itself takes effect; saw {}",
        outer.value()
    );
    assert_eq!(
        host.allocations(),
        after_move,
        "A13: the host queued a re-allocation from inside its own allocation and must \
         not have received one"
    );
    drop(window);
}
