// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tab-strip fixture A132 and A133 share: an `AdwTabBar` bound to an
//! `AdwTabView`, hosted in a [`TabStripHost`] that allocates it a chosen
//! width and its natural height at that width, above the tab view.
//!
//! The bar's template (Libadwaita 1.9 and 1.10, `adw-tab-bar.ui`) is a
//! `GtkRevealer` holding a `GtkBox` of [start action bin, pinned
//! `GtkScrolledWindow` (CSS class `pinned`), unpinned `GtkScrolledWindow`,
//! end action bin]. Each scroller's child is an `AdwTabBox`, whose tabs are
//! `AdwTab` widgets, each inside a gizmo container, with a `page` property.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::session::wait_until;

/// The width the host allocates the bar unless a probe changes it.
pub const STRIP_WIDTH: i32 = 600;
/// The size of the window the probes present the fixture in.
pub const WINDOW_SIZE: (i32, i32) = (1_000, 400);
/// The deepest descendant level the scroller lookup walks: bar → revealer →
/// box → scrolled window.
pub const LOOKUP_DEPTH: u32 = 3;
/// Libadwaita's own scroll and focus animation length
/// (`SCROLL_ANIMATION_DURATION`, `FOCUS_ANIMATION_DURATION`, 200 ms).
pub const BOX_ANIMATION: Duration = Duration::from_millis(200);
/// How long a probe waits for a number of frames.
const FRAME_BUDGET: Duration = Duration::from_secs(5);

/// An `AdwTabBar` over an `AdwTabView`, its width under the probe's control.
pub struct TabStripFixture {
    /// The fixture's root: the host above the tab view.
    pub root: gtk4::Box,
    /// The host allocating the bar.
    pub host: TabStripHost,
    /// The bar under observation.
    pub bar: libadwaita::TabBar,
    /// The tab view the bar shows.
    pub tab_view: libadwaita::TabView,
    pages: RefCell<Vec<libadwaita::TabPage>>,
}

impl TabStripFixture {
    /// A bar over `tabs` pages, allocated [`STRIP_WIDTH`] wide.
    #[must_use]
    pub fn new(tabs: usize) -> Self {
        let tab_view = libadwaita::TabView::new();
        tab_view.set_vexpand(true);
        let bar = libadwaita::TabBar::new();
        bar.set_autohide(false);
        bar.set_view(Some(&tab_view));
        let host = TabStripHost::new(&bar, STRIP_WIDTH);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&host);
        root.append(&tab_view);
        let fixture = Self {
            root,
            host,
            bar,
            tab_view,
            pages: RefCell::new(Vec::new()),
        };
        for _ in 0..tabs {
            fixture.append_tab();
        }
        fixture
    }

    /// Append one page titled `tab NN` and return it.
    pub fn append_tab(&self) -> libadwaita::TabPage {
        let index = self.pages.borrow().len();
        let page = self
            .tab_view
            .append(&gtk4::Label::new(Some(&format!("page {index:02}"))));
        page.set_title(&format!("tab {index:02}"));
        self.pages.borrow_mut().push(page.clone());
        page
    }

    /// The pages, in the order they were appended.
    #[must_use]
    pub fn pages(&self) -> Vec<libadwaita::TabPage> {
        self.pages.borrow().clone()
    }

    /// The bar's measured (minimum, natural) height at its current width.
    #[must_use]
    pub fn measured_height(&self) -> (i32, i32) {
        let (minimum, natural, _, _) = self
            .bar
            .measure(gtk4::Orientation::Vertical, self.host.child_width());
        (minimum, natural)
    }

    /// The scroller holding the unpinned tabs.
    #[must_use]
    pub fn unpinned(&self) -> Option<gtk4::ScrolledWindow> {
        find_tab_scroller(self.bar.upcast_ref(), false)
    }

    /// The scroller holding the pinned tabs.
    #[must_use]
    pub fn pinned(&self) -> Option<gtk4::ScrolledWindow> {
        find_tab_scroller(self.bar.upcast_ref(), true)
    }

    /// The `AdwTab` widget showing `page` in `scroller`.
    #[must_use]
    pub fn tab_widget(
        scroller: &gtk4::ScrolledWindow,
        page: &libadwaita::TabPage,
    ) -> Option<gtk4::Widget> {
        tab_widgets(scroller)
            .into_iter()
            .find(|(shown, _)| shown == page)
            .map(|(_, tab)| tab)
    }

    /// `page`'s tab span (x, width) in `scroller`'s coordinates.
    #[must_use]
    pub fn tab_span(
        scroller: &gtk4::ScrolledWindow,
        page: &libadwaita::TabPage,
    ) -> Option<(f64, f64)> {
        let tab = Self::tab_widget(scroller, page)?;
        let bounds = tab.compute_bounds(scroller)?;
        Some((f64::from(bounds.x()), f64::from(bounds.width())))
    }

    /// Whether any part of `page`'s tab is inside `scroller`'s width.
    #[must_use]
    pub fn tab_overlaps(scroller: &gtk4::ScrolledWindow, page: &libadwaita::TabPage) -> bool {
        Self::tab_span(scroller, page)
            .is_some_and(|(x, width)| x < f64::from(scroller.width()) && x + width > 0.0)
    }

    /// Whether `page`'s whole tab is inside `scroller`'s width, to a pixel.
    #[must_use]
    pub fn tab_inside(scroller: &gtk4::ScrolledWindow, page: &libadwaita::TabPage) -> bool {
        Self::tab_span(scroller, page)
            .is_some_and(|(x, width)| x >= -1.0 && x + width <= f64::from(scroller.width()) + 1.0)
    }
}

