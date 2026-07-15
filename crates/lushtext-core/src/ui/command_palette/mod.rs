// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette widget — floating search overlay for files and commands.

// Private implementation module required by gtk-rs: imp.rs owns template
// children, state, and trait impls; this file exposes the public widget API.
mod imp;
pub mod item;
mod runtime;

use crate::model::palette::{PaletteFileEntry, PaletteNoteEntry, SearchMode};
use crate::services::palette::FileIndex;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use item::PaletteItem;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy)]
enum FileIndexRetirementKind {
    FullReplacement,
    AcceptedIncremental,
    RejectedIncremental,
}

/// Scalar evidence that last-owned indexes reached the bounded worker lane.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileIndexRetirementSnapshot {
    /// Last-owned full replacements destroyed on the worker lane.
    pub full_replacements: usize,
    /// Last-owned accepted incremental bases destroyed on the worker lane.
    pub accepted_incremental: usize,
    /// Last-owned rejected incremental outputs destroyed on the worker lane.
    pub rejected_incremental: usize,
}

#[cfg(feature = "test-utils")]
static FULL_REPLACEMENT_RETIREMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static ACCEPTED_INCREMENTAL_RETIREMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static REJECTED_INCREMENTAL_RETIREMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Transfer possible final destruction of a large index to the bounded worker lane.
fn retire_file_index(index: Arc<FileIndex>, kind: FileIndexRetirementKind) {
    let last_owned = Arc::strong_count(&index) == 1;
    let reached_policy_cap = index.len() == crate::services::palette::MAX_INDEXED_FILES;
    gtk_lush_tasks::spawn_blocking_then(
        (),
        move || {
            drop(index);
            record_file_index_retirement(kind, last_owned && reached_policy_cap);
        },
        |(), ()| {},
    );
}

#[cfg(feature = "test-utils")]
fn record_file_index_retirement(kind: FileIndexRetirementKind, last_owned: bool) {
    use std::sync::atomic::Ordering;

    if !last_owned {
        return;
    }
    let counter = match kind {
        FileIndexRetirementKind::FullReplacement => &FULL_REPLACEMENT_RETIREMENTS,
        FileIndexRetirementKind::AcceptedIncremental => &ACCEPTED_INCREMENTAL_RETIREMENTS,
        FileIndexRetirementKind::RejectedIncremental => &REJECTED_INCREMENTAL_RETIREMENTS,
    };
    counter.fetch_add(1, Ordering::Release);
}

#[cfg(not(feature = "test-utils"))]
fn record_file_index_retirement(_kind: FileIndexRetirementKind, _last_owned: bool) {}

#[cfg(feature = "test-utils")]
#[must_use]
/// Return process-local retirement evidence for deterministic widget tests.
pub fn file_index_retirement_snapshot_for_test() -> FileIndexRetirementSnapshot {
    use std::sync::atomic::Ordering;

    FileIndexRetirementSnapshot {
        full_replacements: FULL_REPLACEMENT_RETIREMENTS.load(Ordering::Acquire),
        accepted_incremental: ACCEPTED_INCREMENTAL_RETIREMENTS.load(Ordering::Acquire),
        rejected_incremental: REJECTED_INCREMENTAL_RETIREMENTS.load(Ordering::Acquire),
    }
}

#[cfg(feature = "test-utils")]
static INDEX_UPDATE_DELAY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(feature = "test-utils")]
/// Delay incremental index workers so replacement races can be tested deterministically.
pub fn set_index_update_delay_for_test(delay_ms: u64) {
    INDEX_UPDATE_DELAY_MS.store(delay_ms, std::sync::atomic::Ordering::Release);
}

fn delay_index_update_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = INDEX_UPDATE_DELAY_MS.load(std::sync::atomic::Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
    }
}

