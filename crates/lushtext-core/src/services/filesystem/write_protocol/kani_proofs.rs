// SPDX-License-Identifier: GPL-3.0-or-later
//! Kani harnesses over the **real** durable-write protocols.
//!
//! These are the protocol harnesses of the feasibility spike (appendix B of
//! `docs/next/formal-verification-evolution.md`), retargeted from a copy of the
//! protocol to the production [`WriteProtocol`], [`MoveProtocol`], and
//! [`RenameProtocol`]. What stays a model is the disk: a `cfg(kani)`
//! abstraction of one destination name, its previous inode, and the one temp
//! inode a write creates, with POSIX crash semantics:
//!
//! - an unsynced file's bytes may be anything after a crash (`Torn`), and so
//!   may metadata applied after the last sync;
//! - a rename whose parent directory was not synced may or may not survive a
//!   crash, so the post-crash name points at either inode.
//!
//! Every backend call's outcome is `kani::any()`, a failed write may leave a
//! torn temp behind, and a crash may happen before any step. The protocols
//! are finite (a write takes at most `WRITE_STEPS` actions), so each harness
//! explores every outcome sequence completely.

use super::{
    MAX_TEMP_NAME_ATTEMPTS, MoveAction, MoveProtocol, RenameAction, RenameProtocol, StepOutcome,
    WriteAction, WriteClass, WriteProtocol,
};

/// The longest atomic write: a probe, every temp-name attempt, content,
/// metadata, sync, rename, and directory sync, then the finish.
const WRITE_STEPS: usize = MAX_TEMP_NAME_ATTEMPTS as usize + 7;

/// The longest move or rename: three backend actions, then the finish.
const TRANSFER_STEPS: usize = 4;

// `#[kani::unwind]` takes a literal, so the bounds below are written out; these
// make a change to either step count fail to compile until they are updated.
// A loop of `n` iterations needs `unwind(n + 1)`.
const _: () = assert!(
    WRITE_STEPS + 2 == 17,
    "update the write harnesses' unwind(17)"
);
const _: () = assert!(
    TRANSFER_STEPS + 1 == 5,
    "update the transfer harnesses' unwind(5)"
);

/// What a file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Content {
    Empty,
    Old,
    New,
    Torn,
}

/// Which inode the destination name points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    Previous,
    Written,
}

/// The inode a write creates as its temp file.
#[derive(Clone, Copy, Debug)]
struct TempInode {
    data: Content,
    /// The bytes a crash would leave, as of the last successful sync.
    durable_data: Content,
    metadata_applied: bool,
    /// Whether the required metadata was applied before the last sync.
    durable_metadata: bool,
    /// Created with the probed destination permissions, not a default mode.
    destination_mode: bool,
    /// The temp name still links it.
    linked: bool,
}

#[derive(Clone, Copy, Debug)]
struct Disk {
    probed: bool,
    temp: Option<TempInode>,
    live_entry: Entry,
    durable_entry: Entry,
    /// A `RemoveTemp` failed, leaving the temp for the startup sweep.
    removal_failed: bool,
}

/// A durable write shell mutant, to show which step a property rests on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shell {
    Production,
    /// Answers `SyncTemp` with `Done` without syncing.
    SkipsTempSync,
}

impl Disk {
    const fn before_write() -> Self {
        Self {
            probed: false,
            temp: None,
            live_entry: Entry::Previous,
            durable_entry: Entry::Previous,
            removal_failed: false,
        }
    }

