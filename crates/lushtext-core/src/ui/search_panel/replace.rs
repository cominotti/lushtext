// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace-preview and undo state for the search panel widget.
//!
//! These methods stay on the widget because they mutate GTK state and preview
//! models, but isolating them here keeps the runtime search loop separate from
//! replace/undo behavior.

use crate::model::content_search::{
    ReplacePreviewBudget, ReplacePreviewSkipReason,
    generate_replacement_preview_with_budget_and_cancel,
};
use crate::services::content_search::ReplaceUndoBackup;
use crate::services::{json_store, search_backup};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use std::sync::Arc;
#[cfg(feature = "test-utils")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use super::LushtextSearchPanel;

/// Latest plain-Rust preview request retained by the panel's single-flight worker.
pub(super) struct ReplacePreviewRequest {
    search_matches: std::sync::Arc<Vec<crate::model::content_search::SearchMatch>>,
    query_spec: crate::model::content_search::SearchQuerySpec,
    replacement_text: String,
    generation: u32,
}

struct PersistedUndoStartupLoad {
    active_backup: Option<ReplaceUndoBackup>,
}

impl LushtextSearchPanel {
    /// Show the undo button (called after a successful replace).
    pub fn show_undo_button(&self) {
        let imp = self.imp();
        // Undo is time-sensitive recovery UI, so make the containing options
        // row visible instead of leaving the newly-shown button collapsed.
        imp.more_toggle.set_active(true);
        imp.undo_button.set_visible(true);
        self.refresh_accessibility_state();
    }

    /// Hide the undo button.
    pub fn hide_undo_button(&self) {
        self.imp().undo_button.set_visible(false);
        self.refresh_accessibility_state();
    }

    /// Store undo backup and persist it as the current retryable journal.
    pub fn set_undo_backup(&self, backup: ReplaceUndoBackup) {
        let backup = Arc::new(backup);
        let (generation, retired) = self.set_undo_backup_in_memory(Arc::clone(&backup));
        self.save_undo_backup_on_disk(backup, retired, generation);
    }

    /// Store undo backup after the replace service already wrote per-file journal entries.
    pub fn set_persisted_undo_backup(&self, backup: ReplaceUndoBackup) {
        let (_, retired) = self.set_undo_backup_in_memory(Arc::new(backup));
        self.retire_undo_backup_off_main(retired);
    }

    fn set_undo_backup_in_memory(
        &self,
        backup: Arc<ReplaceUndoBackup>,
    ) -> (u32, Option<Arc<ReplaceUndoBackup>>) {
        let previous = self
            .imp()
            .preview
            .undo_backup_generation
            .fetch_add(1, Ordering::AcqRel);
        let retired = self.imp().preview.undo_backup.replace(Some(backup));
        self.refresh_accessibility_state();
        (previous.wrapping_add(1), retired)
    }

