// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the draft journal core (`journal_core`).
//!
//! Compiled only under `cfg(kani)` (`make kani`). The environment model the
//! harnesses run is `journal_model`, shared with the draft-service
//! conformance driver; this module keeps the `#[kani::proof]` functions,
//! their assertions, and [`KaniChoices`], which answers the model's inner
//! choices with `kani::any()` and `kani::assume`.

use super::journal_core::{
    BodyFacts, BodyOwner, BodyWriteDecision, CleanupBodyIdentity, CleanupEntryState, CleanupFacts,
    DeletionStep, DraftClaim, DraftOpenDecision, EntryState, JournalLaneAdmission,
    JournalLaneHolder, OrphanBodyDecision, RestoreDisposition, RestoreEnding, SetAsideNameStep,
    SetAsideSlot, StartupRestore, UntrustedCommitDisposition, body_write_decision,
    commit_authority, deletion_start, draft_open_decision, journal_lane_admission,
    may_write_after_registration, must_preserve_before_replacing, next_deletion_step,
    orphan_body_decision, ownership, registration_required, set_aside_name_step,
    startup_may_retire_stale, startup_restore, unapplied_restore_disposition,
    untrusted_commit_disposition, window_may_journal_draft,
};
use super::journal_model::{
    Action, CONTENTS, Choices, ContentId, Fault, Journal, LaneUse, Scope, SingleWindowScope,
    fault_admitted,
};
use lushtext_model::draft::{DraftManifestAuthority, DraftManifestCompleteness};

/// The model's inner choices, exactly as it took them inline before it was
/// shared: any bit, and any restore ending the model admits.
struct KaniChoices;

impl Choices for KaniChoices {
    fn flag(&mut self) -> bool {
        kani::any()
    }

    fn restore_ending(&mut self, admits: impl Fn(RestoreEnding) -> bool) -> RestoreEnding {
        let ending: RestoreEnding = kani::any();
        kani::assume(admits(ending));
        ending
    }
}

fn any_below(bound: usize) -> usize {
    let value: usize = kani::any();
    kani::assume(value < bound);
    value
}

/// The K3 scope: three file-backed ids, before `verify-multi-window-draft-journal`.
/// Only the K8 pin still runs it.
struct K3Scope;

impl Scope for K3Scope {
    const EXTENDED: bool = false;
    const UNTITLED: Option<usize> = None;
    const PROCESS_JOURNAL: bool = true;
    const ALLOW_UNCONFIRMED_SET_ASIDE_COPY: bool = false;
}

/// Nondeterministic actions after the first startup.
const STEPS: usize = 8;

/// S1–S4 over every sequence of `STEPS` actions, faults (`None`,
/// `BeforeEffect`, `AfterEffect`), crashes, and restarts after the first
/// startup, for one window of one process.
///
/// Domain restriction (`AfterEffect`): several unconfirmed manifest commits
/// revert together to the last confirmed entry array rather than an
/// intermediate version (`verify-shell-conformance-against-proven-cores` D6).
#[kani::proof]
#[kani::unwind(9)]
fn journal_invariants_hold_under_crashes() {
    let mut journal = Journal::<SingleWindowScope, 3, 1, 1>::previous_session(&mut KaniChoices);
    journal.step(&mut KaniChoices, 0, 0, Action::Startup, 0, kani::any());
    journal.assert_invariants();
    for _ in 0..STEPS {
        let action: Action = kani::any();
        let fault: Fault = kani::any();
        kani::assume(fault_admitted(action, fault));
        journal.step(&mut KaniChoices, 0, 0, action, any_below(3), fault);
        journal.assert_invariants();
    }
}

/// Scope that restores the pre-D6 `set_aside::place` rule under which an
/// `AfterRename` set-aside copy counted as durably preserved.
struct BrokenSetAsideCopyScope;

impl Scope for BrokenSetAsideCopyScope {
    const EXTENDED: bool = true;
    const UNTITLED: Option<usize> = Some(2);
    const PROCESS_JOURNAL: bool = true;
    const ALLOW_UNCONFIRMED_SET_ASIDE_COPY: bool = true;
}