    /// Execute one action with a nondeterministic backend outcome, and return
    /// that outcome for the protocol. One backend call per action, as in the
    /// production shell.
    fn execute(&mut self, action: WriteAction, shell: Shell) -> StepOutcome {
        let outcome: StepOutcome = kani::any();
        let done = outcome == StepOutcome::Done;
        match action {
            WriteAction::ProbeMetadata => self.probed = done,
            WriteAction::CreateTemp => {
                // `AlreadyExists` is someone else's name: nothing is ours.
                if done {
                    // A write creates one temp at a time: a failure after
                    // creation removes it before the protocol finishes.
                    assert!(self.temp.is_none(), "a second temp was created");
                    self.temp = Some(TempInode {
                        data: Content::Empty,
                        durable_data: Content::Torn,
                        metadata_applied: false,
                        durable_metadata: false,
                        destination_mode: self.probed,
                        linked: true,
                    });
                }
            }
            WriteAction::WriteContent => {
                let temp = self.temp.as_mut().expect("content goes into a temp");
                temp.data = if done { Content::New } else { Content::Torn };
            }
            WriteAction::ApplyMetadata => {
                let temp = self.temp.as_mut().expect("metadata goes on a temp");
                temp.metadata_applied |= done;
            }
            WriteAction::SyncTemp => {
                let temp = self.temp.as_mut().expect("the temp is synced");
                if shell == Shell::SkipsTempSync {
                    return StepOutcome::Done;
                }
                if done {
                    temp.durable_data = temp.data;
                    temp.durable_metadata = temp.metadata_applied;
                }
            }
            WriteAction::Rename => {
                let temp = self.temp.as_mut().expect("the temp is renamed");
                if done {
                    temp.linked = false;
                    self.live_entry = Entry::Written;
                }
            }
            WriteAction::SyncDir => {
                if done {
                    self.durable_entry = self.live_entry;
                }
            }
            WriteAction::RemoveTemp => {
                self.removal_failed |= !done;
                if done {
                    if let Some(temp) = self.temp.as_mut() {
                        temp.linked = false;
                    }
                    if self.live_entry == Entry::Previous {
                        self.temp = None;
                    }
                }
            }
            WriteAction::Finish(_) => {}
        }
        outcome
    }

    /// What the destination name shows after a crash now.
    fn after_crash(&self) -> (Content, bool) {
        let entry = if kani::any() {
            self.durable_entry
        } else {
            self.live_entry
        };
        match entry {
            Entry::Previous => (Content::Old, true),
            Entry::Written => {
                let temp = self.temp.expect("the written inode exists");
                (
                    temp.durable_data,
                    temp.durable_metadata && temp.destination_mode,
                )
            }
        }
    }

    /// What the destination name shows without a crash.
    fn visible(&self) -> Content {
        match self.live_entry {
            Entry::Previous => Content::Old,
            Entry::Written => self.temp.expect("the written inode exists").data,
        }
    }

    /// Mode non-widening: new bytes only ever sit in an inode created with
    /// the destination's permissions.
    fn assert_new_bytes_are_never_wider(&self) {
        if let Some(temp) = self.temp {
            if temp.data == Content::New || temp.durable_data == Content::New {
                assert!(
                    temp.destination_mode,
                    "new bytes sat in a file created with a wider default mode"
                );
            }
        }
    }
}

/// Drive the real `WriteProtocol` against the model disk. A crash may occur
/// before any step; the crash view is checked there and the run ends.
/// Returns the finished class and the final disk when no crash occurred.
fn run_write(shell: Shell) -> Option<(WriteClass, Disk)> {
    let mut disk = Disk::before_write();
    let (mut protocol, mut action) = WriteProtocol::start();
    let crash_at: usize = kani::any();
    for step in 0..=WRITE_STEPS {
        if step == crash_at {
            let (content, metadata_intact) = disk.after_crash();
            assert!(
                content == Content::Old || content == Content::New,
                "a crash left a torn destination"
            );
            kani::cover!(
                content == Content::New,
                "a crash after the rename shows the new bytes"
            );
            if content == Content::New {
                assert!(
                    metadata_intact,
                    "a crash left new bytes without their metadata or with a wider mode"
                );
            }
            return None;
        }
        if let WriteAction::Finish(class) = action {
            return Some((class, disk));
        }
        let outcome = disk.execute(action, shell);
        disk.assert_new_bytes_are_never_wider();
        (protocol, action) = protocol.step(outcome);
    }
    panic!("the write did not finish within WRITE_STEPS actions");
}

/// Crash atomicity, mode non-widening, and metadata-before-final-sync: after
/// a crash at any point of any outcome sequence, the destination holds the
/// complete previous bytes, or the complete new bytes with their metadata and
/// the destination's permissions.
#[kani::proof]
#[kani::unwind(17)]
fn a_crash_never_tears_the_destination() {
    let _ = run_write(Shell::Production);
}

