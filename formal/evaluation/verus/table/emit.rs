// SPDX-License-Identifier: GPL-3.0-or-later

// The drift table, shared verbatim by both sides through `include!`:
// `drift/` (the production functions, workspace toolchain) and
// `proofs/src/bin/table.rs` (the Verus copies, Verus's toolchain). The
// including file defines two modules first:
//   `jc` — the journal_core decisions and types, plus `DraftManifestCompleteness`;
//   `pw` — `clamped_preview_width`.
// Each line is `function<TAB>input<TAB>output`, with `Debug` formatting;
// `emit` returns the lines sorted, so both sides are compared byte for byte.

use std::fmt::Write as _;

// Exhaustive matches: a variant added on either side fails to compile here,
// so the table cannot silently miss it.
const fn _exhaustive(
    a: jc::EntryState,
    b: jc::BodyOwner,
    c: jc::DeletionStep,
    d: jc::RestoreEnding,
    e: jc::CleanupEntryState,
    f: jc::CleanupBodyIdentity,
    g: jc::SetAsideSlot,
    h: jc::JournalLaneHolder,
    i: jc::DraftClaim,
    j: jc::DraftManifestCompleteness,
) {
    use jc::*;
    match a {
        EntryState::Absent | EntryState::Current | EntryState::Superseded => {}
    }
    match b {
        BodyOwner::Nothing
        | BodyOwner::Unseen
        | BodyOwner::OtherVersion
        | BodyOwner::Unregistered
        | BodyOwner::Journal => {}
    }
    match c {
        DeletionStep::Preserve
        | DeletionStep::DeleteBody
        | DeletionStep::RemoveEntry
        | DeletionStep::Done
        | DeletionStep::Stopped => {}
    }
    match d {
        RestoreEnding::Applied
        | RestoreEnding::Stale
        | RestoreEnding::Oversized
        | RestoreEnding::ReadFailed
        | RestoreEnding::EditedOver
        | RestoreEnding::InstallCancelled
        | RestoreEnding::Unavailable
        | RestoreEnding::MissingBody => {}
    }
    match e {
        CleanupEntryState::Unreferenced | CleanupEntryState::Referenced => {}
    }
    match f {
        CleanupBodyIdentity::Missing
        | CleanupBodyIdentity::Inspected
        | CleanupBodyIdentity::Changed
        | CleanupBodyIdentity::NotRegularFile
        | CleanupBodyIdentity::Uncertain => {}
    }
    match g {
        SetAsideSlot::Free | SetAsideSlot::SameBody | SetAsideSlot::OtherBody => {}
    }
    match h {
        JournalLaneHolder::Nobody
        | JournalLaneHolder::ThisWindow
        | JournalLaneHolder::OtherWindow => {}
    }
    match i {
        DraftClaim::Unclaimed | DraftClaim::ThisWindow | DraftClaim::OtherWindow => {}
    }
    match j {
        DraftManifestCompleteness::Complete
        | DraftManifestCompleteness::Partial
        | DraftManifestCompleteness::Failed => {}
    }
}

const BOOLS: [bool; 2] = [false, true];

fn entry_states() -> [jc::EntryState; 3] {
    [
        jc::EntryState::Absent,
        jc::EntryState::Current,
        jc::EntryState::Superseded,
    ]
}

fn owners() -> [jc::BodyOwner; 5] {
    use jc::BodyOwner::*;
    [Nothing, Unseen, OtherVersion, Unregistered, Journal]
}

fn steps() -> [jc::DeletionStep; 5] {
    use jc::DeletionStep::*;
    [Preserve, DeleteBody, RemoveEntry, Done, Stopped]
}

fn endings() -> [jc::RestoreEnding; 8] {
    use jc::RestoreEnding::*;
    [
        Applied,
        Stale,
        Oversized,
        ReadFailed,
        EditedOver,
        InstallCancelled,
        Unavailable,
        MissingBody,
    ]
}

