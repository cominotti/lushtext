// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decisions for the bounded whole-buffer replacement workflow.
//!
//! This module owns the decisions that determine **whether a partially mutated
//! buffer can be left visible** and **which terminal a caller is told about**.
//! Those are the workflow's own decisions, distinct from the cross-cutting
//! sizing and paragraph-boundary arithmetic in
//! [`crate::model::buffer_replacement`], which this workflow **calls** and never
//! copies: forking a shared limit lets it drift while both copies still read as
//! correct.
//!
//! Nothing here touches GTK, so the workflow's freshness, phase, cancellation,
//! and terminal rules stay inside the default mutation scope.

/// Workflow family that owns one replacement ticket.
///
/// Part of the caller-facing seam: the ticket a caller hands in comes back on
/// its terminal, so a workflow can recognise its own completion and refuse
/// another's.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferReplacementWorkflow {
    MemoryEviction,
    DraftRecovery,
    LocalHistoryRestore,
    LocalHistoryUndo,
    SaveFormatting,
    #[cfg(feature = "test-utils")]
    Test,
}

/// Caller-owned freshness identity carried through every scheduled turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferReplacementTicket {
    pub workflow: BufferReplacementWorkflow,
    pub generation: u64,
}

/// Why one replacement stopped without publishing successful terminal state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferReplacementCancelReason {
    /// The caller's own freshness check no longer recognises the editor.
    Stale,
    /// A newer request for the same editor took ownership.
    Superseded,
    /// The editor is leaving the widget hierarchy.
    Disposed,
}

/// Scalar boundedness and cleanup evidence for one replacement.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BufferReplacementMetrics {
    pub slice_count: u64,
    pub cleared_characters: u64,
    pub inserted_bytes: usize,
    pub peak_retained_bodies: usize,
}

impl BufferReplacementMetrics {
    /// Metrics for a request that has taken ownership of exactly one body.
    #[must_use]
    pub(super) const fn for_one_retained_body() -> Self {
        Self {
            slice_count: 0,
            cleared_characters: 0,
            inserted_bytes: 0,
            peak_retained_bodies: 1,
        }
    }

    /// Account for one completed clear turn.
    ///
    /// Saturating rather than wrapping: a document large enough to overflow the
    /// count would report a wildly low figure, and boundedness evidence that
    /// wraps is worse than evidence that saturates.
    pub(super) fn record_cleared_slice(&mut self, deleted_characters: u64) {
        self.cleared_characters = self.cleared_characters.saturating_add(deleted_characters);
        self.slice_count = self.slice_count.saturating_add(1);
    }

    /// Account for one completed insertion turn ending at `installed_end`.
    ///
    /// The high-water mark rather than a sum: a superseded turn can re-insert
    /// from an earlier offset, and adding those bytes would overstate the work.
    pub(super) fn record_inserted_slice(&mut self, installed_end: usize) {
        self.inserted_bytes = self.inserted_bytes.max(installed_end);
        self.slice_count = self.slice_count.saturating_add(1);
    }

    /// Account for a direct, single-turn replacement.
    pub(super) fn record_direct_replacement(&mut self, cleared_characters: u64, inserted: usize) {
        self.cleared_characters = cleared_characters;
        self.inserted_bytes = inserted;
    }
}

/// Which bounded turn the active session will run next.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplacementPhase {
    /// Emptying the previous content before the new body is installed.
    Clearing,
    /// Appending the new body one paragraph-aligned slice at a time.
    Installing,
    /// Emptying a partially mutated buffer after cancellation, because a
    /// half-installed document must never be left visible.
    ClearingCancelled,
    /// Terminal reached; no further turn may mutate the buffer.
    Finalizing,
}

/// Whether a new request may start immediately or must wait its turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StartDisposition {
    /// No session owns the editor, so this request begins now.
    Immediately,
    /// A session is still cleaning up; park this request as the latest intent.
    ParkAsPending,
}

/// Decide whether a request may begin against the editor's current ownership.
///
/// Called after any active session has been asked to cancel: a session whose
/// cancellation reached its terminal synchronously has already released
/// ownership, and the new request must not be parked behind nothing.
#[must_use]
pub(super) const fn start_disposition(editor_has_active_session: bool) -> StartDisposition {
    if editor_has_active_session {
        StartDisposition::ParkAsPending
    } else {
        StartDisposition::Immediately
    }
}

/// How far a cancelled session must go before it may publish its terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CancelDisposition {
    /// Finish now: nothing was mutated, or the widget is going away and no
    /// visible state can be repaired.
    FinishImmediately,
    /// Clear the partial buffer in bounded turns first.
    ClearPartialBuffer,
}

/// Decide what a cancellation owes the user before it reports a terminal.
///
/// The load-bearing case is the third: a session that already started mutating
/// the buffer holds neither the old document nor the new one, so it must finish
/// emptying before anyone sees it. Disposal is exempt because the page is
/// leaving the hierarchy and there is nothing left to repair.
#[must_use]
pub(super) const fn cancel_disposition(
    disposing: bool,
    mutation_started: bool,
) -> CancelDisposition {
    if disposing || !mutation_started {
        CancelDisposition::FinishImmediately
    } else {
        CancelDisposition::ClearPartialBuffer
    }
}

/// What the session does after one clear turn finishes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClearProgress {
    /// Content remains; schedule another clear turn.
    ContinueClearing,
    /// The buffer is empty and there is no body to install.
    Finish,
    /// The buffer is empty; begin installing the body.
    BeginInstalling,
}

/// Decide the next step after a clear turn.
#[must_use]
pub(super) const fn after_clear_slice(buffer_emptied: bool, body_is_empty: bool) -> ClearProgress {
    if !buffer_emptied {
        ClearProgress::ContinueClearing
    } else if body_is_empty {
        ClearProgress::Finish
    } else {
        ClearProgress::BeginInstalling
    }
}

