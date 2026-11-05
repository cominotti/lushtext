// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses for the paced re-analysis rule (`bound-context-flip-keystrokes`).
//!
//! Domain: every session value reachable from [`PacingSession::START`]
//! (spending at most the allowance, step sizes within their bounds, fewer
//! than `MAX_STEPS` steps) and every `u64` report.

use super::*;

/// Any session the rule can produce: `START`, or a `Continue` value.
fn any_reachable_session(allowance_us: u64) -> PacingSession {
    let session = PacingSession {
        spent_us: kani::any(),
        step_chars: kani::any(),
        steps: kani::any(),
    };
    kani::assume(session.spent_us <= allowance_us);
    kani::assume((MIN_STEP_CHARS..=MAX_STEP_CHARS).contains(&session.step_chars));
    kani::assume(session.steps < MAX_STEPS);
    session
}

fn any_report() -> StepReport {
    if kani::any() {
        StepReport::Merged
    } else {
        StepReport::ReachedBound {
            analysed_chars: kani::any(),
            took_us: kani::any(),
        }
    }
}

/// A session continues only within its allowance and its step bound, with a
/// step size in bounds and at most twice the previous one, and its spending
/// and step count grow.
#[kani::proof]
fn a_session_continues_only_within_its_allowance() {
    // Any allowance: the shipped one, or a widget test's override.
    let allowance_us: u64 = kani::any();
    let session = any_reachable_session(allowance_us);
    let report = any_report();
    kani::cover!(allowance_us == REANALYSIS_ALLOWANCE_US);
    match after_step_within(session, report, allowance_us) {
        PacingNext::Continue(next) => {
            assert!(next.spent_us <= allowance_us);
            assert!(next.steps < MAX_STEPS);
            assert!(next.steps == session.steps + 1);
            assert!(next.spent_us >= session.spent_us);
            assert!((MIN_STEP_CHARS..=MAX_STEP_CHARS).contains(&next.step_chars));
            assert!(
                u64::from(next.step_chars)
                    <= u64::from(session.step_chars)
                        .saturating_mul(2)
                        .max(u64::from(MIN_STEP_CHARS))
            );
            kani::cover!(next.step_chars == MIN_STEP_CHARS);
            kani::cover!(next.step_chars > session.step_chars);
        }
        PacingNext::Finished => assert!(matches!(report, StepReport::Merged)),
        PacingNext::Downgrade => {
            let StepReport::ReachedBound { took_us, .. } = report else {
                panic!("a merged report never downgrades");
            };
            assert!(
                session.spent_us.saturating_add(took_us) > allowance_us
                    || session.steps + 1 >= MAX_STEPS
            );
            kani::cover!(session.steps + 1 < MAX_STEPS);
        }
    }
}

/// The first step's size is in bounds, so `START` is a reachable session.
#[kani::proof]
fn the_start_session_is_in_bounds() {
    assert!((MIN_STEP_CHARS..=MAX_STEP_CHARS).contains(&PacingSession::START.step_chars));
    assert!(PacingSession::START.spent_us == 0 && PacingSession::START.steps == 0);
}

/// Liveness: a session whose steps never merge ends in a downgrade within
/// `MAX_STEPS` steps, whatever the reported durations and sizes, under the
/// shipped allowance (a smaller one, a widget test's override, only
/// downgrades sooner: `a_session_continues_only_within_its_allowance` proves
/// a `Continue` stays within any allowance). Four symbolic steps from any
/// reachable session close to the bound stand for the induction: every
/// `Continue` adds exactly one step, and the step bound is checked on each.
#[kani::proof]
#[kani::unwind(6)]
fn a_session_that_never_merges_downgrades_within_the_step_bound() {
    let mut session = any_reachable_session(REANALYSIS_ALLOWANCE_US);
    kani::assume(session.steps >= MAX_STEPS - 4);
    let mut ended = false;
    for _ in 0..4 {
        match after_step(
            session,
            StepReport::ReachedBound {
                analysed_chars: kani::any(),
                took_us: kani::any(),
            },
        ) {
            PacingNext::Continue(next) => session = next,
            PacingNext::Downgrade => {
                ended = true;
                break;
            }
            PacingNext::Finished => panic!("a bounded step never finishes a session"),
        }
    }
    assert!(ended);
}
