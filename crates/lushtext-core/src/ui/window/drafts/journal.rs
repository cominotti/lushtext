// SPDX-License-Identifier: GPL-3.0-or-later

//! The draft manifest and bodies: the record startup recovery reads back.
//!
//! `journal` on slot 3a's reusable test — *does a later stage of the same
//! workflow restore from the record* — which the draft manifest and bodies pass
//! outright: startup recovery reads them and installs their content into the
//! user's buffers. Slot 3a reserved this name for exactly this workflow after
//! rejecting it for document save.
//!
//! Per slot 2b's definition, the record's **mutual-exclusion gate lives inside
//! the journal**, not in a separate `admission`: `mutation_inflight`,
//! `pending_deletes`, `delete_tombstones`, and the `DraftMutationOrder` epoch
//! allocator are all here, because they serialize this record's writes.
//!
//! **Orphan cleanup is here too, and that is a deliberate finding rather than a
//! default.** It looks like `retirement`, but `retirement` in this codebase means
//! the disposal lane's off-GTK destruction of an in-memory payload. Orphan cleanup
//! reloads *this* manifest under *this* record's write lock, is gated by *this*
//! record's authority, and merges its result back into *this* record. A reader
//! asking "what keeps the manifest consistent with the bodies on disk" looks
//! here, so `DraftCleanupContinuation`'s manifest offset lives with the journal it
//! protects.
//!
//! ## The deletion ordering this module must never lose
//!
//! The persisted manifest stays the durable retry marker **until the body is
//! gone**: the body is deleted first, and the manifest entry only if that
//! succeeded. A failed body deletion therefore leaves a fully recoverable
//! pre-delete state across unrelated manifest mutations and a process restart.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::draft::{
    DraftEntry, DraftManifestAuthority, PreloadedDraftRestore, StaleDraftPreservation,
};
use crate::services::notifications::NotificationSeverity;
use crate::services::{draft_service, editor_io, json_store};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::LushtextEditorPage;

use super::policy;
use super::policy::{
    OrphanCleanupFollowUp, grouped_orphan_cleanup_failure_message, orphan_cleanup_follow_up,
};
use super::seams::{DraftManifestFailure, OrphanCleanupUiResult, PendingPreservation};
use super::{
    automatic_draft_limit, delay_draft_delete_for_test, delay_draft_manifest_for_test,
    delay_orphan_cleanup_worker_for_test, fail_next_draft_delete_for_test,
    fail_next_draft_manifest_for_test, orphan_cleanup_followup_delay, orphan_cleanup_start_delay,
};
use crate::ui::window::LushtextWindow;

impl LushtextWindow {
    /// Accept one trusted manifest commit and reapply compact pending tombstones.
    pub(super) fn accept_draft_manifest_commit(
        &self,
        mut commit: draft_service::DraftManifestCommit,
    ) {
        let drafts = &self.imp().drafts;
        let order = drafts.mutation_order.borrow();
        let mut tombstones = drafts.delete_tombstones.borrow_mut();
        tombstones.retain(|_, intent| order.is_current(intent));
        commit
            .manifest
            .drafts
            .retain(|entry| !tombstones.contains_key(entry.draft_id.as_str()));
        drop(tombstones);
        drop(order);
        let became_trusted = !self.imp().drafts.manifest_authority.get().is_trusted()
            && commit.authority.is_trusted();
        self.imp().drafts.manifest_authority.set(commit.authority);
        *self.imp().drafts.manifest.borrow_mut() = commit.manifest;
        if became_trusted {
            self.schedule_orphan_cleanup(true);
        }
    }

    /// Revoke destructive cleanup immediately after a manifest command loses
    /// completeness or durable replacement eligibility.
    pub(super) fn reject_draft_manifest_authority(&self, authority: DraftManifestAuthority) {
        self.imp().drafts.manifest_authority.set(authority);
        self.imp().drafts.orphan_cleanup_pending_offset.set(None);
        self.imp().drafts.orphan_cleanup_timer_pending.set(false);
        let _ = self.imp().drafts.orphan_cleanup_timer.invalidate();
    }

