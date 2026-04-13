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
mod replace;
mod results;
mod runtime;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::content_search::{Replacement, SearchQuerySpec};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

glib::wrapper! {
    pub struct LushtextSearchPanel(ObjectSubclass<imp::LushtextSearchPanel>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
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
            crate::model::content_search::ContentSearchOptions {
                case_sensitive: imp.case_toggle.is_active(),
                regex: imp.regex_toggle.is_active(),
                whole_word: imp.word_toggle.is_active(),
                gitignore: imp.gitignore_toggle.is_active(),
                glob: {
                    let text = imp.glob_entry.text();
                    if text.is_empty() {
                        None
                    } else {
                        Some(text.to_string())
                    }
                },
            },
        )
    }

    /// Update the workspace roots to search. Called when workspaces change.
    pub fn set_workspace_roots(&self, roots: Vec<PathBuf>) {
        self.imp().workspace_roots.replace(roots);
    }

    /// Register a callback invoked when the user activates a match result.
    pub fn connect_open_file<F: Fn(&Path, u32) + 'static>(&self, f: F) {
        self.imp().open_file_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when the user presses Escape.
    pub fn connect_close_requested<F: Fn() + 'static>(&self, f: F) {
        self.imp()
            .close_requested_callback
            .replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when F4/Shift+F4 navigates to a match.
    pub fn connect_navigate_to_match<F: Fn(&Path, u32) + 'static>(&self, f: F) {
        self.imp().navigate_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback invoked on search progress and completion.
    /// Signature: `(files_searched, is_done)`. `is_done=true` on `SearchEvent::Done`.
    pub fn connect_search_progress<F: Fn(usize, bool) + 'static>(&self, f: F) {
        self.imp().progress_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when "Confirm Replace" is clicked with checked replacements.
    pub fn connect_replace_all<F: Fn(Vec<Replacement>) + 'static>(&self, f: F) {
        self.imp().replace_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback invoked when "Undo" is clicked with the backup to restore.
    pub fn connect_undo_all<F: Fn(HashMap<PathBuf, Vec<u8>>) + 'static>(&self, f: F) {
        self.imp().undo_callback.replace(Some(Box::new(f)));
    }

    /// Register a callback for pushing status messages to the window's status bar.
    pub fn connect_message<F: Fn(&str) + 'static>(&self, f: F) {
        self.imp().message_callback.replace(Some(Box::new(f)));
    }
}