/// Exhibit for the D6 set-aside copy finding (task 5.1 / 7.4): when an
/// unconfirmed set-aside copy (`AfterEffect` on `Preserve`) counts as placed,
/// deleting the body and committing the removal makes the unlink durable while a
/// crash can revert the unsynced copy, violating S1.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(9)]
fn an_unconfirmed_set_aside_copy_loses_accepted_work() {
    struct RevertUnconfirmed;
    impl Choices for RevertUnconfirmed {
        fn flag(&mut self) -> bool {
            false
        }

        fn restore_ending(&mut self, admits: impl Fn(RestoreEnding) -> bool) -> RestoreEnding {
            let ending = RestoreEnding::Applied;
            assert!(admits(ending));
            ending
        }
    }

    let mut journal =
        Journal::<BrokenSetAsideCopyScope, 3, 1, 1>::previous_session(&mut RevertUnconfirmed);
    journal.step(
        &mut RevertUnconfirmed,
        0,
        0,
        Action::Startup,
        0,
        Fault::None,
    );
    journal.step(&mut RevertUnconfirmed, 0, 0, Action::Open, 0, Fault::None);
    journal.step(&mut RevertUnconfirmed, 0, 0, Action::Edit, 0, Fault::None);
    journal.step(
        &mut RevertUnconfirmed,
        0,
        0,
        Action::Register,
        0,
        Fault::None,
    );
    journal.step(
        &mut RevertUnconfirmed,
        0,
        0,
        Action::WriteBody,
        0,
        Fault::None,
    );
    journal.step(&mut RevertUnconfirmed, 0, 0, Action::Commit, 0, Fault::None);
    journal.step(
        &mut RevertUnconfirmed,
        0,
        0,
        Action::ExternalMtime,
        0,
        Fault::None,
    );
    journal.step(&mut RevertUnconfirmed, 0, 0, Action::Crash, 0, Fault::None);
    journal.step(
        &mut RevertUnconfirmed,
        0,
        0,
        Action::Startup,
        0,
        Fault::None,
    );
    // Preserve fails its set-aside SyncDir (AfterEffect) -> under the old rule it
    // wrongly succeeds, DeleteBody unlinks, RemoveEntry commits (making the unlink
    // durable), and Crash reverts the unconfirmed copy -> S1 panics.
    journal.step(
        &mut RevertUnconfirmed,
        0,
        0,
        Action::DeletionStep,
        0,
        Fault::AfterEffect,
    );
    journal.step(
        &mut RevertUnconfirmed,
        0,
        0,
        Action::DeletionStep,
        0,
        Fault::None,
    );
    journal.step(
        &mut RevertUnconfirmed,
        0,
        0,
        Action::DeletionStep,
        0,
        Fault::None,
    );
    journal.step(&mut RevertUnconfirmed, 0, 0, Action::Crash, 0, Fault::None);
    journal.assert_invariants();
}

/// Fault-free steps that bound L1: the smallest `k` that covers the longest
/// fault-free path to clean — a pending restore that turns out stale (1), its
/// preserve / delete-body / remove-entry retirement (3), then register, write,
/// and commit (3). `a_dirty_editor_may_need_seven_steps` shows six is too few.
const LIVENESS_STEPS: usize = 7;

/// Fault-free steps that drain the journal lane before L1's editor may use
/// it: at most a whole deletion (preserve, body, entry) of another id.
const LANE_DRAIN_STEPS: usize = 3;