/// Classification soundness: `BeforeRename` means the previous bytes are
/// still the destination, `AfterRename` that the new bytes are, and `Success`
/// that they are durably. A failed write leaves no temp behind unless its
/// removal failed, and the sweep owns that leftover.
#[kani::proof]
#[kani::unwind(17)]
fn every_classification_describes_the_destination() {
    let Some((class, disk)) = run_write(Shell::Production) else {
        return;
    };
    kani::cover!(class == WriteClass::Success, "a write succeeds");
    kani::cover!(class == WriteClass::AfterRename, "a directory sync fails");
    kani::cover!(disk.removal_failed, "a temp removal fails");
    match class {
        WriteClass::BeforeRename => {
            assert_eq!(disk.visible(), Content::Old);
            assert!(
                disk.temp.is_none_or(|temp| !temp.linked) || disk.removal_failed,
                "a failed write left its temp behind"
            );
        }
        WriteClass::AfterRename => {
            assert_eq!(disk.visible(), Content::New);
        }
        WriteClass::Success => {
            assert_eq!(disk.visible(), Content::New);
            assert_eq!(disk.durable_entry, Entry::Written);
            let temp = disk.temp.expect("the written inode exists");
            assert_eq!(temp.durable_data, Content::New);
            assert!(temp.durable_metadata && temp.destination_mode);
            assert!(!temp.linked, "the temp name outlived a successful write");
        }
    }
}

/// The spike's canonical counterexample, now against the real protocol: a
/// shell that skips the temp sync can expose a torn destination after a crash.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(17)]
fn skipping_the_temp_sync_tears_the_destination() {
    let _ = run_write(Shell::SkipsTempSync);
}

/// Move safety: whatever the outcome sequence and wherever a crash lands, the
/// source is removed only once the destination holds the bytes durably, so
/// the bytes are always in at least one durable place.
#[kani::proof]
#[kani::unwind(5)]
fn a_move_removes_its_source_only_after_the_copy_is_durable() {
    let mut source_present = true;
    let mut destination_durable = false;
    let (mut protocol, mut action) = MoveProtocol::start();
    for _ in 0..TRANSFER_STEPS {
        assert!(
            source_present || destination_durable,
            "a move lost its only durable copy"
        );
        let succeeded: bool = kani::any();
        match action {
            // The copy is a whole atomic write: success is `WriteClass::Success`.
            MoveAction::Copy => destination_durable = succeeded,
            MoveAction::RemoveSource => source_present = !succeeded,
            MoveAction::SyncSourceDir => {}
            MoveAction::Finish(done) => {
                kani::cover!(done, "a move completes");
                if !done && !destination_durable {
                    assert!(source_present, "a failed move removed its source");
                }
                return;
            }
        }
        (protocol, action) = protocol.step(succeeded);
    }
    panic!("the move did not finish");
}

/// A completed durable rename synced the source directory, and the
/// destination directory too when it is a different one.
#[kani::proof]
#[kani::unwind(5)]
fn a_completed_rename_synced_every_directory_it_mutated() {
    let cross_directory: bool = kani::any();
    let (mut protocol, mut action) = RenameProtocol::start(cross_directory);
    let mut renamed = false;
    let mut source_synced = false;
    let mut destination_synced = false;
    for _ in 0..TRANSFER_STEPS {
        let succeeded: bool = kani::any();
        match action {
            RenameAction::Rename => renamed = succeeded,
            RenameAction::SyncSourceDir => source_synced = succeeded && renamed,
            RenameAction::SyncDestinationDir => destination_synced = succeeded && renamed,
            RenameAction::Finish(done) => {
                kani::cover!(
                    done && cross_directory,
                    "a cross-directory rename completes"
                );
                if done {
                    assert!(renamed && source_synced);
                    assert!(!cross_directory || destination_synced);
                }
                return;
            }
        }
        (protocol, action) = protocol.step(succeeded);
    }
    panic!("the rename did not finish");
}
