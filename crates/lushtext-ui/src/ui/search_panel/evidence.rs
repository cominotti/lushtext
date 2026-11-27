// SPDX-License-Identifier: GPL-3.0-or-later

//! The search-panel workflow's observable state, in one typed value.
//!
//! [`SearchPanelEvidence`] is the single source of truth for observers of this
//! workflow. Widget tests read it instead of calling per-field `*_for_test`
//! getters, and the read-only D-Bus automation snapshot projects its documented
//! fields from it rather than re-deriving the same state from widgets.
//!
//! Reading evidence is pure observation: it never advances a generation, arms a
//! timer, drains a queue, or requires the workflow to be in a particular stage.
//! The scalar accessors below are the primitives the surface composes, and they
//! stay here so the workflow's observation lives in one place.
//!
//! Reentrancy constraint: [`LushtextSearchPanel::evidence`] takes shared
//! `RefCell` borrows of the search flight, the deferred query, the queued
//! preview request, the parked Replace All run, the retirement observations,
//! and the undo journal. It must
//! therefore be called from workflow code that is not already holding a
//! `borrow_mut()` on any of those cells, or the borrow would panic. Every
//! current caller observes from outside a mutation — widget tests and the
//! read-only automation snapshot — so no live path can reach that state.
//!
//! Reading evidence must not require the workflow to be in a particular stage,
//! and **a disposed widget is a stage**: a teardown test that disposes the
//! widget and then asks what the workflow recorded is a legitimate observation
//! point. GTK4 clears template children in `dispose()`, before Rust's `Drop`,
//! so every field derived from a `TemplateChild` here is read through
//! `try_get()` and gives an honest empty/false answer when the child is gone,
//! rather than panicking. This hazard is created by consolidation: scattered
//! per-field getters each read one narrow thing, while one surface makes every
//! field reachable from every observation point.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use lushtext_model::values::{
    ReplacePreviewGeneration, ReplaceTransactionGeneration, UndoBackupGeneration,
};
use lushtext_model::workspace::content_search::SearchQuerySpec;

use super::LushtextSearchPanel;
use super::skipped_files::SkippedFilesModel;

/// One consistent read of the workspace search and Replace All preview workflow.
///
/// Field groups follow the workflow's stages: the query the user typed, the
/// streaming search and its bounded retirement, the Replace All preview and its
/// worker lane, the undo journal, and the history/navigation projections.
#[expect(
    clippy::struct_excessive_bools,
    reason = "read-only workflow evidence DTO"
)]
pub struct SearchPanelEvidence {
    // --- query surface ---
    /// Current query text.
    pub query: String,
    /// Whether case-sensitive matching is active.
    pub case_sensitive: bool,
    /// Whether regular-expression matching is active.
    pub regex_enabled: bool,
    /// Whether whole-word matching is active.
    pub whole_word_enabled: bool,
    /// Whether `.gitignore` filtering is active.
    pub gitignore_enabled: bool,
    /// Whether dotfiles are searched for the current query.
    pub hidden_enabled: bool,
    /// Current glob filter, when the user set one.
    pub glob_filter: Option<String>,
    /// Current replacement text, which may legitimately be empty.
    pub replace_query: String,

    // --- streaming search execution ---
    /// Whether a worker or result retirement is still running.
    pub searching: bool,
    /// Matches accumulated for the current search.
    pub match_count: u32,
    /// Files with at least one match in the current search.
    pub file_count: u32,
    /// Whether the retained-byte budget cut the current search
    /// (`result_capped` keeps its name for automation clients).
    pub result_capped: bool,
    /// Matches retained and shown (the same as `match_count`).
    pub retained_matches: u32,
    /// Matches found, including those past the budget; equals
    /// `retained_matches` while the budget has not cut.
    pub matches_found: usize,
    /// Events received by the turn that revealed the first results.
    pub reveal_turn_events: usize,
    /// The last turn that applied matches: how many, and how many child-store
    /// and root-store splices applied them (one per touched file plus at most
    /// one root splice).
    pub last_match_batch: (usize, usize, usize),
    /// Active controller/walker groups; the single-flight invariant keeps this at 0 or 1.
    pub active_worker_groups: u8,
    /// High-water active-group count observed by this panel.
    pub active_worker_groups_high_water: u8,
    /// Retained latest superseding search requests.
    pub pending_search_requests: usize,

