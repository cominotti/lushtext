// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-workspace section widget: header, tree, and context-menu callbacks.
//!
//! Root-tree loading and drill-down flows live in `roots.rs`, file operations
//! live in `actions.rs`, and index/cache helpers live in their dedicated files.

mod actions;
mod imp;
mod peek;
mod refresh;
mod roots;
mod tree_index;
mod tree_loading;
mod watch;

use std::path::Path;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::file_tree_item::FileTreeItem;
use crate::model::workspace::WorkspaceId;
use crate::services::notifications::NotificationSeverity;

glib::wrapper! {
    pub struct LushtextWorkspaceSection(ObjectSubclass<imp::LushtextWorkspaceSection>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextWorkspaceSection {
    #[must_use]
    pub fn new(workspace_id: WorkspaceId) -> Self {
        let obj: Self = Object::builder().build();
        *obj.imp().workspace_id.borrow_mut() = workspace_id;
        obj
    }

    pub fn set_workspace_name(&self, name: &str) {
        self.imp().header_label.set_label(name);
    }

    #[must_use]
    pub fn workspace_name(&self) -> String {
        self.imp().header_label.label().to_string()
    }

    #[must_use]
    pub fn workspace_id(&self) -> WorkspaceId {
        self.imp().workspace_id.borrow().clone()
    }

    pub fn connect_file_activated<F: Fn(&Path) + 'static>(&self, f: F) {
        self.imp()
            .file_tree_view
            .connect_activate(move |list_view, position| {
                activate_file_at(list_view, position, &f);
            });
    }

    pub fn connect_file_renamed<F: Fn(&Path, &Path) + 'static>(&self, f: F) {
        *self.imp().rename_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_file_deleted<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().delete_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_file_created<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().create_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback invoked when local history should open for one file row.
    pub fn connect_local_history_requested<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().local_history_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback used for lightweight window-owned status messages.
    pub fn connect_message<F: Fn(&str, NotificationSeverity) + 'static>(&self, f: F) {
        *self.imp().message_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Store the callback used when peek promotion should open a real tab.
    pub fn connect_peek_promoted<F: Fn(&Path) + 'static>(&self, f: F) {
        *self.imp().peek_promote_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_add_folder_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().add_folder_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_rename_workspace_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().rename_workspace_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_unlist_workspace_requested<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().unlist_workspace_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn connect_folder_focused<F: Fn(&WorkspaceId) + 'static>(&self, f: F) {
        *self.imp().folder_focused_callback.borrow_mut() = Some(Box::new(f));
    }

    pub fn notify_add_folder_requested(&self) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().add_folder_callback.borrow() {
            callback(&workspace_id);
        }
    }

    pub fn notify_rename_workspace_requested(&self) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().rename_workspace_callback.borrow() {
            callback(&workspace_id);
        }
    }

    pub fn notify_unlist_workspace_requested(&self) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().unlist_workspace_callback.borrow() {
            callback(&workspace_id);
        }
    }

    pub fn notify_folder_focused(&self) {
        let workspace_id = self.workspace_id();
        if let Some(ref callback) = *self.imp().folder_focused_callback.borrow() {
            callback(&workspace_id);
        }
    }

    pub(super) fn notify_peek_promoted(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().peek_promote_callback.borrow() {
            callback(path);
        }
    }

    pub(super) fn notify_local_history_requested(&self, path: &Path) {
        if let Some(ref callback) = *self.imp().local_history_callback.borrow() {
            callback(path);
        }
    }

    fn emit_message(&self, text: &str, severity: NotificationSeverity) {
        if let Some(ref callback) = *self.imp().message_callback.borrow() {
            callback(text, severity);
        }
    }
}

/// Extract the file item at the given position and call the callback if it's a file.
fn activate_file_at(list_view: &gtk4::ListView, position: u32, callback: &dyn Fn(&Path)) {
    let Some(model) = list_view.model() else {
        return;
    };
    if let Some(item) = model.item(position)
        && let Some(tree_row) = item.downcast_ref::<gtk4::TreeListRow>()
        && let Some(file_item) = tree_row
            .item()
            .and_then(|item| item.downcast::<FileTreeItem>().ok())
    {
        if file_item.is_dir() && !file_item.is_placeholder() && file_item.is_empty() != Some(true) {
            tree_row.set_expanded(!tree_row.is_expanded());
            if let Some(section) = list_view
                .ancestor(LushtextWorkspaceSection::static_type())
                .and_downcast::<LushtextWorkspaceSection>()
            {
                section.restart_workspace_watch();
            }
        } else if !file_item.is_dir()
            && let Some(ref path) = file_item.path()
        {
            callback(path);
        }
    }
}

impl Default for LushtextWorkspaceSection {
    fn default() -> Self {
        Self::new(WorkspaceId::default())
    }
}