    /// Restore a crash-interrupted active journal, or clean inactive stale state.
    pub(crate) fn load_persisted_undo_backup(&self) {
        let data_dir = json_store::data_dir();
        let generation = self
            .imp()
            .preview
            .undo_backup_generation
            .load(Ordering::Acquire);
        let generation_counter = self.imp().preview.undo_backup_generation.clone();
        let callback_generation_counter = generation_counter.clone();
        gtk_lush_tasks::spawn_blocking_then(
            self.clone(),
            move || {
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if generation_counter.load(Ordering::Acquire) != generation {
                    return Ok(PersistedUndoStartupLoad {
                        active_backup: None,
                    });
                }
                let recovery = search_backup::load_recovering(&data_dir);
                if recovery.active {
                    return Ok(PersistedUndoStartupLoad {
                        active_backup: Some(recovery.backup),
                    });
                }
                let mut diagnostics = recovery.diagnostics;
                diagnostics.extend(search_backup::cleanup_stale(&data_dir).diagnostics);
                report_startup_diagnostics(&diagnostics);
                Ok::<PersistedUndoStartupLoad, anyhow::Error>(PersistedUndoStartupLoad {
                    active_backup: None,
                })
            },
            move |panel, result| match result {
                Ok(load) => {
                    if callback_generation_counter.load(Ordering::Acquire) != generation {
                        return;
                    }
                    if let Some(backup) = load.active_backup {
                        panel.set_persisted_undo_backup(backup);
                        panel.show_undo_button();
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load persisted replace backup: {e}");
                }
            },
        );
    }

    /// Clear undo backup and hide the undo button.
    pub(crate) fn clear_undo_backup(&self) {
        let generation = self
            .imp()
            .preview
            .undo_backup_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let retired = self.imp().preview.undo_backup.replace(None);
        self.hide_undo_button();
        self.delete_undo_backup_on_disk(generation, retired);
        self.refresh_accessibility_state();
    }

    fn save_undo_backup_on_disk(
        &self,
        backup: Arc<ReplaceUndoBackup>,
        retired: Option<Arc<ReplaceUndoBackup>>,
        generation: u32,
    ) {
        let data_dir = json_store::data_dir();
        let generation_counter = self.imp().preview.undo_backup_generation.clone();
        spawn_blocking_then(
            self.clone(),
            move || {
                let _retired = retired;
                delay_undo_backup_disk_for_test();
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if generation_counter.load(Ordering::Acquire) != generation {
                    return Ok(());
                }
                search_backup::save(&data_dir, backup.as_ref())
            },
            move |_panel, result| {
                if let Err(e) = result {
                    tracing::error!("Failed to persist replace backup: {e}");
                }
            },
        );
    }

    fn delete_undo_backup_on_disk(&self, generation: u32, retired: Option<Arc<ReplaceUndoBackup>>) {
        let data_dir = json_store::data_dir();
        let generation_counter = self.imp().preview.undo_backup_generation.clone();
        spawn_blocking_then(
            self.clone(),
            move || {
                let _retired = retired;
                delay_undo_backup_disk_for_test();
                let _disk_guard = search_backup::acquire_journal_guard()?;
                if generation_counter.load(Ordering::Acquire) != generation {
                    return Ok(());
                }
                search_backup::delete(&data_dir)
            },
            move |_panel, result| {
                if let Err(e) = result {
                    tracing::warn!("Failed to delete replace backup after undo: {e}");
                }
            },
        );
    }

    fn retire_undo_backup_off_main(&self, retired: Option<Arc<ReplaceUndoBackup>>) {
        let Some(retired) = retired else {
            return;
        };
        // The map can own the full 64 MiB undo window. Hand its final reference
        // to the blocking pool so replacing a persisted backup never releases
        // that payload on GTK.
        spawn_blocking_then(self.clone(), move || drop(retired), |_panel, ()| {});
    }

    /// Whether the panel is in preview mode.
    #[must_use]
    pub fn is_preview_mode(&self) -> bool {
        self.imp().preview.preview_mode.get()
    }

    /// Enter preview mode: generate replacement previews and switch the results
    /// list to show before/after with checkboxes.
    pub fn enter_preview_mode(&self, replacement_text: &str) {
        let imp = self.imp();
        let Some(search_matches) = self.accepted_search_matches() else {
            return;
        };

        let query_spec = self.current_query_spec();
        let generation = self.advance_preview_generation();
        imp.preview.preview_pending.set(true);
        imp.preview.preview_mode.set(false);
        imp.preview.preview_outcome.replace(None);
        imp.preview.checked_match_ids.borrow_mut().clear();
        imp.replace_all_button.set_label("Preparing Preview…");
        imp.replace_all_button.set_sensitive(false);
        self.refresh_accessibility_state();

        let request = ReplacePreviewRequest {
            search_matches,
            query_spec,
            replacement_text: replacement_text.to_string(),
            generation,
        };
        self.enqueue_preview_request(request);
    }

    fn enqueue_preview_request(&self, request: ReplacePreviewRequest) {
        let imp = self.imp();
        if imp.preview.preview_worker_running.get() {
            imp.preview.queued_preview_request.replace(Some(request));
            return;
        }
        self.spawn_preview_request(request);
    }

    fn spawn_preview_request(&self, request: ReplacePreviewRequest) {
        let imp = self.imp();
        imp.preview.preview_worker_running.set(true);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        imp.preview
            .preview_cancel_token
            .replace(Some(cancel.clone()));
        let expected_query_spec = request.query_spec.clone();
        let generation = request.generation;
        spawn_blocking_then(
            self.clone(),
            move || {
                delay_replace_preview_for_test();
                generate_replacement_preview_with_budget_and_cancel(
                    &request.search_matches,
                    &request.query_spec.query,
                    &request.replacement_text,
                    &request.query_spec.options,
                    replace_preview_budget(),
                    || cancel.load(std::sync::atomic::Ordering::Relaxed),
                )
            },
            move |panel, outcome| {
                let imp = panel.imp();
                imp.preview.preview_worker_running.set(false);
                imp.preview.preview_cancel_token.replace(None);
                if imp.preview.preview_generation.get() == generation
                    && imp.preview.preview_pending.get()
                    && panel.current_query_spec() == expected_query_spec
                {
                    imp.preview.preview_pending.set(false);

                    let checked = outcome
                        .replacements
                        .iter()
                        .map(|replacement| replacement.match_id)
                        .collect();
                    imp.preview.checked_match_ids.replace(checked);
                    let total = outcome.replacements.len();
                    imp.preview.preview_outcome.replace(Some(outcome));
                    imp.preview.preview_mode.set(true);

                    panel.refresh_preview_summary();
                    imp.replace_all_button.set_sensitive(total > 0);

                    panel.refresh_results_display();
                    panel.refresh_accessibility_state();
                }

                if let Some(queued) = imp.preview.queued_preview_request.take() {
                    panel.spawn_preview_request(queued);
                }
            },
        );
    }

    /// Exit preview mode: clear preview state and restore normal result display.
    pub fn exit_preview_mode(&self) {
        let imp = self.imp();
        self.advance_preview_generation();
        imp.preview.preview_pending.set(false);
        imp.preview.queued_preview_request.replace(None);
        imp.preview.preview_mode.set(false);
        self.retire_preview_state(
            imp.preview.preview_outcome.take(),
            std::mem::take(&mut *imp.preview.checked_match_ids.borrow_mut()),
        );
        imp.replace_all_button.set_label("Replace All");
        self.restore_search_summary();
        self.update_replace_button_sensitivity();
        self.refresh_results_display();
        self.refresh_accessibility_state();
    }

    /// Update the "Replace All" / "Confirm Replace" button sensitivity.
    pub fn update_replace_button_sensitivity(&self) {
        let imp = self.imp();
        if imp.preview.preview_pending.get() {
            imp.replace_all_button.set_sensitive(false);
        } else if imp.preview.preview_mode.get() {
            imp.replace_all_button
                .set_sensitive(!imp.preview.checked_match_ids.borrow().is_empty());
        } else {
            // Empty replacement text is allowed (deletes matches).
            imp.replace_all_button
                .set_sensitive(imp.runtime.total_matches.get() > 0);
        }
        self.refresh_accessibility_state();
    }

    /// Cancel any pending or visible replace preview after search state changes.
    ///
    /// Advancing the generation prevents late background preview results from
    /// restoring stale replacements.
    pub(crate) fn invalidate_replace_preview_request(&self) {
        let imp = self.imp();
        if !imp.preview.preview_pending.get() && !imp.preview.preview_mode.get() {
            return;
        }
        self.advance_preview_generation();
        imp.preview.preview_pending.set(false);
        imp.preview.queued_preview_request.replace(None);
        imp.preview.preview_mode.set(false);
        self.retire_preview_state(
            imp.preview.preview_outcome.take(),
            std::mem::take(&mut *imp.preview.checked_match_ids.borrow_mut()),
        );
        imp.replace_all_button.set_label("Replace All");
        self.restore_search_summary();
        self.refresh_results_display();
        self.refresh_accessibility_state();
    }

    pub(super) fn advance_preview_generation(&self) -> u32 {
        let imp = self.imp();
        if let Some(cancel) = imp.preview.preview_cancel_token.take() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let generation = imp.preview.preview_generation.get().wrapping_add(1);
        imp.preview.preview_generation.set(generation);
        generation
    }

    /// Refresh visible and accessible confirmation feedback from accepted state.
    pub(crate) fn refresh_preview_summary(&self) {
        let imp = self.imp();
        let checked = imp.preview.checked_match_ids.borrow().len();
        let outcome = imp.preview.preview_outcome.borrow();
        let Some(outcome) = outcome.as_ref() else {
            return;
        };
        let generated = outcome.replacements.len();
        let omitted = outcome.omitted_eligible;
        let truncated = outcome
            .skipped
            .count(ReplacePreviewSkipReason::TruncatedSource);
        let stale_ranges = outcome
            .skipped
            .count(ReplacePreviewSkipReason::RegexRangeMismatch);
        let skipped = truncated.saturating_add(stale_ranges);
        imp.replace_all_button
            .set_label(&format!("Replace {checked} checked"));
        let summary = if generated == 0 {
            format!(
                "No eligible replacements; {omitted} omitted, {truncated} truncated, {stale_ranges} stale ranges"
            )
        } else if omitted > 0 || skipped > 0 {
            format!(
                "{generated} previewed, {checked} checked, {omitted} omitted, {truncated} truncated, {stale_ranges} stale ranges"
            )
        } else {
            format!("{generated} previewed, {checked} checked")
        };
        imp.count_label.set_text(&summary);
        crate::ui::accessibility::set_labelled_description(
            &*imp.replace_all_button,
            &format!("Apply {checked} checked replacements"),
            &summary,
        );
        self.refresh_accessibility_state();
    }

    fn restore_search_summary(&self) {
        let imp = self.imp();
        let total = imp.runtime.total_matches.get();
        let files = imp.runtime.total_files.get();
        if total == 0 {
            imp.count_label.set_text("No results found");
        } else if imp.runtime.result_capped.get() {
            imp.count_label
                .set_text("10,000+ results (truncated) — narrow your search");
            imp.count_label.add_css_class("warning");
        } else {
            imp.count_label.remove_css_class("warning");
            imp.count_label
                .set_text(&format!("{total} results in {files} files"));
        }
        crate::ui::accessibility::set_labelled_description(
            &*imp.replace_all_button,
            "Replace all matches",
            "Preview replacements before applying them",
        );
    }
}

fn report_startup_diagnostics(
    diagnostics: &[crate::services::recovery_metadata::RecoveryDiagnostic],
) {
    const DETAIL_LIMIT: usize = 8;
    for diagnostic in diagnostics.iter().take(DETAIL_LIMIT) {
        tracing::warn!(
            "Replace undo backup startup diagnostic: {}",
            diagnostic.summary()
        );
    }
    if diagnostics.len() > DETAIL_LIMIT {
        tracing::warn!(
            "Replace undo backup startup produced {} additional diagnostics",
            diagnostics.len() - DETAIL_LIMIT
        );
    }
}

#[cfg(feature = "test-utils")]
static UNDO_BACKUP_DISK_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static REPLACE_PREVIEW_DELAY_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static REPLACE_PREVIEW_MAX_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "test-utils")]
static REPLACE_PREVIEW_MAX_BYTES: AtomicU64 = AtomicU64::new(0);

