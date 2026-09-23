// SPDX-License-Identifier: GPL-3.0-or-later

//! Stage order C: orphan cleanup, the draft journal's own maintenance.
//!
//! A `journal` coordination module qualified by the stage order it serves,
//! split from [`super::journal`] for size rather than for a different role.
//! It looks like `retirement`, but `retirement` here means the disposal lane's
//! off-GTK destruction of an in-memory payload. Orphan cleanup instead reloads
//! the draft manifest under its write lock, is gated by its authority, and
//! merges exact committed fingerprints back into it; the manifest offset it
//! re-arms (`orphan_cleanup_pending_offset`) is that record's continuation.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::glib;

use crate::services::notifications::NotificationSeverity;
use crate::services::{draft_service, json_store};

use super::policy;
use super::policy::{
    OrphanCleanupFollowUp, grouped_orphan_cleanup_failure_message, orphan_cleanup_follow_up,
};
use super::seams::OrphanCleanupUiResult;
use super::{
    delay_orphan_cleanup_worker_for_test, orphan_cleanup_followup_delay, orphan_cleanup_start_delay,
};
use crate::ui::window::LushtextWindow;

impl LushtextWindow {
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
            drafts.dispose_orphan_cleanup();
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
                // The pending offset was taken above, so this only stops the timer.
                self.imp().drafts.dispose_orphan_cleanup();
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
