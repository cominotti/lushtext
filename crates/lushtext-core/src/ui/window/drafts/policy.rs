// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decisions for the draft-recovery workflow.
//!
//! Three stage orders' worth of policy, with no GTK anywhere: which tabs an
//! autosave pass may write, when an in-flight pass must be re-run rather than
//! queued, how a bounded orphan-cleanup pass continues or backs off, how a
//! partial pipeline's failures are reported, and the main-thread ordering that
//! keeps a delete from being undone by an older autosave.
//!
//! **This is the workflow whose decisions decide whether the user's unsaved work
//! survives a crash**, so as much of that as can be made pure is made pure — a
//! decision inside a GTK adapter is one mutation testing cannot reach.

use std::collections::HashMap;
use std::time::Duration;

// --- timing policy ---------------------------------------------------------

/// First-dirty draft autosave delay after a clean edit cycle.
///
/// 750 ms persists new unsaved work sooner than the regular 5 s autosave tick
/// while still coalescing quick typing into one draft write.
pub(super) const FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS: u64 = 750;
/// Interval of the always-running autosave tick.
pub(super) const AUTOSAVE_TICK_INTERVAL: Duration = Duration::from_secs(5);
/// Delay before startup releases preloaded bodies and begins orphan inspection.
///
/// Two seconds lets restored editors consume their recovery snapshots before a
/// background cleanup worker revalidates the same persisted artifacts. Read only
/// on the production path; the test build substitutes its own value, which is why
/// a default-feature build is the only one that references it.
#[cfg_attr(
    feature = "test-utils",
    expect(dead_code, reason = "the test build substitutes this delay")
)]
pub(super) const ORPHAN_CLEANUP_START_DELAY: Duration = Duration::from_secs(2);
/// Delay for the one permitted follow-up bounded cleanup pass.
///
/// Thirty seconds avoids a tight retry loop when permissions or storage remain
/// unavailable while still making progress on a directory that exceeded the cap.
pub(super) const ORPHAN_CLEANUP_FOLLOWUP_DELAY: Duration = Duration::from_secs(30);
/// Maximum delay between retryable orphan-cleanup attempts.
pub(super) const ORPHAN_CLEANUP_MAX_FAILURE_BACKOFF: Duration = Duration::from_secs(15 * 60);
/// Low-frequency close/readiness poll while ordered recovery work drains.
pub(super) const DRAFT_MUTATION_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);

// --- autosave admission ----------------------------------------------------

/// Whether one tab's buffer may be written to its draft right now.
///
/// The `installation_incomplete` term is a **data-safety guard**, not an
/// optimisation. A cancelled bounded load installation empties the buffer and
/// clears `modified` **without** clearing `draft_dirty`, so one keystroke
/// afterwards would otherwise make a near-empty buffer look like an ordinary
/// dirty candidate — and the pass would write it over a draft that still holds
/// real unsaved work. Migrated `WFR-DOCUMENT-SAVE` refuses on the same flag with
/// `EditorSaveError::IncompleteLoadInstallation`.
///
/// `require_draft_dirty` is what separates the two collection passes: an autosave
/// writes only tabs whose draft is behind the buffer, while a close writes every
/// modified tab because there is no later pass to catch it.
#[must_use]
pub const fn draft_candidate_is_eligible(
    modified: bool,
    draft_dirty: bool,
    evicted: bool,
    installation_incomplete: bool,
    require_draft_dirty: bool,
) -> bool {
    modified && !evicted && !installation_incomplete && (draft_dirty || !require_draft_dirty)
}

/// Whether a captured snapshot still describes the editor it came from.
///
/// Chunked capture spans main-loop turns, so identity and generation are both
/// re-read afterwards. A mismatch is *unconfirmed*, never "close enough": the
/// close path blocks rather than publishing stale text, and the autosave path
/// re-arms rather than writing it.
#[must_use]
pub(super) fn captured_snapshot_is_current(
    live_draft_id: Option<&str>,
    expected_draft_id: &str,
    live_dirty_generation: u64,
    expected_dirty_generation: u64,
    modified: bool,
    evicted: bool,
    installation_incomplete: bool,
) -> bool {
    live_draft_id == Some(expected_draft_id)
        && live_dirty_generation == expected_dirty_generation
        && modified
        && !evicted
        && !installation_incomplete
}

