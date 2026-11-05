// SPDX-License-Identifier: GPL-3.0-or-later

//! The paced re-analysis of an edit (`bound-context-flip-keystrokes`, C6c).
//!
//! After an edit in a highlighted document, GtkSourceView's context engine
//! re-analyses from the edited line until its analysis merges with the
//! previous one. It does that in one **first update** of up to 10 ms
//! (`G_PRIORITY_HIGH_IDLE`) and, when that slice runs out, in a
//! `G_PRIORITY_LOW` worker of 30 ms slices (ledger A69). A key that flips the
//! syntax context of the rest of the document (an opening comment with no
//! closer after it) re-analyses to the end; in JavaScript and TypeScript an
//! ordinary key re-analyses most of what follows it (ledger A85).
//!
//! The editor takes that work over: from an idle one priority above the
//! first update it calls `gtk_source_buffer_ensure_highlight` on an empty
//! range, which analyses from the invalid point to a bound it chooses and
//! tags nothing, in **steps** sized towards [`STEP_TARGET_US`]. When a step's
//! report ends before its bound, the analysis merged and the first update
//! finds nothing left. When the steps of one pacing session have spent
//! [`REANALYSIS_ALLOWANCE_US`] without merging, the edit is one that on its
//! own would have needed the engine's worker: syntax highlighting is
//! removed for the session (the `DocumentScale` reduction
//! `ReductionReason::EditReanalysis`) before any worker slice runs.
//!
//! This module is the GTK-free rule: what a step's report means, the next
//! step's size, and when to stop. Kani proves (`reanalysis_pacing/kani_proofs.rs`)
//! that a session never continues past its allowance or [`MAX_STEPS`], that
//! every step size stays within [`MIN_STEP_CHARS`] and [`MAX_STEP_CHARS`] and
//! grows at most twofold per step, and that a session that never merges ends
//! in a downgrade within [`MAX_STEPS`] steps.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use super::responsiveness_budget::{B_INTERACT_US, B_SETTLE_US};

/// The re-analysis one pacing session may spend before syntax highlighting
/// is removed, µs: `B_SETTLE_US`, the budget a key that flips the syntax
/// context of the rest of the document is allowed (`bound-context-flip-keystrokes`
/// design D2).
///
/// Every step stays within `B_INTERACT_US`; the allowance bounds how long
/// the frame after the edit waits behind the steps. The engine's own
/// first-update slice (`FIRST_UPDATE_TIME_SLICE`, 10 ms) was measured as the
/// allowance first and rejected: it removed highlighting after flipping keys
/// that cost 11–12 ms on `main`, within `B_SETTLE_US` (design D2).
pub const REANALYSIS_ALLOWANCE_US: u64 = B_SETTLE_US;

/// The duration one step is sized towards, µs: a quarter of
/// `B_INTERACT_US`, so a step that analyses twice as slowly as the one
/// before it still fits the interaction budget.
pub const STEP_TARGET_US: u64 = B_INTERACT_US / 4;

/// Characters the first step of a session analyses.
///
/// Kind: coverage window, a power of two. The slowest grammar measured
/// (TypeScript, about 2.5 µs per character on the reference machine,
/// ledger A85) analyses it in about 1.3 ms.
pub const FIRST_STEP_CHARS: u32 = 512;

/// The smallest step, characters. Kind: cap, a power of two.
pub const MIN_STEP_CHARS: u32 = 128;

/// The largest step, characters. Kind: cap, a power of two.
pub const MAX_STEP_CHARS: u32 = 1 << 20;

/// The most steps one session runs before it is treated as one that does
/// not merge (a liveness bound for a clock that reports zero durations).
/// Kind: cap, a power of two.
pub const MAX_STEPS: u32 = 256;

/// One pacing session: from the first edit after the analysis was complete
/// until the analysis merges or the session downgrades. Later edits during
/// the session join it and do not reset its spending.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PacingSession {
    spent_us: u64,
    step_chars: u32,
    steps: u32,
}

impl PacingSession {
    /// A session before its first step.
    pub const START: Self = Self {
        spent_us: 0,
        step_chars: FIRST_STEP_CHARS,
        steps: 0,
    };

