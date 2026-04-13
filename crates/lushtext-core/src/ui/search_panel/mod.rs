// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-wide content search panel.
//!
//! Opened via Ctrl+Shift+F, this panel slides up from below the content stack
//! and provides streaming file content search across all workspace roots. The
//! widget remains the driving adapter, while runtime search, history, replace
//! flows, and result rendering live in separate files for readability.

mod history;
mod imp;
pub mod item;
mod list_factory;
mod replace;
mod results;
mod runtime;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::content_search::{Replacement, SearchQuerySpec};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{self, gio};

use self::item::SearchResultItem;

glib::wrapper! {
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
    /// Child store containing the file's match rows.
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

    /// Called when the panel is being hidden. Preserves the undo backup so it
    /// does not outlive the panel-close safety boundary.
    pub fn close(&self) {
        // Don't cancel the search — preserve results for when the panel reopens.
        // The polling timer is self-managing (stops when Done is received).
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

    /// Update the workspace roots to search. Called when workspaces change.
    pub fn set_workspace_roots(&self, roots: Vec<PathBuf>) {
        self.imp().runtime.workspace_roots.replace(roots);
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
    pub fn connect_undo_all<F: Fn(HashMap<PathBuf, Vec<u8>>) + 'static>(&self, f: F) {
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
