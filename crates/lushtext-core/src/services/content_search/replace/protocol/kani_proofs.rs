// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses for the Replace All per-file protocols (shard
//! `core-replace-protocol`, design D11).
//!
//! The real [`ReplaceFileProtocol`], [`ReplaceUndoProtocol`] and
//! [`shrink_step`] drive an abstract disk: each target holds its original or
//! its replaced bytes, and each file has an undo body, an entry and the run's
//! manifest. Every action's outcome is chosen by Kani among the outcomes the
//! durable write can produce, and the write-protocol shards' theorems are the
//! assumptions: a write that fails before its rename leaves the previous
//! bytes, and its temp file is gone. A crash can stop the run after any
//! action, so the invariants are asserted after every action:
//!
//! 1. a target holds its original or its replaced bytes;
//! 2. a replaced target has an active entry whose body holds the original;
//! 3. no entry references a missing body;
//! 4. Undo writes only to a target whose current bytes are the replaced ones;
//! 5. cleanup never removes a body before its entry.
//!
//! The application-wide refusal (design D3a) is the model's frame: one run or
//! one Undo mutates the journal at a time, so the harnesses drive one.

use super::{
    CurrentContent, FileAction, FileEnd, FileOutcome, ReplaceFileProtocol, ReplaceUndoProtocol,
    RunJournalState, ShrinkAction, UndoAction, UndoEnd, UndoOutcome, shrink_step,
};

/// Files in one modelled run.
const FILES: usize = 2;
/// Steps one file can take: the longest path is nine actions plus `Finish`.
const MAX_FILE_STEPS: usize = 12;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Original,
    Replaced,
    Diverged,
}

#[derive(Clone, Copy)]
struct FileDisk {
    target: Target,
    body: bool,
    entry: bool,
}

#[derive(Clone, Copy)]
struct Disk {
    files: [FileDisk; FILES],
    /// The incremental manifest is armed.
    armed: bool,
    /// A previous run's journal (one entry and body) is still on disk.
    previous_journal: bool,
}

impl Disk {
    fn assert_invariants(&self) {
        for file in &self.files {
            // 1: the abstract target can only be original or replaced while a
            // run or Undo of this journal owns it (`Diverged` is another
            // program's write, which only the Undo harness introduces).
            // 2: replaced implies an active entry with its body.
            if file.target == Target::Replaced {
                assert!(self.armed && file.entry && file.body);
            }
            // 3: no entry without its body.
            if file.entry {
                assert!(file.body);
            }
        }
    }
}

fn apply_outcome(action: FileAction) -> FileOutcome {
    let outcome: FileOutcome = match kani::any::<u8>() % 7 {
        0 => FileOutcome::Done,
        1 => FileOutcome::Failed,
        2 => FileOutcome::BodyUnconfirmed,
        3 => FileOutcome::NoChange,
        4 => FileOutcome::Defer,
        5 => FileOutcome::Cancelled,
        _ => FileOutcome::RenamedUnconfirmed,
    };
    // Each action can only produce the outcomes its shell maps to it.
    let possible = match action {
        FileAction::AcquireTarget => {
            matches!(
                outcome,
                FileOutcome::Done | FileOutcome::Failed | FileOutcome::Defer
            )
        }
        FileAction::WriteBody => !matches!(outcome, FileOutcome::RenamedUnconfirmed),
        FileAction::WriteTarget => matches!(
            outcome,
            FileOutcome::Done
                | FileOutcome::Failed
                | FileOutcome::Cancelled
                | FileOutcome::RenamedUnconfirmed
        ),
        _ => matches!(outcome, FileOutcome::Done | FileOutcome::Failed),
    };
    kani::assume(possible);
    outcome
}

/// Execute one action on the abstract disk with a chosen outcome.
fn execute(disk: &mut Disk, index: usize, action: FileAction) -> FileOutcome {
    let outcome = apply_outcome(action);
    let file = &mut disk.files[index];
    match (action, outcome) {
        (FileAction::PrepareJournal, FileOutcome::Done) => {
            disk.previous_journal = false;
            disk.armed = false;
        }
        (FileAction::WriteBody, FileOutcome::Done) => file.body = true,
        // Renamed, directory sync unconfirmed: the body may or may not survive.
        (FileAction::WriteBody, FileOutcome::BodyUnconfirmed) => file.body = kani::any(),
        (FileAction::WriteEntry, FileOutcome::Done) => {
            assert!(file.body, "an entry is written only after its body");
            file.entry = true;
        }
        (FileAction::ArmJournal, FileOutcome::Done) => disk.armed = true,
        (FileAction::WriteTarget, FileOutcome::Done | FileOutcome::RenamedUnconfirmed) => {
            file.target = Target::Replaced;
        }
        (FileAction::RemoveEntry, FileOutcome::Done) => file.entry = false,
        (FileAction::RemoveBody, FileOutcome::Done) => {
            // 5: cleanup never removes a body its entry may still reference.
            assert!(!file.entry, "a body is removed only after its entry");
            file.body = false;
        }
        _ => {}
    }
    outcome
}

