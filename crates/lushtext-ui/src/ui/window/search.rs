// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** for `WFR-SEARCH-REPLACE` — not a role.
//!
//! The window side of the workspace search panel: the toggle action and its
//! shortcut, selection pre-fill, result activation and F4 navigation, workspace
//! folder forwarding, focus save and restore, and the window-side entry points
//! that hand replace/preview/confirm/undo to the panel's own coordination roles.
//! Every one of those projects the workflow onto widgets or resolves a
//! window-side target.
//!
//! **One exception: the window side of Replace stage 4.** The `impl` block
//! that claims a confirmed run's transaction, admits it or lets it wait, and
//! starts it (`replace-all-confirm-disposal-lane` design D2, D3) orders the
//! window-owned parts of that stage: the unsaved-tab skip set, the status lane,
//! the worker spawn and its completion. The admission attempt and the wait
//! themselves are the panel's (`ui/search_panel/replace_admission.rs`), and
//! the matrix row records this split.
//!
//! **The one coordination job this module used to hold has moved out.** Search
//! progress tracking — arm a delayed visibility, publish bounded progress,
//! heartbeat the notification alive, retire it at the terminal — is a third
//! ordered stage sequence of this row and is now
//! `ui/window/search_progress_execution.rs` (`execution`, stage-order-qualified).
//!
//! Slot 7b recorded this classification because `WFR-SEARCH-REPLACE`'s matrix
//! cell had claimed its files were *"all under `ui/search_panel/**`"*, which was
//! false by 928 production lines.
//!
//! All methods are `impl LushtextWindow` called from `new()` and `constructed()`.

use crate::config::keys;
use crate::ui::accessibility::AnnouncementLane;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::search_panel::SearchProgressUpdate;
use crate::ui::search_panel::journal::UndoRestoreClaim;
use crate::ui::search_panel::policy::ReplaceWaitEnd;
use crate::ui::search_panel::replace_admission::{
    AdmissionAttempt, ParkedReplaceRun, ReplaceAdmission, ReplaceAdmissionResumption,
};
use crate::ui::status_bar::MessageKind;
use crate::ui::window::search_progress_execution::format_search_progress_message;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use gtk4::{self, gio, glib};
use lushtext_services::coordination::notifications::NotificationSeverity;
use lushtext_services::{
    filesystem::metadata as fs_metadata, persistence::json_store, search::content_search,
    search::saved_searches, search::search_history,
};
use std::collections::HashSet;
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::LushtextWindow;
use crate::ui::window::StagedSurface;

enum GuardedReplaceApplyResult {
    Applied {
        result: Box<lushtext_model::workspace::content_search::ReplaceResult>,
        backup: Option<crate::ui::search_panel::GuardedReplaceUndoBackup>,
        skip_report: Box<lushtext_model::workspace::content_search::ReplaceSkipReport>,
        /// Cancelled, with files the rollback could not restore.
        cancelled: bool,
    },
    Superseded,
    /// Cancelled, and every file it wrote was restored.
    Cancelled,
    Failed(content_search::ReplaceApplyError),
}

struct GuardedUndoResult {
    outcome: content_search::UndoReplaceOutcome,
    remaining_backup: Option<crate::ui::search_panel::GuardedReplaceUndoBackup>,
}

/// Delay that lets the in-editor Find revealer finish closing before workspace search opens.
///
/// The 260 ms value tracks the panel transition budget; too short can hand
/// focus over mid-animation, while much longer makes Ctrl+Shift+F feel laggy.
pub(super) const SEARCH_PANEL_TRANSITION_DELAY_MS: u64 = 260;

