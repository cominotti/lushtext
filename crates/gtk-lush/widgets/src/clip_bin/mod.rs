// SPDX-License-Identifier: MIT OR Apache-2.0

//! Single-child clipping widget.

mod imp;

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

glib::wrapper! {
    /// A single-child widget with zero minimum size and clipped snapshots.
    ///
    /// `ClipBin` lets flexible content yield all available space to persistent
    /// chrome such as status bars. It reports a zero minimum size, delegates
    /// natural size to its child, suppresses baseline reporting to keep that
    /// zero-minimum contract internally consistent, allocates the child into
    /// the actual allocation, and clips child snapshots to that allocation.
    ///
    /// The `glib::wrapper!` block connects the public Rust type to the private
    /// `imp::ClipBin` subclass so GTK Builder templates can instantiate the
    /// registered `GtkLushClipBin` type.
    pub struct ClipBin(ObjectSubclass<imp::ClipBin>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl ClipBin {
    /// Create an empty `ClipBin`.
    #[must_use]
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    /// Create a `ClipBin` with an initial child.
    #[must_use]
    pub fn with_child<W>(child: &W) -> Self
    where
        W: IsA<gtk4::Widget>,
    {
        glib::Object::builder().property("child", child).build()
    }

    /// Set or clear the contained child.
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
}

impl Default for ClipBin {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gtk_available() -> bool {
        gtk4::init().is_ok()
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn starts_empty() {
        if !gtk_available() {
            return;
        }
        let bin = ClipBin::new();
        assert!(bin.child().is_none());
    }

    #[test]
    #[ignore = "requires an initialized GTK display"]
    fn sets_replaces_and_clears_child() {
        if !gtk_available() {
            return;
        }
        let bin = ClipBin::new();
        let first = gtk4::Label::new(Some("first"));
        let second = gtk4::Label::new(Some("second"));

        bin.set_child(Some(&first));
        assert!(bin.child().is_some());

        bin.set_child(Some(&first));
        assert!(bin.child().is_some());

        bin.set_child(Some(&second));
        assert!(bin.child().is_some());
        assert!(first.parent().is_none());

        bin.set_child(None::<&gtk4::Widget>);
        assert!(bin.child().is_none());
        assert!(second.parent().is_none());
    }
}
