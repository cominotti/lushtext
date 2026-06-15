// SPDX-License-Identifier: GPL-3.0-or-later

//! GObject row item for the Open popover recent-document list.
//!
//! The GTK list model needs `glib::Object` values, while the service layer
//! returns plain Rust rows. This adapter stores only display fields and the
//! path to activate; search and persistence stay outside the widget object.

use crate::model::recent_document::RecentDocumentRow;
use glib::subclass::prelude::*;
use gtk4::glib;
use std::cell::RefCell;
use std::path::PathBuf;

// The private GObject implementation holds row data; the public wrapper below
// is what `gio::ListStore` stores.
mod imp {
    use super::{ObjectImpl, ObjectSubclass, PathBuf, RefCell, glib};

    /// Private storage for one recent-document row.
    #[derive(Default)]
    pub struct OpenPopoverItem {
        /// Primary row title.
        pub title: RefCell<String>,
        /// Secondary path/location text.
        pub subtitle: RefCell<String>,
        /// Short age/context label.
        pub age_label: RefCell<Option<String>>,
        /// File path opened when the row activates.
        pub path: RefCell<PathBuf>,
    }

    // ObjectSubclass registers this data object with GLib so ListStore can hold it.
    #[glib::object_subclass]
    impl ObjectSubclass for OpenPopoverItem {
        const NAME: &'static str = "LushtextOpenPopoverItem";
        type Type = super::OpenPopoverItem;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for OpenPopoverItem {}
}

// glib::wrapper! generates the public GObject wrapper for ListStore rows.
glib::wrapper! {
    /// Public GObject row model used by the Open popover ListView.
    pub struct OpenPopoverItem(ObjectSubclass<imp::OpenPopoverItem>);
}

impl OpenPopoverItem {
    /// Convert a service row into a GTK row item.
    #[must_use]
    pub fn from_row(row: RecentDocumentRow) -> Self {
        let obj: Self = glib::Object::builder().build();
        let imp = obj.imp();
        imp.title.replace(row.title);
        imp.subtitle.replace(row.subtitle);
        imp.age_label.replace(row.age_label);
        imp.path.replace(row.path);
        obj
    }

    /// Primary row title.
    #[must_use]
    pub fn title(&self) -> String {
        self.imp().title.borrow().clone()
    }

    /// Secondary row text.
    #[must_use]
    pub fn subtitle(&self) -> String {
        self.imp().subtitle.borrow().clone()
    }

    /// Optional age/context label.
    #[must_use]
    pub fn age_label(&self) -> Option<String> {
        self.imp().age_label.borrow().clone()
    }

    /// File path opened when activated.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.imp().path.borrow().clone()
    }
}
