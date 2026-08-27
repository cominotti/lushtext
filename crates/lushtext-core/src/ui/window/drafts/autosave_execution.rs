// SPDX-License-Identifier: GPL-3.0-or-later

//! Writing dirty buffers to their drafts, on a timer and at close.
//!
//! Stage-order-qualified `autosave_execution`, the sibling of
//! `restore_execution`. Two stage orders share this module because they share
//! their shape exactly: collect candidates, snapshot **one at a time**, write each
//! body on a worker, then commit every accepted entry to the manifest in one
//! pass. What differs is only their admission rule and their terminal — an
//! autosave re-arms, a close reports to its caller — which is why they are one
//! module rather than two.
//!
//! ## One body at a time, and why the pipeline is recursive
//!
//! Each candidate's snapshot, worker write, and completion are chained: the next
//! candidate is admitted **from inside** the previous one's completion. That is
//! what bounds the lane to one complete document-sized body regardless of how
//! many tabs are dirty. A loop would hold them all.
//!
//! ## The close path's deliberate exception
//!
//! `flush_dirty_drafts` (in `journal`) blocks, on the reasoning that the process
//! is about to exit and the last recoverable buffer state matters more than the
//! stall. Every other path here yields. Note that **no production path currently
//! reaches it**: window close goes through `flush_dirty_drafts_async` below, and
//! the synchronous variant is exercised only by widget tests.

use std::collections::HashSet;

use anyhow::Result;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;
use gtk4::prelude::*;

use crate::model::draft::{DraftEntry, DraftManifestAuthority};
use crate::services::notifications::NotificationSeverity;
use crate::services::{draft_service, editor_io, json_store};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::LushtextEditorPage;

use super::policy;
use super::policy::DraftPipelineFailures;
use super::policy::{
    AutosaveAdmission, autosave_admission, captured_snapshot_is_current, close_flush_must_wait,
    draft_candidate_is_eligible,
};
use super::seams::{
    AcceptedDraft, DirtyDraftCandidate, DirtyDraftCompletion, DraftFlushError, DraftManifestFailure,
};
use super::{
    automatic_draft_limit, delay_draft_body_for_test, delay_draft_manifest_completion_for_test,
    delay_draft_manifest_for_test, fail_next_draft_body_for_test,
    fail_next_draft_manifest_for_test, first_dirty_autosave_debounce,
};
use crate::ui::window::LushtextWindow;

impl LushtextWindow {
    /// Start the global 5-second autosave timer.
    pub fn start_autosave_timer(&self) {
        let window_weak = self.downgrade();
        let source_id = glib::timeout_add_local(policy::AUTOSAVE_TICK_INTERVAL, move || {
            let Some(window) = window_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            window.autosave_tick();
            glib::ControlFlow::Continue
        });
        *self.imp().drafts.autosave_source_id.borrow_mut() = Some(source_id);
    }

    /// Single autosave tick: collect dirty tabs and write drafts.
    pub(super) fn autosave_tick(&self) {
        self.cancel_first_dirty_draft_autosave();
        let drafts = &self.imp().drafts;
        if autosave_admission(
            drafts.autosave_inflight.get(),
            drafts.mutation_inflight.get(),
            drafts.orphan_cleanup_inflight.get(),
        ) == AutosaveAdmission::MarkPending
        {
            drafts.autosave_pending.set(true);
            return;
        }

        let dirty_tabs = self.collect_dirty_draft_candidates();
        if dirty_tabs.is_empty() {
            return;
        }

        self.imp().drafts.autosave_inflight.set(true);
        self.imp().drafts.mutation_inflight.set(true);
        self.drive_dirty_draft_pipeline(dirty_tabs, Vec::new(), DraftPipelineFailures::default());
    }

    /// Drive one autosave pass without waiting for the production timer.
    #[cfg(feature = "test-utils")]
    pub fn autosave_tick_for_test(&self) {
        self.autosave_tick();
    }

    /// Schedule a short autosave after the first dirty edit in a clean cycle.
    pub(crate) fn schedule_first_dirty_draft_autosave(&self) {
        let drafts = &self.imp().drafts;
        if autosave_admission(
            drafts.autosave_inflight.get(),
            drafts.mutation_inflight.get(),
            false,
        ) == AutosaveAdmission::MarkPending
        {
            drafts.autosave_pending.set(true);
            return;
        }
        drafts.first_dirty_autosave_pending.set(true);
        self.imp().drafts.first_dirty_autosave_timer.arm(
            self,
            first_dirty_autosave_debounce(),
            move |window, _| {
                window.imp().drafts.first_dirty_autosave_pending.set(false);
                window.autosave_tick();
            },
        );
    }