/// What an autosave tick does when the lane is already owned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AutosaveAdmission {
    /// Start a pass now.
    Start,
    /// Mark a follow-up pass needed and return.
    ///
    /// Deliberately *not* a queue: only "another pass is needed" is remembered,
    /// so a burst of ticks during one long pass cannot fan out into many.
    MarkPending,
}

/// Decide whether an autosave tick may start a pass.
#[must_use]
pub(super) const fn autosave_admission(
    autosave_inflight: bool,
    mutation_inflight: bool,
    cleanup_inflight: bool,
) -> AutosaveAdmission {
    if autosave_inflight || mutation_inflight || cleanup_inflight {
        AutosaveAdmission::MarkPending
    } else {
        AutosaveAdmission::Start
    }
}

/// Whether close-safety work must wait for the recovery lane to drain.
///
/// A close that ran while a delete or restore was mid-flight could write a draft
/// the user had just discarded, or race a restore's own manifest update.
#[must_use]
pub(super) const fn close_flush_must_wait(
    mutation_inflight: bool,
    cleanup_inflight: bool,
    pending_deletes: bool,
    restores_inflight: bool,
) -> bool {
    mutation_inflight || cleanup_inflight || pending_deletes || restores_inflight
}

/// Whether draft persistence or deferred startup restore blocks readiness.
#[must_use]
pub(super) const fn draft_workflow_blocks_readiness(
    autosave_inflight: bool,
    mutation_inflight: bool,
    pending_deletes: bool,
    restores_inflight: bool,
    lazy_restore_inflight: bool,
    lazy_queue_non_empty: bool,
) -> bool {
    autosave_inflight
        || mutation_inflight
        || pending_deletes
        || restores_inflight
        || lazy_restore_inflight
        || lazy_queue_non_empty
}

// --- pipeline failure reporting --------------------------------------------

/// Failures accumulated without retaining any completed draft bodies.
///
/// Counts and bounded detail strings only: a pipeline that retained the bodies
/// it failed on would hold one document per failure.
#[derive(Debug, Default)]
pub(super) struct DraftPipelineFailures {
    /// Candidates cancelled or invalidated before acceptance.
    pub(super) snapshot_cancelled: usize,
    /// Candidates rejected by the automatic-recovery byte policy.
    pub(super) over_limit: usize,
    /// Body-write details retained without retaining body text.
    pub(super) body_write: Vec<String>,
}

impl DraftPipelineFailures {
    /// Total candidates that failed an acceptance stage.
    #[must_use]
    pub(super) fn total(&self) -> usize {
        self.snapshot_cancelled
            .saturating_add(self.over_limit)
            .saturating_add(self.body_write.len())
    }

    /// Whether every candidate reached acceptance.
    #[must_use]
    pub(super) fn all_confirmed(&self) -> bool {
        self.total() == 0
    }

    /// The user-facing retryable-documents message, or `None` when there is
    /// nothing to report.
    #[must_use]
    pub(super) fn retryable_status_message(&self) -> Option<String> {
        if self.all_confirmed() {
            return None;
        }
        Some(format!(
            "Draft autosave left {} document(s) retryable (cancelled: {}, over limit: {}, write: {}).",
            self.total(),
            self.snapshot_cancelled,
            self.over_limit,
            self.body_write.len(),
        ))
    }
}

// --- orphan cleanup continuation -------------------------------------------

/// Whether a bounded orphan-cleanup pass schedules another, and when.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OrphanCleanupFollowUp {
    Stop,
    Schedule {
        manifest_offset: usize,
        delay: Duration,
        next_failure_streak: u32,
    },
}