fn identities() -> [jc::CleanupBodyIdentity; 5] {
    use jc::CleanupBodyIdentity::*;
    [Missing, Inspected, Changed, NotRegularFile, Uncertain]
}

fn completeness() -> [jc::DraftManifestCompleteness; 3] {
    use jc::DraftManifestCompleteness::*;
    [Complete, Partial, Failed]
}

/// The integer inputs: the Kani harnesses' interesting points, then a seeded
/// sample (a fixed 64-bit LCG, so both sides draw the same pairs).
fn integer_inputs() -> Vec<(i32, i32)> {
    let edges = [
        i32::MIN,
        i32::MIN + 1,
        -301,
        -3,
        -2,
        -1,
        0,
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        299,
        300,
        301,
        899,
        900,
        901,
        i32::MAX - 2,
        i32::MAX - 1,
        i32::MAX,
    ];
    let mut inputs = Vec::new();
    for &a in &edges {
        for &b in &edges {
            inputs.push((a, b));
        }
    }
    let mut state: u64 = 0x5eed_0f_d21f7;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 32) as u32 as i32
    };
    for _ in 0..4096 {
        let a = next();
        let b = next();
        inputs.push((a, b));
        // Also small values, where the floor and the one-third bound meet.
        inputs.push((a % 1024, b % 1024));
    }
    inputs
}

