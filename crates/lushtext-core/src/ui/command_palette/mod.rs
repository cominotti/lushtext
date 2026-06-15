// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette widget — floating search overlay for files and commands.

// Private implementation module required by gtk-rs: imp.rs owns template
// children, state, and trait impls; this file exposes the public widget API.
mod imp;
pub mod item;

use crate::model::palette::{IndexedFile, PaletteFileEntry, PaletteNoteEntry, SearchMode};
use crate::services::palette::FileIndex;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use item::PaletteItem;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// A pending incremental mutation to the palette's file index.
///
/// Sidebar file operations queue these and a short main-loop debounce coalesces
/// bursts before applying them to the in-memory index.
#[derive(Clone)]
pub(super) enum FileIndexUpdate {
    Create(IndexedFile),
    Delete(PathBuf),
    Rename {
        old_path: PathBuf,
        new_path: PathBuf,
    },
}

impl FileIndexUpdate {
    fn apply(self, index: &mut FileIndex) {
        match self {
            Self::Create(file) => index.add_file(file),
            Self::Delete(path) => index.remove_path(&path),
            Self::Rename { old_path, new_path } => index.rename_path(&old_path, &new_path),
        }
    }
}

// glib::wrapper! generates the public wrapper type for this widget.
// @extends declares the GTK class hierarchy; @implements lists interfaces.
glib::wrapper! {
    /// Floating command/search widget owned by the window shell.
    ///
    /// The widget stays on the GTK main thread; expensive indexing and fuzzy
    /// matching live in the GTK-free palette service.
    pub struct LushtextCommandPalette(ObjectSubclass<imp::LushtextCommandPalette>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextCommandPalette {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Replace the file index. Called when workspace folders change.
    pub fn set_file_index(&self, index: FileIndex) {
        *self.imp().file_index.borrow_mut() = Arc::new(index);
        // Re-run search if the palette is currently showing results
        let query = self.imp().search_entry.text();
        self.imp().rebuild_results(&query);
    }

    /// Replace the open file-backed tab source used by grouped file results.
    pub fn set_open_tabs(&self, open_tabs: Vec<PaletteFileEntry>) {
        *self.imp().open_tabs.borrow_mut() = open_tabs;
        if self.is_visible() {
            let query = self.imp().search_entry.text();
            self.imp().rebuild_results(&query);
        }
    }

    /// Replace the cached note rows used by Notes and All mode.
    pub fn set_note_entries(&self, note_entries: Vec<PaletteNoteEntry>) {
        *self.imp().note_entries.borrow_mut() = Arc::from(note_entries);
        if self.is_visible() {
            let query = self.imp().search_entry.text();
            self.imp().rebuild_results(&query);
        }
    }

    /// Set the label for the workspace-indexed file group.
    pub fn set_workspace_group_label(&self, label: impl Into<String>) {
        let label = label.into();
        if *self.imp().workspace_group_label.borrow() == label {
            return;
        }
        *self.imp().workspace_group_label.borrow_mut() = label;
        if self.is_visible() {
            let query = self.imp().search_entry.text();
            self.imp().rebuild_results(&query);
        }
    }

    /// Refresh all source metadata that is owned by the window shell.
    pub fn set_sources(&self, open_tabs: Vec<PaletteFileEntry>, workspace_group_label: &str) {
        *self.imp().open_tabs.borrow_mut() = open_tabs;
        *self.imp().workspace_group_label.borrow_mut() = workspace_group_label.to_string();
        if self.is_visible() {
            let query = self.imp().search_entry.text();
            self.imp().rebuild_results(&query);
        }
    }

    /// Open the palette: focus the search entry and show initial results.
    pub fn open(&self) {
        let imp = self.imp();
        imp.set_mode(SearchMode::All);
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
    #[must_use]
    pub fn mode(&self) -> SearchMode {
        self.imp().mode.get()
    }

    /// Set the visible search mode and rebuild the current result list.
    pub fn set_search_mode(&self, mode: SearchMode) {
        let imp = self.imp();
        imp.set_mode(mode);
        let query = imp.search_entry.text();
        imp.rebuild_results(&query);
        imp.search_entry.grab_focus();
    }

    /// Current query text in the palette entry.
    #[must_use]
    pub fn query(&self) -> String {
        self.imp().search_entry.text().to_string()
    }

    /// Set the visible query text and rebuild through the normal search pipeline.
    pub fn set_query(&self, query: &str) {
        let imp = self.imp();
        if imp.search_entry.text().as_str() != query {
            imp.search_entry.set_text(query);
        }
        imp.rebuild_results(query);
        imp.search_entry.grab_focus();
    }

    /// Number of rows currently rendered by the palette results model.
    #[must_use]
    pub fn result_count(&self) -> u32 {
        self.imp().results_store.n_items()
    }

    /// Number of files in the current index (used as capacity hint for rebuilds).
    #[must_use]
    pub fn file_index_len(&self) -> usize {
        self.imp().file_index.borrow().len()
    }

    /// Number of open file-backed tabs supplied by the window shell.
    #[must_use]
    pub fn open_tab_source_count(&self) -> usize {
        self.imp().open_tabs.borrow().len()
    }

    /// Number of cached note entries supplied by the window shell.
    #[must_use]
    pub fn note_source_count(&self) -> usize {
        self.imp().note_entries.borrow().len()
    }

    /// Number of queued index mutations waiting for the debounce flush.
    #[must_use]
    pub fn pending_index_update_count(&self) -> usize {
        self.imp().pending_index_updates.borrow().len()
    }

    // --- Incremental index updates ---

    /// Add a newly created file to the search index.
    pub fn update_index_file_created(&self, path: &Path) {
        let folder = self.imp().file_index.borrow().workspace_folder_for(path);
        if let Some(workspace_folder) = folder {
            self.enqueue_index_update(FileIndexUpdate::Create(IndexedFile::new(
                path.to_path_buf(),
                workspace_folder,
            )));
        }
    }

    /// Remove a deleted file (or all files under a directory) from the index.
    pub fn update_index_file_deleted(&self, path: &Path) {
        self.enqueue_index_update(FileIndexUpdate::Delete(path.to_path_buf()));
    }

    /// Update a renamed file (or directory prefix) in the index.
    pub fn update_index_file_renamed(&self, old_path: &Path, new_path: &Path) {
        self.enqueue_index_update(FileIndexUpdate::Rename {
            old_path: old_path.to_path_buf(),
            new_path: new_path.to_path_buf(),
        });
    }

    fn enqueue_index_update(&self, update: FileIndexUpdate) {
        self.imp().pending_index_updates.borrow_mut().push(update);
        self.schedule_index_update_flush();
    }

    fn schedule_index_update_flush(&self) {
        let imp = self.imp();
        imp.index_update_debounce.schedule(
            self,
            Duration::from_millis(INDEX_UPDATE_DEBOUNCE_MS),
            move |palette, _| {
                let imp = palette.imp();
                if imp.pending_index_updates.borrow().is_empty() {
                    return;
                }

                // Apply mutations in place on the main thread. The incremental
                // operations (add/remove/rename) are O(n) linear scans with no I/O,
                // so they're cheaper than the clone that would be needed to send the
                // index to a background thread. Arc::make_mut avoids cloning when no
                // concurrent search holds a reference (the common case).
                let updates = std::mem::take(&mut *imp.pending_index_updates.borrow_mut());
                let mut file_index = imp.file_index.borrow_mut();
                let index = Arc::make_mut(&mut file_index);
                for update in updates {
                    update.apply(index);
                }
                drop(file_index);

                if palette.is_visible() {
                    let query = imp.search_entry.text();
                    imp.rebuild_results(&query);
                }
            },
        );
    }
}

impl Default for LushtextCommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// Debounce interval for flushing incremental index updates on the GTK main thread.
///
/// Seventy-five milliseconds coalesces rapid sidebar mutations while keeping
/// the common in-place `Arc::make_mut` path cheaper than cloning the index for
/// a worker.
const INDEX_UPDATE_DEBOUNCE_MS: u64 = 75;