/// Whether, after any three-action prefix (faults allowed), then the
/// fault-free steps that drain whatever holds the journal lane (at most
/// `LANE_DRAIN_STEPS`, asserted), and then only fault-free steps of the
/// production pass order, a dirty open editor is clean within `steps` steps.
fn dirty_editor_becomes_clean_within(steps: usize) -> bool {
    let mut journal = Journal::<SingleWindowScope, 3, 1, 1>::previous_session(&mut KaniChoices);
    journal.step(&mut KaniChoices, 0, 0, Action::Startup, 0, Fault::None);
    for _ in 0..3 {
        let action: Action = kani::any();
        let fault: Fault = kani::any();
        kani::assume(fault_admitted(action, fault));
        journal.step(&mut KaniChoices, 0, 0, action, any_below(3), fault);
    }
    let id = any_below(3);
    let me = journal.me();
    kani::assume(me.running && me.editors[id].open && me.editors[id].dirty);
    // Drain the lane: the work holding it finishes first, as production's
    // mark-pending admission waits for it.
    for _ in 0..LANE_DRAIN_STEPS {
        let Some(holder) = journal.lane_holder() else {
            break;
        };
        let (action, other) = match holder.lane_use {
            LaneUse::Pass(other) | LaneUse::Deletion(other) if usize::from(other) == id => break,
            LaneUse::Cleanup => (Action::ExecCleanup, 0),
            LaneUse::Deletion(other) => (Action::DeletionStep, usize::from(other)),
            LaneUse::Pass(other) => {
                let other = usize::from(other);
                let action = if journal.me().editors[other].written.is_some() {
                    Action::Commit
                } else {
                    Action::WriteBody
                };
                (action, other)
            }
        };
        journal.step(&mut KaniChoices, 0, 0, action, other, Fault::None);
    }
    assert!(
        journal.lane_holder().is_none_or(|holder| matches!(
            holder.lane_use,
            LaneUse::Pass(other) | LaneUse::Deletion(other) if usize::from(other) == id
        )),
        "L1: the journal lane did not drain"
    );
    for _ in 0..steps {
        let editor = journal.me().editors[id];
        if !editor.dirty {
            break;
        }
        // The production pass order: resolve a pending restore, drain the
        // serialized mutation, then register, write, and commit.
        let action = if editor.restore_pending {
            Action::RestoreApply
        } else if editor.deletion.is_some() {
            Action::DeletionStep
        } else if editor.written.is_some() {
            Action::Commit
        } else if editor.token {
            Action::WriteBody
        } else {
            Action::Register
        };
        journal.step(&mut KaniChoices, 0, 0, action, id, Fault::None);
    }
    !journal.me().editors[id].dirty
}

/// L1: without I/O faults, a dirty open editor becomes clean within
/// `LIVENESS_STEPS` steps of the autosave pass, after any prefix of actions
/// and once the journal lane has drained.
#[kani::proof]
#[kani::unwind(9)]
fn a_dirty_editor_becomes_clean_without_faults() {
    assert!(
        dirty_editor_becomes_clean_within(LIVENESS_STEPS),
        "L1: a dirty editor stayed dirty"
    );
}

/// The L1 bound is tight: six fault-free steps are not always enough.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(9)]
fn a_dirty_editor_may_need_seven_steps() {
    assert!(dirty_editor_becomes_clean_within(LIVENESS_STEPS - 1));
}

/// The two-window scope: one file-backed id and one untitled id (id 1).
struct TwoWindowScope;

impl Scope for TwoWindowScope {
    const EXTENDED: bool = true;
    const UNTITLED: Option<usize> = Some(1);
    const PROCESS_JOURNAL: bool = true;
    const ALLOW_UNCONFIRMED_SET_ASIDE_COPY: bool = false;
}

/// The two-window scope as production stood before this change: every
/// piece of the journal coordinator per window. No harness runs it any more;
/// `two_windows::<TwoWindowBaselineScope>(6)` fails "a body was written
/// without an entry" (321 s and 14.6 GB with the default solver, 923 s and
/// 2.3 GB with kissat), the trace in the programme record, phase 4.
#[expect(
    dead_code,
    reason = "kept so the recorded baseline counterexample can be re-run"
)]
struct TwoWindowBaselineScope;

impl Scope for TwoWindowBaselineScope {
    const EXTENDED: bool = true;
    const UNTITLED: Option<usize> = Some(1);
    const PROCESS_JOURNAL: bool = false;
    const ALLOW_UNCONFIRMED_SET_ASIDE_COPY: bool = false;
}

/// Nondeterministic actions after the first startup in the two-window
/// harness. Design D5 asked for eight; eight proved too (646.6 s), but peaked
/// at 15.3 GB with the default solver, over the 12 GiB runner margin, and took
/// 1724 s with kissat (3.1 GB), over the 25-minute margin; minisat was slower
/// still. Seven proves in 287 s and 2.7 GB. So the harness claims S1–S4 and one
/// owning window per id for every sequence of **seven** actions; the programme
/// record keeps the eight-action result as a local measurement.
const TWO_WINDOW_STEPS: usize = 7;

fn two_windows<S: Scope>(steps: usize) {
    let mut journal = Journal::<S, 2, 2, 1>::previous_session(&mut KaniChoices);
    journal.step(&mut KaniChoices, 0, 0, Action::Startup, 0, kani::any());
    journal.assert_invariants();
    for _ in 0..steps {
        let action: Action = kani::any();
        let fault: Fault = kani::any();
        kani::assume(fault_admitted(action, fault));
        journal.step(
            &mut KaniChoices,
            0,
            any_below(2),
            action,
            any_below(2),
            fault,
        );
        journal.assert_invariants();
    }
}