    // --- bounded result retirement ---
    /// Whether detached result generations still await bounded disposal.
    pub retirement_pending: bool,
    /// Detached generations currently queued behind the disposer.
    pub retirement_backlog: usize,
    /// Queue ceiling, including the slot reserved for the close/clear escape path.
    pub retirement_backlog_limit: usize,
    /// High-water detached generations retained by the disposer.
    pub retirement_generations_high_water: usize,
    /// High-water references released by any one bounded retirement turn.
    pub retirement_rows_per_slice_high_water: usize,
    /// Whether a latest query is deferred by retirement backpressure.
    pub deferred_search_pending: bool,
    /// Zero-copy accepted-snapshot handoffs into Replace Preview.
    pub shared_snapshot_handoffs: u64,
    /// Prohibited whole-result deep clones; a regression probe that must stay zero.
    pub whole_result_clones: u64,
    /// Before/after ownership observed for every bounded retirement turn.
    #[cfg(feature = "test-utils")]
    pub retirement_observations: Vec<super::retirement::SearchRetirementSliceObservation>,
    /// Every receive turn that dispatched at least one event: how many it
    /// dispatched and how long it took, in µs (`power-of-two-thresholds`
    /// task 5.4 times `MAX_SEARCH_EVENTS_PER_TICK` with it).
    #[cfg(feature = "test-utils")]
    pub event_turn_observations: Vec<(usize, i64)>,

    // --- Replace All preview ---
    /// Whether the result list renders preview rows with checkboxes.
    pub replace_preview_mode: bool,
    /// Whether preview generation, selection, or preview retirement is pending.
    pub replace_preview_pending: bool,
    /// Preview rows currently held in memory.
    pub replace_preview_count: u32,
    /// Preview rows the user selected for apply.
    pub checked_replacement_count: u32,
    /// Eligible matches omitted by the preview resource budget.
    pub omitted_replacement_count: u32,
    /// Truncated or invalid matches skipped by the preview.
    pub skipped_replacement_count: u32,
    /// Whether this panel currently owns one active preview worker.
    pub preview_worker_running: bool,
    /// Retained latest superseding preview requests.
    pub queued_preview_requests: usize,
    /// Document-sized preview payloads handed to worker retirement.
    pub preview_retirement_jobs: u64,
    /// Checked-row partitions executed on the worker lane.
    pub preview_selection_jobs: u64,
    /// Superseded preview payloads still awaiting final off-main destruction.
    pub preview_retirement_pending: usize,
    /// Generation identifying the current preview attempt.
    pub preview_generation: ReplacePreviewGeneration,
    /// Whether the preview owns its single disposal-capacity retry source.
    ///
    /// Test-gated for the same reason as
    /// [`SearchPanelEvidence::undo_capacity_retry_pending`]: the underlying
    /// wakeup only reports armed state under the `test-utils` feature.
    #[cfg(feature = "test-utils")]
    pub preview_capacity_retry_pending: bool,