#[cfg(feature = "test-utils")]
static RELOAD_FACTS_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Configure an artificial Replace All reload metadata delay for freshness tests.
#[cfg(feature = "test-utils")]
pub fn set_replace_reload_facts_delay_for_test(delay_ms: u64) {
    RELOAD_FACTS_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Set up the search panel action, callbacks, and workspace folder forwarding.
pub fn setup_search_panel(window: &LushtextWindow) {
    let imp = window.imp();

    // --- Workspace folders: forward current folders and future changes ---
    let initial_folders = window.current_workspace_folder_paths();
    imp.search_panel.set_workspace_folders(initial_folders);

    // --- Result activation: open file at line ---
    // GTK signal closures may outlive the current window instance; weak refs
    // let callbacks no-op instead of keeping closed windows alive.
    let window_weak = window.downgrade();
    imp.search_panel.connect_open_file(move |path, line| {
        if let Some(window) = window_weak.upgrade() {
            open_file_at_line(&window, path, line);
        }
    });

    // --- Skipped Files row: Open and Replace in Editor (design D7) ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_replace_in_editor(move |path| {
        if let Some(window) = window_weak.upgrade() {
            window.replace_in_editor(path);
        }
    });

    // --- F4/Shift+F4 navigation: open file at line (same as activation) ---
    let window_weak = window.downgrade();
    imp.search_panel
        .connect_navigate_to_match(move |path, line| {
            if let Some(window) = window_weak.upgrade() {
                open_file_at_line(&window, path, line);
            }
        });

    // --- Search progress: notification store + 500ms delay + navigation action update ---
    let window_weak = window.downgrade();
    imp.search_panel
        .search_entry()
        .connect_search_changed(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.prepare_search_progress_tracking();
            }
        });

    {
        let window_weak = window.downgrade();
        imp.search_panel.connect_search_progress(move |update| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };

            match update {
                SearchProgressUpdate::Started => {
                    window.prepare_search_progress_tracking();
                    window.announce_workflow_update(
                        AnnouncementLane::DebouncedResults,
                        "workspace-search-started",
                        "Workspace search started",
                    );
                }
                SearchProgressUpdate::Cancelled { files_searched } => {
                    let was_visible = window.imp().search_progress.visible.get();
                    window.finish_search_progress_tracking();
                    if was_visible {
                        window.announce_workflow_update(
                            AnnouncementLane::StatusUpdate,
                            "workspace-search-cancelled",
                            &format!("Workspace search cancelled after {files_searched} files"),
                        );
                    }
                    window.update_search_navigation_actions();
                }
                SearchProgressUpdate::Done { files_searched } => {
                    window.finish_search_progress_tracking();
                    let matches = window.imp().search_panel.navigation_match_count();
                    let files = window.imp().search_panel.result_file_count();
                    let message = if matches == 0 {
                        format!("Workspace search complete: no results in {files_searched} files")
                    } else {
                        format!("Workspace search complete: {matches} results in {files} files")
                    };
                    window.announce_workflow_update(
                        AnnouncementLane::StatusUpdate,
                        "workspace-search-complete",
                        &message,
                    );
                    window.update_search_navigation_actions();
                }
                SearchProgressUpdate::Progress { files_searched } => {
                    // Only show progress after the 500ms delay has elapsed
                    // and while the search panel is still visible.
                    if !window.imp().search_progress.visible.get()
                        || !window.imp().search_panel_revealer.reveals_child()
                    {
                        return;
                    }

                    let message = format_search_progress_message(files_searched);
                    window.update_search_progress_message(&message);
                }
            }
        });
    }

    // --- Close request: hide panel + restore focus ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_close_requested(move || {
        if let Some(window) = window_weak.upgrade() {
            window.close_search_panel();
        }
    });

    // --- Status messages from search panel (e.g., "Search saved as '...'" ) ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_message(move |text, severity| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        window.publish_status_message(text, severity);
        // A warning or error from this panel is a recovery-relevant alert, not
        // chatter: an undo journal that failed to persist is the case that
        // exists today, and a screen-reader user must not have to notice the
        // status lane changed colour to learn it. Informational messages such
        // as "Search saved as ..." stay silent.
        if matches!(
            severity,
            NotificationSeverity::Warning | NotificationSeverity::Error
        ) {
            window.announce_workflow_update(
                AnnouncementLane::Alert,
                "workspace-search-alert",
                text,
            );
        }
    });

    // --- Replace All: claim, then admit or wait, then start (design D2) ---
    let window_weak = window.downgrade();
    imp.search_panel
        .connect_guarded_replace_all(move |replacements| {
            if let Some(window) = window_weak.upgrade() {
                window.claim_replace_run(replacements);
            }
        });
    let window_weak = window.downgrade();
    imp.search_panel
        .connect_replace_run_resumed(move |resumption| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            match resumption {
                ReplaceAdmissionResumption::Resume(run) => {
                    window.admit_or_wait_replace_run(run, AdmissionAttempt::Resumed);
                }
                ReplaceAdmissionResumption::Ended(end) => window.end_replace_wait(end),
            }
        });

    // --- Undo All: restore files, status bar message ---
    let window_weak = window.downgrade();
    imp.search_panel.connect_guarded_undo_all(move |backup| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };

        // The panel owns the transaction gate, the disposal reservation, and the
        // affordance each refusal must put back, so this stage delegates the
        // whole claim rather than re-reading and re-mutating panel state here.
        let undo_reservation = match window.imp().search_panel.begin_undo_restore() {
            UndoRestoreClaim::Claimed(reservation) => reservation,
            UndoRestoreClaim::TransactionBusy => return,
            UndoRestoreClaim::CapacityDeferred => {
                window.publish_status_message(
                    "Undo deferred while memory pressure clears; try again shortly",
                    MessageKind::Warning,
                );
                return;
            }
        };

        let mut open_canonical_identities = HashSet::new();
        for index in 0..window.imp().tab_view.n_pages() {
            let page = window.imp().tab_view.nth_page(index);
            if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
                && let Some(path) = editor.file_path()
            {
                open_canonical_identities.insert(editor.canonical_file_path().unwrap_or(path));
            }
        }

        spawn_blocking_then(
            window,
            move || {
                let mut outcome = content_search::undo_replacements_for_open_identities(
                    &backup,
                    &open_canonical_identities,
                );
                let remaining = std::mem::take(&mut outcome.remaining_backup);
                let remaining_backup = if remaining.is_empty() {
                    drop(undo_reservation);
                    None
                } else {
                    Some(crate::ui::search_panel::own_undo_journal_payload(
                        undo_reservation,
                        remaining,
                    ))
                };
                GuardedUndoResult {
                    outcome,
                    remaining_backup,
                }
            },
            move |window, guarded| {
                let GuardedUndoResult {
                    outcome,
                    remaining_backup,
                } = guarded;
                let restored_paths = outcome
                    .restored_open_paths
                    .iter()
                    .cloned()
                    .collect::<HashSet<_>>();
                if !restored_paths.is_empty() {
                    reload_affected_tabs(&window, &restored_paths);
                }

                if let Some(remaining_backup) = remaining_backup {
                    let remaining = remaining_backup.len();
                    let skipped = outcome.skipped_count;
                    let failed = outcome.failed_count;
                    let message = if outcome.restored_count() > 0 {
                        format!(
                            "Reverted {} files; {remaining} files still need attention",
                            outcome.restored_count()
                        )
                    } else if skipped > 0 && failed == 0 {
                        format!(
                            "Undo skipped {skipped} files changed after Replace All; backup kept"
                        )
                    } else {
                        format!("Undo could not restore {remaining} files; backup kept")
                    };
                    window.publish_status_message(&message, MessageKind::Warning);
                    window
                        .imp()
                        .search_panel
                        .finish_undo_restore(Some(remaining_backup));
                } else {
                    let message = format!("Reverted {} files", outcome.restored_count());
                    window.publish_status_message(&message, MessageKind::Info);
                    window.announce_workflow_update(
                        AnnouncementLane::StatusUpdate,
                        "replace-all-undo-complete",
                        &message,
                    );
                    window.imp().search_panel.finish_undo_restore(None);
                }
            },
        );
    });

    // --- Restore panel visibility from GSettings ---
    let panel_visible = imp.settings.boolean(keys::SEARCH_PANEL_VISIBLE);
    if panel_visible {
        imp.search_panel_revealer.set_reveal_child(true);
        // The actions that require a visible panel follow it, as they do when
        // the panel opens (`setup_actions` ran in an earlier stage and left
        // them disabled).
        window.set_search_panel_actions_enabled(true);
        // Don't grab focus on startup — let session restore handle focus.
    }

    // --- Load search history from disk ---
    let data_dir = json_store::data_dir();
    let data_dir_saved = data_dir.clone();
    spawn_blocking_then(
        window.clone(),
        move || search_history::load_recovering(&data_dir),
        |window, load| {
            for diagnostic in &load.diagnostics {
                tracing::warn!("{}", diagnostic.summary());
            }
            window.imp().search_panel.set_search_history(load.value);
        },
    );

    // --- Load saved searches from disk (parallel to history) ---
    spawn_blocking_then(
        window.clone(),
        move || saved_searches::load_recovering(&data_dir_saved),
        |window, load| {
            for diagnostic in &load.diagnostics {
                tracing::warn!("{}", diagnostic.summary());
            }
            if !load.diagnostics.is_empty() {
                window.publish_status_message(
                    "Saved searches needed recovery; unsupported metadata was preserved",
                    MessageKind::Warning,
                );
            }
            window.imp().search_panel.set_saved_searches(load.value);
        },
    );
}