pub fn emit() -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = |f: &str, input: String, output: String| {
        let mut s = String::new();
        let _ = write!(s, "{f}\t{input}\t{output}");
        lines.push(s);
    };
    for r in BOOLS {
        for p in BOOLS {
            line(
                "from_window_view",
                format!("{r:?},{p:?}"),
                format!(
                    "{:?}",
                    jc::BodyFacts::from_window_view(jc::WindowBodyFacts {
                        registered: r,
                        restore_pending: p,
                    })
                ),
            );
        }
    }
    for body_present in BOOLS {
        for entry in entry_states() {
            for restore_pending in BOOLS {
                for backing_stale in BOOLS {
                    let facts = jc::BodyFacts {
                        body_present,
                        entry,
                        restore_pending,
                        backing_stale,
                    };
                    line(
                        "ownership",
                        format!("{facts:?}"),
                        format!("{:?}", jc::ownership(facts)),
                    );
                }
            }
        }
    }
    for owner in owners() {
        line(
            "must_preserve_before_replacing",
            format!("{owner:?}"),
            format!("{:?}", jc::must_preserve_before_replacing(owner)),
        );
        line(
            "deletion_start",
            format!("{owner:?}"),
            format!("{:?}", jc::deletion_start(owner)),
        );
        for needed in BOOLS {
            line(
                "body_write_decision",
                format!("{owner:?},{needed:?}"),
                format!("{:?}", jc::body_write_decision(owner, needed)),
            );
        }
    }
    for a in BOOLS {
        for b in BOOLS {
            for c in BOOLS {
                line(
                    "registration_required",
                    format!("{a:?},{b:?},{c:?}"),
                    format!(
                        "{:?}",
                        jc::registration_required(jc::RegistrationFacts {
                            file_backed: a,
                            registered_in_known_manifest: b,
                            known_manifest_trusted: c,
                        })
                    ),
                );
            }
            let req = if a {
                jc::RegistrationRequirement::Required
            } else {
                jc::RegistrationRequirement::NotRequired
            };
            let com = if b {
                jc::RegistrationCommit::Committed
            } else {
                jc::RegistrationCommit::Uncommitted
            };
            line(
                "may_write_after_registration",
                format!("{a:?},{b:?}"),
                format!("{:?}", jc::may_write_after_registration(req, com)),
            );
            let mode = if a {
                jc::CommitWriteMode::InsertOnly
            } else {
                jc::CommitWriteMode::Reconcile
            };
            let repl = if b {
                jc::ExistingBodyReplacement::Allowed
            } else {
                jc::ExistingBodyReplacement::Forbidden
            };
            line(
                "untrusted_commit_disposition",
                format!("{a:?},{b:?}"),
                format!("{:?}", jc::untrusted_commit_disposition(mode, repl)),
            );
        }
        line(
            "startup_may_retire_stale",
            format!("{a:?}"),
            format!("{:?}", jc::startup_may_retire_stale(a)),
        );
        line(
            "startup_restore",
            format!("{a:?}"),
            format!("{:?}", jc::startup_restore(a)),
        );
    }
    for step in steps() {
        for ok in BOOLS {
            line(
                "next_deletion_step",
                format!("{step:?},{ok:?}"),
                format!("{:?}", jc::next_deletion_step(step, ok)),
            );
        }
        // run_deletion: every start step with every outcome sequence of the
        // longest deletion (three steps); record the terminal and the steps
        // performed. A fourth call would overrun the sequence and is recorded.
        for bits in 0u8..8 {
            let outcomes = [bits & 1 != 0, bits & 2 != 0, bits & 4 != 0];
            let mut performed: Vec<String> = Vec::new();
            let mut overrun = false;
            let terminal = jc::run_deletion(step, |s| {
                let n = performed.len();
                performed.push(format!("{s:?}"));
                if n < 3 {
                    outcomes[n]
                } else {
                    overrun = true;
                    false
                }
            });
            line(
                "run_deletion",
                format!("{step:?},{outcomes:?}"),
                format!("{terminal:?},[{}],overrun={overrun}", performed.join(",")),
            );
        }
    }
    for ending in endings() {
        line(
            "unapplied_restore_disposition",
            format!("{ending:?}"),
            format!("{:?}", jc::unapplied_restore_disposition(ending)),
        );
    }
    for manifest_trusted in BOOLS {
        for path_matches in BOOLS {
            for entry in [
                jc::CleanupEntryState::Unreferenced,
                jc::CleanupEntryState::Referenced,
            ] {
                for write_guard_held in BOOLS {
                    for identity in identities() {
                        let facts = jc::CleanupFacts {
                            manifest_trusted,
                            path_matches,
                            entry,
                            write_guard_held,
                            identity,
                        };
                        line(
                            "orphan_body_decision",
                            format!("{facts:?}"),
                            format!("{:?}", jc::orphan_body_decision(facts)),
                        );
                    }
                }
            }
        }
    }
    for c in completeness() {
        for durable in BOOLS {
            let authority = jc::commit_authority(c, durable);
            line(
                "commit_authority",
                format!("{c:?},{durable:?}"),
                format!("{authority:?},trusted={:?}", authority.is_trusted()),
            );
        }
    }
    for slot in [
        jc::SetAsideSlot::Free,
        jc::SetAsideSlot::SameBody,
        jc::SetAsideSlot::OtherBody,
    ] {
        line(
            "set_aside_name_step",
            format!("{slot:?}"),
            format!("{:?}", jc::set_aside_name_step(slot)),
        );
    }
    for holder in [
        jc::JournalLaneHolder::Nobody,
        jc::JournalLaneHolder::ThisWindow,
        jc::JournalLaneHolder::OtherWindow,
    ] {
        line(
            "journal_lane_admission",
            format!("{holder:?}"),
            format!("{:?}", jc::journal_lane_admission(holder)),
        );
    }
    for claim in [
        jc::DraftClaim::Unclaimed,
        jc::DraftClaim::ThisWindow,
        jc::DraftClaim::OtherWindow,
    ] {
        line(
            "draft_open_decision",
            format!("{claim:?}"),
            format!("{:?}", jc::draft_open_decision(claim)),
        );
        line(
            "window_may_journal_draft",
            format!("{claim:?}"),
            format!("{:?}", jc::window_may_journal_draft(claim)),
        );
    }
    for (preferred, available) in integer_inputs() {
        line(
            "clamped_preview_width",
            format!("{preferred},{available}"),
            format!("{}", pw::clamped_preview_width(preferred, available)),
        );
    }
    lines.sort();
    lines.dedup();
    lines
}
