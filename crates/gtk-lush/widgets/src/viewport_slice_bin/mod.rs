// SPDX-License-Identifier: MIT OR Apache-2.0

//! Single-child container that virtualizes a scrollable child inside an outer
//! scroller.

mod imp;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

glib::wrapper! {
    /// A single-child widget that hosts a `GtkScrollable` inside an outer
    /// `GtkScrolledWindow` without giving up virtualization.
    ///
    /// The bin reports its child's full natural height so the outer scroller's
    /// range stays exact, but allocates the child only the band of content that
    /// intersects the outer viewport (see [`crate::viewport_slice`]) and owns
    /// the child's vertical adjustment so the child realizes only that band.
    /// This matters for `GtkListView`, which caps the row widgets it creates for
    /// one visible range at `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200 plus two extra):
    /// handed its whole content as viewport, a list view renders blank space
    /// after roughly the two-hundredth row.
    ///
    /// Scrolling the outer scroller re-slices the child. When the child moves
    /// its own adjustment (keyboard focus, `scroll_to`), the bin translates that
    /// into an outer scroll so the requested row enters the real viewport.
    ///
    /// The outer scroller is discovered as the nearest `GtkScrolledWindow`
    /// ancestor when the bin is rooted, or set explicitly through the
    /// `outer-scrolled-window` property. Without one the child is allocated its
    /// whole height, which is the plain non-virtualized behaviour.
    ///
    /// The `glib::wrapper!` block connects the public Rust type to the private
    /// `imp::ViewportSliceBin` subclass so GTK Builder templates can
    /// instantiate the registered `GtkLushViewportSliceBin` type.
    pub struct ViewportSliceBin(ObjectSubclass<imp::ViewportSliceBin>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl ViewportSliceBin {
    /// Create an empty bin.
    #[must_use]
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Create a bin with an initial scrollable child.
    #[must_use]
    pub fn with_child<W>(child: &W) -> Self
    where
        W: IsA<gtk4::Widget>,
    {
        glib::Object::builder().property("child", child).build()
    }

    /// Set or clear the contained child.
    ///
    /// A child implementing `GtkScrollable` receives the bin's own vertical and
    /// horizontal adjustments.
    pub fn set_child<W>(&self, child: Option<&W>)
    where
        W: IsA<gtk4::Widget>,
    {
        self.imp().set_child(child.map(std::convert::AsRef::as_ref));
    }

    /// Return the current child.
    #[must_use]
    pub fn child(&self) -> Option<gtk4::Widget> {
        self.imp().child.borrow().clone()
    }

    /// Return the extra content allocated above and below the viewport.
    #[must_use]
    pub fn overscan(&self) -> f64 {
        self.imp().overscan.get()
    }

    /// Set the extra content allocated above and below the viewport.
    ///
    /// Zero (the default) keeps the child's viewport identical to the visible
    /// one so `GtkListView::scroll_to` and keyboard focus decide visibility
    /// correctly. Positive values reduce row churn while scrolling at the cost
    /// of that precision.
    pub fn set_overscan(&self, overscan: f64) {
        self.set_property("overscan", overscan);
    }

    /// Return the outer scroller the bin currently slices against.
    #[must_use]
    pub fn outer_scrolled_window(&self) -> Option<gtk4::ScrolledWindow> {
        self.imp().outer_scrolled_window()
    }

    /// Override ancestor discovery with an explicit outer scroller.
    pub fn set_outer_scrolled_window(&self, scroller: Option<&gtk4::ScrolledWindow>) {
        self.set_property("outer-scrolled-window", scroller);
    }
}

impl Default for ViewportSliceBin {
    fn default() -> Self {
        Self::new()
    }
}
