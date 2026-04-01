// SPDX-License-Identifier: GPL-3.0-or-later

//! File tree sidebar widget.

pub mod file_tree_item;
mod imp;

use crate::services;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::gio;
use gtk4::prelude::*;
use std::path::{Path, PathBuf};

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

    /// Load root paths into the file tree. Builds the `TreeListModel`
    /// and child models asynchronously for responsive UI.
    pub fn load_roots(&self, roots: &[PathBuf]) {
        let root_store = gio::ListStore::new::<file_tree_item::FileTreeItem>();
        for root in roots {
            root_store.append(&file_tree_item::FileTreeItem::new(
                root.clone(),
                root.is_dir(),
            ));
        }

        let tree_model = gtk4::TreeListModel::new(root_store, false, false, |item| {
            item.downcast_ref::<file_tree_item::FileTreeItem>()
                .filter(|fi| fi.is_dir())
                .map(|fi| build_children_model(&fi.path()))
                .map(|m| m.upcast::<gio::ListModel>())
        });

        let selection = gtk4::SingleSelection::new(Some(tree_model));
        self.imp().file_tree_view.set_model(Some(&selection));
    }

    /// Connect a handler for when a file is activated (double-clicked or Enter).
    ///
    /// Two activation paths are needed because `GtkTreeExpander` installs an
    /// internal `GtkGestureClick` (BUBBLE phase, exclusive) that claims click
    /// events for ALL rows — even non-expandable files. This prevents
    /// `GtkListView`'s built-in double-click activation from ever firing via
    /// mouse. The workaround: a CAPTURE-phase gesture that fires before the
    /// expander can claim the event.
    pub fn connect_file_activated<F: Fn(&std::path::Path) + 'static>(&self, f: F) {
        let callback = std::rc::Rc::new(f);

        // Keyboard activation (Enter key) — GtkListView::activate fires normally
        // because GtkTreeExpander only intercepts mouse gestures, not key events.
        let cb = callback.clone();
        self.imp()
            .file_tree_view
            .connect_activate(move |list_view, position| {
                activate_file_at(list_view, position, &*cb);
            });

        // Mouse double-click — CAPTURE phase fires before the expander's
        // BUBBLE-phase gesture can claim the event. The first click's BUBBLE
        // phase sets the selection; on n_press==2, we read that selection.
        let cb = callback;
        let gesture = gtk4::GestureClick::new();
        gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
        gesture.connect_released(move |gesture, n_press, _, _| {
            if n_press != 2 {
                return;
            }
            let Some(widget) = gesture.widget() else {
                return;
            };
            let Ok(list_view) = widget.downcast::<gtk4::ListView>() else {
                return;
            };
            let Some(model) = list_view.model() else {
                return;
            };
            if let Some(sel) = model.downcast_ref::<gtk4::SingleSelection>() {
                let pos: u32 = sel.selected();
                if pos != u32::MAX {
                    activate_file_at(&list_view, pos, &*cb);
                }
            }
        });
        self.imp().file_tree_view.add_controller(gesture);
    }
}

/// Extract the file item at the given position and call the callback if it's a file.
fn activate_file_at(
    list_view: &gtk4::ListView,
    position: u32,
    callback: &dyn Fn(&std::path::Path),
) {
    let Some(model) = list_view.model() else {
        return;
    };
    if let Some(item) = model.item(position) {
        if let Some(tree_row) = item.downcast_ref::<gtk4::TreeListRow>() {
            if let Some(file_item) = tree_row
                .item()
                .and_then(|i| i.downcast::<file_tree_item::FileTreeItem>().ok())
            {
                if !file_item.is_dir() {
                    callback(&file_item.path());
                }
            }
        }
    }
}

/// Build a child `ListStore` for a directory's contents.
/// Returns an empty store immediately and populates it from a background
/// thread via `spawn_blocking_then`.
fn build_children_model(dir_path: &Path) -> gio::ListStore {
    let store = gio::ListStore::new::<file_tree_item::FileTreeItem>();
    let path = dir_path.to_path_buf();

    services::async_task::spawn_blocking_then(
        store.clone(),
        move || services::file_tree::scan_directory(&path),
        |store, entries| {
            for (path, is_dir) in entries {
                store.append(&file_tree_item::FileTreeItem::new(path, is_dir));
            }
        },
    );

    store
}

impl Default for LushtextSidebar {
    fn default() -> Self {
        Self::new()
    }
}