    // --- durable apply transaction ---
    /// Whether one Replace All apply or undo transaction owns journal mutation.
    ///
    /// Distinct from [`SearchPanelEvidence::replace_preview_pending`], which
    /// folds this together with preview and retirement work. Only this field
    /// answers "is the apply transaction claimed".
    pub replace_transaction_pending: bool,
    /// Journal generation the claimed transaction reserved and has not handed off.
    pub replace_transaction_generation: Option<ReplaceTransactionGeneration>,
    /// Counts published by the most recent durable Replace All apply.
    pub last_apply_counts: Option<super::policy::ReplaceApplyCounts>,
    /// Whether a durable Replace All run is in flight (design D8), including
    /// a confirmed run still waiting for disposal capacity to start.
    pub replace_in_flight: bool,
    /// Whether that run is still waiting for the disposal capacity its undo
    /// reservation needs (`replace-all-confirm-disposal-lane`).
    pub replace_admission_deferred: bool,
    /// Whether that run was asked to stop and roll back.
    pub replace_cancel_requested: bool,
    /// The run's progress: files finished, files in the run, target bytes
    /// written, and whether it is rolling back; `None` with no run.
    pub replace_progress: Option<ReplaceProgressEvidence>,
    /// Files the last run skipped, per reason in `ReplaceSkipReason::ALL`
    /// order (design D7); all zero when nothing was skipped.
    pub replace_skip_counts: [usize; 11],
    /// Files the last run replaced only partly.
    pub replace_skip_partly_replaced: usize,
    /// Files the skip list does not name because of its byte bound.
    pub replace_skip_not_listed: usize,
    /// Whether Show Skipped Files is offered.
    pub skipped_files_offered: bool,
    /// Row objects the open Skipped Files dialog's lazy model has built.
    pub skipped_files_item_requests: Option<u64>,
    /// Rows the open Skipped Files dialog lists: its staged first rows while
    /// its first publish grows, then every listed file (the final length).
    pub skipped_files_listed: Option<u32>,

    // --- undo journal ---
    /// Whether a Replace All undo backup is available.
    pub has_undo_backup: bool,
    /// Generation invalidating stale journal installs, clears, saves, and deletes.
    pub undo_backup_generation: UndoBackupGeneration,
    /// Files the installed undo journal can restore.
    pub undo_backup_entry_count: usize,
    /// Disposal weight the installed undo journal retains.
    pub undo_backup_retained_bytes: u64,
    /// Undo-journal disk save, delete, and recovery jobs dispatched to workers.
    pub journal_disk_jobs: u64,
    /// Dispatched undo-journal disk jobs whose GTK completion has not run yet.
    pub journal_disk_jobs_in_flight: usize,
    /// Whether persisted Undo owns its single capacity-retry source, or waits
    /// for this panel's claimed transaction to end before it loads.
    ///
    /// Test-gated because the underlying wakeup only reports armed state under
    /// the `test-utils` feature; production readiness uses
    /// [`SearchPanelEvidence::replace_preview_pending`] instead.
    #[cfg(feature = "test-utils")]
    pub undo_capacity_retry_pending: bool,
    /// The in-memory journal itself, for identity comparison by tests.
    #[cfg(feature = "test-utils")]
    undo_backup: Option<std::sync::Arc<super::GuardedReplaceUndoBackup>>,

    // --- history and navigation ---
    /// Recent search-history entries loaded into the panel.
    pub history_count: u32,
    /// Named saved searches loaded into the panel.
    pub saved_search_count: u32,
    /// Flat match targets available for keyboard navigation.
    pub navigation_match_count: u32,
    /// Current flat navigation index, if a match has been selected.
    pub current_navigation_match_index: Option<u32>,
}

/// Progress of the Replace All run in flight, from its worker's atomics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplaceProgressEvidence {
    /// Files finished, whatever their outcome.
    pub files_done: usize,
    /// Files in the run.
    pub files_total: usize,
    /// Target bytes written so far.
    pub bytes_written: u64,
    /// Whether the run is restoring the files it wrote.
    pub rolling_back: bool,
}

#[cfg(feature = "test-utils")]
impl SearchPanelEvidence {
    /// Compare the guarded in-memory journal without exposing its owner type.
    #[must_use]
    pub fn undo_backup_matches(
        &self,
        expected: &lushtext_services::search::content_search::ReplaceUndoBackup,
    ) -> bool {
        self.undo_backup
            .as_ref()
            .is_some_and(|backup| &***backup == expected)
    }
}

