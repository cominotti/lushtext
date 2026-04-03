// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette widget — floating search overlay for files and commands.

// Private implementation module (GObject pattern: imp.rs has data + trait
// impls, this file has the public API).
mod imp;
pub mod item;

use crate::model::palette::{IndexedFile, SearchMode};
use crate::services::{async_task, palette::FileIndex};
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use item::PaletteItem;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// A pending incremental mutation to the palette's file index.
/// Queued by sidebar file operations and flushed to the background thread
/// after a debounce interval to avoid rebuilding the full index.
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
    pub fn update_index_file_created(&self, path: &Path) {
        let root = self
            .imp()
            .file_index
            .borrow()
            .workspace_root_for(path)
            .map(|r| Arc::clone(&r));
        if let Some(workspace_root) = root {
            self.enqueue_index_update(FileIndexUpdate::Create(IndexedFile::new(
                path.to_path_buf(),
                workspace_root,
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
        if imp.index_update_inflight.get() {
            return;
        }

        let generation = imp.index_update_generation.get().wrapping_add(1);
        imp.index_update_generation.set(generation);

        let palette_weak = self.downgrade();
        glib::timeout_add_local_once(Duration::from_millis(INDEX_UPDATE_DEBOUNCE_MS), move || {
            let Some(palette) = palette_weak.upgrade() else {
                return;
            };
            let imp = palette.imp();
            if imp.index_update_inflight.get()
                || imp.index_update_generation.get() != generation
                || imp.pending_index_updates.borrow().is_empty()
            {
                return;
            }

            let updates = std::mem::take(&mut *imp.pending_index_updates.borrow_mut());
            let base_index = (*imp.file_index.borrow()).as_ref().clone();
            imp.index_update_inflight.set(true);

            async_task::spawn_blocking_then(
                palette.clone(),
                move || {
                    let mut index = base_index;
                    for update in updates {
                        update.apply(&mut index);
                    }
                    index
                },
                |palette, index| {
                    let imp = palette.imp();
                    *imp.file_index.borrow_mut() = Arc::new(index);
                    imp.index_update_inflight.set(false);

                    // Only rebuild results if the palette is currently visible;
                    // the next open() call rebuilds from scratch anyway.
                    if palette.is_visible() {
                        let query = imp.search_entry.text();
                        imp.rebuild_results(&query);
                    }

                    if !imp.pending_index_updates.borrow().is_empty() {
                        palette.schedule_index_update_flush();
                    }
                },
            );
        });
    }
}

impl Default for LushtextCommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// Debounce interval for flushing incremental index updates (ms).
/// 75ms coalesces rapid sidebar operations (e.g., deleting a directory
/// with many files) into a single background index update.
const INDEX_UPDATE_DEBOUNCE_MS: u64 = 75;