impl LushtextWindow {
    /// Toggle the search panel visibility. Handles open, re-invocation, and pre-fill.
    /// If the in-editor Find bar is open, closes it first with animation, then
    /// opens the search panel after the animation completes (260ms delay).
    pub fn toggle_search_panel(&self) {
        let imp = self.imp();
        let revealer = &imp.search_panel_revealer;

        if revealer.reveals_child() {
            self.close_search_panel();
            return;
        }

        // If the in-editor Find bar is visible, close it first with animation.
        if let Some(editor) = self.active_editor()
            && editor.is_search_visible()
        {
            editor.hide_search();
            self.after_search_panel_transition(|window| {
                window.open_search_panel();
            });
            return;
        }

        self.open_search_panel();
    }

    /// Internal helper: open the search panel with pre-fill and focus.
    fn open_search_panel(&self) {
        let imp = self.imp();

        // Save focus before the panel steals it.
        let weak = glib::WeakRef::new();
        if let Some(focused) = gtk4::prelude::GtkWindowExt::focus(self) {
            weak.set(Some(&focused));
        }
        imp.search_saved_focus.replace(Some(weak));

        // Pre-fill: if active editor has selected text, use it.
        if let Some(editor) = self.active_editor()
            && let Some(selected) =
                crate::ui::editor_page::search_prefill_text(editor.buffer().upcast_ref())
        {
            imp.search_panel.set_query(&selected);
        }

        imp.search_panel_revealer.set_reveal_child(true);
        imp.search_panel.open();
        self.set_search_panel_actions_enabled(true);
        crate::ui::settings_writes::set_boolean(keys::SEARCH_PANEL_VISIBLE, true);
        self.update_search_navigation_actions();
    }