/// Decide whether an insertion turn was the last one.
#[must_use]
pub(super) const fn insertion_is_complete(installed_end: usize, body_len: usize) -> bool {
    installed_end >= body_len
}

/// Whether a terminal must restore the guard it installed.
///
/// A disposed page must not have its editability, cursor, syntax highlighting,
/// or file monitor restored: the widget is being torn down, and touching those
/// would resurrect projections against a dying buffer.
#[must_use]
pub(super) const fn guard_restores_on_terminal(
    cancellation: Option<BufferReplacementCancelReason>,
) -> bool {
    !matches!(cancellation, Some(BufferReplacementCancelReason::Disposed))
}

/// Whether a scheduled turn may still touch the buffer.
///
/// A cancelled clear is deliberately allowed to run even when the caller's own
/// freshness check has gone stale: the partial buffer still has to be emptied,
/// and refusing here would leave a half-installed document on screen.
#[must_use]
pub(super) const fn turn_may_run(phase: ReplacementPhase, caller_is_current: bool) -> bool {
    matches!(phase, ReplacementPhase::ClearingCancelled) || caller_is_current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_owned_editor_parks_the_newer_request() {
        assert_eq!(
            start_disposition(true),
            StartDisposition::ParkAsPending,
            "a session still cleaning up owns the editor"
        );
        assert_eq!(
            start_disposition(false),
            StartDisposition::Immediately,
            "a synchronously terminated cancellation must not park the newcomer behind nothing"
        );
    }

    #[test]
    fn only_a_started_mutation_owes_the_user_a_clear_pass() {
        assert_eq!(
            cancel_disposition(false, true),
            CancelDisposition::ClearPartialBuffer,
            "a half-installed document must not be left visible"
        );
        assert_eq!(
            cancel_disposition(false, false),
            CancelDisposition::FinishImmediately
        );
        // Disposal wins over a started mutation: nothing visible remains to fix.
        assert_eq!(
            cancel_disposition(true, true),
            CancelDisposition::FinishImmediately
        );
        assert_eq!(
            cancel_disposition(true, false),
            CancelDisposition::FinishImmediately
        );
    }

    #[test]
    fn clear_progress_distinguishes_a_clear_only_request_from_an_install() {
        assert_eq!(
            after_clear_slice(false, false),
            ClearProgress::ContinueClearing
        );
        assert_eq!(
            after_clear_slice(false, true),
            ClearProgress::ContinueClearing,
            "an empty body still has to finish emptying the buffer first"
        );
        assert_eq!(after_clear_slice(true, true), ClearProgress::Finish);
        assert_eq!(
            after_clear_slice(true, false),
            ClearProgress::BeginInstalling
        );
    }

    #[test]
    fn insertion_completes_only_at_or_past_the_body_end() {
        assert!(!insertion_is_complete(0, 10));
        assert!(!insertion_is_complete(9, 10));
        assert!(insertion_is_complete(10, 10));
        // `>=` rather than `==`: an empty body is complete at offset zero.
        assert!(insertion_is_complete(0, 0));
    }

    #[test]
    fn only_disposal_skips_guard_restoration() {
        assert!(guard_restores_on_terminal(None));
        assert!(guard_restores_on_terminal(Some(
            BufferReplacementCancelReason::Stale
        )));
        assert!(guard_restores_on_terminal(Some(
            BufferReplacementCancelReason::Superseded
        )));
        assert!(!guard_restores_on_terminal(Some(
            BufferReplacementCancelReason::Disposed
        )));
    }

    #[test]
    fn a_cancelled_clear_runs_even_after_the_caller_goes_stale() {
        assert!(turn_may_run(ReplacementPhase::ClearingCancelled, false));
        assert!(turn_may_run(ReplacementPhase::ClearingCancelled, true));
        assert!(!turn_may_run(ReplacementPhase::Clearing, false));
        assert!(!turn_may_run(ReplacementPhase::Installing, false));
        assert!(turn_may_run(ReplacementPhase::Clearing, true));
        assert!(turn_may_run(ReplacementPhase::Installing, true));
    }

    #[test]
    fn metrics_saturate_and_take_the_installed_high_water_mark() {
        let mut metrics = BufferReplacementMetrics::for_one_retained_body();
        assert_eq!(metrics.peak_retained_bodies, 1);

        metrics.record_cleared_slice(7);
        metrics.record_cleared_slice(5);
        assert_eq!(metrics.cleared_characters, 12);
        assert_eq!(metrics.slice_count, 2);

        metrics.record_inserted_slice(100);
        // A superseded turn re-inserting from an earlier offset must not add.
        metrics.record_inserted_slice(40);
        assert_eq!(metrics.inserted_bytes, 100);
        assert_eq!(metrics.slice_count, 4);

        let mut saturating = BufferReplacementMetrics {
            cleared_characters: u64::MAX,
            slice_count: u64::MAX,
            ..BufferReplacementMetrics::default()
        };
        saturating.record_cleared_slice(1);
        assert_eq!(saturating.cleared_characters, u64::MAX);
        assert_eq!(saturating.slice_count, u64::MAX);
    }

    #[test]
    fn direct_replacement_records_both_sides_exactly() {
        let mut metrics = BufferReplacementMetrics::for_one_retained_body();
        metrics.record_direct_replacement(21, 34);
        assert_eq!(metrics.cleared_characters, 21);
        assert_eq!(metrics.inserted_bytes, 34);
        // A direct replacement is one turn, and the caller counts it separately.
        assert_eq!(metrics.slice_count, 0);
    }
}
