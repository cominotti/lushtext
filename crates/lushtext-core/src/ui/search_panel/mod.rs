// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-wide content search and Replace All preview: the workflow facade.
//!
//! Opened with Ctrl+Shift+F, this panel slides up from below the content stack
//! and searches file contents across every workspace folder. This module is the
//! workflow's narrative facade: it names the ordered stages and delegates each
//! one. It owns no timers, generation counters, admission bookkeeping, stage
//! machinery, or widget mutation beyond reading and writing the visible query
//! controls that make up its entry-point surface.
//!
//! # Stage order: search
//!
//! 1. **Capture the query.** Ctrl+Shift+F, a search-entry edit, or an option
//!    toggle snapshots query text plus every option into one `SearchQuerySpec`
//!    (`evidence::current_query_spec`). Typed input is debounced in `imp`.
//! 2. **Apply retirement backpressure.** If two detached result generations are
//!    still being released, the latest query is retained instead of started
//!    (`retirement::result_retirement_saturated`).
//! 3. **Detach the previous generation.** Visible results, navigation caches,
//!    and the accepted snapshot move out of live state into the bounded
//!    disposer (`retirement::detach_visible_results`), and any superseded
//!    Replace All preview is released with them
//!    (`replace_execution::release_superseded_preview`).
//! 4. **Admit one flight.** `execution::start_search` submits the request to the
//!    single-flight policy, which either starts it or keeps it as the one
//!    replaceable latest request (`policy::WorkspaceSearchFlight`).
//! 5. **Stream results.** `execution` spawns the walker thread and a paced GTK
//!    turn appends match rows into the grouped tree model.
//!
//! Two inversions connect those stages:
//!
//! - Stage 5 returns once the worker and its poll timer are armed, resuming in
//!   the 50 ms poll callback in `execution` once per tick. The terminal tick
//!   finishes the flight and, if a latest query was retained, re-enters stage 3
//!   from there rather than returning to this facade.
//! - Stage 3's bounded retirement resumes in a `glib::idle_add_local` callback
//!   in `retirement`, once per GTK turn. Its final turn is also where a query
//!   deferred by stage 2 restarts.
//!
//! # Stage order: Replace All
//!
//! 1. **Open one preview attempt.** The Replace All button, or
//!    [`LushtextSearchPanel::activate_replace_preview`], opens a preview
//!    generation and captures its identity once as a
//!    `policy::ReplacePreviewTicket` (`replace_execution::issue_preview_ticket`).
//! 2. **Generate the preview.** `replace_execution::enter_preview_mode`
//!    reserves disposal capacity and hands the accepted match snapshot to a
//!    worker.
//! 3. **Confirm the checked rows.**
//!    [`LushtextSearchPanel::activate_confirm_replacements`] delegates to
//!    `replace_execution::begin_confirmed_replacement`, which claims `journal`'s
//!    single apply transaction, opens a fresh attempt, and hands the partition
//!    to `replace_execution::apply_checked_replacements`.
//! 4. **Write the files and record the journal.** The window's Replace All
//!    callback takes `journal`'s reserved generation, writes the files, and
//!    publishes the resulting undo journal back through
//!    `journal::publish_undo_journal_for_generation`.
//! 5. **Offer undo.** [`LushtextSearchPanel::activate_undo_replacements`]
//!    delegates to `journal::hand_back_undo_backup`, which hands the backup
//!    back to the window; the window claims the panel through
//!    `journal::begin_undo_restore` and reports through
//!    `journal::finish_undo_restore`.
//!
//! Ten inversions connect those stages — one per point where control leaves
//! this workflow and later resumes at a named place. With the search order's
//! two above, the workflow has twelve. Each module documents its own in detail.
//!
//! - Stage 2's reservation may be refused: the request parks and resumes in
//!   `replace_execution`'s `preview_capacity_wakeup`, revalidated by `may_dispatch`.
//! - Stages 2 and 3 each return once their worker is dispatched, resuming in a
//!   `replace_execution` completion closure that revalidates the attempt's
//!   ticket against live `policy::ReplacePreviewFacts`. A stale completion
//!   publishes nothing and routes its payload to bounded retirement.
//! - That preview retirement resumes in a `glib::idle_add_once` callback which
//!   re-enters `replace_execution::finish_preview_worker`; that drain re-enters
//!   *itself* by tail recursion while the retained request is undispatchable.
//! - Stages 4 and 5 each leave the panel for `ui/window/search.rs`, which
//!   performs the durable write in `services/content_search`. Control resumes
//!   in `journal` — stage 4 via the publish/clear and finish operations above,
//!   stage 5 via `begin_undo_restore` and `finish_undo_restore`.
//! - `journal`'s disk save and delete resume in completion closures that only
//!   report, the in-memory journal being already published under its guard.
//! - Startup recovery re-enters `journal::load_persisted_undo_backup` from a
//!   disposal-capacity wakeup when admission defers it, and otherwise resumes
//!   in a worker completion that re-checks the journal generation.
//!
//! # Roles
//!
//! | Role | Module |
//! | --- | --- |
//! | facade | this module |
//! | pure policy | `policy` |
//! | coordination | `execution` (streaming search), `retirement` (bounded result disposal), `replace_execution` (the Replace All preview attempt and checked apply), `journal` (the durable undo journal, its transaction gate, and its three generation-guarded fields) |
//! | evidence | `evidence` |
//! | adapter detail | `imp`, `list_factory`, `item`, `results`, `history`, `accessibility` |
//!
//! See `docs/workflow-readability-matrix.md`, row `WFR-SEARCH-REPLACE`.