    /// Close the search panel and restore focus.
    /// No-ops if the command palette is open — Escape should close the topmost
    /// overlay first (command palette), not the search panel underneath it.
    pub fn close_search_panel(&self) {
        let imp = self.imp();
        if imp.palette_revealer.reveals_child() {
            return; // Command palette is the topmost overlay — let it handle Escape.
        }
        imp.search_panel.close();
        imp.search_panel_revealer.set_reveal_child(false);
        self.set_search_panel_actions_enabled(false);
        self.finish_search_progress_tracking();
        crate::ui::settings_writes::set_boolean(keys::SEARCH_PANEL_VISIBLE, false);
        self.update_search_navigation_actions();
        self.restore_search_saved_focus();
    }

    /// Restore focus saved before the search panel was opened.
    fn restore_search_saved_focus(&self) {
        // The saved widget or the editor fallback may be staged content.
        self.ensure_composed(StagedSurface::Content);
        let saved = self.imp().search_saved_focus.take();
        let target = saved.as_ref().and_then(glib::WeakRef::upgrade).or_else(|| {
            self.active_editor()
                .map(|e| e.source_view().clone().upcast::<gtk4::Widget>())
        });

        match target {
            Some(widget) => {
                widget.grab_focus();
            }
            None => {
                gtk4::prelude::GtkWindowExt::set_focus(self, gtk4::Widget::NONE);
            }
        }
    }

    /// Enable actions that require the visible workspace-search panel.
    pub(super) fn set_search_panel_actions_enabled(&self, enabled: bool) {
        for action_name in [
            "set-search-panel-query",
            "set-search-panel-replace-query",
            "preview-search-panel-replacements",
            "confirm-search-panel-replacements",
            "undo-search-panel-replacements",
        ] {
            if let Some(action) = self.lookup_action(action_name)
                && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
            {
                simple.set_enabled(enabled);
            }
        }
    }