/// S1–S4, and one owning window per draft id, over every sequence of
/// `TWO_WINDOW_STEPS` actions by either of two windows of one process (one
/// file-backed and one untitled id, at most three edits), with faults (`None`,
/// `BeforeEffect`, `AfterEffect`), window opens and closes, crashes, and
/// restarts.
///
/// Domain restriction (`AfterEffect`): several unconfirmed manifest commits
/// revert together to the last confirmed entry array rather than an
/// intermediate version (`verify-shell-conformance-against-proven-cores` D6).
#[kani::proof]
#[kani::unwind(9)]
fn journal_invariants_hold_across_two_windows() {
    two_windows::<TwoWindowScope>(TWO_WINDOW_STEPS);
}

/// Actions after both startups in the K8 harness. Six still reach a
/// counterexample (a body written without an entry: one process registers and
/// writes while the other discards and deletes the same id); eight steps, which
/// also reach the S1 loss recorded in the programme record, took 17 minutes and
/// about 18 GB — too much for a CI runner, and more than a `should_panic`
/// harness needs.
const SECOND_WRITER_STEPS: usize = 6;

/// K8: drop axiom A6 (one process per data directory). A second LushText
/// process — its own window, editors, and journal coordinator — runs over the
/// same disk. Kani reports which of S1–S4 fail; the programme record keeps the
/// counterexamples and the decision. Kept as `should_panic`: the day this
/// passes, A6 is no longer needed and the record must change.
///
/// Domain restriction (`AfterEffect`): several unconfirmed manifest commits
/// revert together to the last confirmed entry array rather than an
/// intermediate version (`verify-shell-conformance-against-proven-cores` D6).
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(9)]
fn a_second_writer_breaks_the_journal_invariants() {
    let mut journal = Journal::<K3Scope, 3, 1, 2>::previous_session(&mut KaniChoices);
    journal.step(&mut KaniChoices, 0, 0, Action::Startup, 0, Fault::None);
    journal.step(&mut KaniChoices, 1, 0, Action::Startup, 0, Fault::None);
    journal.assert_invariants();
    for _ in 0..SECOND_WRITER_STEPS {
        let action: Action = kani::any();
        let fault: Fault = kani::any();
        kani::assume(fault_admitted(action, fault));
        journal.step(
            &mut KaniChoices,
            any_below(2),
            0,
            action,
            any_below(3),
            fault,
        );
        journal.assert_invariants();
    }
}

// --- set-aside naming ---------------------------------------------------------
//
// The journal model above keeps preserved content as a set, so it cannot see
// how the set-aside area names what it keeps. This separate model does: one
// draft id, whose copies are named by the entry stamp the caller passes plus a
// collision suffix, exactly as `set_aside::place` probes them, with every
// naming decision taken by the production `set_aside_name_step`.

/// Stamps a caller may pass (the entry's `saved_at_secs`).
const SET_ASIDE_STAMPS: usize = 2;
/// Names probed per stamp: the primary name plus collision suffixes.
const SET_ASIDE_NAMES: usize = 3;
/// `keep_copy` calls in one run.
const SET_ASIDE_CALLS: usize = 4;

/// The set-aside area for one id: which content, if any, each name holds.
struct SetAsideArea {
    names: [[Option<ContentId>; SET_ASIDE_NAMES]; SET_ASIDE_STAMPS],
}

impl SetAsideArea {
    /// `keep_copy(id, stamp)` of a body holding `content`: probes names in
    /// order and asks `decide` what to do with each. `true` means the call
    /// reported the body kept.
    fn keep_copy(
        &mut self,
        stamp: usize,
        content: ContentId,
        decide: fn(SetAsideSlot) -> SetAsideNameStep,
    ) -> bool {
        for name in 0..SET_ASIDE_NAMES {
            let slot = match self.names[stamp][name] {
                None => SetAsideSlot::Free,
                Some(held) if held == content => SetAsideSlot::SameBody,
                Some(_) => SetAsideSlot::OtherBody,
            };
            match decide(slot) {
                SetAsideNameStep::Place => {
                    // `place` refuses an existing name (no-replace rename or a
                    // free-name probe), so it can only fill an empty one.
                    if self.names[stamp][name].is_none() {
                        self.names[stamp][name] = Some(content);
                    }
                    return true;
                }
                SetAsideNameStep::AlreadyKept => return true,
                SetAsideNameStep::NextName => {}
            }
        }
        false
    }

