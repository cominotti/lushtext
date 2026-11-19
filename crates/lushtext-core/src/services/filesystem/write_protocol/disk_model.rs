// SPDX-License-Identifier: GPL-3.0-or-later
//! The disk environment the durable-write protocols are proved in, shared by
//! the Kani harnesses (`kani_proofs`) and the shell conformance driver
//! (`services::conformance::write_shell`).
//!
//! It is verification code, compiled only under `cfg(kani)`, `cfg(test)` and
//! the `test-utils` feature, and never in a shipping build. The model holds
//! no choice of its own: every nondeterministic value is an argument. A Kani
//! harness passes `kani::any()` at the call site, which is the same
//! nondeterminism the model used to take inside itself; the conformance
//! driver passes what the real backend did, so both run this one source.
//!
//! - [`Disk`] is one destination name, its previous inode, and the one temp
//!   inode a write creates, with POSIX crash semantics: an unsynced file's
//!   bytes may be anything after a crash (`Torn`), and so may metadata applied
//!   after the last sync; a rename whose parent directory was not synced may
//!   or may not survive a crash, so the post-crash name points at either
//!   inode.
//! - [`MoveDisk`] is a durable move's source and destination.
//! - [`RenameNamespace`] is the name a durable rename moves and the two
//!   parent directories it may mutate.

use super::{MoveAction, RenameAction, StepOutcome, WriteAction};

/// What a file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Content {
    Empty,
    Old,
    New,
    Torn,
}

/// Which inode the destination name points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    Previous,
    Written,
}

/// The inode a write creates as its temp file.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TempInode {
    pub(crate) data: Content,
    /// The bytes a crash would leave, as of the last successful sync.
    pub(crate) durable_data: Content,
    pub(crate) metadata_applied: bool,
    /// Whether the required metadata was applied before the last sync.
    pub(crate) durable_metadata: bool,
    /// Created with the probed destination permissions, not a default mode.
    pub(crate) destination_mode: bool,
    /// The temp name still links it.
    pub(crate) linked: bool,
}

/// One destination of one atomic write.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Disk {
    pub(crate) probed: bool,
    pub(crate) temp: Option<TempInode>,
    pub(crate) live_entry: Entry,
    pub(crate) durable_entry: Entry,
    /// A `RemoveTemp` failed, leaving the temp for the startup sweep.
    pub(crate) removal_failed: bool,
    /// `CreateTemp` actions the protocol asked for.
    pub(crate) temp_name_attempts: u8,
    /// Every `CreateTemp` answered `AlreadyExists`: every name was taken.
    pub(crate) every_temp_name_taken: bool,
    /// Every backend action so far answered `Done`.
    pub(crate) every_step_done: bool,
}

/// A durable write shell mutant, to show which step a property rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shell {
    Production,
    /// Answers `SyncTemp` with `Done` without syncing.
    SkipsTempSync,
}

impl Disk {
    pub(crate) const fn before_write() -> Self {
        Self {
            probed: false,
            temp: None,
            live_entry: Entry::Previous,
            durable_entry: Entry::Previous,
            removal_failed: false,
            temp_name_attempts: 0,
            every_temp_name_taken: true,
            every_step_done: true,
        }
    }