/// A pending incremental mutation to the palette's file index.
///
/// Sidebar file operations queue these and a short main-loop debounce coalesces
/// bursts before a serialized background worker applies them to the index.
#[derive(Clone)]
pub(super) enum FileIndexUpdate {
    Create {
        path: PathBuf,
        workspace_folder: Arc<PathBuf>,
    },
    Delete(PathBuf),
    Rename {
        old_path: PathBuf,
        new_path: PathBuf,
    },
}

impl FileIndexUpdate {
    fn apply(&self, index: &mut FileIndex) {
        match self {
            Self::Create {
                path,
                workspace_folder,
            } => index.add_path(path.clone(), Arc::clone(workspace_folder)),
            Self::Delete(path) => index.remove_path(path),
            Self::Rename { old_path, new_path } => index.rename_path(old_path, new_path),
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
        let imp = self.imp();
        let previous = std::mem::replace(&mut *imp.file_index.borrow_mut(), Arc::new(index));
        retire_file_index(previous, FileIndexRetirementKind::FullReplacement);
        imp.file_index_generation
            .set(imp.file_index_generation.get().wrapping_add(1));
        // Re-run search if the palette is currently showing results
        if self.imp().palette_open.get() {
            let query = self.imp().search_entry.text();
            self.imp().rebuild_results(&query);
        }
    }

    /// Replace the open file-backed tab source used by grouped file results.
    pub fn set_open_tabs(&self, open_tabs: Vec<PaletteFileEntry>) {
        *self.imp().open_tabs.borrow_mut() = Arc::from(open_tabs);
        if self.imp().palette_open.get() {
            let query = self.imp().search_entry.text();
            self.imp().rebuild_results(&query);
        }
    }

    /// Replace the cached note rows used by Notes and All mode.
    pub fn set_note_entries(&self, note_entries: Vec<PaletteNoteEntry>) {
        let previous = std::mem::replace(
            &mut *self.imp().note_entries.borrow_mut(),
            Arc::from(note_entries),
        );
        gtk_lush_tasks::spawn_blocking_then((), move || drop(previous), |(), ()| {});
        if self.imp().palette_open.get()
            && matches!(self.mode(), SearchMode::All | SearchMode::Notes)
        {
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
        if self.imp().palette_open.get() {
            let query = self.imp().search_entry.text();
            self.imp().rebuild_results(&query);
        }
    }

    /// Refresh all source metadata that is owned by the window shell.
    pub fn set_sources(&self, open_tabs: Vec<PaletteFileEntry>, workspace_group_label: &str) {
        *self.imp().open_tabs.borrow_mut() = Arc::from(open_tabs);
        *self.imp().workspace_group_label.borrow_mut() = workspace_group_label.to_string();
        if self.imp().palette_open.get() {
            let query = self.imp().search_entry.text();
            self.imp().rebuild_results(&query);
        }
    }

    /// Open the palette: focus the search entry and show initial results.
    pub fn open(&self) {
        let imp = self.imp();
        imp.palette_open.set(true);
        imp.set_mode(SearchMode::All);
        imp.search_entry.set_text("");
        imp.rebuild_results("");
        imp.search_entry.grab_focus();
    }

    /// Close the palette: clear the search entry.
    pub fn close(&self) {
        let imp = self.imp();
        imp.palette_open.set(false);
        let _ = imp.search_debounce.advance();
        imp.search_runtime.borrow_mut().invalidate();
        imp.searching.set(false);
        imp.search_entry.set_text("");
        imp.results_store.remove_all();
        imp.no_results_label.set_visible(false);
        imp.refresh_accessibility_state();
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

    /// Number of queued mutations plus any active index-mutation worker.
    #[must_use]
    pub fn pending_index_update_count(&self) -> usize {
        self.imp()
            .pending_index_updates
            .borrow()
            .len()
            .saturating_add(usize::from(self.imp().index_update_worker_running.get()))
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    /// Report whether the serialized incremental index worker is active.
    pub fn index_update_worker_running_for_test(&self) -> bool {
        self.imp().index_update_worker_running.get()
    }

    /// Whether the visible palette owns active or latest query work.
    #[must_use]
    pub fn is_searching(&self) -> bool {
        self.imp().searching.get()
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn search_runtime_snapshot_for_test(
        &self,
    ) -> crate::services::palette::PaletteSearchCoordinatorSnapshot {
        self.imp().search_runtime.borrow().snapshot()
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn observed_search_cancellations_for_test(&self) -> usize {
        self.imp().observed_search_cancellations.get()
    }

    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn last_cancelled_search_examined_for_test(&self) -> usize {
        self.imp().last_cancelled_search_examined.get()
    }

    /// Test seam for forcing accessibility projection after mutating adapter state.
    #[cfg(feature = "test-utils")]
    pub fn refresh_accessibility_state_for_test(&self) {
        self.imp().refresh_accessibility_state();
    }

    // --- Incremental index updates ---

    /// Add a newly created file to the search index.
    pub fn update_index_file_created(&self, path: &Path) {
        let folder = self.imp().file_index.borrow().workspace_folder_for(path);
        if let Some(workspace_folder) = folder {
            self.enqueue_index_update(FileIndexUpdate::Create {
                path: path.to_path_buf(),
                workspace_folder,
            });
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
        self.imp().index_update_debounce.schedule(
            self,
            Duration::from_millis(INDEX_UPDATE_DEBOUNCE_MS),
            move |palette, _| palette.flush_index_updates(),
        );
    }

    fn flush_index_updates(&self) {
        let imp = self.imp();
        if imp.index_update_worker_running.get() || imp.pending_index_updates.borrow().is_empty() {
            return;
        }

        let updates = std::mem::take(&mut *imp.pending_index_updates.borrow_mut());
        let base = Arc::clone(&imp.file_index.borrow());
        let base_generation = imp.file_index_generation.get();
        imp.index_update_worker_running.set(true);
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                delay_index_update_for_test();
                let mut index = (*base).clone();
                for update in &updates {
                    update.apply(&mut index);
                }
                (index, updates)
            },
            move |palette, (index, applied_updates)| {
                let imp = palette.imp();
                imp.index_update_worker_running.set(false);
                if imp.file_index_generation.get() == base_generation {
                    let previous =
                        std::mem::replace(&mut *imp.file_index.borrow_mut(), Arc::new(index));
                    retire_file_index(previous, FileIndexRetirementKind::AcceptedIncremental);
                    imp.file_index_generation
                        .set(base_generation.wrapping_add(1));
                    if imp.palette_open.get() {
                        let query = imp.search_entry.text();
                        imp.rebuild_results(&query);
                    }
                } else {
                    retire_file_index(
                        Arc::new(index),
                        FileIndexRetirementKind::RejectedIncremental,
                    );
                    // A full replacement won the race. Replay this worker's
                    // mutations before newer queued ones so neither source of
                    // truth is silently lost.
                    imp.pending_index_updates
                        .borrow_mut()
                        .splice(0..0, applied_updates);
                }
                palette.flush_index_updates();
            },
        );
    }
}

impl Default for LushtextCommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

/// Test seam for asserting the palette row metadata used by the list factory.
#[cfg(feature = "test-utils")]
pub fn apply_palette_row_accessibility_for_test(
    row: &gtk4::Box,
    item: &PaletteItem,
    selected: bool,
    position: i32,
    set_size: i32,
) {
    imp::apply_palette_row_accessibility(row, item, selected, position, set_size);
}

/// Debounce interval for handing incremental index updates to the worker.
///
/// Seventy-five milliseconds coalesces rapid sidebar mutations while keeping
/// file creation, deletion, and rename projections responsive.
const INDEX_UPDATE_DEBOUNCE_MS: u64 = 75;

#[cfg(feature = "test-utils")]
pub use runtime::set_search_delay_for_test;