    /// Set workspace-search text through the visible search panel.
    pub(super) fn set_search_panel_query(&self, query: &str) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.set_query(query);
    }

    /// Set workspace-search replacement text through the visible search panel.
    pub(super) fn set_search_panel_replace_query(&self, text: &str) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.set_replace_query(text);
    }

    /// Build the Replace All preview through the visible search panel workflow.
    pub(super) fn preview_search_panel_replacements(&self) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.activate_replace_preview();
    }

    /// Confirm checked Replace All preview rows through the visible panel workflow.
    pub(super) fn confirm_search_panel_replacements(&self) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.activate_confirm_replacements();
    }

    /// Undo the last Replace All through the visible panel workflow.
    pub(super) fn undo_search_panel_replacements(&self) {
        if !self.imp().search_panel_revealer.reveals_child() {
            return;
        }
        self.imp().search_panel.activate_undo_replacements();
    }

    pub(super) fn after_search_panel_transition<F: FnOnce(&LushtextWindow) + 'static>(
        &self,
        callback: F,
    ) {
        let window_weak = self.downgrade();
        let callback = std::cell::RefCell::new(Some(callback));
        // Schedule on GTK's main loop so focus changes happen after the panel
        // animation frame, without blocking input while the delay elapses.
        glib::timeout_add_local_once(
            Duration::from_millis(SEARCH_PANEL_TRANSITION_DELAY_MS),
            move || {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                if let Some(callback) = callback.borrow_mut().take() {
                    callback(&window);
                }
            },
        );
    }
}

/// Replace stage 4 on the window side (`replace-all-confirm-disposal-lane`
/// design D2, D3): claim the transaction's reserved generation, then admit the
/// run or let it wait for disposal capacity, then start it. A wait resumes
/// through the panel's `replace_run_resumed` callback, into
/// [`LushtextWindow::admit_or_wait_replace_run`] again or
/// [`LushtextWindow::end_replace_wait`].
impl LushtextWindow {
    /// Claim: take the transaction's reserved journal generation.
    fn claim_replace_run(&self, replacements: crate::ui::search_panel::GuardedReplacements) {
        let Some(freshness) = self.imp().search_panel.take_replace_transaction() else {
            self.publish_status_message(
                "Replace All selection no longer owns the active transaction",
                MessageKind::Warning,
            );
            return;
        };
        self.admit_or_wait_replace_run(
            ParkedReplaceRun {
                replacements,
                freshness,
            },
            AdmissionAttempt::First,
        );
    }

    /// Admit or wait: the same steps on the first attempt and on every
    /// resumption. The skip set is recomputed from the current tabs, and the
    /// all-unsaved report comes before any wait. The attempt itself, epoch
    /// then reservation, is the panel's (`replace_admission`).
    fn admit_or_wait_replace_run(&self, run: ParkedReplaceRun, attempt: AdmissionAttempt) {
        let imp = self.imp();
        // Only a resumed attempt finds the run open: the first one opens it at
        // its first refusal, or in `start_replace_run`.
        let resumed = attempt == AdmissionAttempt::Resumed;
        debug_assert_eq!(resumed, imp.search_panel.replace_run_in_flight());
        let (skip_paths, open_canonical_identities) = replace_skip_sets(self);
        if !run
            .replacements
            .replacements
            .iter()
            .any(|replacement| !skip_paths.contains(&replacement.path))
        {
            // Every file is open with unsaved changes or an active save:
            // nothing runs, and the report says which files and why (5.5a).
            let report = lushtext_model::workspace::content_search::ReplaceSkipReport::unsaved_only(
                &run.replacements.replacements,
            );
            let mut message = String::from(
                "No replacements to apply (all files have unsaved changes or active saves)",
            );
            if let Some(clause) = crate::ui::search_panel::policy::skip_completion_clause(&report) {
                message.push_str(&clause);
            }
            if resumed {
                self.stop_replace_wait_keepalive();
            }
            self.publish_status_message(&message, MessageKind::Warning);
            imp.search_panel.finish_replace_run(Some(report));
            imp.search_panel.finish_replace_transaction();
            drop(run);
            if resumed {
                self.close_if_requested_after_replace_run();
            }
            return;
        }

        match imp.search_panel.try_admit_replace_run(run) {
            ReplaceAdmission::Admitted {
                run,
                undo_reservation,
            } => {
                self.start_replace_run(
                    run,
                    skip_paths,
                    open_canonical_identities,
                    undo_reservation,
                );
            }
            // Refused: the run waits as its first phase, holding the transaction.
            ReplaceAdmission::Refused(refused) => {
                if !resumed {
                    imp.search_panel.begin_replace_run();
                    self.show_replace_wait();
                }
                imp.search_panel.defer_replace_run(refused, attempt);
            }
        }
    }

