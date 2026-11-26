// SPDX-License-Identifier: GPL-3.0-or-later
//! Kani harnesses over the **real** durable-write protocols.
//!
//! These are the protocol harnesses of the feasibility spike (appendix B of
//! `docs/next/formal-verification-evolution.md`), retargeted from a copy of the
//! protocol to the production [`WriteProtocol`], [`MoveProtocol`], and
//! [`RenameProtocol`]. What stays a model is the disk (`disk_model`, shared
//! with the shell conformance driver): an abstraction of one destination
//! name, its previous inode, and the one temp inode a write creates, with
//! POSIX crash semantics:
//!
//! - an unsynced file's bytes may be anything after a crash (`Torn`), and so
//!   may metadata applied after the last sync;
//! - a rename whose parent directory was not synced may or may not survive a
//!   crash, so the post-crash name points at either inode.
//!
//! Every backend call's outcome is `kani::any()`, passed to the model at the
//! call site (the model takes no choice itself), a failed write may leave a
//! torn temp behind, and a crash may happen before any step. The protocols
//! are finite (a write takes at most `WRITE_STEPS` actions), so each harness
//! explores every outcome sequence completely.

use super::disk_model::{Content, Disk, Entry, MoveDisk, RenameNamespace, Shell};
use super::{
    MAX_TEMP_NAME_ATTEMPTS, MoveAction, MoveProtocol, RenameAction, RenameClass, RenameProtocol,
    StepOutcome, WriteAction, WriteClass, WriteProtocol,
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

/// Drive the real `WriteProtocol` against the model disk. A crash may occur
/// before any step; the crash view is checked there and the run ends.
/// Returns the finished class and the final disk when no crash occurred.
fn run_write(shell: Shell) -> Option<(WriteClass, Disk)> {
    let mut disk = Disk::before_write();
    let (mut protocol, mut action) = WriteProtocol::start();
    let crash_at: usize = kani::any();
    for step in 0..=WRITE_STEPS {
        if step == crash_at {
            let (content, metadata_intact) = disk.after_crash(kani::any());
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
        let outcome = disk.execute(action, shell, kani::any());
        disk.assert_new_bytes_are_never_wider();
        assert!(
            disk.temp_name_attempts <= MAX_TEMP_NAME_ATTEMPTS,
            "a write tried more temp names than MAX_TEMP_NAME_ATTEMPTS"
        );
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
/// removal failed, and the sweep owns that leftover. A write whose every
/// backend action succeeded is a `Success`, and a write that found every temp
/// name taken gave up only after trying all `MAX_TEMP_NAME_ATTEMPTS` of them
/// (both added by `measure-proof-strength-with-mutation`, whose mutants showed
/// the classification and the retry bound were otherwise unconstrained).
#[kani::proof]
#[kani::unwind(17)]
fn every_classification_describes_the_destination() {
    let Some((class, disk)) = run_write(Shell::Production) else {
        return;
    };
    kani::cover!(class == WriteClass::Success, "a write succeeds");
    kani::cover!(class == WriteClass::AfterRename, "a directory sync fails");
    kani::cover!(disk.removal_failed, "a temp removal fails");
    kani::cover!(
        disk.temp_name_attempts == MAX_TEMP_NAME_ATTEMPTS,
        "a write tries every temp name"
    );
    if disk.every_step_done {
        assert_eq!(
            class,
            WriteClass::Success,
            "a write with no failure did not succeed"
        );
    }
    if disk.temp_name_attempts > 0 && disk.every_temp_name_taken {
        assert_eq!(
            disk.temp_name_attempts, MAX_TEMP_NAME_ATTEMPTS,
            "a write gave up on taken temp names before trying all of them"
        );
    }
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
    let mut disk = MoveDisk::before_move();
    let mut every_step_succeeded = true;
    let (mut protocol, mut action) = MoveProtocol::start();
    for _ in 0..TRANSFER_STEPS {
        assert!(
            disk.source_present || disk.destination_durable,
            "a move lost its only durable copy"
        );
        let succeeded: bool = kani::any();
        if let MoveAction::Finish(done) = action {
            kani::cover!(done, "a move completes");
            assert_eq!(
                done, every_step_succeeded,
                "a move reported the wrong completion"
            );
            if !done && !disk.destination_durable {
                assert!(disk.source_present, "a failed move removed its source");
            }
            return;
        }
        disk.execute(action, succeeded);
        every_step_succeeded &= succeeded;
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
    let mut namespace = RenameNamespace::before_rename();
    let mut every_step_succeeded = true;
    for _ in 0..TRANSFER_STEPS {
        let succeeded: bool = kani::any();
        if let RenameAction::Finish(class) = action {
            let done = class == RenameClass::Success;
            kani::cover!(
                done && cross_directory,
                "a cross-directory rename completes"
            );
            assert_eq!(
                done, every_step_succeeded,
                "a rename reported the wrong completion"
            );
            if done {
                assert!(namespace.renamed && namespace.source_synced);
                assert!(!cross_directory || namespace.destination_synced);
            }
            return;
        }
        namespace.execute(action, succeeded);
        every_step_succeeded &= succeeded;
        (protocol, action) = protocol.step(succeeded);
    }
    panic!("the rename did not finish");
}

/// Rename classification soundness (`rust-1-100-adoption` A8): `BeforeRename`
/// means the entry kept its old name, `AfterRename` that it has the new name
/// and some directory sync failed, and `Success` that it has the new name and
/// every directory the rename mutated was synced. The no-replace fallback
/// relies on the first: it may retry by another route only when nothing moved.
#[kani::proof]
#[kani::unwind(5)]
fn every_rename_classification_describes_the_namespace() {
    let cross_directory: bool = kani::any();
    let (mut protocol, mut action) = RenameProtocol::start(cross_directory);
    let mut namespace = RenameNamespace::before_rename();
    let mut a_sync_failed = false;
    for _ in 0..TRANSFER_STEPS {
        let succeeded: bool = kani::any();
        match action {
            RenameAction::Rename => {}
            RenameAction::SyncSourceDir => a_sync_failed |= !succeeded,
            RenameAction::SyncDestinationDir => {
                assert!(cross_directory, "a same-directory rename synced twice");
                a_sync_failed |= !succeeded;
            }
            RenameAction::Finish(class) => {
                kani::cover!(class == RenameClass::Success, "a rename succeeds");
                kani::cover!(class == RenameClass::BeforeRename, "the rename step fails");
                kani::cover!(
                    class == RenameClass::AfterRename,
                    "a directory sync fails after the rename"
                );
                kani::cover!(
                    class == RenameClass::AfterRename && cross_directory && namespace.source_synced,
                    "the destination directory sync fails"
                );
                match class {
                    RenameClass::BeforeRename => {
                        assert!(!namespace.renamed, "a before-rename class after a rename");
                    }
                    RenameClass::AfterRename => {
                        assert!(namespace.renamed, "an after-rename class without a rename");
                        assert!(a_sync_failed, "an after-rename class with no failed sync");
                    }
                    RenameClass::Success => {
                        assert!(namespace.renamed && namespace.source_synced && !a_sync_failed);
                        assert!(!cross_directory || namespace.destination_synced);
                    }
                }
                return;
            }
        }
        // The shared namespace asserts a sync follows only a rename that
        // took effect.
        namespace.execute(action, succeeded);
        (protocol, action) = protocol.step(succeeded);
    }
    panic!("the rename did not finish");
}