mod accessibility;
mod evidence;
mod execution;
mod history;
pub(crate) mod journal;
// Private implementation module required by gtk-rs: imp.rs owns template
// children, state, and trait impls; this file exposes the public widget API.
mod imp;
pub mod item;
mod list_factory;
// Public because the GTK-free policy benchmarks in `benches/benchmarks.rs`
// address these pure types directly; nothing else outside this workflow does.
pub mod policy;
mod replace_execution;
mod results;
mod retirement;
#[cfg(feature = "test-utils")]
mod test_policy;

#[cfg(feature = "test-utils")]
pub use accessibility::apply_search_result_row_accessibility_for_test;
// Internal typed evidence surface: `evidence()` is callable in-crate by
// `ui/automation.rs`, and only the external widget harness needs to name the
// type. Re-exporting it unconditionally would widen this crate's default public
// API for an internal readability goal.
#[cfg(feature = "test-utils")]
pub use evidence::SearchPanelEvidence;
pub(crate) use journal::own_undo_journal_payload;
#[cfg(feature = "test-utils")]
pub use retirement::{SearchRetirementOwnership, SearchRetirementSliceObservation};
#[cfg(feature = "test-utils")]
pub use test_policy::SearchPanelTestPolicy;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::content_search::Replacement;
use crate::services::content_search::ReplaceUndoBackup;
use crate::services::notifications::NotificationSeverity;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{self, gio};

use self::item::SearchResultItem;

pub(crate) type GuardedReplaceUndoBackup =
    crate::ui::plain_disposal::DisposalOwned<ReplaceUndoBackup>;
pub(crate) type GuardedReplacements = crate::ui::plain_disposal::DisposalOwned<Vec<Replacement>>;

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
    /// A non-empty search worker has started after the input debounce.
    Started,
    /// The worker is still running and has visited this many files so far.
    Progress { files_searched: usize },
    /// The previous worker was cancelled by a newer query or panel close.
    Cancelled { files_searched: usize },
    /// The worker finished and this is the final visited-file count.
    Done { files_searched: usize },
}

