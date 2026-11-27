// SPDX-License-Identifier: GPL-3.0-or-later

//! Retirement classification and lifecycle accounting for released file indexes.
//!
//! **What this module owns, stated plainly:** it decides *which retirement lane*
//! a released index qualifies for, and it records that. It does **not** perform
//! the retirement. The actual off-GTK destruction of a document-sized index is
//! performed by the disposal lane (`ui::plain_disposal`), driven by the
//! `DisposalOwned` drop at the `index_execution` call sites. This module is the
//! classification and accounting half of the palette's `retirement` role; the
//! disposal lane is the executing half, and it is cross-cutting rather than
//! palette-owned.
//!
//! Only one of the palette's two stage orders retires anything, so this module
//! needs no stage-order qualifier. A file index is released in three places —
//! a full workspace-folder replacement, the base index a winning incremental
//! batch supersedes, and the output of an incremental batch that lost its
//! generation race — and all three ask the same question: did a *last-owned*
//! index *at the policy cap* reach the bounded worker lane? That predicate lives
//! in [`policy::classify_index_retirement`], shared by all three call sites so
//! the conjunction is written once.
//!
//! There is no control inversion here. The `Arc` drop that hands the index to
//! the disposal lane happens inline at the call site, *before*
//! [`record_file_index_retirement`] is told what happened, because the strong
//! count has to be read while the previous index is still alive.
//!
//! The counters are process-global rather than per-widget, and that is
//! deliberate: they answer "did this process ever observe a last-owned at-cap
//! retirement", which is a monotonic lifecycle fact a per-widget evidence field
//! cannot express. They are therefore classified as lifecycle probes and are
//! **not** folded into [`super::evidence::CommandPaletteEvidence`]. See
//! `docs/workflow-readability-matrix.md`, row `WFR-COMMAND-PALETTE`.

use super::policy::{self, FileIndexRetirementKind};

/// Scalar evidence that last-owned indexes reached the bounded worker lane.
#[cfg(feature = "test-utils")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileIndexRetirementSnapshot {
    /// Last-owned full replacements destroyed on the worker lane.
    pub full_replacements: usize,
    /// Last-owned accepted incremental bases destroyed on the worker lane.
    pub accepted_incremental: usize,
    /// Last-owned rejected incremental outputs destroyed on the worker lane.
    pub rejected_incremental: usize,
}

#[cfg(feature = "test-utils")]
static FULL_REPLACEMENT_RETIREMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static ACCEPTED_INCREMENTAL_RETIREMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "test-utils")]
static REJECTED_INCREMENTAL_RETIREMENTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Record one released index against the retirement lane it qualifies for.
///
/// `last_owned` is the strong-count observation taken while the released index
/// was still alive, and `released_len` is that index's file count. The
/// classification itself is pure policy.
pub(super) fn record_file_index_retirement(
    kind: FileIndexRetirementKind,
    last_owned: bool,
    released_len: usize,
) {
    let classified = policy::classify_index_retirement(kind, last_owned, released_len);
    #[cfg(feature = "test-utils")]
    if let Some(kind) = classified {
        use std::sync::atomic::Ordering;

        let counter = match kind {
            FileIndexRetirementKind::FullReplacement => &FULL_REPLACEMENT_RETIREMENTS,
            FileIndexRetirementKind::AcceptedIncremental => &ACCEPTED_INCREMENTAL_RETIREMENTS,
            FileIndexRetirementKind::RejectedIncremental => &REJECTED_INCREMENTAL_RETIREMENTS,
        };
        counter.fetch_add(1, Ordering::Release);
    }
    // A production build has no counters to increment, but the classification
    // still runs so both builds exercise the same pure decision. It is a `const
    // fn` over three scalars, so it costs nothing once inlined.
    #[cfg(not(feature = "test-utils"))]
    let _ = classified;
}

/// Return process-local retirement evidence for deterministic widget tests.
#[cfg(feature = "test-utils")]
#[must_use]
pub fn file_index_retirement_snapshot_for_test() -> FileIndexRetirementSnapshot {
    use std::sync::atomic::Ordering;

    FileIndexRetirementSnapshot {
        full_replacements: FULL_REPLACEMENT_RETIREMENTS.load(Ordering::Acquire),
        accepted_incremental: ACCEPTED_INCREMENTAL_RETIREMENTS.load(Ordering::Acquire),
        rejected_incremental: REJECTED_INCREMENTAL_RETIREMENTS.load(Ordering::Acquire),
    }
}
