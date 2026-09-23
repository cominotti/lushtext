// SPDX-License-Identifier: GPL-3.0-or-later

//! The durable-write protocols as I/O-free state machines.
//!
//! Every decision the durable writer makes — which filesystem operation comes
//! next, what a failure means, whether the source of a move may go — lives
//! here. The durable-write shell (`durable_write.rs`) owns no decisions: it executes
//! each [`WriteAction`] (or [`MoveAction`], [`RenameAction`]) with exactly one
//! backend call and feeds the call's [`StepOutcome`] back unchanged.
//!
//! Because the protocols are finite, Kani explores **every** outcome sequence
//! and every crash point of the real machines (`kani_proofs` beside this
//! module), which makes these complete proofs rather than bounded ones:
//!
//! - **crash atomicity** — after any crash the destination holds the complete
//!   previous bytes or the complete new bytes, never a torn file;
//! - **classification soundness** — [`WriteClass::BeforeRename`] implies the
//!   previous bytes are still visible, [`WriteClass::AfterRename`] that the new
//!   bytes are;
//! - **mode non-widening** — new bytes only ever sit in a temp file created
//!   with the destination's permissions;
//! - **move safety** — a move removes its source only after the destination is
//!   durable.
//!
//! The atomic write, in order: probe the destination's metadata, create the
//! temp file with those permissions (a taken name is retried with a fresh one,
//! a bounded number of times), write and flush the content, apply the required
//! metadata, sync the temp file **after** that metadata, rename it over the
//! destination, then sync the parent directory. Any failure before the rename
//! removes the temp file and reports `BeforeRename`; a failed directory sync
//! after it reports `AfterRename`.

/// Bounded fresh names tried when a temp name already exists.
pub const MAX_TEMP_NAME_ATTEMPTS: u8 = 8;

/// The outcome of the last executed action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum StepOutcome {
    /// The operation succeeded.
    Done,
    /// The operation failed because its target name already exists.
    AlreadyExists,
    /// The operation failed for any other reason.
    Failed,
}

/// How an atomic write ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteClass {
    /// The new bytes are the destination, durably.
    Success,
    /// Nothing was committed: the destination still holds its previous bytes.
    BeforeRename,
    /// The new bytes are the destination, but the directory sync that makes
    /// the rename durable did not complete.
    AfterRename,
}

/// The next operation of an atomic write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteAction {
    /// Read the destination's (or explicit source's) metadata.
    ProbeMetadata,
    /// Create the temp file, with a fresh name, with the probed permissions.
    CreateTemp,
    /// Write and flush the content into the temp file.
    WriteContent,
    /// Apply the required metadata to the temp file.
    ApplyMetadata,
    /// Sync the temp file.
    SyncTemp,
    /// Rename the temp file over the destination.
    Rename,
    /// Sync the destination's parent directory.
    SyncDir,
    /// Remove the temp file after a failure (its own outcome is ignored).
    RemoveTemp,
    /// The write is over.
    Finish(WriteClass),
}

/// The atomic-write protocol's state: the action it last asked for and how
/// many temp names it has tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteProtocol {
    pending: WriteAction,
    temp_attempts: u8,
}

impl WriteProtocol {
    /// Start a write; the first action is [`WriteAction::ProbeMetadata`].
    #[must_use]
    pub const fn start() -> (Self, WriteAction) {
        (
            Self {
                pending: WriteAction::ProbeMetadata,
                temp_attempts: 0,
            },
            WriteAction::ProbeMetadata,
        )
    }