impl LushtextSearchPanel {
    /// Read this workflow's observable state as one consistent value.
    ///
    /// Every field the panel's retired `*_for_test` inspection functions
    /// exposed is readable here. Reading does not mutate workflow state.
    #[must_use]
    pub fn evidence(&self) -> SearchPanelEvidence {
        let imp = self.imp();
        let journal = &imp.journal;
        let exec = &imp.replace_execution;
        // Derive everything the installed journal contributes inside one short
        // block, then let the borrow end. The struct literal below calls two
        // dozen accessors, and holding a `RefCell` borrow across them is what
        // would make this module's reentrancy constraint a matter of care rather
        // than of structure: nothing in that literal can re-borrow a cell this
        // function is no longer holding.
        let (has_undo_backup, undo_backup_entry_count, undo_backup_retained_bytes) = {
            let installed_journal = journal.undo_backup.borrow();
            let backup = installed_journal.as_backup();
            (
                backup.is_some(),
                backup.map_or(0, |backup| backup.len()),
                backup
                    .and_then(|backup| backup.reservation_weight())
                    .unwrap_or(0),
            )
        };
        let replace_progress = self.replace_run_progress().map(|progress| {
            let (files_done, files_total, bytes_written, phase) = progress.snapshot();
            ReplaceProgressEvidence {
                files_done,
                files_total,
                bytes_written,
                rolling_back: phase
                    == lushtext_services::search::content_search::ReplacePhase::RollingBack,
            }
        });
        let skip_report = self.last_skip_report();
        let replace_skip_counts = skip_report.as_ref().map_or([0; 11], |report| {
            lushtext_model::workspace::content_search::ReplaceSkipReason::ALL
                .map(|reason| report.count(reason))
        });
        let skipped_files_model = self.skipped_files_model();
        let skipped_files_item_requests = skipped_files_model
            .as_ref()
            .map(SkippedFilesModel::item_requests);
        let skipped_files_listed = skipped_files_model.as_ref().map(SkippedFilesModel::listed);
        // The identity clone is a separate short borrow for the same reason.
        #[cfg(feature = "test-utils")]
        let undo_backup = journal.undo_backup();
        let replace_transaction_generation = match journal.transaction.get() {
            super::journal::ReplaceTransactionPhase::Idle => None,
            super::journal::ReplaceTransactionPhase::Claimed {
                reserved_generation,
                ..
            } => reserved_generation,
        };
        #[cfg(feature = "test-utils")]
        let undo_capacity_retry_pending = journal.undo_capacity_wakeup.is_armed()
            || matches!(
                journal.transaction.get(),
                super::journal::ReplaceTransactionPhase::Claimed {
                    resume_persisted_load: true,
                    ..
                }
            );
        SearchPanelEvidence {
            query: self.query(),
            case_sensitive: self.case_sensitive(),
            regex_enabled: self.regex_enabled(),
            whole_word_enabled: self.whole_word_enabled(),
            gitignore_enabled: self.gitignore_enabled(),
            hidden_enabled: self.hidden_enabled(),
            glob_filter: self.glob_filter(),
            replace_query: self.replace_query(),

            searching: self.is_searching(),
            match_count: self.total_matches(),
            file_count: self.total_files(),
            result_capped: self.result_capped(),
            retained_matches: self.total_matches(),
            matches_found: imp
                .runtime
                .matches_found
                .get()
                .max(self.total_matches() as usize),
            reveal_turn_events: imp.runtime.reveal_turn_events.get(),
            last_match_batch: imp.runtime.last_match_batch.get(),
            active_worker_groups: imp.runtime.active_worker_groups.get(),
            active_worker_groups_high_water: imp.runtime.active_worker_groups_high_water.get(),
            pending_search_requests: imp.runtime.flight.borrow().snapshot().pending,

            retirement_pending: self.result_retirement_pending(),
            retirement_backlog: self.result_retirement_backlog(),
            retirement_backlog_limit: super::retirement::MAX_SEARCH_RETIREMENT_GENERATIONS + 1,
            retirement_generations_high_water: imp.retirement.generations_high_water.get(),
            retirement_rows_per_slice_high_water: imp.retirement.rows_per_slice_high_water.get(),
            deferred_search_pending: self.deferred_search_pending(),
            shared_snapshot_handoffs: imp.runtime.shared_snapshot_handoffs.get(),
            whole_result_clones: imp.runtime.whole_result_clones.get(),
            #[cfg(feature = "test-utils")]
            retirement_observations: imp.retirement.observations.borrow().clone(),
            #[cfg(feature = "test-utils")]
            event_turn_observations: imp.runtime.event_turn_observations.borrow().clone(),

            replace_preview_mode: self.replace_preview_mode(),
            replace_preview_pending: self.replace_preview_pending(),
            replace_preview_count: self.replace_preview_count(),
            checked_replacement_count: self.checked_replacement_count(),
            omitted_replacement_count: self.omitted_replacement_count(),
            skipped_replacement_count: self.skipped_replacement_count(),
            preview_worker_running: exec.is_preview_worker_running(),
            queued_preview_requests: usize::from(exec.has_queued_request()),
            preview_retirement_jobs: exec.preview_retirement_jobs.get(),
            preview_selection_jobs: exec.preview_selection_jobs.get(),
            preview_retirement_pending: exec
                .preview_retirement_pending
                .load(std::sync::atomic::Ordering::Acquire),
            preview_generation: exec.preview_generation.get(),
            #[cfg(feature = "test-utils")]
            preview_capacity_retry_pending: exec.preview_capacity_wakeup.is_armed(),

            replace_transaction_pending: self.replace_transaction_claimed(),
            replace_transaction_generation,
            last_apply_counts: journal.last_apply_counts.get().into_option(),
            replace_in_flight: self.replace_run_in_flight(),
            replace_admission_deferred: self.replace_admission_deferred(),
            replace_cancel_requested: self.replace_cancel_requested(),
            replace_progress,
            replace_skip_counts,
            replace_skip_partly_replaced: skip_report
                .as_ref()
                .map_or(0, |report| report.partly_replaced()),
            replace_skip_not_listed: skip_report.as_ref().map_or(0, |report| report.not_listed()),
            skipped_files_offered: skip_report.is_some(),
            skipped_files_item_requests,
            skipped_files_listed,

            has_undo_backup,
            undo_backup_generation: UndoBackupGeneration::new(
                journal
                    .undo_backup_generation
                    .load(std::sync::atomic::Ordering::Acquire),
            ),
            undo_backup_entry_count,
            undo_backup_retained_bytes,
            journal_disk_jobs: journal.disk_jobs.get(),
            journal_disk_jobs_in_flight: journal.disk_jobs_in_flight.get(),
            #[cfg(feature = "test-utils")]
            undo_capacity_retry_pending,
            #[cfg(feature = "test-utils")]
            undo_backup,

            history_count: self.history_count(),
            saved_search_count: self.saved_search_count(),
            navigation_match_count: self.navigation_match_count(),
            current_navigation_match_index: self.current_navigation_match_index(),
        }
    }