    /// A wait for disposal capacity ended before the run started: nothing
    /// was written, and the earlier Undo and skip report stay (design D4).
    fn end_replace_wait(&self, end: ReplaceWaitEnd) {
        let imp = self.imp();
        self.stop_replace_wait_keepalive();
        imp.search_panel.end_deferred_replace_run();
        match end {
            ReplaceWaitEnd::Cancelled => {
                self.publish_status_message(REPLACE_WAIT_CANCELLED, MessageKind::Info);
                self.announce_workflow_update(
                    AnnouncementLane::StatusUpdate,
                    "replace-all-cancelled",
                    REPLACE_WAIT_CANCELLED,
                );
            }
            ReplaceWaitEnd::Superseded => {
                self.publish_status_message(REPLACE_SUPERSEDED, MessageKind::Warning);
            }
        }
        self.close_if_requested_after_replace_run();
    }

    /// Start: the run is admitted. Supersede every window's earlier Undo,
    /// open the run or reuse the one its wait opened, give the earlier skip
    /// report way, and stream the files on a worker.
    fn start_replace_run(
        &self,
        run: ParkedReplaceRun,
        skip_paths: HashSet<std::path::PathBuf>,
        open_canonical_identities: HashSet<std::path::PathBuf>,
        undo_reservation: crate::ui::plain_disposal::DisposalReservation,
    ) {
        let imp = self.imp();
        let ParkedReplaceRun {
            replacements,
            freshness,
        } = run;
        let total_replacements = replacements.len();
        let data_dir = json_store::data_dir();
        imp.search_panel.supersede_prior_undo_for_replace();
        let undo_generation = freshness.expected();
        let control = imp
            .search_panel
            .replace_run_control()
            .unwrap_or_else(|| imp.search_panel.begin_replace_run());
        imp.search_panel.clear_previous_skip_report();
        self.start_replace_progress_tracking();
        let cancel = std::sync::Arc::clone(&control.cancel);
        let progress = std::sync::Arc::clone(&control.progress);
        spawn_blocking_then(
            self.clone(),
            move || {
                let run = replacements.into_inner_on_worker();
                match content_search::apply_request_if_current(
                    content_search::ReplaceRunRequest {
                        replacements: &run.replacements,
                        skip_paths: &skip_paths,
                        open_canonical_identities: &open_canonical_identities,
                        ineligible: &run.ineligible,
                        cancel: &cancel,
                        progress: Some(&progress),
                        journal_data_dir: &data_dir,
                    },
                    &freshness,
                ) {
                    Ok(Some(mut outcome)) => {
                        let skip_report = std::mem::take(&mut outcome.skip_report);
                        let cancelled = outcome.cancelled;
                        let (result, backup) = outcome.into_parts();
                        let backup = if backup.is_empty() {
                            drop(undo_reservation);
                            None
                        } else {
                            Some(crate::ui::search_panel::own_undo_journal_payload(
                                undo_reservation,
                                backup,
                            ))
                        };
                        GuardedReplaceApplyResult::Applied {
                            result: Box::new(result),
                            backup,
                            skip_report: Box::new(skip_report),
                            cancelled,
                        }
                    }
                    Ok(None) => GuardedReplaceApplyResult::Superseded,
                    Err(error) if cancel.load(std::sync::atomic::Ordering::Acquire) => {
                        drop(error);
                        GuardedReplaceApplyResult::Cancelled
                    }
                    Err(error) => GuardedReplaceApplyResult::Failed(error),
                }
            },
            // The completion terminal is one span in the measurement build
            // (`stream-replace-all` task 9.1, bar (a): `B_SETTLE_US`).
            move |window, result| {
                crate::ui::responsiveness_probe::span(
                    crate::ui::responsiveness_probe::REPLACE_COMPLETION_SPAN,
                    move || {
                        let imp = window.imp();
                        window.stop_replace_progress_tracking();
                        match result {
                            GuardedReplaceApplyResult::Applied {
                                result: replace_result,
                                backup,
                                skip_report,
                                cancelled,
                            } => {
                                let replace_result = *replace_result;
                                imp.search_panel.record_replace_apply_counts(
                                    crate::ui::search_panel::policy::ReplaceApplyCounts {
                                        replaced: u32::try_from(replace_result.replaced_count)
                                            .unwrap_or(u32::MAX),
                                        skipped: u32::try_from(replace_result.skipped_count)
                                            .unwrap_or(u32::MAX),
                                        errors: u32::try_from(replace_result.error_count)
                                            .unwrap_or(u32::MAX),
                                    },
                                );
                                let mut msg = if cancelled {
                                    format!(
                                        "Replace All cancelled; {} files could not be restored",
                                        replace_result.files_affected,
                                    )
                                } else {
                                    format!(
                                        "Replaced {} of {} matches in {} files",
                                        replace_result.replaced_count,
                                        total_replacements,
                                        replace_result.files_affected,
                                    )
                                };
                                if let Some(clause) =
                                    crate::ui::search_panel::policy::skip_completion_clause(
                                        &skip_report,
                                    )
                                {
                                    msg.push_str(&clause);
                                }
                                if replace_result.error_count > 0 {
                                    msg.push_str(&format!(
                                        " ({} errors)",
                                        replace_result.error_count
                                    ));
                                }
                                let kind = if replace_result.error_count == 0 && !cancelled {
                                    MessageKind::Info
                                } else {
                                    MessageKind::Warning
                                };
                                window.publish_status_message(&msg, kind);
                                if matches!(kind, MessageKind::Info) {
                                    window.announce_workflow_update(
                                        AnnouncementLane::StatusUpdate,
                                        "replace-all-complete",
                                        &msg,
                                    );
                                }
                                imp.search_panel.finish_replace_run(Some(*skip_report));

                                if let Some(backup) = backup {
                                    if imp.search_panel.publish_undo_journal_for_generation(
                                        backup,
                                        undo_generation,
                                    ) {
                                        window.announce_workflow_update(
                                            AnnouncementLane::StatusUpdate,
                                            "replace-all-undo-available",
                                            "Undo is available for the last Replace All",
                                        );
                                    }
                                } else {
                                    imp.search_panel
                                        .clear_undo_backup_for_generation(undo_generation);
                                }

                                // Reload affected open tabs to show updated content.
                                let affected_paths = replace_result
                                    .affected_open_paths
                                    .into_iter()
                                    .collect::<HashSet<_>>();
                                reload_affected_tabs(&window, &affected_paths);
                            }
                            GuardedReplaceApplyResult::Superseded => {
                                imp.search_panel.finish_replace_run(None);
                                window.publish_status_message(
                                    REPLACE_SUPERSEDED,
                                    MessageKind::Warning,
                                );
                            }
                            GuardedReplaceApplyResult::Cancelled => {
                                imp.search_panel.finish_replace_run(None);
                                imp.search_panel
                                    .clear_undo_backup_for_generation(undo_generation);
                                window.publish_status_message(
                                    "Replace All cancelled; every file was restored",
                                    MessageKind::Info,
                                );
                                window.announce_workflow_update(
                                    AnnouncementLane::StatusUpdate,
                                    "replace-all-cancelled",
                                    "Replace All cancelled; every file was restored",
                                );
                            }
                            GuardedReplaceApplyResult::Failed(e) => {
                                imp.search_panel.finish_replace_run(None);
                                window.publish_status_message(
                                    &format!("Replace failed: {e}"),
                                    MessageKind::Error,
                                );
                            }
                        }
                        imp.search_panel.finish_replace_transaction();
                        window.close_if_requested_after_replace_run();
                    },
                );
            },
        );
    }
}

