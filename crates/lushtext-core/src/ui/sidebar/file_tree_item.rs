// SPDX-License-Identifier: GPL-3.0-or-later

//! GObject wrapper for a file tree entry (file or directory).
//!
//! Used as the item model in `GtkTreeListModel`. Must be a GObject
//! so GTK can hold references to it.

use glib::subclass::prelude::*;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

use crate::model::workspace::WorkspaceFolderId;

// Private implementation module. In GTK's GObject system, every type has
// a private struct (imp) holding data and a public wrapper providing the API.
mod imp {
    use super::{Cell, ObjectImpl, ObjectSubclass, PathBuf, RefCell, WorkspaceFolderId, glib};

    // GObject methods take &self (not &mut self) because GTK's list model
    // infrastructure holds multiple references to each item. Interior
    // mutability via Cell/RefCell allows mutation through shared references.
    #[derive(Default)]
    pub struct FileTreeItem {
        /// Absolute path on disk. `None` for placeholder rows.
        pub path: RefCell<Option<PathBuf>>,
        /// Display name shown in the tree row (file name component of the path).
        pub display_name: RefCell<String>,
        /// Whether this entry is a directory (affects icon and expand behavior).
        pub is_dir: Cell<bool>,
        /// True for synthetic rows like "10,000+ items — showing first 10,000".
        pub is_placeholder: Cell<bool>,
        /// Flag set on freshly created items (New File/Folder) to trigger
        /// inline rename in `connect_bind`. Cleared after rename begins.
        pub pending_rename: Cell<bool>,
        /// Whether the directory was confirmed empty during scan (peeking).
        pub is_empty: Cell<Option<bool>>,
        /// Stable workspace-folder id for persisted top-level folder rows.
        pub workspace_folder_id: RefCell<Option<WorkspaceFolderId>>,
    }

    // ObjectSubclass registers this struct with GLib's runtime type system.
    #[glib::object_subclass]
    impl ObjectSubclass for FileTreeItem {
        const NAME: &str = "LushtextFileTreeItem";
        type Type = super::FileTreeItem;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for FileTreeItem {}
}

// glib::wrapper! generates the public wrapper type. Since FileTreeItem is a
// pure data GObject (not a widget), the @extends chain is empty.
glib::wrapper! {
    /// Public GObject row model used by the workspace file tree.
    ///
    /// The wrapper lets GTK list and tree models hold files, directories, and
    /// placeholder rows while the sidebar adapter owns all presentation logic.
    pub struct FileTreeItem(ObjectSubclass<imp::FileTreeItem>);
}

impl FileTreeItem {
    #[must_use]
    pub fn new(path: PathBuf, is_dir: bool, is_empty: Option<bool>) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.imp().display_name.replace(display_name_for_path(&path));
        obj.imp().path.replace(Some(path));
        obj.imp().is_dir.set(is_dir);
        obj.imp().is_empty.set(is_empty);
        obj
    }

    #[must_use]
    pub fn new_workspace_folder(
        path: PathBuf,
        folder_id: WorkspaceFolderId,
        is_empty: Option<bool>,
    ) -> Self {
        let obj = Self::new(path, true, is_empty);
        obj.imp().workspace_folder_id.replace(Some(folder_id));
        obj
    }

    pub fn new_placeholder(display_name: impl Into<String>) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.imp().display_name.replace(display_name.into());
        obj.imp().is_placeholder.set(true);
        obj
    }

    #[must_use]
    pub fn path(&self) -> Option<PathBuf> {
        self.imp().path.borrow().clone()
    }

    #[must_use]
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

    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.imp().is_dir.get()
    }

    #[must_use]
    pub fn is_empty(&self) -> Option<bool> {
        self.imp().is_empty.get()
    }

    /// Update the empty-directory hint after an async top-level or child scan finishes.
    pub fn set_is_empty(&self, is_empty: Option<bool>) {
        self.imp().is_empty.set(is_empty);
    }

    #[must_use]
    pub fn is_placeholder(&self) -> bool {
        self.imp().is_placeholder.get()
    }

    #[must_use]
    pub fn is_pending_rename(&self) -> bool {
        self.imp().pending_rename.get()
    }

    pub fn set_pending_rename(&self, pending: bool) {
        self.imp().pending_rename.set(pending);
    }

    #[must_use]
    pub fn workspace_folder_id(&self) -> Option<WorkspaceFolderId> {
        self.imp().workspace_folder_id.borrow().clone()
    }
}

pub(crate) fn display_name_for_path(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