    fn holds(&self, content: ContentId) -> bool {
        self.names
            .iter()
            .any(|names| names.iter().any(|held| *held == Some(content)))
    }
}

/// Runs arbitrary `keep_copy` calls (arbitrary stamp and body content, as a
/// crash between a body write and its commit allows) and asserts that a call
/// reported kept left that exact content in the area, and that no call ever
/// changed a name that already held a body.
fn set_aside_keeps_what_it_reports(decide: fn(SetAsideSlot) -> SetAsideNameStep) {
    let mut area = SetAsideArea {
        names: [[None; SET_ASIDE_NAMES]; SET_ASIDE_STAMPS],
    };
    for _ in 0..SET_ASIDE_CALLS {
        let stamp: usize = kani::any();
        kani::assume(stamp < SET_ASIDE_STAMPS);
        let content: ContentId = kani::any();
        kani::assume(usize::from(content) < CONTENTS);
        let before = area.names;
        let kept = area.keep_copy(stamp, content, decide);
        if kept {
            assert!(
                area.holds(content),
                "a body reported kept is not in the area"
            );
        }
        for (stamp, names) in before.iter().enumerate() {
            for (name, held) in names.iter().enumerate() {
                if held.is_some() {
                    assert_eq!(area.names[stamp][name], *held, "a kept body was replaced");
                }
            }
        }
    }
}

/// K9: every body `keep_copy` reports kept is in the set-aside area, byte for
/// byte, and no kept body is ever replaced, for any sequence of stamps and
/// contents.
#[kani::proof]
#[kani::unwind(5)]
fn journal_set_aside_keeps_every_body_it_reports_kept() {
    set_aside_keeps_what_it_reports(set_aside_name_step);
}

/// The stamp-only rule the E1 finding exposed (an existing name counts as
/// kept, whatever it holds) breaks K9, so the harness catches that class.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(5)]
fn journal_set_aside_stamp_only_naming_loses_a_newer_body() {
    fn stamp_only(slot: SetAsideSlot) -> SetAsideNameStep {
        match slot {
            SetAsideSlot::Free => SetAsideNameStep::Place,
            SetAsideSlot::SameBody | SetAsideSlot::OtherBody => SetAsideNameStep::AlreadyKept,
        }
    }
    set_aside_keeps_what_it_reports(stamp_only);
}

