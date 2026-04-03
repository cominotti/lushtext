// SPDX-License-Identifier: GPL-3.0-or-later

//! GObject wrapper for a file tree entry (file or directory).
//!
//! Used as the item model in `GtkTreeListModel`. Must be a GObject
//! so GTK can hold references to it.

use glib::subclass::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FileTreeItem {
        pub path: RefCell<Option<PathBuf>>,
        pub display_name: RefCell<String>,
        pub is_dir: Cell<bool>,
        pub is_placeholder: Cell<bool>,
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
        obj.imp().display_name.replace(display_name_for_path(&path));
        obj.imp().path.replace(Some(path));
        obj.imp().is_dir.set(is_dir);
        obj
    }

    pub fn new_placeholder(display_name: impl Into<String>) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.imp().display_name.replace(display_name.into());
        obj.imp().is_placeholder.set(true);
        obj
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.imp().path.borrow().clone()
    }

    pub fn name(&self) -> String {
        self.imp().display_name.borrow().clone()
    }

    pub fn set_path(&self, new_path: PathBuf) {
        self.imp()
            .display_name
            .replace(display_name_for_path(&new_path));
        self.imp().path.replace(Some(new_path));
        self.imp().is_placeholder.set(false);
    }

    pub fn is_dir(&self) -> bool {
        self.imp().is_dir.get()
    }

    pub fn is_placeholder(&self) -> bool {
        self.imp().is_placeholder.get()
    }

    pub fn is_pending_rename(&self) -> bool {
        self.imp().pending_rename.get()
    }

    pub fn set_pending_rename(&self, pending: bool) {
        self.imp().pending_rename.set(pending);
    }
}

pub(crate) fn display_name_for_path(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