/// Grouped GTK state for one file section in the hierarchical results list.
///
/// The search panel keeps the file-header item together with its child store so
/// result streaming and list-factory lookups share one named bundle.
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
    ///
    /// Closing cancels pending search intent and detaches the visible
    /// generation. That generation is still released over later GTK turns by
    /// `retirement`, so readiness stays pending until the disposer drains.
    pub fn close(&self) {
        self.cancel_active_search();
        self.clear_results(false, false);
        self.refresh_accessibility_state();
    }

    /// Pre-fill the search entry with text (e.g., editor selection).
    pub fn set_query(&self, text: &str) {
        self.imp().search_entry.set_text(text);
    }

    /// Pre-fill the replacement entry without starting a Replace All preview.
    ///
    /// Applying replacements still requires the explicit preview and confirm
    /// steps. Revealing the options row that holds the entry is presentation,
    /// not an entry-point query write, so it is delegated.
    pub fn set_replace_query(&self, text: &str) {
        self.reveal_replacement_entry(text);
    }

    /// Activate the same Replace All preview step as the panel button.
    ///
    /// Replace stages 1 and 2: read the visible replacement text on the main
    /// thread, then delegate to `replace_execution::enter_preview_mode`.
    pub fn activate_replace_preview(&self) {
        let imp = self.imp();
        let text = imp.replace_entry.text().to_string();
        self.enter_preview_mode(&text);
    }

    /// Confirm the checked replacement preview rows through the normal callback.
    ///
    /// Replace stage 3, delegated to
    /// `replace_execution::begin_confirmed_replacement`. The two-step
    /// preview/apply split is a safety contract: only rows generated and checked
    /// by the current preview can be applied.
    pub fn activate_confirm_replacements(&self) {
        self.begin_confirmed_replacement();
    }

    /// Trigger the visible Undo Replacements affordance through the normal callback.
    ///
    /// Replace stage 5, delegated to `journal::hand_back_undo_backup`. The
    /// durable undo journal and its generation guards stay in `journal` and
    /// `services/content_search`.
    pub fn activate_undo_replacements(&self) {
        self.hand_back_undo_backup();
    }

    /// Update the workspace folders to search. Called when workspaces change.
    pub fn set_workspace_folders(&self, folders: Vec<PathBuf>) {
        self.imp()
            .runtime
            .workspace_folders
            .replace(Arc::from(folders));
        self.refresh_accessibility_state();
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
    pub(crate) fn connect_guarded_replace_all<F: Fn(GuardedReplacements) + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .replace_callback
            .replace(Some(Box::new(f)));
    }

    /// Register the raw-vector callback used by the external widget harness.
    #[cfg(feature = "test-utils")]
    pub fn connect_replace_all<F: Fn(Vec<Replacement>) + 'static>(&self, f: F) {
        self.connect_guarded_replace_all(move |replacements| {
            f(replacements.into_inner_for_current_install());
        });
    }

    /// Register a callback invoked when "Undo" is clicked with the backup to restore.
    pub(crate) fn connect_guarded_undo_all<
        F: Fn(std::sync::Arc<GuardedReplaceUndoBackup>) + 'static,
    >(
        &self,
        f: F,
    ) {
        self.imp()
            .callbacks
            .undo_callback
            .replace(Some(Box::new(f)));
    }

    /// Register the raw backup callback used by the external widget harness.
    #[cfg(feature = "test-utils")]
    pub fn connect_undo_all<F: Fn(std::sync::Arc<ReplaceUndoBackup>) + 'static>(&self, f: F) {
        self.connect_guarded_undo_all(move |backup| {
            f(std::sync::Arc::new((**backup).clone()));
        });
    }

    /// Register a callback pushing status messages to the window's status bar.
    ///
    /// Carries a severity like the sidebar's: a failed journal write is a warning.
    pub fn connect_message<F: Fn(&str, NotificationSeverity) + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .message_callback
            .replace(Some(Box::new(f)));
    }
}
