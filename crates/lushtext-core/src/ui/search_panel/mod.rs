// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-wide content search panel.
//!
//! Opened via Ctrl+Shift+F, this panel slides up from below the content stack
//! and provides streaming file content search across all workspace folders. The
//! widget remains the driving adapter, while runtime search, history, replace
//! flows, and result rendering live in separate files for readability.

mod history;
// Private implementation module required by gtk-rs: imp.rs owns template
// children, state, and trait impls; this file exposes the public widget API.
mod imp;
pub mod item;
mod list_factory;
mod replace;
mod results;
mod runtime;

#[cfg(feature = "test-utils")]
pub use replace::{set_replace_preview_delay_for_test, set_undo_backup_disk_delay_for_test};

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::model::content_search::{Replacement, SearchQuerySpec};
use crate::services::content_search::ReplaceUndoBackup;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{self, gio};

use self::item::SearchResultItem;

glib::wrapper! {
    /// Workspace search and Replace All panel owned by the main window shell.
    ///
    /// This is the GTK adapter for entries, toggles, and result models; search
    /// execution and persistence details stay in services and split modules.
    pub struct LushtextSearchPanel(ObjectSubclass<imp::LushtextSearchPanel>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

/// Callback update emitted by the search panel while one search is running.
///
/// A named enum keeps callers from depending on positional booleans for
/// "progress vs done" semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchProgressUpdate {
    /// The worker is still running and has visited this many files so far.
    Progress { files_searched: usize },
    /// The worker finished and this is the final visited-file count.
    Done { files_searched: usize },
}

/// Grouped GTK state for one file section in the hierarchical results list.
///
/// The search panel keeps the file-header item together with its child store so
/// runtime result streaming and list-factory lookups share one named bundle.
#[derive(Clone)]
pub struct SearchFileGroup {
    /// Root-level row representing one file in the results tree.
    pub header_item: SearchResultItem,
    /// Observable GObject list that the results tree watches for this file's match rows.
    pub child_store: gio::ListStore,
}

impl SearchFileGroup {
    /// Build one grouped result bucket for a file and its matches.
    #[must_use]
    pub fn new(header_item: SearchResultItem, child_store: gio::ListStore) -> Self {
        Self {
            header_item,
            child_store,
        }
    }
}

/// Flat navigation target for F4 / Shift+F4 match cycling.
///
/// The tree model is hierarchical, but keyboard navigation needs one stable
/// linear sequence of file/line destinations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatchLocation {
    /// File containing the match.
    pub path: PathBuf,
    /// 1-based line number for the match.
    pub line_number: u32,
}

impl SearchMatchLocation {
    /// Build one match-navigation target.
    #[must_use]
    pub fn new(path: PathBuf, line_number: u32) -> Self {
        Self { path, line_number }
    }
}

impl LushtextSearchPanel {
    /// Prepare the panel for display: grab focus on the search entry.
    pub fn open(&self) {
        self.imp().search_entry.grab_focus();
    }

    /// Called when the panel is being hidden.
    pub fn close(&self) {
        if let Some(cancel) = self.imp().runtime.cancel_token.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.imp().runtime.searching.set(false);

        // Replace All journal files are intentionally bounded to the active
        // panel lifetime so a later session cannot inherit stale rollback state.
        self.clear_undo_backup();
    }

    /// Pre-fill the search entry with text (e.g., editor selection).
    pub fn set_query(&self, text: &str) {
        self.imp().search_entry.set_text(text);
    }

    /// Get the current query text.
    #[must_use]
    pub fn query(&self) -> String {
        self.imp().search_entry.text().to_string()
    }

    /// Return whether the workspace search worker is currently running.
    #[must_use]
    pub fn is_searching(&self) -> bool {
        self.imp().runtime.searching.get()
    }

    /// Return total matches accumulated for the current workspace search.
    #[must_use]
    pub fn total_matches(&self) -> u32 {
        self.imp().runtime.total_matches.get()
    }

    /// Return the number of files with matches for the current workspace search.
    #[must_use]
    pub fn total_files(&self) -> u32 {
        self.imp().runtime.total_files.get()
    }

    /// Return whether the current workspace search hit its result cap.
    #[must_use]
    pub fn result_capped(&self) -> bool {
        self.imp().runtime.result_capped.get()
    }

    /// Return whether the case-sensitive option is active.
    #[must_use]
    pub fn case_sensitive(&self) -> bool {
        self.imp().case_toggle.is_active()
    }

    /// Return whether regular-expression search is active.
    #[must_use]
    pub fn regex_enabled(&self) -> bool {
        self.imp().regex_toggle.is_active()
    }

    /// Return whether whole-word matching is active.
    #[must_use]
    pub fn whole_word_enabled(&self) -> bool {
        self.imp().word_toggle.is_active()
    }