    /// Execute one action whose backend outcome was `outcome`, and return the
    /// outcome the shell feeds back to the protocol. One backend outcome per
    /// action, as in the production shell.
    pub(crate) fn execute(
        &mut self,
        action: WriteAction,
        shell: Shell,
        outcome: StepOutcome,
    ) -> StepOutcome {
        let done = outcome == StepOutcome::Done;
        self.every_step_done &= done;
        match action {
            WriteAction::ProbeMetadata => self.probed = done,
            WriteAction::CreateTemp => {
                self.temp_name_attempts += 1;
                self.every_temp_name_taken &= outcome == StepOutcome::AlreadyExists;
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

    /// What the destination name shows after a crash now: the entry as of
    /// the last directory sync when `durable_branch`, else the live one.
    pub(crate) fn after_crash(&self, durable_branch: bool) -> (Content, bool) {
        let entry = if durable_branch {
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
    pub(crate) fn visible(&self) -> Content {
        match self.live_entry {
            Entry::Previous => Content::Old,
            Entry::Written => self.temp.expect("the written inode exists").data,
        }
    }

    /// Mode non-widening: new bytes only ever sit in an inode created with
    /// the destination's permissions.
    pub(crate) fn assert_new_bytes_are_never_wider(&self) {
        if let Some(temp) = self.temp
            && (temp.data == Content::New || temp.durable_data == Content::New)
        {
            assert!(
                temp.destination_mode,
                "new bytes sat in a file created with a wider default mode"
            );
        }
    }
}

/// A durable move by copy: whether its source still exists and whether the
/// destination holds the bytes durably.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MoveDisk {
    pub(crate) source_present: bool,
    pub(crate) destination_durable: bool,
}

impl MoveDisk {
    pub(crate) const fn before_move() -> Self {
        Self {
            source_present: true,
            destination_durable: false,
        }
    }

    /// Execute one action of the move whose backend result was `succeeded`.
    pub(crate) fn execute(&mut self, action: MoveAction, succeeded: bool) {
        match action {
            // The copy is a whole atomic write: success is `WriteClass::Success`.
            MoveAction::Copy => self.destination_durable = succeeded,
            MoveAction::RemoveSource => self.source_present = !succeeded,
            MoveAction::SyncSourceDir | MoveAction::Finish(_) => {}
        }
    }
}

/// The namespace a durable rename changes: whether the entry has its new
/// name, and whether each parent directory was synced after the rename.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RenameNamespace {
    pub(crate) renamed: bool,
    pub(crate) source_synced: bool,
    pub(crate) destination_synced: bool,
}

impl RenameNamespace {
    pub(crate) const fn before_rename() -> Self {
        Self {
            renamed: false,
            source_synced: false,
            destination_synced: false,
        }
    }

    /// Execute one action of the rename whose backend result was `succeeded`.
    /// A directory sync is asked for only after a rename that took effect.
    pub(crate) fn execute(&mut self, action: RenameAction, succeeded: bool) {
        match action {
            RenameAction::Rename => self.renamed = succeeded,
            RenameAction::SyncSourceDir => {
                assert!(
                    self.renamed,
                    "a directory was synced for a rename that failed"
                );
                self.source_synced = succeeded;
            }
            RenameAction::SyncDestinationDir => {
                assert!(
                    self.renamed,
                    "a directory was synced for a rename that failed"
                );
                self.destination_synced = succeeded;
            }
            RenameAction::Finish(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{MoveProtocol, RenameProtocol, WriteClass, WriteProtocol};
    use super::*;

    #[test]
    fn a_fault_free_write_leaves_the_new_bytes_durably_visible() {
        let mut disk = Disk::before_write();
        let (mut protocol, mut action) = WriteProtocol::start();
        while !matches!(action, WriteAction::Finish(_)) {
            let fed = disk.execute(action, Shell::Production, StepOutcome::Done);
            (protocol, action) = protocol.step(fed);
            disk.assert_new_bytes_are_never_wider();
        }
        assert_eq!(action, WriteAction::Finish(WriteClass::Success));
        assert_eq!(disk.visible(), Content::New);
        assert_eq!(disk.after_crash(true), (Content::New, true));
        assert_eq!(disk.after_crash(false), (Content::New, true));
    }

    #[test]
    fn a_failed_rename_keeps_the_previous_bytes_and_unlinks_the_temp() {
        let mut disk = Disk::before_write();
        let (mut protocol, mut action) = WriteProtocol::start();
        while !matches!(action, WriteAction::Finish(_)) {
            let outcome = if action == WriteAction::Rename {
                StepOutcome::Failed
            } else {
                StepOutcome::Done
            };
            let fed = disk.execute(action, Shell::Production, outcome);
            (protocol, action) = protocol.step(fed);
        }
        assert_eq!(action, WriteAction::Finish(WriteClass::BeforeRename));
        assert_eq!(disk.visible(), Content::Old);
        assert!(disk.temp.is_none());
    }

    #[test]
    fn a_move_and_a_rename_track_their_namespace() {
        let mut moved = MoveDisk::before_move();
        let (mut protocol, mut action) = MoveProtocol::start();
        while !matches!(action, MoveAction::Finish(_)) {
            moved.execute(action, true);
            (protocol, action) = protocol.step(true);
        }
        assert!(!moved.source_present && moved.destination_durable);

        let mut namespace = RenameNamespace::before_rename();
        let (mut protocol, mut action) = RenameProtocol::start(true);
        while !matches!(action, RenameAction::Finish(_)) {
            namespace.execute(action, true);
            (protocol, action) = protocol.step(true);
        }
        assert!(namespace.renamed && namespace.source_synced && namespace.destination_synced);
    }
}
