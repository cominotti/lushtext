// SPDX-License-Identifier: GPL-3.0-or-later

//! GObject wrapper for a file tree entry (file or directory).
//!
//! Used as the item model in `GtkTreeListModel`. Must be a GObject
//! so GTK can hold references to it.

use glib::subclass::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FileTreeItem {
        pub path: RefCell<PathBuf>,
        pub is_dir: RefCell<bool>,
        pub pending_rename: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileTreeItem {
        const NAME: &'static str = "LushtextFileTreeItem";
        type Type = super::FileTreeItem;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for FileTreeItem {}
}

glib::wrapper! {
    pub struct FileTreeItem(ObjectSubclass<imp::FileTreeItem>);
}

impl FileTreeItem {
    pub fn new(path: PathBuf, is_dir: bool) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.imp().path.replace(path);
        obj.imp().is_dir.replace(is_dir);
        obj
    }

    pub fn path(&self) -> PathBuf {
        self.imp().path.borrow().clone()
    }

    pub fn name(&self) -> String {
        self.imp()
            .path
            .borrow()
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| self.imp().path.borrow().display().to_string())
    }

    pub fn set_path(&self, new_path: PathBuf) {
        self.imp().path.replace(new_path);
    }

    pub fn is_dir(&self) -> bool {
        *self.imp().is_dir.borrow()
    }

    pub fn is_pending_rename(&self) -> bool {
        self.imp().pending_rename.get()
    }

    pub fn set_pending_rename(&self, pending: bool) {
        self.imp().pending_rename.set(pending);
    }
}
