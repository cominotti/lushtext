// SPDX-License-Identifier: GPL-3.0-or-later

//! File tree sidebar widget.

pub mod file_tree_item;
mod imp;

use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::prelude::*;

glib::wrapper! {
    pub struct LushtextSidebar(ObjectSubclass<imp::LushtextSidebar>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextSidebar {
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Set the workspace name displayed in the header.
    pub fn set_workspace_name(&self, name: &str) {
        self.imp().workspace_label.set_label(name);
    }

    /// Set the tree model for the file list view.
    pub fn set_model(&self, model: &gtk4::TreeListModel) {
        let selection = gtk4::SingleSelection::new(Some(model.clone()));
        self.imp().file_tree_view.set_model(Some(&selection));
    }

    /// Connect a handler for when a file is activated (double-clicked or Enter).
    pub fn connect_file_activated<F: Fn(&std::path::Path) + 'static>(&self, f: F) {
        self.imp().file_tree_view.connect_activate(move |list_view, position| {
            let model = list_view.model().expect("list view has a model");
            if let Some(item) = model.item(position) {
                // Unwrap through SingleSelection → TreeListRow → FileTreeItem
                let tree_row = item
                    .downcast_ref::<gtk4::TreeListRow>()
                    .expect("item is a TreeListRow");
                if let Some(file_item) = tree_row
                    .item()
                    .and_then(|i| i.downcast::<file_tree_item::FileTreeItem>().ok())
                {
                    if !file_item.is_dir() {
                        f(&file_item.path());
                    }
                }
            }
        });
    }
}

impl Default for LushtextSidebar {
    fn default() -> Self {
        Self::new()
    }
}
