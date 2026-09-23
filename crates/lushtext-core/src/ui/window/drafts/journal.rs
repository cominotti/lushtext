// SPDX-License-Identifier: GPL-3.0-or-later

//! The draft manifest and bodies: the record startup recovery reads back.
//!
//! It is `journal` because a later stage of the same workflow restores from
//! this record: startup recovery reads the manifest and bodies back and installs
//! their content into the user's buffers.
//!
//! The record's **mutual-exclusion gate lives here**, not in a separate
//! `admission`: `mutation_inflight`, `pending_deletes`, `delete_tombstones`, and
//! the `DraftMutationOrder` epoch allocator serialize this record's writes.
//!
//! Orphan cleanup is journal maintenance too; it lives in the
//! stage-order-qualified sibling [`super::cleanup_journal`] (stage order C).
//!
//! ## The deletion ordering this module must never lose
//!
//! The persisted manifest stays the durable retry marker **until the body is
//! gone**: the body is deleted first, and the manifest entry only if that
//! succeeded. A failed body deletion therefore leaves a fully recoverable
//! pre-delete state across unrelated manifest mutations and a process restart.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::draft::{
    DraftEntry, DraftManifestAuthority, PreloadedDraftRestore, StaleDraftPreservation,
};
use crate::services::draft_service::journal_core;
use crate::services::notifications::NotificationSeverity;
use crate::services::{draft_service, editor_io, json_store};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::LushtextEditorPage;