/// Configure an artificial Replace All undo persistence delay for widget tests.
#[cfg(feature = "test-utils")]
pub fn set_undo_backup_disk_delay_for_test(delay_ms: u64) {
    UNDO_BACKUP_DISK_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Configure an artificial Replace preview generation delay for widget tests.
#[cfg(feature = "test-utils")]
pub fn set_replace_preview_delay_for_test(delay_ms: u64) {
    REPLACE_PREVIEW_DELAY_MS.store(delay_ms, Ordering::Release);
}

/// Override Replace Preview limits for state-extreme widget tests; zero restores production.
#[cfg(feature = "test-utils")]
pub fn set_replace_preview_budget_for_test(max_rows: u64, max_bytes: u64) {
    REPLACE_PREVIEW_MAX_ROWS.store(max_rows, Ordering::Release);
    REPLACE_PREVIEW_MAX_BYTES.store(max_bytes, Ordering::Release);
}

fn replace_preview_budget() -> ReplacePreviewBudget {
    #[cfg(feature = "test-utils")]
    {
        let max_rows = REPLACE_PREVIEW_MAX_ROWS.load(Ordering::Acquire);
        let max_bytes = REPLACE_PREVIEW_MAX_BYTES.load(Ordering::Acquire);
        if max_rows > 0 || max_bytes > 0 {
            return ReplacePreviewBudget {
                max_rows: usize::try_from(max_rows).unwrap_or(usize::MAX),
                max_bytes: usize::try_from(max_bytes).unwrap_or(usize::MAX),
            };
        }
    }
    ReplacePreviewBudget::default()
}

fn delay_undo_backup_disk_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = UNDO_BACKUP_DISK_DELAY_MS.load(Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
}

fn delay_replace_preview_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = REPLACE_PREVIEW_DELAY_MS.load(Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
}