    pub(super) fn cancel_first_dirty_draft_autosave(&self) {
        self.imp().drafts.first_dirty_autosave_pending.set(false);
        let _ = self.imp().drafts.first_dirty_autosave_timer.invalidate();
    }

    /// Capture retry-eligible autosave candidates, excluding clean generations.
    ///
    /// A tab whose load installation is incomplete is **skipped, not deferred**.
    /// A cancelled installation leaves the buffer holding neither the old
    /// document nor the new one, and it clears `modified` without clearing
    /// `draft_dirty` — so one keystroke afterwards would otherwise make a
    /// near-empty buffer look like a normal dirty candidate and write it over a
    /// draft that still holds real unsaved work. Skipping keeps that draft as the
    /// best available recovery record; deferring through `autosave_pending` would
    /// spin for as long as the installation stays incomplete. A retry install
    /// clears the flag, and the next edit re-arms the first-dirty autosave
    /// through the ordinary path.
    pub(super) fn collect_dirty_draft_candidates(&self) -> Vec<DirtyDraftCandidate> {
        self.collect_draft_candidates(true, &HashSet::new())
    }

    /// Capture every modified close candidate except explicit discards.
    pub(super) fn collect_close_draft_candidates(&self) -> Vec<DirtyDraftCandidate> {
        let discarded_draft_ids = self.imp().drafts.close_discard_ids.borrow().clone();
        self.collect_draft_candidates(false, &discarded_draft_ids)
    }

    /// Walk the tab list once and capture every eligible draft candidate.
    ///
    /// Shared by both admission passes above, because the walk, the eligibility
    /// question, and the candidate record are the same for both. Exactly two
    /// things differ, and both are parameters: `require_draft_dirty` is the policy
    /// term that separates an autosave from a close, and `discarded_draft_ids` is
    /// the close path's explicit discard set — empty for autosave, which has none.
    ///
    /// The step order is load-bearing and matches both original passes exactly:
    /// eligibility, then the draft id, then the discard check, and only then
    /// `advance`, so a tab that is skipped for any reason never consumes a
    /// mutation epoch.
    fn collect_draft_candidates(
        &self,
        require_draft_dirty: bool,
        discarded_draft_ids: &HashSet<String>,
    ) -> Vec<DirtyDraftCandidate> {
        let tab_view = &self.imp().tab_view;
        let mut dirty_tabs = Vec::new();
        for i in 0..tab_view.n_pages() {
            let page = tab_view.nth_page(i);
            let child = page.child();
            let Some(editor) = child.downcast_ref::<LushtextEditorPage>() else {
                continue;
            };
            if !draft_candidate_is_eligible(
                editor.is_modified(),
                editor.draft_dirty(),
                editor.is_evicted(),
                editor.has_incomplete_load_installation(),
                require_draft_dirty,
            ) {
                continue;
            }
            let Some(draft_id) = editor.draft_id() else {
                continue;
            };
            if discarded_draft_ids.contains(&draft_id) {
                continue;
            }
            let intent = self
                .imp()
                .drafts
                .mutation_order
                .borrow_mut()
                .advance(&draft_id);
            dirty_tabs.push(DirtyDraftCandidate {
                draft_id,
                original_path: editor.file_path(),
                dirty_generation: editor.draft_dirty_generation(),
                editor: editor.downgrade(),
                buffer: editor.buffer(),
                intent,
            });
        }
        dirty_tabs
    }