use super::policy;
use super::seams::{DraftManifestFailure, PendingPreservation};
use super::{
    automatic_draft_limit, delay_draft_delete_for_test, delay_draft_manifest_for_test,
    fail_next_draft_delete_for_test, fail_next_draft_manifest_for_test,
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
        self.imp().drafts.dispose_orphan_cleanup();
    }

    /// Adopt the outcome of one write-ahead registration.
    ///
    /// Both pipelines share this: a trusted commit is accepted whole, while an
    /// additive registration mirrors exactly what the service persisted (absent
    /// ids only) and leaves the journal untrusted. Returns the body-write
    /// tokens the registration minted; on failure there are none and the caller
    /// must not write those bodies.
    pub(super) fn apply_draft_registration(
        &self,
        result: std::result::Result<draft_service::DraftRegistration, DraftManifestFailure>,
        registered: Vec<DraftEntry>,
    ) -> std::result::Result<Vec<draft_service::RegisteredDraft>, String> {
        match result {
            Ok(mut registration) => {
                let tokens = registration.take_registered();
                match registration {
                    draft_service::DraftRegistration::Committed { commit, .. } => {
                        self.accept_draft_manifest_commit(commit);
                    }
                    draft_service::DraftRegistration::Additive { authority, .. } => {
                        self.reject_draft_manifest_authority(authority);
                        let mut manifest = self.imp().drafts.manifest.borrow_mut();
                        for entry in registered {
                            manifest.insert_if_absent(entry);
                        }
                    }
                }
                Ok(tokens)
            }
            Err(error) => {
                self.reject_draft_manifest_authority(error.authority);
                Err(error.detail)
            }
        }
    }

    /// The journal core's body-write decision for one candidate of this
    /// window, from what the window knows: its manifest copy, its authority,
    /// and whether a restore of the id is still pending.
    ///
    /// The window cannot see the body file, so it answers "present" whenever a
    /// restore is pending or its manifest lists the id — the conservative side
    /// of the ownership check.
    pub(super) fn draft_body_write_decision(
        &self,
        draft_id: &str,
        file_backed: bool,
    ) -> journal_core::BodyWriteDecision {
        let registered = self.draft_manifest_entry(draft_id).is_some();
        let restore_pending = self.draft_restore_is_pending(draft_id);
        let owner = journal_core::ownership(journal_core::BodyFacts::from_window_view(
            registered,
            restore_pending,
        ));
        journal_core::body_write_decision(
            owner,
            journal_core::registration_required(
                file_backed,
                registered,
                self.imp().drafts.manifest_authority.get().is_trusted(),
            ),
        )
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
            if self.draft_body_write_decision(&draft_id, editor.file_path().is_some())
                == journal_core::BodyWriteDecision::Hold
            {
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
            let mtime = original_path.as_deref().and_then(editor_io::mtime_secs);
            // Write-ahead registration, as in the asynchronous pipelines: a
            // body is never written for an id the persisted manifest lacks.
            let authority = self.imp().drafts.manifest_authority.get();
            let mut token = draft_service::RegisteredDraft::without_registration(
                &draft_id,
                original_path.is_some(),
                self.draft_manifest_entry(&draft_id).is_some(),
                authority.is_trusted(),
            );
            // Decided afresh: an earlier tab's registration in this same loop may
            // have changed the window's manifest or authority.
            if self.draft_body_write_decision(&draft_id, original_path.is_some())
                == journal_core::BodyWriteDecision::RegisterFirst
            {
                let session = self.collect_session_for_draft_reconciliation();
                let entry = DraftEntry {
                    draft_id: draft_id.clone(),
                    original_path: original_path.clone(),
                    original_mtime_secs: mtime,
                    saved_at_secs: now,
                };
                let result = draft_service::register_draft_entries(
                    &data_dir,
                    &session,
                    authority,
                    std::slice::from_ref(&entry),
                )
                .map_err(DraftManifestFailure::from);
                match self.apply_draft_registration(result, vec![entry]) {
                    Ok(mut tokens) => token = tokens.pop(),
                    Err(detail) => {
                        registration_errors.push(format!("{draft_id}: {detail}"));
                        continue;
                    }
                }
            }
            let Some(token) = token else {
                registration_errors
                    .push(format!("{draft_id}: recovery metadata was not registered"));
                continue;
            };
            if let Err(e) = draft_service::write_draft(&data_dir, &token, &text) {
                tracing::error!("Failed to write draft on close: {e}");
                write_errors.push(format!("{draft_id}: {e}"));
                continue;
            }
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

    /// Act on a restore attempt that did not apply its body, as the journal
    /// core's `unapplied_restore_disposition` decides: keep a preserved copy,
    /// preserve and retire a stale body, or do nothing.
    pub(super) fn dispose_unapplied_restore(
        &self,
        ending: journal_core::RestoreEnding,
        entry: DraftEntry,
    ) {
        match journal_core::unapplied_restore_disposition(ending) {
            journal_core::RestoreDisposition::Nothing => {}
            journal_core::RestoreDisposition::PreserveCopy => self.preserve_unrestored_draft(entry),
            journal_core::RestoreDisposition::PreserveThenRetire => self.retire_stale_draft(entry),
        }
    }

    /// Keep a copy of a recovery body the user edited over before it could be
    /// restored, then let autosave replace it.
    ///
    /// The body stays in the journal; only a preserved copy is added (local
    /// history for a file, and the set-aside area). Autosave of the id stays
    /// held until the copy **exists**, so the copy always reads the unrestored
    /// body and a failed copy never lets autosave replace the only one; the
    /// autosave tick retries it (`retry_unrestored_copies`).
    pub(super) fn preserve_unrestored_draft(&self, entry: DraftEntry) {
        self.hold_draft_restore(&entry.draft_id);
        self.attempt_unrestored_copy(entry, 0);
    }

    /// Retry every unrestored copy that failed, keeping each one's hold. Called
    /// from the autosave tick, so a persistent failure costs one small worker
    /// per tick rather than a loop.
    pub(super) fn retry_unrestored_copies(&self) {
        let retries = std::mem::take(&mut *self.imp().drafts.unrestored_copy_retries.borrow_mut());
        for (entry, failures) in retries.into_values() {
            self.attempt_unrestored_copy(entry, failures);
        }
    }

    fn attempt_unrestored_copy(&self, entry: DraftEntry, failures: u32) {
        let data_dir = json_store::data_dir();
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                let outcome = draft_service::preserve_stale_draft_body(&data_dir, &entry);
                (entry, outcome)
            },
            move |(), (entry, outcome)| {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let draft_id = entry.draft_id.clone();
                let preservation = match outcome {
                    Ok(Some(preservation)) => preservation,
                    Ok(None) => {
                        window.release_draft_restore(&draft_id);
                        return;
                    }
                    Err(error) => {
                        if failures == 0 {
                            tracing::warn!(
                                "Could not preserve unrestored draft {draft_id}: {error}"
                            );
                            window.publish_status_message(
                                policy::UNRESTORED_COPY_FAILED_STATUS,
                                NotificationSeverity::Warning,
                            );
                        } else {
                            tracing::debug!(
                                "Retry {failures} of unrestored draft copy {draft_id} failed: {error}"
                            );
                        }
                        window
                            .imp()
                            .drafts
                            .unrestored_copy_retries
                            .borrow_mut()
                            .insert(draft_id, (entry, failures.saturating_add(1)));
                        return;
                    }
                };
                window.release_draft_restore(&draft_id);
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
        let entry = self.draft_manifest_entry(draft_id);
        let restore_pending = self.draft_restore_is_pending(draft_id);
        let owner = journal_core::ownership(journal_core::BodyFacts::from_window_view(
            entry.is_some(),
            restore_pending,
        ));
        if journal_core::deletion_start(owner) == journal_core::DeletionStep::Preserve
            && !preservation_queued
            && let Some(entry) = entry
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
                // The journal core's deletion order: a queued preservation runs
                // first, then the body, then the manifest entry. The persisted
                // entry stays the durable retry marker until the body is gone,
                // so a failure at any step leaves a fully recoverable pre-delete
                // state across unrelated manifest mutations and process restart.
                let start = if stale_entry.is_some() {
                    journal_core::DeletionStep::Preserve
                } else {
                    journal_core::DeletionStep::DeleteBody
                };
                let mut preservation = None;
                let mut body_error = None;
                let mut manifest_result = None;
                let mut stale_entry = stale_entry;
                journal_core::run_deletion(start, |step| match step {
                    journal_core::DeletionStep::Preserve => {
                        let Some(pending) = stale_entry.take() else {
                            return false;
                        };
                        let outcome =
                            draft_service::preserve_stale_draft_body(&data_dir, &pending.entry)
                                .map_err(|error| error.to_string());
                        let succeeded = outcome.is_ok();
                        if let Err(error) = &outcome {
                            body_error =
                                Some(format!("stale draft body could not be preserved: {error}"));
                        }
                        preservation = Some((pending, outcome));
                        succeeded
                    }
                    journal_core::DeletionStep::DeleteBody => {
                        delay_draft_delete_for_test();
                        body_error = fail_next_draft_delete_for_test()
                            .and_then(|()| draft_service::delete_draft_file(&data_dir, &draft_id))
                            .err()
                            .map(|error| error.to_string());
                        body_error.is_none()
                    }
                    _ => {
                        delay_draft_manifest_for_test();
                        let result = match fail_next_draft_manifest_for_test() {
                            Ok(()) => draft_service::remove_manifest_entry(
                                &data_dir, &session, authority, &draft_id,
                            )
                            .map_err(DraftManifestFailure::from),
                            Err(error) => Err(DraftManifestFailure::injected(&error)),
                        };
                        let succeeded = result.is_ok();
                        manifest_result = Some(result);
                        succeeded
                    }
                });
                // A failed preservation deleted nothing: report it as the
                // stopping error only, never as a body-deletion failure.
                if matches!(preservation, Some((_, Err(_)))) {
                    return (body_error, None, preservation);
                }
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
}