/// Every journal decision follows the rule its documentation states, for
/// every input: ownership's safety precedence (unseen work, then a version
/// conflict, then registration), the window's view of a body, preservation
/// before any replacement or deletion of unseen or other-version bytes, the
/// body-write and registration table, the deletion order, commit authority,
/// the orphan-deletion conditions, the unapplied-restore dispositions, and the
/// one-journal-per-process decisions.
///
/// The crash harnesses above check what these decisions *achieve* over whole
/// action sequences, but their content model abstracts away which body a
/// decision protects, so the proof-strength lane found mutants of `ownership`
/// (`||` with `&&` in the version-conflict test, which drops the preservation
/// of a stale body) and of `BodyFacts::from_window_view` surviving every one of
/// them at 20 minutes each (`measure-proof-strength-with-mutation`). This
/// harness pins the table itself, in about a second.
#[kani::proof]
fn journal_decisions_follow_their_documented_rules() {
    let facts: BodyFacts = kani::any();
    let owner = ownership(facts);
    let expected = if !facts.body_present {
        BodyOwner::Nothing
    } else if facts.restore_pending {
        BodyOwner::Unseen
    } else if facts.backing_stale || facts.entry == EntryState::Superseded {
        BodyOwner::OtherVersion
    } else if facts.entry == EntryState::Absent {
        BodyOwner::Unregistered
    } else {
        BodyOwner::Journal
    };
    assert_eq!(owner, expected, "ownership left its safety precedence");
    let preserve = matches!(owner, BodyOwner::Unseen | BodyOwner::OtherVersion);
    assert_eq!(must_preserve_before_replacing(owner), preserve);
    assert_eq!(
        deletion_start(owner),
        if preserve {
            DeletionStep::Preserve
        } else {
            DeletionStep::DeleteBody
        }
    );
    let needed: bool = kani::any();
    assert_eq!(
        body_write_decision(owner, needed),
        match owner {
            BodyOwner::Unseen => BodyWriteDecision::Hold,
            _ if needed => BodyWriteDecision::RegisterFirst,
            BodyOwner::OtherVersion => BodyWriteDecision::PreserveThenWrite,
            _ => BodyWriteDecision::Write,
        }
    );

    let (registered, pending): (bool, bool) = (kani::any(), kani::any());
    assert_eq!(
        BodyFacts::from_window_view(registered, pending),
        BodyFacts {
            body_present: registered || pending,
            entry: if registered {
                EntryState::Current
            } else {
                EntryState::Absent
            },
            restore_pending: pending,
            backing_stale: false,
        }
    );

    let (file_backed, known, trusted): (bool, bool, bool) = (kani::any(), kani::any(), kani::any());
    assert_eq!(
        registration_required(file_backed, known, trusted),
        file_backed && (!known || !trusted)
    );
    let (required, committed): (bool, bool) = (kani::any(), kani::any());
    assert_eq!(
        may_write_after_registration(required, committed),
        !required || committed
    );
    let (insert_only, allowed): (bool, bool) = (kani::any(), kani::any());
    assert_eq!(
        untrusted_commit_disposition(insert_only, allowed) == UntrustedCommitDisposition::Additive,
        insert_only && allowed
    );
    assert_eq!(startup_may_retire_stale(trusted), trusted);
    assert_eq!(
        startup_restore(trusted) == StartupRestore::StartEmpty,
        trusted
    );

    let step: DeletionStep = kani::any();
    let succeeded: bool = kani::any();
    assert_eq!(
        next_deletion_step(step, succeeded),
        match step {
            DeletionStep::Preserve if succeeded => DeletionStep::DeleteBody,
            DeletionStep::DeleteBody if succeeded => DeletionStep::RemoveEntry,
            DeletionStep::RemoveEntry if succeeded => DeletionStep::Done,
            DeletionStep::Done => DeletionStep::Done,
            _ => DeletionStep::Stopped,
        }
    );

    let completeness = match kani::any::<u8>() % 3 {
        0 => DraftManifestCompleteness::Complete,
        1 => DraftManifestCompleteness::Partial,
        _ => DraftManifestCompleteness::Failed,
    };
    let written: bool = kani::any();
    assert_eq!(
        commit_authority(completeness, written),
        if completeness == DraftManifestCompleteness::Complete && written {
            DraftManifestAuthority::TRUSTED
        } else {
            DraftManifestAuthority::untrusted(completeness)
        }
    );

    let cleanup: CleanupFacts = kani::any();
    let eligible = cleanup.manifest_trusted
        && cleanup.path_matches
        && cleanup.entry == CleanupEntryState::Unreferenced
        && cleanup.write_guard_held;
    let decision = orphan_body_decision(cleanup);
    assert_eq!(
        decision == OrphanBodyDecision::Delete,
        eligible && cleanup.identity == CleanupBodyIdentity::Inspected,
        "orphan cleanup deleted outside its conditions"
    );
    assert_eq!(
        decision == OrphanBodyDecision::AlreadyAbsent,
        eligible && cleanup.identity == CleanupBodyIdentity::Missing
    );

    let ending: RestoreEnding = kani::any();
    assert_eq!(
        unapplied_restore_disposition(ending),
        match ending {
            RestoreEnding::Stale => RestoreDisposition::PreserveThenRetire,
            RestoreEnding::Applied | RestoreEnding::MissingBody => RestoreDisposition::Nothing,
            _ => RestoreDisposition::PreserveCopy,
        }
    );

    let slot: SetAsideSlot = kani::any();
    assert_eq!(
        set_aside_name_step(slot),
        match slot {
            SetAsideSlot::Free => SetAsideNameStep::Place,
            SetAsideSlot::SameBody => SetAsideNameStep::AlreadyKept,
            SetAsideSlot::OtherBody => SetAsideNameStep::NextName,
        }
    );
    let holder: JournalLaneHolder = kani::any();
    assert_eq!(
        journal_lane_admission(holder) == JournalLaneAdmission::Admit,
        holder == JournalLaneHolder::Nobody
    );
    let claim: DraftClaim = kani::any();
    let other = claim == DraftClaim::OtherWindow;
    assert_eq!(
        draft_open_decision(claim) == DraftOpenDecision::PresentOwner,
        other
    );
    assert_eq!(window_may_journal_draft(claim), !other);
}