    /// Snapshot and write one close candidate before admitting the next body.
    fn drive_close_draft_pipeline<F: FnOnce(Result<()>) + 'static>(
        &self,
        mut candidates: Vec<DirtyDraftCandidate>,
        accepted: Vec<AcceptedDraft>,
        failures: DraftPipelineFailures,
        on_done: F,
    ) {
        let Some(candidate) = candidates.pop() else {
            self.commit_close_draft_pipeline(accepted, failures, on_done);
            return;
        };

        let window_weak = self.downgrade();
        // Every terminal outcome clears this capture's token before the next
        // candidate starts, preventing stale disposal cancellation.
        let finish_snapshot = move |outcome: buffer_snapshot::BufferSnapshotOutcome| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            match outcome {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                    window.imp().drafts.close_snapshot.take();
                    let Some(editor) = candidate.editor.upgrade() else {
                        let mut failures = failures;
                        failures.snapshot_cancelled += 1;
                        window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
                        return;
                    };
                    // Chunked capture yields to GTK. A mismatch is unconfirmed and
                    // must block close rather than publish stale text.
                    if !captured_snapshot_is_current(
                        editor.draft_id().as_deref(),
                        candidate.draft_id.as_str(),
                        editor.draft_dirty_generation(),
                        candidate.dirty_generation,
                        editor.is_modified(),
                        editor.is_evicted(),
                        editor.has_incomplete_load_installation(),
                    ) {
                        let mut failures = failures;
                        failures.snapshot_cancelled += 1;
                        window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
                        return;
                    }
                    let data_dir = json_store::data_dir();
                    let draft_id = candidate.draft_id.clone();
                    let original_path = candidate.original_path;
                    let completion = DirtyDraftCompletion {
                        draft_id: candidate.draft_id,
                        dirty_generation: candidate.dirty_generation,
                        editor: candidate.editor,
                        intent: candidate.intent,
                    };
                    let window_weak = window.downgrade();
                    // Move the only complete body to the worker and admit the next
                    // candidate only after this durable write releases it.
                    spawn_blocking_then(
                        (),
                        move || {
                            let text = text.into_string_on_worker();
                            delay_draft_body_for_test();
                            fail_next_draft_body_for_test()?;
                            draft_service::write_draft(&data_dir, &draft_id, &text)?;
                            Ok::<_, anyhow::Error>(DraftEntry {
                                draft_id,
                                original_mtime_secs: original_path
                                    .as_deref()
                                    .and_then(editor_io::mtime_secs),
                                original_path,
                                saved_at_secs: editor_io::now_epoch_secs(),
                            })
                        },
                        move |(), result| {
                            let Some(window) = window_weak.upgrade() else {
                                return;
                            };
                            let mut accepted = accepted;
                            let mut failures = failures;
                            match result {
                                Ok(entry) => accepted.push(AcceptedDraft { entry, completion }),
                                Err(error) => {
                                    tracing::error!("Failed to write draft on close: {error}");
                                    failures.body_write.push(error.to_string());
                                }
                            }
                            window.drive_close_draft_pipeline(
                                candidates, accepted, failures, on_done,
                            );
                        },
                    );
                }
                buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                    window.imp().drafts.close_snapshot.take();
                    let mut failures = failures;
                    failures.over_limit += 1;
                    if let Some(editor) = candidate.editor.upgrade() {
                        Self::show_automatic_recovery_limit(&editor);
                    }
                    window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
                }
                buffer_snapshot::BufferSnapshotOutcome::Cancelled(_) => {
                    window.imp().drafts.close_snapshot.take();
                    let mut failures = failures;
                    failures.snapshot_cancelled += 1;
                    window.drive_close_draft_pipeline(candidates, accepted, failures, on_done);
                }
            }
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&candidate.buffer) {
            let snapshot = buffer_snapshot::snapshot_buffer_text_async_budgeted(
                candidate.buffer,
                automatic_draft_limit(),
                finish_snapshot,
            );
            *self.imp().drafts.close_snapshot.borrow_mut() = Some(snapshot);
        } else {
            finish_snapshot(buffer_snapshot::snapshot_buffer_text_direct_budgeted(
                &candidate.buffer,
                automatic_draft_limit(),
            ));
        }
    }

    /// Commit successful close bodies once and report every unconfirmed draft.
    fn commit_close_draft_pipeline<F: FnOnce(Result<()>) + 'static>(
        &self,
        accepted: Vec<AcceptedDraft>,
        mut failures: DraftPipelineFailures,
        on_done: F,
    ) {
        let mut accepted_entries = Vec::new();
        for accepted in accepted {
            let completion = accepted.completion;
            let Some(editor) = completion.editor.upgrade() else {
                failures.snapshot_cancelled += 1;
                continue;
            };
            if editor.draft_id().as_deref() == Some(completion.draft_id.as_str())
                && editor.draft_dirty_generation() == completion.dirty_generation
                && editor.is_modified()
                && !editor.is_evicted()
                && self
                    .imp()
                    .drafts
                    .mutation_order
                    .borrow()
                    .is_current(&completion.intent)
            {
                accepted_entries.push(accepted.entry);
            } else {
                failures.snapshot_cancelled += 1;
            }
        }
        let data_dir = json_store::data_dir();
        let session = self.collect_session();
        let authority = self.imp().drafts.manifest_authority.get();
        let window_weak = self.downgrade();

        spawn_blocking_then(
            (),
            move || {
                delay_draft_manifest_for_test();
                if let Err(error) = fail_next_draft_manifest_for_test() {
                    return (
                        None,
                        Err(DraftFlushError::Manifest {
                            authority: DraftManifestAuthority::default(),
                            detail: error.to_string(),
                        }),
                    );
                }
                let commit = if accepted_entries.is_empty() {
                    None
                } else {
                    match draft_service::update_manifest(
                        &data_dir,
                        &session,
                        authority,
                        |manifest| {
                            for entry in accepted_entries {
                                manifest.upsert(entry);
                            }
                        },
                    ) {
                        Ok(commit) => Some(commit),
                        Err(error) => {
                            return (
                                None,
                                Err(DraftFlushError::Manifest {
                                    authority: error.authority(),
                                    detail: error.to_string(),
                                }),
                            );
                        }
                    }
                };
                let result = if failures.all_confirmed() {
                    Ok(())
                } else {
                    Err(DraftFlushError::Unconfirmed {
                        total: failures.total(),
                        cancelled: failures.snapshot_cancelled,
                        over_limit: failures.over_limit,
                        body_write: failures.body_write.len(),
                    })
                };
                (commit, result)
            },
            move |(), (commit, result)| {
                if let Some(window) = window_weak.upgrade() {
                    if let Some(commit) = commit {
                        window.accept_draft_manifest_commit(commit);
                    }
                    if let Err(DraftFlushError::Manifest { authority, .. }) = &result {
                        window.reject_draft_manifest_authority(*authority);
                    }
                    if result.is_ok() {
                        window.clear_close_discard_drafts();
                    }
                    // Close flush owns this transaction's acceptance result.
                    // Do not let an edit-coalesced regular tick clear retry state
                    // before the close caller observes success or failure.
                    window.imp().drafts.autosave_pending.set(false);
                    window.imp().drafts.mutation_inflight.set(false);
                    window.drive_pending_draft_mutations();
                    window.wait_for_draft_mutations_then(move || {
                        on_done(result.map_err(anyhow::Error::from));
                    });
                }
            },
        );
    }

    /// Run a close continuation only after queued draft mutations have drained.
    fn wait_for_draft_mutations_then<F: FnOnce() + 'static>(&self, on_done: F) {
        if close_flush_must_wait(
            self.imp().drafts.mutation_inflight.get(),
            false,
            !self.imp().drafts.pending_deletes.borrow().is_empty(),
            self.imp().drafts.restore_inflight_count.get() > 0,
        ) {
            let window_weak = self.downgrade();
            glib::timeout_add_local_once(policy::DRAFT_MUTATION_WAIT_POLL_INTERVAL, move || {
                if let Some(window) = window_weak.upgrade() {
                    window.wait_for_draft_mutations_then(on_done);
                }
            });
            return;
        }
        on_done();
    }

    /// Snapshot and durably write one autosave candidate at a time.
    pub(super) fn drive_dirty_draft_pipeline(
        &self,
        mut candidates: Vec<DirtyDraftCandidate>,
        accepted: Vec<AcceptedDraft>,
        failures: DraftPipelineFailures,
    ) {
        let Some(candidate) = candidates.pop() else {
            self.commit_dirty_draft_pipeline(accepted, failures);
            return;
        };

        let window_weak = self.downgrade();
        // Every terminal outcome clears this capture's token before the next
        // candidate starts, preventing stale disposal cancellation.
        let finish_snapshot = move |outcome: buffer_snapshot::BufferSnapshotOutcome| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            match outcome {
                buffer_snapshot::BufferSnapshotOutcome::Captured(text) => {
                    window.imp().drafts.autosave_snapshot.take();
                    let Some(editor) = candidate.editor.upgrade() else {
                        window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                        return;
                    };
                    // Capture spans main-loop turns. Discard stale text and
                    // request a new pass when identity or generation changed.
                    if !captured_snapshot_is_current(
                        editor.draft_id().as_deref(),
                        candidate.draft_id.as_str(),
                        editor.draft_dirty_generation(),
                        candidate.dirty_generation,
                        editor.is_modified(),
                        editor.is_evicted(),
                        editor.has_incomplete_load_installation(),
                    ) {
                        window.imp().drafts.autosave_pending.set(true);
                        window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                        return;
                    }

                    let data_dir = json_store::data_dir();
                    let draft_id = candidate.draft_id.clone();
                    let original_path = candidate.original_path.clone();
                    let completion = DirtyDraftCompletion {
                        draft_id: candidate.draft_id,
                        dirty_generation: candidate.dirty_generation,
                        editor: candidate.editor,
                        intent: candidate.intent,
                    };
                    let window_weak = window.downgrade();
                    window.note_complete_draft_body_admitted();
                    // The worker owns the only complete body and drops it as
                    // soon as the durable write finishes.
                    spawn_blocking_then(
                        (),
                        move || {
                            let text = text.into_string_on_worker();
                            delay_draft_body_for_test();
                            fail_next_draft_body_for_test()?;
                            let result = draft_service::write_draft(&data_dir, &draft_id, &text)
                                .map(|()| DraftEntry {
                                    draft_id,
                                    original_mtime_secs: original_path
                                        .as_deref()
                                        .and_then(editor_io::mtime_secs),
                                    original_path,
                                    saved_at_secs: editor_io::now_epoch_secs(),
                                });
                            drop(text);
                            result
                        },
                        move |(), result| {
                            let Some(window) = window_weak.upgrade() else {
                                return;
                            };
                            window.note_complete_draft_body_released();
                            let mut accepted = accepted;
                            let mut failures = failures;
                            match result {
                                Ok(entry) => accepted.push(AcceptedDraft { entry, completion }),
                                Err(error) => {
                                    tracing::warn!("Failed to write draft: {error}");
                                    failures.body_write.push(completion.draft_id);
                                }
                            }
                            window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                        },
                    );
                }
                buffer_snapshot::BufferSnapshotOutcome::ExceededLimit { .. } => {
                    window.imp().drafts.autosave_snapshot.take();
                    let mut failures = failures;
                    failures.over_limit += 1;
                    if let Some(editor) = candidate.editor.upgrade()
                        && editor.draft_id().as_deref() == Some(candidate.draft_id.as_str())
                        && editor.draft_dirty_generation() == candidate.dirty_generation
                    {
                        Self::show_automatic_recovery_limit(&editor);
                    }
                    window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                }
                buffer_snapshot::BufferSnapshotOutcome::Cancelled(_) => {
                    window.imp().drafts.autosave_snapshot.take();
                    let mut failures = failures;
                    failures.snapshot_cancelled += 1;
                    window.imp().drafts.autosave_pending.set(true);
                    window.drive_dirty_draft_pipeline(candidates, accepted, failures);
                }
            }
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&candidate.buffer) {
            let snapshot = buffer_snapshot::snapshot_buffer_text_async_budgeted(
                candidate.buffer,
                automatic_draft_limit(),
                finish_snapshot,
            );
            *self.imp().drafts.autosave_snapshot.borrow_mut() = Some(snapshot);
        } else {
            finish_snapshot(buffer_snapshot::snapshot_buffer_text_direct_budgeted(
                &candidate.buffer,
                automatic_draft_limit(),
            ));
        }
    }

    /// Commit compact successful entries once, then accept matching generations.
    pub(super) fn commit_dirty_draft_pipeline(
        &self,
        accepted: Vec<AcceptedDraft>,
        failures: DraftPipelineFailures,
    ) {
        if accepted.is_empty() {
            self.finish_autosave_pipeline(&failures);
            return;
        }
        let data_dir = json_store::data_dir();
        let window_weak = self.downgrade();
        let entries: Vec<DraftEntry> = accepted.iter().map(|item| item.entry.clone()).collect();
        let session = self.collect_session();
        let authority = self.imp().drafts.manifest_authority.get();

        spawn_blocking_then(
            (),
            move || {
                delay_draft_manifest_for_test();
                if let Err(error) = fail_next_draft_manifest_for_test() {
                    return Err(DraftManifestFailure::injected(&error));
                }
                let result =
                    draft_service::update_manifest(&data_dir, &session, authority, |manifest| {
                        for entry in entries {
                            manifest.upsert(entry);
                        }
                    })
                    .map_err(DraftManifestFailure::from);
                delay_draft_manifest_completion_for_test();
                result
            },
            move |(), result| {
                if let Some(window) = window_weak.upgrade() {
                    match result {
                        Ok(commit) => {
                            window.accept_draft_manifest_commit(commit);
                            for accepted in accepted {
                                let completion = accepted.completion;
                                let Some(editor) = completion.editor.upgrade() else {
                                    continue;
                                };
                                // Durability covers only this captured generation;
                                // a newer edit must remain dirty for a later pass.
                                if editor.draft_id().as_deref()
                                    == Some(completion.draft_id.as_str())
                                    && editor.draft_dirty_generation()
                                        == completion.dirty_generation
                                    && window
                                        .imp()
                                        .drafts
                                        .mutation_order
                                        .borrow()
                                        .is_current(&completion.intent)
                                {
                                    editor.set_draft_dirty(false);
                                    window.clear_automatic_recovery_limit(&editor);
                                }
                            }
                        }
                        Err(error) => {
                            window.reject_draft_manifest_authority(error.authority);
                            tracing::warn!("Failed to save draft manifest: {}", error.detail);
                            window.publish_status_message(
                                "Draft autosave could not confirm recovery metadata; changes remain retryable.",
                                NotificationSeverity::Warning,
                            );
                        }
                    }
                    window.finish_autosave_pipeline(&failures);
                }
            },
        );
    }

    /// Release the in-flight gate and run one coalesced follow-up when needed.
    pub(super) fn finish_autosave_pipeline(&self, failures: &DraftPipelineFailures) {
        if let Some(message) = failures.retryable_status_message() {
            self.publish_status_message(&message, NotificationSeverity::Warning);
        }
        self.imp().drafts.autosave_inflight.set(false);
        self.imp().drafts.mutation_inflight.set(false);
        self.drive_pending_draft_mutations();
    }

    /// Record that the worker now owns the one complete draft body.
    ///
    /// Always compiled: the draft evidence surface reports the retained count and
    /// its high-water mark, and the boundedness invariant they prove — never more
    /// than one complete body in flight — holds in production too.
    pub(super) fn note_complete_draft_body_admitted(&self) {
        let retained = self.imp().drafts.retained_complete_bodies.get() + 1;
        self.imp().drafts.retained_complete_bodies.set(retained);
        self.imp().drafts.max_retained_complete_bodies.set(
            self.imp()
                .drafts
                .max_retained_complete_bodies
                .get()
                .max(retained),
        );
    }

    /// Record that the worker released the complete draft body.
    pub(super) fn note_complete_draft_body_released(&self) {
        self.imp().drafts.retained_complete_bodies.set(0);
    }

    /// Remember that a fresh autosave pass is needed after the active batch.
    pub(crate) fn mark_draft_autosave_pending_if_inflight(&self) {
        if self.imp().drafts.autosave_inflight.get() || self.imp().drafts.mutation_inflight.get() {
            self.imp().drafts.autosave_pending.set(true);
        }
    }

    /// Flush dirty drafts for close without monopolizing a GTK main-loop turn.
    ///
    /// Copies are serialized on GTK, writes run on workers, and `on_done` runs
    /// back on GTK after every candidate is accepted or classified.
    pub fn flush_dirty_drafts_async<F: FnOnce(Result<()>) + 'static>(&self, on_done: F) {
        if close_flush_must_wait(
            self.imp().drafts.mutation_inflight.get(),
            self.imp().drafts.orphan_cleanup_inflight.get(),
            !self.imp().drafts.pending_deletes.borrow().is_empty(),
            self.imp().drafts.restore_inflight_count.get() > 0,
        ) {
            let window_weak = self.downgrade();
            glib::timeout_add_local_once(policy::DRAFT_MUTATION_WAIT_POLL_INTERVAL, move || {
                if let Some(window) = window_weak.upgrade() {
                    window.flush_dirty_drafts_async(on_done);
                }
            });
            return;
        }
        let candidates = self.collect_close_draft_candidates();
        if candidates.is_empty() {
            self.clear_close_discard_drafts();
            on_done(Ok(()));
            return;
        }
        self.imp().drafts.mutation_inflight.set(true);
        self.drive_close_draft_pipeline(
            candidates,
            Vec::new(),
            DraftPipelineFailures::default(),
            on_done,
        );
    }
}