/// Every outcome sequence and every crash point of a two-file apply run.
#[kani::proof]
#[kani::unwind(14)]
fn an_apply_run_keeps_every_target_recoverable() {
    let mut disk = Disk {
        files: [FileDisk {
            target: Target::Original,
            body: false,
            entry: false,
        }; FILES],
        armed: false,
        previous_journal: kani::any(),
    };
    let mut journal = RunJournalState {
        prepared: false,
        armed: false,
    };
    for index in 0..FILES {
        let (mut protocol, mut action) = ReplaceFileProtocol::start(journal);
        let mut finished = None;
        for _ in 0..MAX_FILE_STEPS {
            if let FileAction::Finish(end) = action {
                finished = Some(end);
                break;
            }
            // A body is written only once the previous run's journal is gone,
            // so this run never writes into it (D3a).
            if action == FileAction::WriteBody {
                assert!(!disk.previous_journal);
            }
            // The target is rewritten only after body, entry and manifest.
            if action == FileAction::WriteTarget {
                let file = disk.files[index];
                assert!(disk.armed && file.entry && file.body);
            }
            let outcome = execute(&mut disk, index, action);
            disk.assert_invariants();
            action = protocol.step(outcome);
        }
        let end = finished.expect("every file reaches Finish within its bound");
        let file = disk.files[index];
        match end {
            FileEnd::Replaced { .. } => assert_eq!(file.target, Target::Replaced),
            FileEnd::NotReplaced { cleaned } => {
                assert_eq!(file.target, Target::Original);
                if cleaned {
                    assert!(!file.entry && !file.body);
                }
            }
            FileEnd::Deferred | FileEnd::Unchanged | FileEnd::Cancelled => {
                assert_eq!(file.target, Target::Original);
                assert!(!file.entry && !file.body);
            }
        }
        journal = protocol.journal();
        // The protocol's journal facts agree with the disk.
        assert_eq!(journal.armed, disk.armed);
    }
}

/// Undo over every current content, outcome sequence and crash point.
#[kani::proof]
#[kani::unwind(10)]
fn undo_writes_only_to_a_file_holding_the_replaced_bytes() {
    let target = match kani::any::<u8>() % 3 {
        0 => Target::Original,
        1 => Target::Replaced,
        _ => Target::Diverged,
    };
    let mut file = FileDisk {
        target,
        body: true,
        entry: true,
    };
    let retire = kani::any();
    let (mut protocol, mut action) = ReplaceUndoProtocol::start(kani::any(), retire);
    let mut end = None;
    for _ in 0..8 {
        if let UndoAction::Finish(finish) = action {
            end = Some(finish);
            break;
        }
        let succeeded: bool = kani::any();
        let outcome = match action {
            UndoAction::HashCurrent => {
                // A read failure, or a file past the limit, reads as diverged.
                let observed = if succeeded {
                    match file.target {
                        Target::Original => CurrentContent::Original,
                        Target::Replaced => CurrentContent::Replaced,
                        Target::Diverged => CurrentContent::Diverged,
                    }
                } else {
                    CurrentContent::Diverged
                };
                UndoOutcome::Current(observed)
            }
            UndoAction::WriteRestore => {
                // 4: the restore is written only over the replaced bytes (the
                // producer's identity check holds the target to what U1 hashed).
                assert_eq!(file.target, Target::Replaced);
                if succeeded {
                    file.target = Target::Original;
                    UndoOutcome::Done
                } else {
                    UndoOutcome::Failed
                }
            }
            UndoAction::RemoveEntry => {
                if succeeded {
                    file.entry = false;
                    UndoOutcome::Done
                } else {
                    UndoOutcome::Failed
                }
            }
            UndoAction::RemoveBody => {
                assert!(!file.entry, "a body is removed only after its entry");
                if succeeded {
                    file.body = false;
                    UndoOutcome::Done
                } else {
                    UndoOutcome::Failed
                }
            }
            _ => {
                if succeeded {
                    UndoOutcome::Done
                } else {
                    UndoOutcome::Failed
                }
            }
        };
        // 3 holds through every crash point.
        if file.entry {
            assert!(file.body);
        }
        action = protocol.step(outcome);
    }
    match end.expect("every entry reaches Finish within its bound") {
        UndoEnd::Restored { retired } => {
            assert_eq!(file.target, Target::Original);
            assert_eq!(retired, !file.entry);
        }
        UndoEnd::Diverged | UndoEnd::Failed => {
            // Nothing was written, and the entry stays retryable.
            assert_eq!(file.target, target);
            assert!(file.entry && file.body);
        }
    }
}

/// A shrink removes a retired entry before its body, and keeps the body when
/// the entry removal fails.
#[kani::proof]
#[kani::unwind(4)]
fn shrink_removes_the_entry_before_the_body() {
    let mut entry = true;
    let mut body = true;
    let mut action = ShrinkAction::RemoveEntry;
    for _ in 0..3 {
        let succeeded: bool = kani::any();
        match action {
            ShrinkAction::RemoveEntry => {
                if succeeded {
                    entry = false;
                }
            }
            ShrinkAction::RemoveBody => {
                assert!(!entry, "a body is removed only after its entry");
                if succeeded {
                    body = false;
                }
            }
            ShrinkAction::Finish { body_kept } => {
                assert_eq!(body_kept, entry);
                break;
            }
        }
        if entry {
            assert!(body);
        }
        action = shrink_step(action, succeeded);
    }
}
