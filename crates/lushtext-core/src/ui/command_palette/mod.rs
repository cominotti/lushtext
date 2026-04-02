// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette widget — floating search overlay for files and commands.

mod imp;
pub mod item;

use crate::model::palette::{IndexedFile, SearchMode};
use crate::services::palette::FileIndex;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::Object;
use gtk4::prelude::*;
use item::PaletteItem;
use std::path::Path;
use std::sync::Arc;

glib::wrapper! {
    pub struct LushtextCommandPalette(ObjectSubclass<imp::LushtextCommandPalette>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextCommandPalette {
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Replace the file index. Called when workspace roots change.
    pub fn set_file_index(&self, index: FileIndex) {
        *self.imp().file_index.borrow_mut() = Arc::new(index);
        // Re-run search if the palette is currently showing results
        let query = self.imp().search_entry.text();
        self.imp().rebuild_results(&query);
    }

    /// Open the palette: focus the search entry and show initial results.
    pub fn open(&self) {
        let imp = self.imp();
        imp.mode.set(SearchMode::All);
        imp.mode_label.set_label(SearchMode::All.label());
        imp.search_entry.set_text("");
        imp.rebuild_results("");
        imp.search_entry.grab_focus();
    }

    /// Close the palette: clear the search entry.
    pub fn close(&self) {
        let imp = self.imp();
        imp.search_entry.set_text("");
        imp.results_store.remove_all();
        imp.no_results_label.set_visible(false);
    }

    /// Register a callback for when an item is activated (Enter or click).
    pub fn connect_item_activated<F: Fn(&PaletteItem) + 'static>(&self, f: F) {
        *self.imp().activate_callback.borrow_mut() = Some(Box::new(f));
    }

    /// Register a callback for when the palette should close (Escape).
    pub fn connect_close_requested<F: Fn() + 'static>(&self, f: F) {
        *self.imp().close_callback.borrow_mut() = Some(Box::new(f));
    }

    /// The current search mode.
    pub fn mode(&self) -> SearchMode {
        self.imp().mode.get()
    }

    /// Number of files in the current index (used as capacity hint for rebuilds).
    pub fn file_index_len(&self) -> usize {
        self.imp().file_index.borrow().len()
    }

    // --- Incremental index updates ---

    /// Add a newly created file to the search index.
    /// Uses `Arc::make_mut` for copy-on-write if a background search holds a ref.
    pub fn update_index_file_created(&self, path: &Path) {
        let mut arc_ref = self.imp().file_index.borrow_mut();
        let root = arc_ref.workspace_root_for(path).map(|r| Arc::clone(&r));
        if let Some(workspace_root) = root {
            Arc::make_mut(&mut arc_ref)
                .add_file(IndexedFile::new(path.to_path_buf(), workspace_root));
        }
    }

    /// Remove a deleted file (or all files under a directory) from the index.
    pub fn update_index_file_deleted(&self, path: &Path) {
        let mut arc_ref = self.imp().file_index.borrow_mut();
        Arc::make_mut(&mut arc_ref).remove_path(path);
    }

    /// Update a renamed file (or directory prefix) in the index.
    pub fn update_index_file_renamed(&self, old_path: &Path, new_path: &Path) {
        let mut arc_ref = self.imp().file_index.borrow_mut();
        Arc::make_mut(&mut arc_ref).rename_path(old_path, new_path);
    }
}

impl Default for LushtextCommandPalette {
    fn default() -> Self {
        Self::new()
    }
}