/// Decide the continuation for one finished orphan-cleanup pass.
///
/// The exponential backoff exists so a directory that cannot be read — a removed
/// volume, a permissions change — does not retry every thirty seconds forever.
/// A pass with *no* retryable failure resets the streak, so a transient problem
/// does not leave a long backoff behind. A cursorless failure restarts from
/// offset zero rather than guessing, because a wrong offset would silently skip
/// entries the pass never inspected.
#[must_use]
pub(super) fn orphan_cleanup_follow_up(
    has_more_work: bool,
    next_manifest_offset: Option<usize>,
    retryable_failure: bool,
    failure_streak: u32,
) -> OrphanCleanupFollowUp {
    if !has_more_work {
        return OrphanCleanupFollowUp::Stop;
    }

    let next_failure_streak = if retryable_failure {
        failure_streak.saturating_add(1)
    } else {
        0
    };
    let delay = if retryable_failure {
        let exponent = next_failure_streak.saturating_sub(1).min(31);
        ORPHAN_CLEANUP_FOLLOWUP_DELAY
            .saturating_mul(1u32 << exponent)
            .min(ORPHAN_CLEANUP_MAX_FAILURE_BACKOFF)
    } else {
        ORPHAN_CLEANUP_FOLLOWUP_DELAY
    };
    OrphanCleanupFollowUp::Schedule {
        manifest_offset: next_manifest_offset.unwrap_or(0),
        delay,
        next_failure_streak,
    }
}

/// Build one grouped cleanup status message without exposing private recovery
/// contents.
#[must_use]
pub(super) fn orphan_cleanup_failure_message(
    status: usize,
    delete: usize,
    manifest: usize,
) -> String {
    format!(
        "Draft recovery cleanup preserved retryable items (status: {status}, delete: {delete}, manifest: {manifest})"
    )
}

/// Group typed cleanup failures by category and render the grouped message.
///
/// The walk lives here rather than in the journal adapter so that both the
/// enum-to-count mapping *and* the message wording stay inside the mutation
/// scope. The service enum it matches on is plain data with no GTK dependency,
/// so importing it costs policy nothing: a mis-mapped arm — counting a delete
/// failure as a status failure — would otherwise be invisible to every gate,
/// because the adapter that owned the walk is deliberately outside
/// `examine_globs`.
#[must_use]
pub(super) fn grouped_orphan_cleanup_failure_message(
    failures: &[crate::services::draft_service::DraftOrphanCleanupFailure],
) -> String {
    use crate::services::draft_service::DraftOrphanCleanupFailure;

    let mut status = 0usize;
    let mut delete = 0usize;
    let mut manifest = 0usize;
    for failure in failures {
        match failure {
            DraftOrphanCleanupFailure::Status(_) => status += 1,
            DraftOrphanCleanupFailure::Delete(_) => delete += 1,
            DraftOrphanCleanupFailure::Manifest(_) => manifest += 1,
        }
    }
    orphan_cleanup_failure_message(status, delete, manifest)
}

// --- main-thread mutation ordering -----------------------------------------

/// User intent assigned before a draft snapshot or deletion starts background work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DraftMutationIntent {
    pub(crate) draft_id: String,
    pub(crate) sequence: u64,
    pub(crate) epoch: u64,
}

/// Per-window allocator for globally ordered commands and per-draft freshness epochs.
#[derive(Debug, Default)]
pub(crate) struct DraftMutationOrder {
    next_sequence: u64,
    epochs: HashMap<String, u64>,
}

impl DraftMutationOrder {
    /// Assign intent synchronously, before document-sized or filesystem work can reorder it.
    pub(crate) fn advance(&mut self, draft_id: &str) -> DraftMutationIntent {
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let epoch = self
            .epochs
            .entry(draft_id.to_string())
            .and_modify(|epoch| *epoch = epoch.wrapping_add(1))
            .or_insert(1);
        DraftMutationIntent {
            draft_id: draft_id.to_string(),
            sequence: self.next_sequence,
            epoch: *epoch,
        }
    }

    /// Equality, rather than numeric ordering, keeps freshness correct across wraparound.
    pub(crate) fn is_current(&self, intent: &DraftMutationIntent) -> bool {
        self.epochs.get(&intent.draft_id).copied() == Some(intent.epoch)
    }