/// The status line after a wait for memory was cancelled (design D4).
const REPLACE_WAIT_CANCELLED: &str = "Replace All cancelled; no file was changed";

/// The status line when a run was superseded, at its worker or while it
/// waited, before any file changed.
const REPLACE_SUPERSEDED: &str = "Replace All was superseded before any files changed";

/// The skip set of one admission attempt: files open with unsaved edits or an
/// in-flight save, and every open file's canonical identity. The replacement
/// service also takes the same per-path advisory lock as editor saves, so a
/// save that starts after this snapshot cannot race the final replacement
/// rename for that file.
fn replace_skip_sets(
    window: &LushtextWindow,
) -> (HashSet<std::path::PathBuf>, HashSet<std::path::PathBuf>) {
    let mut skip_paths = HashSet::new();
    let mut open_canonical_identities = HashSet::new();
    let tab_view = &window.imp().tab_view;
    for i in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(i);
        // Tab pages store generic GTK widgets, so the cast gives access to
        // editor-specific save and path state only for editor tabs.
        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
            && let Some(path) = editor.file_path()
        {
            open_canonical_identities
                .insert(editor.canonical_file_path().unwrap_or_else(|| path.clone()));
            if editor.is_modified() || editor.is_saving() {
                skip_paths.insert(path);
            }
        }
    }
    (skip_paths, open_canonical_identities)
}