    /// Microseconds the session's steps have taken.
    #[must_use]
    pub const fn spent_us(self) -> u64 {
        self.spent_us
    }

    /// Characters the next step analyses.
    #[must_use]
    pub const fn step_chars(self) -> u32 {
        self.step_chars
    }

    /// Steps run.
    #[must_use]
    pub const fn steps(self) -> u32 {
        self.steps
    }
}

impl Default for PacingSession {
    fn default() -> Self {
        Self::START
    }
}

/// What one step's `highlight-updated` report said.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepReport {
    /// The analysis merged with the previous one before the bound, reached
    /// the buffer end, or had nothing to analyse (no report).
    Merged,
    /// The analysis reached the step's bound without merging.
    ReachedBound {
        /// Characters the report covered.
        analysed_chars: u64,
        /// Microseconds the step took.
        took_us: u64,
    },
}

/// What the session does next.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PacingNext {
    /// The analysis is complete; the session ends.
    Finished,
    /// Run another step with this session.
    Continue(PacingSession),
    /// The allowance (or [`MAX_STEPS`]) is spent: remove syntax highlighting.
    Downgrade,
}

/// The next step's size after one that analysed `analysed_chars` in
/// `took_us`: the size that would take [`STEP_TARGET_US`] at that rate,
/// at most twice `current`, within [`MIN_STEP_CHARS`] and
/// [`MAX_STEP_CHARS`].
#[must_use]
pub const fn next_step_chars(current: u32, analysed_chars: u64, took_us: u64) -> u32 {
    let took = if took_us == 0 { 1 } else { took_us };
    let scaled = analysed_chars.saturating_mul(STEP_TARGET_US) / took;
    let doubled = (current as u64).saturating_mul(2);
    let grown_cap = if doubled > MAX_STEP_CHARS as u64 {
        MAX_STEP_CHARS as u64
    } else {
        doubled
    };
    let capped = if scaled > grown_cap {
        grown_cap
    } else {
        scaled
    };
    let clamped = if capped < MIN_STEP_CHARS as u64 {
        MIN_STEP_CHARS as u64
    } else {
        capped
    };
    // `clamped` is at most MAX_STEP_CHARS, which fits u32.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "clamped is at most MAX_STEP_CHARS (2^20), which fits u32; const fn cannot use try_from"
    )]
    let narrowed = clamped as u32;
    narrowed
}

/// The session after one step reported `report`, against
/// [`REANALYSIS_ALLOWANCE_US`].
#[must_use]
pub const fn after_step(session: PacingSession, report: StepReport) -> PacingNext {
    after_step_within(session, report, REANALYSIS_ALLOWANCE_US)
}

/// [`after_step`] against `allowance_us`; the editor passes the shipped
/// allowance unless a widget test overrides it (`test-utils`).
#[must_use]
pub const fn after_step_within(
    session: PacingSession,
    report: StepReport,
    allowance_us: u64,
) -> PacingNext {
    match report {
        StepReport::Merged => PacingNext::Finished,
        StepReport::ReachedBound {
            analysed_chars,
            took_us,
        } => {
            let spent_us = session.spent_us.saturating_add(took_us);
            let steps = session.steps.saturating_add(1);
            if spent_us > allowance_us || steps >= MAX_STEPS {
                PacingNext::Downgrade
            } else {
                PacingNext::Continue(PacingSession {
                    spent_us,
                    step_chars: next_step_chars(session.step_chars, analysed_chars, took_us),
                    steps,
                })
            }
        }
    }
}

/// The allowance the editor applies: [`REANALYSIS_ALLOWANCE_US`] unless a
/// test holds [`override_reanalysis_allowance_for_test`].
#[cfg(feature = "test-utils")]
#[must_use]
pub fn reanalysis_allowance_us() -> u64 {
    match ALLOWANCE_FOR_TESTS.load(std::sync::atomic::Ordering::Relaxed) {
        0 => REANALYSIS_ALLOWANCE_US,
        allowance => allowance,
    }
}

/// The allowance the editor applies: always [`REANALYSIS_ALLOWANCE_US`] in a
/// build without `test-utils`.
#[cfg(not(feature = "test-utils"))]
#[must_use]
pub const fn reanalysis_allowance_us() -> u64 {
    REANALYSIS_ALLOWANCE_US
}