    /// Feed the outcome of the action last returned; get the next one.
    ///
    /// Once [`WriteAction::Finish`] has been returned, further steps keep
    /// returning it.
    #[must_use]
    pub const fn step(self, outcome: StepOutcome) -> (Self, WriteAction) {
        let ok = matches!(outcome, StepOutcome::Done);
        let mut temp_attempts = self.temp_attempts;
        let next = match self.pending {
            WriteAction::ProbeMetadata if ok => WriteAction::CreateTemp,
            // A failed probe created nothing; a removed temp leaves nothing.
            WriteAction::ProbeMetadata | WriteAction::RemoveTemp => {
                WriteAction::Finish(WriteClass::BeforeRename)
            }
            WriteAction::CreateTemp => {
                temp_attempts = temp_attempts.saturating_add(1);
                match outcome {
                    StepOutcome::Done => WriteAction::WriteContent,
                    // Never reuse an existing name: it may be another writer's
                    // temp file. Nothing was created, so nothing is removed.
                    StepOutcome::AlreadyExists if temp_attempts < MAX_TEMP_NAME_ATTEMPTS => {
                        WriteAction::CreateTemp
                    }
                    StepOutcome::AlreadyExists | StepOutcome::Failed => {
                        WriteAction::Finish(WriteClass::BeforeRename)
                    }
                }
            }
            WriteAction::WriteContent if ok => WriteAction::ApplyMetadata,
            WriteAction::ApplyMetadata if ok => WriteAction::SyncTemp,
            WriteAction::SyncTemp if ok => WriteAction::Rename,
            WriteAction::WriteContent
            | WriteAction::ApplyMetadata
            | WriteAction::SyncTemp
            | WriteAction::Rename => {
                if ok {
                    WriteAction::SyncDir
                } else {
                    WriteAction::RemoveTemp
                }
            }
            WriteAction::SyncDir if ok => WriteAction::Finish(WriteClass::Success),
            WriteAction::SyncDir => WriteAction::Finish(WriteClass::AfterRename),
            WriteAction::Finish(class) => WriteAction::Finish(class),
        };
        (
            Self {
                pending: next,
                temp_attempts,
            },
            next,
        )
    }
}

/// The next operation of a durable move by copy ([`MoveProtocol`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveAction {
    /// Durably copy the source to the destination (a whole [`WriteProtocol`]).
    Copy,
    /// Remove the source.
    RemoveSource,
    /// Sync the source's parent directory.
    SyncSourceDir,
    /// The move is over; `true` when it completed.
    Finish(bool),
}

/// A durable move by copy: the source is removed only after the destination
/// is durable, so every failure before that leaves the source in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveProtocol {
    pending: MoveAction,
}

impl MoveProtocol {
    /// Start a move; the first action is [`MoveAction::Copy`].
    #[must_use]
    pub const fn start() -> (Self, MoveAction) {
        (
            Self {
                pending: MoveAction::Copy,
            },
            MoveAction::Copy,
        )
    }

    /// Feed whether the last action succeeded; get the next one. For
    /// [`MoveAction::Copy`], success means the copy's write finished
    /// [`WriteClass::Success`].
    #[must_use]
    pub const fn step(self, succeeded: bool) -> (Self, MoveAction) {
        let next = match self.pending {
            MoveAction::Copy if succeeded => MoveAction::RemoveSource,
            MoveAction::RemoveSource if succeeded => MoveAction::SyncSourceDir,
            MoveAction::SyncSourceDir if succeeded => MoveAction::Finish(true),
            MoveAction::Copy | MoveAction::RemoveSource | MoveAction::SyncSourceDir => {
                MoveAction::Finish(false)
            }
            MoveAction::Finish(done) => MoveAction::Finish(done),
        };
        (Self { pending: next }, next)
    }
}

/// The next operation of a durable rename ([`RenameProtocol`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameAction {
    /// Rename the source to the destination.
    Rename,
    /// Sync the source's parent directory.
    SyncSourceDir,
    /// Sync the destination's parent directory (a different directory).
    SyncDestinationDir,
    /// The rename is over; `true` when it completed.
    Finish(bool),
}

/// A durable rename: rename, then sync every parent directory it mutated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenameProtocol {
    pending: RenameAction,
    cross_directory: bool,
}

impl RenameProtocol {
    /// Start a rename; `cross_directory` when source and destination live in
    /// different directories.
    #[must_use]
    pub const fn start(cross_directory: bool) -> (Self, RenameAction) {
        (
            Self {
                pending: RenameAction::Rename,
                cross_directory,
            },
            RenameAction::Rename,
        )
    }

    /// Feed whether the last action succeeded; get the next one.
    #[must_use]
    pub const fn step(self, succeeded: bool) -> (Self, RenameAction) {
        let next = match self.pending {
            RenameAction::Rename if succeeded => RenameAction::SyncSourceDir,
            RenameAction::SyncSourceDir if succeeded && self.cross_directory => {
                RenameAction::SyncDestinationDir
            }
            RenameAction::SyncSourceDir | RenameAction::SyncDestinationDir if succeeded => {
                RenameAction::Finish(true)
            }
            RenameAction::Rename
            | RenameAction::SyncSourceDir
            | RenameAction::SyncDestinationDir => RenameAction::Finish(false),
            RenameAction::Finish(done) => RenameAction::Finish(done),
        };
        (
            Self {
                pending: next,
                cross_directory: self.cross_directory,
            },
            next,
        )
    }
}