/// The first `GtkScrolledWindow` under `root`, at most [`LOOKUP_DEPTH`]
/// levels down, that carries the CSS class `pinned` when `pinned`, or does
/// not carry it otherwise.
#[must_use]
pub fn find_tab_scroller(root: &gtk4::Widget, pinned: bool) -> Option<gtk4::ScrolledWindow> {
    let mut level = vec![root.clone()];
    for _ in 0..LOOKUP_DEPTH {
        let mut next = Vec::new();
        for widget in level {
            let mut child = widget.first_child();
            while let Some(current) = child {
                if let Some(scroller) = current.downcast_ref::<gtk4::ScrolledWindow>()
                    && scroller.has_css_class("pinned") == pinned
                {
                    return Some(scroller.clone());
                }
                child = current.next_sibling();
                next.push(current);
            }
        }
        level = next;
    }
    None
}

/// Every `AdwTab` in `scroller`'s tab box with the page it shows, in
/// widget order.
#[must_use]
pub fn tab_widgets(scroller: &gtk4::ScrolledWindow) -> Vec<(libadwaita::TabPage, gtk4::Widget)> {
    let mut tabs = Vec::new();
    let Some(tab_box) = scroller.child() else {
        return tabs;
    };
    let mut container = tab_box.first_child();
    while let Some(current) = container {
        let mut child = current.first_child();
        while let Some(candidate) = child {
            if candidate.type_().name() == "AdwTab"
                && candidate.find_property("page").is_some()
                && let Some(page) = candidate.property::<Option<libadwaita::TabPage>>("page")
            {
                tabs.push((page, candidate.clone()));
            }
            child = candidate.next_sibling();
        }
        container = current.next_sibling();
    }
    tabs
}

/// Whether `adjustment`'s range exceeds its page by at least one pixel.
#[must_use]
pub fn overflows(adjustment: &gtk4::Adjustment) -> bool {
    adjustment.upper() - adjustment.lower() - adjustment.page_size() >= 1.0
}

/// `adjustment`'s value, page size, upper and page increment, for a
/// measurement.
#[must_use]
pub fn describe_adjustment(adjustment: &gtk4::Adjustment) -> String {
    format!(
        "value {} page {} upper {} increment {}",
        adjustment.value(),
        adjustment.page_size(),
        adjustment.upper(),
        adjustment.page_increment()
    )
}

/// Run the default main context for `duration`, dispatching every source as
/// it becomes ready, so frame-clock animations advance frame by frame (a
/// sleep followed by one flush would let an animation jump to its end).
pub fn run_for(duration: Duration) {
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + duration;
    while std::time::Instant::now() < deadline {
        if !context.iteration(false) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Wait until `widget`'s frame clock has ticked `frames` more times.
/// Returns whether it did within the budget.
pub fn wait_frames(widget: &impl IsA<gtk4::Widget>, frames: u32) -> bool {
    let seen = Rc::new(Cell::new(0u32));
    let counter = Rc::clone(&seen);
    let tick = widget.add_tick_callback(move |_, _| {
        counter.set(counter.get() + 1);
        glib::ControlFlow::Continue
    });
    let done = wait_until(FRAME_BUDGET, || seen.get() >= frames);
    tick.remove();
    done
}

mod imp {
    use std::cell::{Cell, RefCell};

    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct TabStripHost {
        pub child: RefCell<Option<gtk4::Widget>>,
        pub child_width: Cell<i32>,
        pub allocating_child: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TabStripHost {
        const NAME: &str = "GtkLushAxiomsTabStripHost";
        type Type = super::TabStripHost;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for TabStripHost {
        fn dispose(&self) {
            if let Some(child) = self.child.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for TabStripHost {
        fn measure(&self, orientation: gtk4::Orientation, _: i32) -> (i32, i32, i32, i32) {
            match (orientation, self.child.borrow().as_ref()) {
                (gtk4::Orientation::Vertical, Some(child)) => {
                    let (_, natural, _, _) = child.measure(orientation, self.child_width.get());
                    (natural, natural, -1, -1)
                }
                _ => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, _width: i32, _height: i32, _baseline: i32) {
            if let Some(child) = self.child.borrow().as_ref() {
                let width = self.child_width.get();
                let _ = child.measure(gtk4::Orientation::Horizontal, -1);
                let (_, natural, _, _) = child.measure(gtk4::Orientation::Vertical, width);
                self.allocating_child.set(true);
                child.allocate(width, natural, -1, None);
                self.allocating_child.set(false);
            }
        }
    }
}

glib::wrapper! {
    /// Allocates its one child a chosen width, whatever width it receives,
    /// and the child's natural height at that width, which it also requests;
    /// it records whether it is inside the child's allocation.
    pub struct TabStripHost(ObjectSubclass<imp::TabStripHost>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl TabStripHost {
    /// Host `child`, allocating it `width` pixels wide.
    #[must_use]
    pub fn new(child: &impl IsA<gtk4::Widget>, width: i32) -> Self {
        let host: Self = glib::Object::new();
        child.set_parent(&host);
        host.imp().child.replace(Some(child.clone().upcast()));
        host.imp().child_width.set(width);
        host.set_hexpand(true);
        host
    }

    /// Allocate the child `width` pixels wide from the next layout on.
    pub fn set_child_width(&self, width: i32) {
        self.imp().child_width.set(width);
        self.queue_resize();
    }

    /// The width the child is allocated.
    #[must_use]
    pub fn child_width(&self) -> i32 {
        self.imp().child_width.get()
    }

    /// Whether the host is inside its child's `allocate` call right now.
    #[must_use]
    pub fn is_allocating_child(&self) -> bool {
        self.imp().allocating_child.get()
    }
}