    /// Return whether .gitignore filtering is active.
    #[must_use]
    pub fn gitignore_enabled(&self) -> bool {
        self.imp().gitignore_toggle.is_active()
    }

    /// Return the current glob filter text, if any.
    #[must_use]
    pub fn glob_filter(&self) -> Option<String> {
        let text = self.imp().glob_entry.text();
        (!text.is_empty()).then(|| text.to_string())
    }

    /// Return the current replacement text without applying it.
    #[must_use]
    pub fn replace_query(&self) -> String {
        self.imp().replace_entry.text().to_string()
    }

    /// Return whether the result list is showing Replace All preview rows.
    #[must_use]
    pub fn replace_preview_mode(&self) -> bool {
        self.imp().preview.preview_mode.get()
    }

    /// Return whether replacement preview generation is still running.
    #[must_use]
    pub fn replace_preview_pending(&self) -> bool {
        self.imp().preview.preview_pending.get()
    }

    /// Return the number of replacement preview rows currently held in memory.
    #[must_use]
    pub fn replace_preview_count(&self) -> u32 {
        u32::try_from(self.imp().preview.preview_replacements.borrow().len()).unwrap_or(u32::MAX)
    }

    /// Return the number of replacement preview rows selected for apply.
    #[must_use]
    pub fn checked_replacement_count(&self) -> u32 {
        u32::try_from(self.imp().preview.checked_indices.borrow().len()).unwrap_or(u32::MAX)
    }

    /// Return whether a Replace All undo backup is currently available.
    #[must_use]
    pub fn has_undo_backup(&self) -> bool {
        self.imp().preview.undo_backup.borrow().is_some()
    }

    /// Return the number of recent search-history entries loaded into the panel.
    #[must_use]
    pub fn history_count(&self) -> u32 {
        u32::try_from(self.imp().history.history_entries.borrow().len()).unwrap_or(u32::MAX)
    }

    /// Return the number of named saved searches loaded into the panel.
    #[must_use]
    pub fn saved_search_count(&self) -> u32 {
        u32::try_from(self.imp().history.saved_searches.borrow().len()).unwrap_or(u32::MAX)
    }

    /// Return the number of flat match targets available for keyboard navigation.
    #[must_use]
    pub fn navigation_match_count(&self) -> u32 {
        u32::try_from(self.imp().navigation.match_positions.borrow().len()).unwrap_or(u32::MAX)
    }

    /// Return the current flat navigation index, if a match has been selected.
    #[must_use]
    pub fn current_navigation_match_index(&self) -> Option<u32> {
        self.imp()
            .navigation
            .current_match_index
            .get()
            .and_then(|index| u32::try_from(index).ok())
    }

    /// Snapshot the current query text plus all search toggles into one value object.
    #[must_use]
    pub(super) fn current_query_spec(&self) -> SearchQuerySpec {
        let imp = self.imp();
        SearchQuerySpec::new(
            imp.search_entry.text().to_string(),
            crate::model::content_search::ContentSearchOptions::new(
                imp.case_toggle.is_active(),
                imp.regex_toggle.is_active(),
                imp.word_toggle.is_active(),
                imp.gitignore_toggle.is_active(),
                {
                    let text = imp.glob_entry.text();
                    if text.is_empty() {
                        None
                    } else {
                        Some(text.to_string())
                    }
                },
            ),
        )
    }

    /// Update the workspace folders to search. Called when workspaces change.
    pub fn set_workspace_folders(&self, folders: Vec<PathBuf>) {
        self.imp().runtime.workspace_folders.replace(folders);
    }

    /// Register a callback invoked when the user activates a match result.
    pub fn connect_open_file<F: Fn(&Path, u32) + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .open_file_callback
            .replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when the user presses Escape.
    pub fn connect_close_requested<F: Fn() + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .close_requested_callback
            .replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when F4/Shift+F4 navigates to a match.
    pub fn connect_navigate_to_match<F: Fn(&Path, u32) + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .navigate_callback
            .replace(Some(Box::new(f)));
    }

    /// Register a callback invoked on search progress and completion.
    ///
    /// The callback receives a named progress update instead of positional
    /// booleans so callers can pattern-match the workflow state explicitly.
    pub fn connect_search_progress<F: Fn(SearchProgressUpdate) + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .progress_callback
            .replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when "Confirm Replace" is clicked with checked replacements.
    pub fn connect_replace_all<F: Fn(Vec<Replacement>) + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .replace_callback
            .replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when "Undo" is clicked with the backup to restore.
    pub fn connect_undo_all<F: Fn(ReplaceUndoBackup) + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .undo_callback
            .replace(Some(Box::new(f)));
    }

    /// Register a callback for pushing status messages to the window's status bar.
    pub fn connect_message<F: Fn(&str) + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .message_callback
            .replace(Some(Box::new(f)));
    }
}