    /// Drop a completed identity only when no later user intent superseded it.
    pub(crate) fn retire_if_current(&mut self, intent: &DraftMutationIntent) {
        if self.is_current(intent) {
            self.epochs.remove(&intent.draft_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The enum-to-count mapping is pinned against **real** service variants,
    /// one category at a time, with an exact expected string.
    ///
    /// Counting a delete failure as a status failure would be invisible without
    /// this: the grouped message is the only user-visible output, and every
    /// category renders into the same sentence shape. Distinct multiplicities
    /// (1 status, 2 delete, 3 manifest) mean a swapped pair of arms cannot
    /// produce the expected text.
    #[test]
    fn grouped_cleanup_message_maps_each_service_variant_to_its_own_category() {
        use crate::services::draft_service::{
            DraftOrphanCleanupDeleteError, DraftOrphanCleanupFailure,
            DraftOrphanCleanupManifestError, DraftOrphanCleanupStatusError,
        };
        use std::path::PathBuf;

        let status = DraftOrphanCleanupFailure::Status(DraftOrphanCleanupStatusError {
            path: PathBuf::from("/drafts/a.draft"),
            detail: "denied".to_string(),
        });
        let delete = DraftOrphanCleanupFailure::Delete(DraftOrphanCleanupDeleteError {
            draft_id: "a".to_string(),
            path: PathBuf::from("/drafts/a.draft"),
            detail: "busy".to_string(),
        });
        let manifest =
            DraftOrphanCleanupFailure::Manifest(DraftOrphanCleanupManifestError::Write {
                path: PathBuf::from("/drafts/manifest.json"),
                detail: "no space".to_string(),
            });

        let failures = vec![
            status,
            delete.clone(),
            delete,
            manifest.clone(),
            manifest.clone(),
            manifest,
        ];

        assert_eq!(
            grouped_orphan_cleanup_failure_message(&failures),
            "Draft recovery cleanup preserved retryable items \
             (status: 1, delete: 2, manifest: 3)"
                .replace("             ", ""),
        );
    }

    /// No failures still renders zeros rather than panicking or omitting a
    /// category, because the caller only reaches this path when it has a
    /// grouped status to report.
    #[test]
    fn grouped_cleanup_message_with_no_failures_reports_zero_for_every_category() {
        assert_eq!(
            grouped_orphan_cleanup_failure_message(&[]),
            "Draft recovery cleanup preserved retryable items (status: 0, delete: 0, manifest: 0)"
        );
    }

    /// A manifest `Load` failure counts in the same category as a `Write`
    /// failure: both mean the durable record could not be advanced.
    #[test]
    fn grouped_cleanup_message_counts_both_manifest_variants_as_manifest() {
        use crate::services::draft_service::{
            DraftOrphanCleanupFailure, DraftOrphanCleanupManifestError,
        };
        use std::path::PathBuf;

        let failures = vec![
            DraftOrphanCleanupFailure::Manifest(DraftOrphanCleanupManifestError::Load {
                path: PathBuf::from("/drafts/manifest.json"),
                detail: "corrupt".to_string(),
            }),
            DraftOrphanCleanupFailure::Manifest(DraftOrphanCleanupManifestError::Write {
                path: PathBuf::from("/drafts/manifest.json"),
                detail: "no space".to_string(),
            }),
        ];

        assert_eq!(
            grouped_orphan_cleanup_failure_message(&failures),
            "Draft recovery cleanup preserved retryable items (status: 0, delete: 0, manifest: 2)"
        );
    }

    #[test]
    fn an_incomplete_installation_is_never_an_autosave_candidate() {
        // The data-safety guard. Everything else about the tab says "write me".
        assert!(!draft_candidate_is_eligible(true, true, false, true, true));
        assert!(!draft_candidate_is_eligible(true, true, false, true, false));
        // And the same tab without that flag is eligible.
        assert!(draft_candidate_is_eligible(true, true, false, false, true));
    }

    #[test]
    fn autosave_requires_draft_dirty_while_close_takes_every_modified_tab() {
        // Modified but the draft is already current: autosave skips, close writes.
        assert!(!draft_candidate_is_eligible(
            true, false, false, false, true
        ));
        assert!(draft_candidate_is_eligible(
            true, false, false, false, false
        ));
    }

    #[test]
    fn an_unmodified_or_evicted_tab_is_never_a_candidate() {
        assert!(!draft_candidate_is_eligible(
            false, true, false, false, true
        ));
        assert!(!draft_candidate_is_eligible(
            false, true, false, false, false
        ));
        assert!(!draft_candidate_is_eligible(true, true, true, false, true));
        assert!(!draft_candidate_is_eligible(true, true, true, false, false));
    }

    #[test]
    fn a_captured_snapshot_is_rejected_on_every_identity_dimension() {
        assert!(captured_snapshot_is_current(
            Some("draft-a"),
            "draft-a",
            7,
            7,
            true,
            false,
            false
        ));
        // Identity swapped, or lost entirely.
        assert!(!captured_snapshot_is_current(
            Some("draft-b"),
            "draft-a",
            7,
            7,
            true,
            false,
            false
        ));
        assert!(!captured_snapshot_is_current(
            None, "draft-a", 7, 7, true, false, false
        ));
        // A newer edit landed during the capture.
        assert!(!captured_snapshot_is_current(
            Some("draft-a"),
            "draft-a",
            8,
            7,
            true,
            false,
            false
        ));
        // Went clean, was evicted, or a load installation was cancelled mid-capture.
        assert!(!captured_snapshot_is_current(
            Some("draft-a"),
            "draft-a",
            7,
            7,
            false,
            false,
            false
        ));
        assert!(!captured_snapshot_is_current(
            Some("draft-a"),
            "draft-a",
            7,
            7,
            true,
            true,
            false
        ));
        assert!(!captured_snapshot_is_current(
            Some("draft-a"),
            "draft-a",
            7,
            7,
            true,
            false,
            true
        ));
    }

    #[test]
    fn an_autosave_tick_marks_pending_rather_than_queueing() {
        assert_eq!(
            autosave_admission(false, false, false),
            AutosaveAdmission::Start
        );
        for (autosave, mutation, cleanup) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (true, true, true),
        ] {
            assert_eq!(
                autosave_admission(autosave, mutation, cleanup),
                AutosaveAdmission::MarkPending
            );
        }
    }

    #[test]
    fn close_flush_waits_for_any_lane_owner() {
        assert!(!close_flush_must_wait(false, false, false, false));
        for (m, c, d, r) in [
            (true, false, false, false),
            (false, true, false, false),
            (false, false, true, false),
            (false, false, false, true),
        ] {
            assert!(close_flush_must_wait(m, c, d, r));
        }
    }

    #[test]
    fn readiness_is_blocked_by_every_recovery_lane() {
        assert!(!draft_workflow_blocks_readiness(
            false, false, false, false, false, false
        ));
        for index in 0..6 {
            let mut flags = [false; 6];
            flags[index] = true;
            assert!(
                draft_workflow_blocks_readiness(
                    flags[0], flags[1], flags[2], flags[3], flags[4], flags[5]
                ),
                "flag {index} must block readiness"
            );
        }
    }

    #[test]
    fn pipeline_failures_report_every_category_and_stay_silent_when_clean() {
        let clean = DraftPipelineFailures::default();
        assert!(clean.all_confirmed());
        assert_eq!(clean.total(), 0);
        assert!(clean.retryable_status_message().is_none());

        let failures = DraftPipelineFailures {
            snapshot_cancelled: 2,
            over_limit: 1,
            body_write: vec!["disk full".to_string()],
        };
        assert!(!failures.all_confirmed());
        assert_eq!(failures.total(), 4);
        let message = failures
            .retryable_status_message()
            .expect("failures report a message");
        assert!(message.contains("4 document(s)"));
        assert!(message.contains("cancelled: 2"));
        assert!(message.contains("over limit: 1"));
        assert!(message.contains("write: 1"));
    }

    #[test]
    fn cleanup_follow_up_stops_when_there_is_nothing_left() {
        assert_eq!(
            orphan_cleanup_follow_up(false, Some(9), true, 5),
            OrphanCleanupFollowUp::Stop
        );
    }

    #[test]
    fn cleanup_follow_up_resumes_pagination_and_resets_a_clean_streak() {
        assert_eq!(
            orphan_cleanup_follow_up(true, Some(42), false, 7),
            OrphanCleanupFollowUp::Schedule {
                manifest_offset: 42,
                delay: ORPHAN_CLEANUP_FOLLOWUP_DELAY,
                next_failure_streak: 0,
            }
        );
    }

    #[test]
    fn a_cursorless_failure_restarts_from_zero_rather_than_guessing() {
        assert_eq!(
            orphan_cleanup_follow_up(true, None, true, 0),
            OrphanCleanupFollowUp::Schedule {
                manifest_offset: 0,
                delay: ORPHAN_CLEANUP_FOLLOWUP_DELAY,
                next_failure_streak: 1,
            }
        );
    }

    #[test]
    fn cleanup_backoff_doubles_and_then_caps() {
        let OrphanCleanupFollowUp::Schedule { delay, .. } =
            orphan_cleanup_follow_up(true, Some(0), true, 1)
        else {
            panic!("a retryable failure schedules a follow-up");
        };
        assert_eq!(delay, ORPHAN_CLEANUP_FOLLOWUP_DELAY * 2);

        let OrphanCleanupFollowUp::Schedule { delay, .. } =
            orphan_cleanup_follow_up(true, Some(0), true, 999)
        else {
            panic!("a retryable failure schedules a follow-up");
        };
        assert_eq!(delay, ORPHAN_CLEANUP_MAX_FAILURE_BACKOFF);
        // Pinned as a concrete duration, not only against the constant: asserting
        // `delay == CONST` alone cannot detect the constant itself changing,
        // because both sides move together. Fifteen minutes is the contract — a
        // user whose storage came back should not wait an hour for cleanup to
        // resume, and a permanently unavailable volume should not be retried
        // every thirty seconds forever.
        assert_eq!(
            ORPHAN_CLEANUP_MAX_FAILURE_BACKOFF,
            Duration::from_secs(900),
            "the retry backoff cap is fifteen minutes"
        );
        // And the base delay it caps, for the same reason.
        assert_eq!(ORPHAN_CLEANUP_FOLLOWUP_DELAY, Duration::from_secs(30));
    }

    #[test]
    fn cleanup_failure_message_groups_categories_without_naming_files() {
        let message = orphan_cleanup_failure_message(1, 2, 3);
        assert!(message.contains("status: 1"));
        assert!(message.contains("delete: 2"));
        assert!(message.contains("manifest: 3"));
        assert!(!message.contains('/'), "no path may reach the user");
    }

    #[test]
    fn later_delete_invalidates_older_autosave_intent() {
        let mut order = DraftMutationOrder::default();
        let autosave = order.advance("draft-a");
        let delete = order.advance("draft-a");

        assert!(!order.is_current(&autosave));
        assert!(order.is_current(&delete));
        assert_eq!(delete.sequence, autosave.sequence + 1);
    }

    #[test]
    fn later_edit_can_create_recovery_after_delete() {
        let mut order = DraftMutationOrder::default();
        let _autosave = order.advance("draft-a");
        let delete = order.advance("draft-a");
        let later_edit = order.advance("draft-a");

        assert!(!order.is_current(&delete));
        assert!(order.is_current(&later_edit));
    }

    #[test]
    fn one_draft_does_not_invalidate_another() {
        let mut order = DraftMutationOrder::default();
        let first = order.advance("draft-a");
        let second = order.advance("draft-b");

        assert!(order.is_current(&first));
        assert!(order.is_current(&second));
        assert!(second.sequence > first.sequence);
    }

    #[test]
    fn wraparound_uses_exact_epoch_equality() {
        let mut order = DraftMutationOrder {
            next_sequence: u64::MAX,
            epochs: HashMap::from([("draft-a".to_string(), u64::MAX)]),
        };
        let wrapped = order.advance("draft-a");

        assert_eq!(wrapped.sequence, 0);
        assert_eq!(wrapped.epoch, 0);
        assert!(order.is_current(&wrapped));
        assert!(!order.is_current(&DraftMutationIntent {
            draft_id: "draft-a".to_string(),
            sequence: u64::MAX,
            epoch: u64::MAX,
        }));
    }

    #[test]
    fn completed_delete_retires_only_the_current_identity() {
        let mut order = DraftMutationOrder::default();
        let current = order.advance("draft-a");
        let stale = order.advance("draft-b");
        let later = order.advance("draft-b");

        order.retire_if_current(&current);
        order.retire_if_current(&stale);

        assert!(!order.is_current(&current));
        assert!(order.is_current(&later));
    }
}