    /// Get the current query text.
    ///
    /// Empty when the template child is gone, which is the disposed-widget case
    /// described in this module's reentrancy and observation notes.
    #[must_use]
    pub fn query(&self) -> String {
        self.imp()
            .search_entry
            .try_get()
            .map_or_default(|entry| entry.text().to_string())
    }

    /// Whether a workspace-search worker is in flight, without the result
    /// retirement [`Self::is_searching`] also counts: the window's search
    /// progress and Replace All waiting text yield to the walk alone.
    #[must_use]
    pub(crate) fn search_worker_running(&self) -> bool {
        self.imp().runtime.is_searching()
    }

    /// Return whether worker cancellation/search or result retirement is pending.
    #[must_use]
    pub fn is_searching(&self) -> bool {
        self.imp().runtime.is_searching() || self.result_retirement_pending()
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
        self.imp().runtime.is_result_capped()
    }

    /// Return whether the case-sensitive option is active.
    #[must_use]
    pub fn case_sensitive(&self) -> bool {
        self.imp()
            .case_toggle
            .try_get()
            .is_some_and(|toggle| toggle.is_active())
    }

    /// Return whether regular-expression search is active.
    #[must_use]
    pub fn regex_enabled(&self) -> bool {
        self.imp()
            .regex_toggle
            .try_get()
            .is_some_and(|toggle| toggle.is_active())
    }

    /// Return whether whole-word matching is active.
    #[must_use]
    pub fn whole_word_enabled(&self) -> bool {
        self.imp()
            .word_toggle
            .try_get()
            .is_some_and(|toggle| toggle.is_active())
    }