/// Open a file at a specific line number. Shared by result activation (double-click/Enter),
/// F4/Shift+F4 navigation and Open and Replace in Editor. Handles evicted tabs,
/// already-loaded buffers, and newly-opened tabs where the async load is still in progress.
pub(super) fn open_file_at_line(window: &LushtextWindow, path: &std::path::Path, line: u32) {
    window.open_document(path);

    let tab_view = &window.imp().tab_view;
    if let Some(page) = tab_view.selected_page()
        && let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
    {
        if editor.is_evicted() {
            let line_0 = line.saturating_sub(1);
            editor.set_restore_position(line_0, 0, line_0.saturating_sub(3));
            window.reload_if_evicted();
        } else if editor.buffer().char_count() > 0 {
            scroll_editor_to_line(editor, line);
        } else {
            let line_0 = line.saturating_sub(1);
            editor.set_restore_position(line_0, 0, line_0.saturating_sub(3));
        }
        // Explicitly move focus to the editor (AC#4: "focus moves to the editor").
        // Without this, GTK4's focus traversal may leave focus on the search panel
        // or sidebar after tab switch.
        window.ensure_composed(StagedSurface::Content);
        editor.source_view().grab_focus();
    }
}

/// Reload open tabs whose file was affected by replace/undo.
/// Updates `last_known_mtime` to suppress the file monitor's "File Has Changed" bar,
/// then reloads the file content via `load_file_async`.
fn reload_affected_tabs(window: &LushtextWindow, affected_paths: &HashSet<std::path::PathBuf>) {
    let tab_view = &window.imp().tab_view;
    for i in 0..tab_view.n_pages() {
        let page = tab_view.nth_page(i);
        if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>()
            && let Some(path) = editor.file_path()
            && affected_paths.contains(&path)
            && !editor.has_unsaved_changes()
            && !editor.is_saving()
        {
            let editor_weak = editor.downgrade();
            let path_for_facts = path.clone();
            spawn_blocking_then(
                path,
                move || {
                    delay_replace_reload_facts_for_test();
                    fs_metadata::file_facts(&path_for_facts)
                        .ok()
                        .and_then(|facts| facts.modified_at_secs)
                },
                move |path, modified_at_secs| {
                    let Some(editor) = editor_weak.upgrade() else {
                        return;
                    };
                    if editor.file_path().as_deref() != Some(path.as_path())
                        || editor.has_unsaved_changes()
                        || editor.is_saving()
                    {
                        return;
                    }
                    // Update mtime before reload to suppress the file monitor's
                    // "changed" warning for writes made by Replace All/Undo.
                    editor.imp().monitor.last_known_mtime.set(modified_at_secs);
                    editor.reload_preserving_position(&path);
                },
            );
        }
    }
}

#[cfg(feature = "test-utils")]
fn delay_replace_reload_facts_for_test() {
    let delay_ms = RELOAD_FACTS_DELAY_MS.load(Ordering::Acquire);
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}

#[cfg(not(feature = "test-utils"))]
fn delay_replace_reload_facts_for_test() {}

/// Exercise Replace All tab reload freshness from the external widget harness.
#[cfg(feature = "test-utils")]
impl LushtextWindow {
    pub fn reload_affected_tabs_for_test(&self, affected_paths: &HashSet<std::path::PathBuf>) {
        reload_affected_tabs(self, affected_paths);
    }
}

/// Scroll an editor to a specific line number (1-based).
/// If the editor is still loading, this is a best-effort attempt — the content
/// may not be available yet, in which case the cursor ends up at line 0.
fn scroll_editor_to_line(editor: &LushtextEditorPage, line: u32) {
    let line_0 = line.saturating_sub(1);
    if editor.navigate_protected_line(u64::from(line_0)) {
        return;
    }
    let buffer = editor.buffer();
    let line_index = i32::try_from(line_0).unwrap_or(i32::MAX);
    let iter = buffer
        .iter_at_line(line_index)
        .unwrap_or_else(|| buffer.end_iter());
    buffer.place_cursor(&iter);
    editor.scroll_to_document_iter(&iter, 0.0, true, 0.0, 0.3);
}