#[cfg(kani)]
mod kani_proofs;

#[cfg(test)]
mod tests {
    use super::*;

    fn run_write(outcomes: &[StepOutcome]) -> Vec<WriteAction> {
        let (mut protocol, mut action) = WriteProtocol::start();
        let mut actions = vec![action];
        for outcome in outcomes {
            (protocol, action) = protocol.step(*outcome);
            actions.push(action);
        }
        actions
    }

    #[test]
    fn a_successful_write_runs_the_whole_ordered_protocol() {
        use WriteAction::*;
        assert_eq!(
            run_write(&[StepOutcome::Done; 7]),
            vec![
                ProbeMetadata,
                CreateTemp,
                WriteContent,
                ApplyMetadata,
                SyncTemp,
                Rename,
                SyncDir,
                Finish(WriteClass::Success),
            ]
        );
    }

    #[test]
    fn every_failure_before_the_rename_removes_the_temp_and_is_before_rename() {
        use StepOutcome::{Done, Failed};
        for failing_at in 2..=5 {
            let mut outcomes = vec![Done; failing_at];
            outcomes.push(Failed);
            outcomes.push(Done);
            let actions = run_write(&outcomes);
            assert_eq!(
                actions[failing_at + 1],
                WriteAction::RemoveTemp,
                "{failing_at}"
            );
            assert_eq!(
                actions[failing_at + 2],
                WriteAction::Finish(WriteClass::BeforeRename)
            );
        }
        // Probe and create failures leave nothing to remove.
        assert_eq!(
            run_write(&[Failed]),
            vec![
                WriteAction::ProbeMetadata,
                WriteAction::Finish(WriteClass::BeforeRename)
            ]
        );
        assert_eq!(
            run_write(&[Done, Failed])[2],
            WriteAction::Finish(WriteClass::BeforeRename)
        );
    }

    #[test]
    fn a_failed_directory_sync_after_the_rename_is_after_rename() {
        let mut outcomes = vec![StepOutcome::Done; 6];
        outcomes.push(StepOutcome::Failed);
        assert_eq!(
            run_write(&outcomes).last(),
            Some(&WriteAction::Finish(WriteClass::AfterRename))
        );
    }

    #[test]
    fn a_taken_temp_name_is_retried_a_bounded_number_of_times() {
        let mut outcomes = vec![StepOutcome::Done];
        outcomes.extend([StepOutcome::AlreadyExists; 7]);
        let actions = run_write(&outcomes);
        assert!(
            actions[1..]
                .iter()
                .all(|action| *action == WriteAction::CreateTemp)
        );
        outcomes.push(StepOutcome::AlreadyExists);
        assert_eq!(
            run_write(&outcomes).last(),
            Some(&WriteAction::Finish(WriteClass::BeforeRename))
        );
        let mut recovered = vec![
            StepOutcome::Done,
            StepOutcome::AlreadyExists,
            StepOutcome::Done,
        ];
        recovered.push(StepOutcome::Done);
        assert_eq!(run_write(&recovered)[3], WriteAction::WriteContent);
    }

    #[test]
    fn a_move_removes_its_source_only_after_a_successful_copy() {
        let (protocol, first) = MoveProtocol::start();
        assert_eq!(first, MoveAction::Copy);
        assert_eq!(protocol.step(false).1, MoveAction::Finish(false));
        let (protocol, next) = protocol.step(true);
        assert_eq!(next, MoveAction::RemoveSource);
        let (protocol, next) = protocol.step(true);
        assert_eq!(next, MoveAction::SyncSourceDir);
        assert_eq!(protocol.step(true).1, MoveAction::Finish(true));
        assert_eq!(protocol.step(false).1, MoveAction::Finish(false));
    }

    #[test]
    fn a_rename_syncs_each_directory_it_mutated() {
        let (same, _) = RenameProtocol::start(false);
        let (same, next) = same.step(true);
        assert_eq!(next, RenameAction::SyncSourceDir);
        assert_eq!(same.step(true).1, RenameAction::Finish(true));
        let (cross, _) = RenameProtocol::start(true);
        let (cross, _) = cross.step(true);
        let (cross, next) = cross.step(true);
        assert_eq!(next, RenameAction::SyncDestinationDir);
        assert_eq!(cross.step(true).1, RenameAction::Finish(true));
        assert_eq!(
            RenameProtocol::start(true).0.step(false).1,
            RenameAction::Finish(false)
        );
    }
}