    /// Adopt the outcome of one write-ahead registration.
    ///
    /// Both pipelines share this: a trusted commit is accepted whole, while an
    /// additive registration mirrors exactly what the service persisted (absent
    /// ids only) and leaves the journal untrusted. Returns whether the bodies of
    /// `registered` may now be written; on failure the caller must not write
    /// them.
    pub(super) fn apply_draft_registration(
        &self,
        result: std::result::Result<draft_service::DraftRegistration, DraftManifestFailure>,
        registered: Vec<DraftEntry>,
    ) -> std::result::Result<(), String> {
        match result {
            Ok(draft_service::DraftRegistration::Committed(commit)) => {
                self.accept_draft_manifest_commit(commit);
                Ok(())
            }
            Ok(draft_service::DraftRegistration::Additive { authority }) => {
                self.reject_draft_manifest_authority(authority);
                let mut manifest = self.imp().drafts.manifest.borrow_mut();
                for entry in registered {
                    manifest.insert_if_absent(entry);
                }
                Ok(())
            }
            Err(error) => {
                self.reject_draft_manifest_authority(error.authority);
                Err(error.detail)
            }
        }
    }

    /// The window's current manifest entry for `draft_id`, if any.
    pub(super) fn draft_manifest_entry(&self, draft_id: &str) -> Option<DraftEntry> {
        self.imp()
            .drafts
            .manifest
            .borrow()
            .find_by_id(draft_id)
            .cloned()
    }

    /// Whether `entry` is still exactly the manifest's entry for its id.
    pub(super) fn draft_manifest_entry_is_current(&self, entry: &DraftEntry) -> bool {
        self.imp()
            .drafts
            .manifest
            .borrow()
            .find_by_id(&entry.draft_id)
            == Some(entry)
    }

    /// Hold autosave of `draft_id` while one more restore ticket for it is outstanding.
    pub(super) fn hold_draft_restore(&self, draft_id: &str) {
        *self
            .imp()
            .drafts
            .restore_pending_ids
            .borrow_mut()
            .entry(draft_id.to_owned())
            .or_insert(0) += 1;
    }

