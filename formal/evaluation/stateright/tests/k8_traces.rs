// SPDX-License-Identifier: GPL-3.0-or-later

//! Replays the two K8 counterexamples decoded in `docs/next/formal-verification.md`
//! ("K8 — dropping axiom A6") against the ported environment, which calls the
//! real `journal_core`, plus a one-process control.

use lushtext_journal_stateright::env::{Action, IDS, Journal, PreviousId};
use lushtext_journal_stateright::model::JournalModel;
use stateright::{Checker, Model};

const A: bool = false;
const B: bool = true;

/// Id 0 has an entry and no body; ids 1 and 2 have neither.
fn k8_previous_session() -> [PreviousId; IDS] {
    let none = PreviousId {
        entry: false,
        body: false,
        backing_moved: false,
    };
    [
        PreviousId {
            entry: true,
            ..none
        },
        none,
        none,
    ]
}

fn start_two_processes() -> Journal {
    JournalModel {
        max_steps: 0,
        two_processes: true,
        count_steps: true,
    }
    .start(k8_previous_session(), false)
}

fn all_hold(journal: &Journal) -> bool {
    journal.all_invariants_hold()
}

/// Replays `trace` fault-free on id 0, asserting every invariant holds before
/// the last step; returns the final journal.
fn replay(trace: &[(bool, Action)]) -> Journal {
    let mut journal = start_two_processes();
    assert!(all_hold(&journal), "invariants must hold after both startups");
    for (index, &(actor, action)) in trace.iter().enumerate() {
        journal.step_as(actor, action, 0, false, None);
        if index + 1 < trace.len() {
            assert!(all_hold(&journal), "violated early, at step {index}: {action:?}");
        }
    }
    journal
}

#[test]
fn k8_s1_accepted_work_is_lost() {
    let journal = replay(&[
        (A, Action::Edit),
        (A, Action::Register),
        (A, Action::WriteBody),
        (A, Action::Commit),
        (A, Action::Crash),
        (B, Action::Discard),
        (B, Action::DeletionStep),
    ]);
    assert!(!journal.acceptance_durability_holds(), "S1 must fail");
}

#[test]
fn k8_body_without_entry_eight_actions() {
    let journal = replay(&[
        (B, Action::Edit),
        (A, Action::Discard),
        (B, Action::Register),
        (A, Action::DeletionStep),
        (B, Action::ExternalMtime),
        (B, Action::RestoreApply),
        (A, Action::DeletionStep),
        (B, Action::WriteBody),
    ]);
    assert!(journal.body_without_entry, "NoBodyWithoutEntry must fail");
}

#[test]
fn k8_body_without_entry_six_actions() {
    let journal = replay(&[
        (B, Action::Edit),
        (A, Action::Discard),
        (B, Action::Register),
        (A, Action::DeletionStep),
        (A, Action::DeletionStep),
        (B, Action::WriteBody),
    ]);
    assert!(journal.body_without_entry, "NoBodyWithoutEntry must fail");
}

/// The same S1 trace with one process: the crash takes the only window, and
/// nothing discards the committed body, so every invariant holds.
#[test]
fn one_process_control_trace_holds() {
    let model = JournalModel {
        max_steps: 0,
        two_processes: false,
        count_steps: true,
    };
    let mut journal = model.start(k8_previous_session(), false);
    for action in [
        Action::Edit,
        Action::Register,
        Action::WriteBody,
        Action::Commit,
        Action::Crash,
        Action::Startup,
        Action::Discard,
        Action::DeletionStep,
        Action::DeletionStep,
    ] {
        journal.step(action, 0, false, None);
        assert!(all_hold(&journal), "violated at {action:?}");
    }
}

/// One-process S1–S4 hold over the whole bounded space at two steps.
#[test]
fn one_process_invariants_hold_at_two_steps() {
    let checker = JournalModel {
        max_steps: 2,
        two_processes: false,
        count_steps: true,
    }
    .checker()
    .spawn_dfs()
    .join();
    checker.assert_properties();
    assert!(checker.unique_state_count() > 1_000);
}