    /// Return whether .gitignore filtering is active.
    #[must_use]
    pub fn gitignore_enabled(&self) -> bool {
        self.imp()
            .gitignore_toggle
            .try_get()
            .is_some_and(|toggle| toggle.is_active())
    }

    /// Return whether dotfiles are searched for the current query.
    #[must_use]
    pub fn hidden_enabled(&self) -> bool {
        self.imp()
            .hidden_toggle
            .try_get()
            .is_some_and(|toggle| toggle.is_active())
    }

    /// Return the current glob filter text, if any.
    #[must_use]
    pub fn glob_filter(&self) -> Option<String> {
        let text = self.imp().glob_entry.try_get()?.text();
        (!text.is_empty()).then(|| text.to_string())
    }

    /// Return the current replacement text without applying it.
    #[must_use]
    pub fn replace_query(&self) -> String {
        self.imp()
            .replace_entry
            .try_get()
            .map_or_default(|entry| entry.text().to_string())
    }

    /// Return whether the result list is showing Replace All preview rows.
    #[must_use]
    pub fn replace_preview_mode(&self) -> bool {
        self.imp().replace_execution.is_preview_mode()
    }

    /// Return whether replacement preview generation, selection, or retirement is pending.
    #[must_use]
    pub fn replace_preview_pending(&self) -> bool {
        let exec = &self.imp().replace_execution;
        exec.is_preview_pending()
            || self.replace_transaction_claimed()
            || exec.is_preview_worker_running()
            || exec
                .preview_retirement_pending
                .load(std::sync::atomic::Ordering::Acquire)
                > 0
    }

    /// Return the number of replacement preview rows currently held in memory.
    #[must_use]
    pub fn replace_preview_count(&self) -> u32 {
        let count = match &*self.imp().replace_execution.preview.borrow() {
            super::replace_execution::PreviewPhase::Active { outcome, .. } => {
                outcome.replacements.len()
            }
            _ => 0,
        };
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    /// Return the number of replacement preview rows selected for apply.
    #[must_use]
    pub fn checked_replacement_count(&self) -> u32 {
        let count = match &*self.imp().replace_execution.preview.borrow() {
            super::replace_execution::PreviewPhase::Active {
                checked_match_ids, ..
            } => checked_match_ids.len(),
            _ => 0,
        };
        u32::try_from(count).unwrap_or(u32::MAX)
    }

    /// Return eligible matches omitted by the current preview resource budget.
    #[must_use]
    pub fn omitted_replacement_count(&self) -> u32 {
        let omitted = match &*self.imp().replace_execution.preview.borrow() {
            super::replace_execution::PreviewPhase::Active { outcome, .. } => {
                outcome.omitted_eligible
            }
            _ => 0,
        };
        u32::try_from(omitted).unwrap_or(u32::MAX)
    }

    /// Return source-truncated or invalid matches skipped by the current preview.
    #[must_use]
    pub fn skipped_replacement_count(&self) -> u32 {
        let skipped = match &*self.imp().replace_execution.preview.borrow() {
            super::replace_execution::PreviewPhase::Active { outcome, .. } => {
                outcome.skipped_source_count()
            }
            _ => 0,
        };
        u32::try_from(skipped).unwrap_or(u32::MAX)
    }

    /// Return whether a Replace All undo backup is currently available.
    #[must_use]
    pub fn has_undo_backup(&self) -> bool {
        self.imp().journal.undo_backup.borrow().is_installed()
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

    /// Return the number of file groups currently represented in search results.
    #[must_use]
    pub fn result_file_count(&self) -> u32 {
        self.imp().runtime.total_files.get()
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
            lushtext_model::workspace::content_search::ContentSearchOptions {
                case_sensitive: imp.case_toggle.is_active(),
                regex: imp.regex_toggle.is_active(),
                whole_word: imp.word_toggle.is_active(),
                gitignore: imp.gitignore_toggle.is_active(),
                hidden: imp.hidden_toggle.is_active(),
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
}