#[cfg(feature = "test-utils")]
static ALLOWANCE_FOR_TESTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Give the pacing an allowance of `allowance_us` until the returned guard
/// drops, so a widget test can drive the downgrade on any machine without a
/// document whose re-analysis only a slow machine exceeds. Process-wide: the
/// widget harness runs one test at a time.
#[cfg(feature = "test-utils")]
#[must_use = "the allowance returns to the shipped one when the guard drops"]
pub fn override_reanalysis_allowance_for_test(allowance_us: u64) -> ReanalysisAllowanceGuard {
    ALLOWANCE_FOR_TESTS.store(allowance_us.max(1), std::sync::atomic::Ordering::Relaxed);
    ReanalysisAllowanceGuard(())
}

/// Restores the shipped allowance when dropped.
#[cfg(feature = "test-utils")]
#[derive(Debug)]
pub struct ReanalysisAllowanceGuard(());

#[cfg(feature = "test-utils")]
impl Drop for ReanalysisAllowanceGuard {
    fn drop(&mut self) {
        ALLOWANCE_FOR_TESTS.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_merged_step_finishes_the_session() {
        assert_eq!(
            after_step(PacingSession::START, StepReport::Merged),
            PacingNext::Finished
        );
    }

    #[test]
    fn a_bounded_step_within_the_allowance_continues_with_its_spending() {
        let next = after_step(
            PacingSession::START,
            StepReport::ReachedBound {
                analysed_chars: 512,
                took_us: 1_000,
            },
        );
        let PacingNext::Continue(session) = next else {
            panic!("expected Continue, got {next:?}");
        };
        assert_eq!(session.spent_us(), 1_000);
        assert_eq!(session.steps(), 1);
        // 512 chars per ms towards 2 ms is 1024, the doubling cap.
        assert_eq!(session.step_chars(), 1_024);
    }

    #[test]
    fn spending_past_the_allowance_downgrades() {
        let session = PacingSession {
            spent_us: REANALYSIS_ALLOWANCE_US - 100,
            step_chars: 4_096,
            steps: 5,
        };
        assert_eq!(
            after_step(
                session,
                StepReport::ReachedBound {
                    analysed_chars: 4_096,
                    took_us: 101,
                }
            ),
            PacingNext::Downgrade
        );
        assert!(matches!(
            after_step(
                session,
                StepReport::ReachedBound {
                    analysed_chars: 4_096,
                    took_us: 100,
                }
            ),
            PacingNext::Continue(_)
        ));
    }

    #[test]
    fn the_step_count_bounds_a_session_whose_clock_reports_nothing() {
        let mut session = PacingSession::START;
        for _ in 0..MAX_STEPS - 1 {
            match after_step(
                session,
                StepReport::ReachedBound {
                    analysed_chars: 1,
                    took_us: 0,
                },
            ) {
                PacingNext::Continue(next) => session = next,
                other => panic!("stopped early: {other:?}"),
            }
        }
        assert_eq!(
            after_step(
                session,
                StepReport::ReachedBound {
                    analysed_chars: 1,
                    took_us: 0,
                }
            ),
            PacingNext::Downgrade
        );
    }

    #[test]
    fn step_sizes_follow_the_measured_rate_within_their_bounds() {
        // Slow: 256 chars in 4 ms towards 2 ms is 128.
        assert_eq!(next_step_chars(512, 256, 4_000), 128);
        // Very slow: clamped up to the minimum.
        assert_eq!(next_step_chars(512, 10, 8_000), MIN_STEP_CHARS);
        // Fast: at most twice the current step.
        assert_eq!(next_step_chars(512, 100_000, 100), 1_024);
        // Never above the maximum.
        assert_eq!(next_step_chars(MAX_STEP_CHARS, u64::MAX, 1), MAX_STEP_CHARS);
        // A zero duration counts as one microsecond.
        assert_eq!(next_step_chars(512, 1, 0), 1_024);
        // On target: unchanged.
        assert_eq!(next_step_chars(1_000, 1_000, STEP_TARGET_US), 1_000);
    }

    #[test]
    fn the_step_target_is_a_quarter_of_the_interaction_budget() {
        assert_eq!(STEP_TARGET_US * 4, B_INTERACT_US);
    }
}