    /// Release one [`Self::hold_draft_restore`] hold; the last release lifts it.
    pub(super) fn release_draft_restore(&self, draft_id: &str) {
        let mut pending = self.imp().drafts.restore_pending_ids.borrow_mut();
        if let Some(count) = pending.get_mut(draft_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                pending.remove(draft_id);
            }
        }
    }

    /// Whether a restore of `draft_id` is still queued or reading its body.
    pub(super) fn draft_restore_is_pending(&self, draft_id: &str) -> bool {
        self.imp()
            .drafts
            .restore_pending_ids
            .borrow()
            .contains_key(draft_id)
    }

    /// Adopt the draft records one startup journal read produced.
    ///
    /// The session-restore workflow's startup read produces the draft manifest,
    /// its authority, and the guarded preload graph in the same worker pass,
    /// because the session descriptors and the draft records have to agree. It
    /// hands them over through this one named operation rather than writing three
    /// `DraftState` fields from another workflow's file: the records are this
    /// workflow's, and a cross-workflow field reach is exactly what the readability
    /// convention exists to remove.
    pub(crate) fn adopt_startup_draft_records(
        &self,
        manifest: crate::model::draft::DraftManifest,
        authority: DraftManifestAuthority,
        preloaded: crate::ui::plain_disposal::DisposalOwned<HashMap<String, PreloadedDraftRestore>>,
    ) {
        *self.imp().drafts.manifest.borrow_mut() = manifest;
        self.imp().drafts.manifest_authority.set(authority);
        *self.imp().drafts.preloaded.borrow_mut() = preloaded;
    }

    /// Write all dirty drafts synchronously during window close.
    ///
    /// Regular autosave uses chunked snapshots plus background writes. This is the
    /// deliberate blocking variant, on the reasoning that the process is about to
    /// exit and the last recoverable buffer state matters more than the stall.
    ///
    /// **No production path currently reaches it.** Window close goes through
    /// `flush_dirty_drafts_async`; this entry point is exercised only by widget
    /// tests. Read it as an available synchronous variant, not as the live
    /// close-time path.
    ///
    /// # Errors
    ///
    /// Returns an error when any dirty draft file cannot be written or when
    /// the draft manifest cannot be updated after successful draft writes.
    pub fn flush_dirty_drafts(&self) -> Result<()> {
        if self.imp().drafts.mutation_inflight.get()
            || self.imp().drafts.orphan_cleanup_inflight.get()
        {
            anyhow::bail!("draft persistence is already in progress");
        }
        let tab_view = &self.imp().tab_view;
        let data_dir = json_store::data_dir();
        let now = editor_io::now_epoch_secs();
        let mut manifest_updates = Vec::new();
        let mut write_errors = Vec::new();
        let mut registration_errors = Vec::new();
        let discarded_draft_ids = self.imp().drafts.close_discard_ids.borrow().clone();

        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            // AdwTabPage exposes a generic GtkWidget. GObject's runtime downcast
            // checks for EditorPage before exposing editor-specific APIs.
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            // The same policy predicate the two admission collectors use, with
            // `require_draft_dirty = false` because close is the last pass and
            // there is none after it to catch a modified tab. Calling the owner
            // rather than restating the terms is what keeps the
            // `installation_incomplete` data-safety guard single-sourced: an added
            // term reaches this path, where a missed one is worst, automatically.
            if !policy::draft_candidate_is_eligible(
                editor.is_modified(),
                editor.draft_dirty(),
                editor.is_evicted(),
                editor.has_incomplete_load_installation(),
                false,
            ) {
                continue;
            }
            let Some(draft_id) = editor.draft_id() else {
                continue;
            };
            if discarded_draft_ids.contains(&draft_id) {
                continue;
            }
            // As in the asynchronous collectors: a body whose restore is still
            // pending must not be overwritten.
            if self.draft_restore_is_pending(&draft_id) {
                write_errors.push(format!("{draft_id}: its draft restore is still pending"));
                continue;
            }
            let buffer = editor.buffer();
            let text = match buffer_snapshot::snapshot_buffer_text_direct_budgeted(
                &buffer,
                automatic_draft_limit(),
            ) {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => text.into_direct_string(),
                buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                    Self::show_automatic_recovery_limit(editor);
                    write_errors.push(format!(
                        "{draft_id}: document exceeds the automatic recovery limit"
                    ));
                    continue;
                }
                buffer_snapshot::BufferSnapshotOutcome::Cancelled(_) => {
                    write_errors.push(format!("{draft_id}: snapshot was cancelled"));
                    continue;
                }
            };
            let original_path = editor.file_path();
            // Write-ahead registration, as in the asynchronous pipelines: a
            // body is never written for an id the persisted manifest lacks.
            let authority = self.imp().drafts.manifest_authority.get();
            let registered = self
                .imp()
                .drafts
                .manifest
                .borrow()
                .find_by_id(&draft_id)
                .is_some();
            if policy::draft_requires_registration(
                original_path.is_some(),
                registered,
                authority.is_trusted(),
            ) {
                let session = self.collect_session_for_draft_reconciliation();
                let entry = DraftEntry {
                    draft_id: draft_id.clone(),
                    original_path: original_path.clone(),
                    original_mtime_secs: original_path.as_deref().and_then(editor_io::mtime_secs),
                    saved_at_secs: now,
                };
                let result = draft_service::register_draft_entries(
                    &data_dir,
                    &session,
                    authority,
                    std::slice::from_ref(&entry),
                )
                .map_err(DraftManifestFailure::from);
                if let Err(detail) = self.apply_draft_registration(result, vec![entry]) {
                    registration_errors.push(format!("{draft_id}: {detail}"));
                    continue;
                }
            }
            if let Err(e) = draft_service::write_draft(&data_dir, &draft_id, &text) {
                tracing::error!("Failed to write draft on close: {e}");
                write_errors.push(format!("{draft_id}: {e}"));
                continue;
            }
            let mtime = original_path
                .as_ref()
                .and_then(|path| editor_io::mtime_secs(path));
            manifest_updates.push(DraftEntry {
                draft_id,
                original_path,
                original_mtime_secs: mtime,
                saved_at_secs: now,
            });
        }
        let had_manifest_updates = !manifest_updates.is_empty();
        if had_manifest_updates {
            let session = self.collect_session_for_draft_reconciliation();
            let authority = self.imp().drafts.manifest_authority.get();
            let commit =
                match draft_service::update_manifest(&data_dir, &session, authority, |manifest| {
                    for entry in manifest_updates {
                        manifest.upsert(entry);
                    }
                }) {
                    Ok(commit) => commit,
                    Err(error) => {
                        self.reject_draft_manifest_authority(error.authority());
                        return Err(anyhow::anyhow!(
                            "failed to save draft manifest on close: {error}"
                        ));
                    }
                };
            self.accept_draft_manifest_commit(commit);
        }
        if !registration_errors.is_empty() {
            return Err(anyhow::anyhow!(
                "failed to save draft manifest on close: {}",
                registration_errors.join("; ")
            ));
        }
        if !write_errors.is_empty() {
            return Err(anyhow::anyhow!(
                "failed to write {} drafts on close: {}",
                write_errors.len(),
                write_errors.join("; ")
            ));
        }
        self.clear_close_discard_drafts();
        Ok(())
    }

    /// Deferred orphan cleanup — runs after restore so startup stays responsive.
    ///
    /// Cleanup is skipped when startup recovery did not trust the manifest,
    /// preventing deletion based on unsafe metadata.
    pub(crate) fn schedule_orphan_cleanup(&self, cleanup_allowed: bool) {
        let drafts = &self.imp().drafts;
        drafts.orphan_cleanup_failure_streak.set(0);
        drafts.orphan_cleanup_pending_offset.set(None);
        drafts.orphan_cleanup_timer_pending.set(true);
        drafts.orphan_cleanup_timer.arm(
            self,
            orphan_cleanup_start_delay(),
            move |window, _| {
                window
                    .imp()
                    .drafts
                    .orphan_cleanup_timer_pending
                    .set(false);
                // Eager strings can be released after the ordinary restore window,
                // but compact lazy markers must survive slow file loads so they
                // cannot bypass the serialized admission queue.
                super::retirement::release_eager_preloads(&mut window.imp().drafts.preloaded.borrow_mut());
                if !cleanup_allowed {
                    tracing::warn!(
                        "Skipped draft orphan cleanup because startup recovery did not trust the draft manifest"
                    );
                    return;
                }
                window.run_orphan_cleanup_pass(0);
            },
        );
    }

    /// Run one inspect/execute pass off the GTK thread and merge exact commits.
    pub(super) fn run_orphan_cleanup_pass(&self, manifest_offset: usize) {
        let drafts = &self.imp().drafts;
        if !drafts.manifest_authority.get().is_trusted() {
            drafts.orphan_cleanup_pending_offset.set(None);
            drafts.orphan_cleanup_timer_pending.set(false);
            let _ = drafts.orphan_cleanup_timer.invalidate();
            return;
        }
        if drafts.mutation_inflight.get() {
            self.arm_orphan_cleanup_follow_up(
                manifest_offset,
                policy::DRAFT_MUTATION_WAIT_POLL_INTERVAL,
            );
            return;
        }
        if drafts.orphan_cleanup_inflight.replace(true) {
            drafts
                .orphan_cleanup_pending_offset
                .set(Some(manifest_offset));
            return;
        }
        {
            drafts.orphan_cleanup_workers_started.set(
                drafts
                    .orphan_cleanup_workers_started
                    .get()
                    .saturating_add(1),
            );
            drafts
                .orphan_cleanup_workers_high_water
                .set(drafts.orphan_cleanup_workers_high_water.get().max(1));
        }
        let data_dir = json_store::data_dir();
        // Clone GTK-owned state before dispatch so the worker receives plain
        // owned data and never borrows through the window's interior mutability.
        let manifest = self.imp().drafts.manifest.borrow().clone();
        spawn_blocking_then(
            self.clone(),
            move || {
                delay_orphan_cleanup_worker_for_test();
                draft_service::inspect_orphan_cleanup_from(&data_dir, &manifest, manifest_offset)
                    .map(|plan| {
                        let mut outcome = draft_service::execute_orphan_cleanup(&data_dir, plan);
                        // Drop the full manifest before crossing back to GTK; the
                        // callback needs only fingerprints, failures, and continuation.
                        outcome.latest_persisted_manifest.take();
                        let committed_by_id = outcome
                            .committed_manifest_removals
                            .iter()
                            .map(|fingerprint| (fingerprint.draft_id.clone(), fingerprint.clone()))
                            .collect();
                        OrphanCleanupUiResult {
                            outcome,
                            committed_by_id,
                        }
                    })
            },
            move |window, result| {
                window.imp().drafts.orphan_cleanup_inflight.set(false);
                let follow_up = match result {
                    Ok(result) => {
                        let OrphanCleanupUiResult {
                            outcome,
                            committed_by_id,
                        } = result;
                        // Merge exact generations instead of replacing live state;
                        // autosaves accepted while the worker ran must survive.
                        draft_service::merge_committed_orphan_removals(
                            &mut window.imp().drafts.manifest.borrow_mut(),
                            &committed_by_id,
                        );
                        if !outcome.failures.is_empty() {
                            let message = grouped_orphan_cleanup_failure_message(&outcome.failures);
                            tracing::warn!("{message}");
                            window.publish_status_message(&message, NotificationSeverity::Warning);
                        }
                        orphan_cleanup_follow_up(
                            outcome.has_more_work,
                            outcome.next_manifest_offset,
                            !outcome.failures.is_empty(),
                            window.imp().drafts.orphan_cleanup_failure_streak.get(),
                        )
                    }
                    Err(error) => {
                        let message = format!("Draft recovery cleanup scan failed: {error}");
                        tracing::warn!("{message}");
                        window.publish_status_message(&message, NotificationSeverity::Warning);
                        orphan_cleanup_follow_up(
                            true,
                            None,
                            true,
                            window.imp().drafts.orphan_cleanup_failure_streak.get(),
                        )
                    }
                };
                window.finish_orphan_cleanup_pass(follow_up);
                window.drive_pending_draft_mutations();
            },
        );
    }

    pub(super) fn finish_orphan_cleanup_pass(&self, follow_up: OrphanCleanupFollowUp) {
        if let Some(manifest_offset) = self.imp().drafts.orphan_cleanup_pending_offset.take() {
            self.imp().drafts.orphan_cleanup_failure_streak.set(0);
            self.arm_orphan_cleanup_follow_up(
                manifest_offset,
                orphan_cleanup_followup_delay(policy::ORPHAN_CLEANUP_FOLLOWUP_DELAY),
            );
            return;
        }

        match follow_up {
            OrphanCleanupFollowUp::Stop => {
                self.imp().drafts.orphan_cleanup_failure_streak.set(0);
                self.imp().drafts.orphan_cleanup_timer_pending.set(false);
                let _ = self.imp().drafts.orphan_cleanup_timer.invalidate();
            }
            OrphanCleanupFollowUp::Schedule {
                manifest_offset,
                delay,
                next_failure_streak,
            } => {
                self.imp()
                    .drafts
                    .orphan_cleanup_failure_streak
                    .set(next_failure_streak);
                self.arm_orphan_cleanup_follow_up(
                    manifest_offset,
                    orphan_cleanup_followup_delay(delay),
                );
            }
        }
    }

    pub(super) fn arm_orphan_cleanup_follow_up(&self, manifest_offset: usize, delay: Duration) {
        self.imp().drafts.orphan_cleanup_timer_pending.set(true);
        self.imp()
            .drafts
            .orphan_cleanup_timer
            .arm(self, delay, move |window, _| {
                window.imp().drafts.orphan_cleanup_timer_pending.set(false);
                window.run_orphan_cleanup_pass(manifest_offset);
            });
    }

    /// Delete the draft for a given file path.
    pub fn delete_draft_for_path(&self, path: &Path) {
        let draft_id = {
            let manifest = self.imp().drafts.manifest.borrow();
            manifest
                .find_by_path(path)
                .map(|entry| entry.draft_id.clone())
        };
        if let Some(draft_id) = draft_id {
            self.delete_draft_by_id(&draft_id);
        }
    }

    /// Retire a stale file-backed draft, preserving its body first.
    ///
    /// The ordinary serialized delete runs, but its worker first preserves the
    /// body (`draft_service::preserve_stale_draft_body`) and deletes nothing
    /// when that fails. The warning is published once the destination is known.
    pub(super) fn retire_stale_draft(&self, entry: DraftEntry) {
        let draft_id = entry.draft_id.clone();
        self.imp().drafts.stale_preservations.borrow_mut().insert(
            draft_id.clone(),
            PendingPreservation {
                entry,
                announce_as_stale: true,
            },
        );
        self.delete_draft_by_id(&draft_id);
    }

    /// Keep a copy of a recovery body the user edited over before it could be
    /// restored, then let autosave replace it.
    ///
    /// The body stays in the journal; only a preserved copy is added (local
    /// history for a file, and the set-aside area). Autosave of the id stays
    /// held until the copy has been attempted, so the copy always reads the
    /// unrestored body.
    pub(super) fn preserve_unrestored_draft(&self, entry: DraftEntry) {
        let draft_id = entry.draft_id.clone();
        self.hold_draft_restore(&draft_id);
        let data_dir = json_store::data_dir();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || draft_service::preserve_stale_draft_body(&data_dir, &entry),
            move |(), outcome| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                window.release_draft_restore(&draft_id);
                let preservation = match outcome {
                    Ok(Some(preservation)) => preservation,
                    Ok(None) => return,
                    Err(error) => {
                        tracing::warn!("Could not preserve unrestored draft {draft_id}: {error}");
                        StaleDraftPreservation::Kept
                    }
                };
                window.publish_preservation_outcome(&draft_id, preservation, false);
            },
        );
    }

    /// Show the stale-draft warning on every unmodified open editor of that
    /// draft; when the user is already editing it, say where the earlier
    /// edits went in the status bar instead of interrupting the tab.
    fn publish_preservation_outcome(
        &self,
        draft_id: &str,
        preservation: StaleDraftPreservation,
        announce_as_stale: bool,
    ) {
        let tab_view = &self.imp().tab_view;
        let mut alerted = false;
        if announce_as_stale {
            for index in 0..tab_view.n_pages() {
                let child = tab_view.nth_page(index).child();
                if let Some(editor) = child.downcast_ref::<LushtextEditorPage>()
                    && editor.draft_id().as_deref() == Some(draft_id)
                    && !editor.is_modified()
                {
                    Self::show_stale_draft_skipped(editor, preservation);
                    alerted = true;
                }
            }
        }
        if !alerted {
            self.publish_status_message(
                &policy::stale_draft_status_message(
                    preservation,
                    &draft_service::set_aside_dir(&json_store::data_dir()),
                ),
                NotificationSeverity::Warning,
            );
        }
    }

    /// Delete a draft by its ID and persist the manifest update.
    ///
    /// A draft whose restore is still pending was never shown to the user; a
    /// save or discard of its tab then deletes it only after preserving it.
    pub fn delete_draft_by_id(&self, draft_id: &str) {
        let preservation_queued = self
            .imp()
            .drafts
            .stale_preservations
            .borrow()
            .contains_key(draft_id);
        if self.draft_restore_is_pending(draft_id)
            && !preservation_queued
            && let Some(entry) = self.draft_manifest_entry(draft_id)
        {
            self.imp().drafts.stale_preservations.borrow_mut().insert(
                draft_id.to_string(),
                PendingPreservation {
                    entry,
                    announce_as_stale: false,
                },
            );
        }
        // Intent is assigned on GTK before an older body worker can finish and
        // before this compact delete waits behind the single-flight mutation.
        let intent = self
            .imp()
            .drafts
            .mutation_order
            .borrow_mut()
            .advance(draft_id);
        self.imp()
            .drafts
            .delete_tombstones
            .borrow_mut()
            .insert(draft_id.to_string(), intent.clone());
        self.imp()
            .drafts
            .manifest
            .borrow_mut()
            .remove_by_id(draft_id);

        let drafts = &self.imp().drafts;
        let already_pending = !drafts
            .pending_delete_ids
            .borrow_mut()
            .insert(draft_id.to_string());
        let mut pending_deletes = drafts.pending_deletes.borrow_mut();
        // Preserve global order by moving a superseded same-ID command to the
        // tail. Distinct-ID admission stays O(1) for large close batches.
        if already_pending
            && let Some(index) = pending_deletes
                .iter()
                .position(|pending| pending.draft_id == draft_id)
        {
            pending_deletes.remove(index);
        }
        pending_deletes.push_back(intent);
        drop(pending_deletes);
        self.drive_pending_draft_mutations();
    }

    /// Run queued compact deletes only after every earlier body/manifest command.
    pub(super) fn drive_pending_draft_mutations(&self) {
        if self.imp().drafts.mutation_inflight.get()
            || self.imp().drafts.orphan_cleanup_inflight.get()
        {
            return;
        }
        let Some(intent) = self.imp().drafts.pending_deletes.borrow_mut().pop_front() else {
            let rerun = self.imp().drafts.autosave_pending.replace(false);
            if rerun {
                self.autosave_tick();
            }
            return;
        };
        self.imp()
            .drafts
            .pending_delete_ids
            .borrow_mut()
            .remove(&intent.draft_id);
        self.imp().drafts.mutation_inflight.set(true);

        let data_dir = json_store::data_dir();
        let draft_id = intent.draft_id.clone();
        let stale_entry = self
            .imp()
            .drafts
            .stale_preservations
            .borrow_mut()
            .remove(&draft_id);
        let session = self.collect_session_for_draft_reconciliation();
        let authority = self.imp().drafts.manifest_authority.get();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                // A stale draft is preserved before anything is deleted; when
                // preservation fails, the body and its entry both stay.
                let preservation = stale_entry.map(|pending| {
                    let outcome =
                        draft_service::preserve_stale_draft_body(&data_dir, &pending.entry)
                            .map_err(|error| error.to_string());
                    (pending, outcome)
                });
                if let Some((_, Err(error))) = &preservation {
                    return (
                        Some(format!("stale draft body could not be preserved: {error}")),
                        None,
                        preservation,
                    );
                }
                // Keep the persisted manifest as the durable retry marker until
                // the body is gone. A failed body deletion therefore leaves a
                // fully recoverable pre-delete state across unrelated manifest
                // mutations and process restart.
                delay_draft_delete_for_test();
                let body_error = fail_next_draft_delete_for_test()
                    .and_then(|()| draft_service::delete_draft_file(&data_dir, &draft_id))
                    .err()
                    .map(|error| error.to_string());
                let manifest_result = if body_error.is_none() {
                    delay_draft_manifest_for_test();
                    Some(match fail_next_draft_manifest_for_test() {
                        Ok(()) => draft_service::remove_manifest_entry(
                            &data_dir, &session, authority, &draft_id,
                        )
                        .map_err(DraftManifestFailure::from),
                        Err(error) => Err(DraftManifestFailure::injected(&error)),
                    })
                } else {
                    None
                };
                (body_error, manifest_result, preservation)
            },
            move |(), (body_error, manifest_result, preservation)| {
                if let Some(window) = window_weak.upgrade() {
                    match preservation {
                        Some((pending, Ok(Some(preserved)))) => {
                            window.publish_preservation_outcome(
                                &intent.draft_id,
                                preserved,
                                pending.announce_as_stale,
                            );
                        }
                        Some((pending, Err(_))) => {
                            // Nothing was deleted; any later delete of this id
                            // must still preserve the body first.
                            let announce = pending.announce_as_stale;
                            window
                                .imp()
                                .drafts
                                .stale_preservations
                                .borrow_mut()
                                .insert(pending.entry.draft_id.clone(), pending);
                            window.publish_preservation_outcome(
                                &intent.draft_id,
                                StaleDraftPreservation::Kept,
                                announce,
                            );
                        }
                        Some((_, Ok(None))) | None => {}
                    }
                    let deletion_terminal =
                        body_error.is_none() && manifest_result.as_ref().is_some_and(Result::is_ok);
                    if let Some(error) = body_error.as_deref() {
                        tracing::warn!("Failed to delete draft file {}: {error}", intent.draft_id);
                        window.publish_status_message(
                            "Draft cleanup could not remove one recovery body; cleanup remains retryable.",
                            NotificationSeverity::Warning,
                        );
                    }
                    match manifest_result {
                        Some(Ok(commit)) => window.accept_draft_manifest_commit(commit),
                        Some(Err(error)) => {
                            window.reject_draft_manifest_authority(error.authority);
                            tracing::warn!(
                                "Failed to save manifest before draft deletion {}: {}",
                                intent.draft_id,
                                error.detail,
                            );
                            window.publish_status_message(
                                "Draft cleanup could not confirm recovery metadata; cleanup remains retryable.",
                                NotificationSeverity::Warning,
                            );
                        }
                        None => {}
                    }
                    if deletion_terminal {
                        let drafts = &window.imp().drafts;
                        let tombstone_is_current =
                            drafts.delete_tombstones.borrow().get(&intent.draft_id)
                                == Some(&intent);
                        if tombstone_is_current {
                            drafts
                                .delete_tombstones
                                .borrow_mut()
                                .remove(&intent.draft_id);
                            drafts
                                .mutation_order
                                .borrow_mut()
                                .retire_if_current(&intent);
                        }
                    }
                    window.imp().drafts.mutation_inflight.set(false);
                    window.drive_pending_draft_mutations();
                }
            },
        );
    }

    /// Allocate a draft ID for a new editor page.
    pub fn assign_draft_id(&self, editor: &LushtextEditorPage) {
        let id = if let Some(ref path) = editor.file_path() {
            draft_service::draft_id_for_path(path)
        } else {
            draft_service::new_untitled_draft_id()
        };
        editor.set_draft_id(id);
    }

    /// Schedule startup orphan cleanup through the production timer owner.
    #[cfg(feature = "test-utils")]
    pub fn schedule_orphan_cleanup_for_test(&self, cleanup_allowed: bool) {
        self.schedule_orphan_cleanup(cleanup_allowed);
    }

    /// Exercise the same orphan-cleanup cancellation used by window disposal.
    #[cfg(feature = "test-utils")]
    pub fn dispose_orphan_cleanup_for_test(&self) {
        self.imp().drafts.dispose_orphan_cleanup();
    }
}
