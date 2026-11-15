// SPDX-License-Identifier: GPL-3.0-or-later

//! The I/O-free per-file protocols of Replace All and its Undo (design D3, D6).
//!
//! Each protocol is a `step(outcome) -> action` machine in the style of
//! `services::filesystem::write_protocol`: the shell in `apply.rs` or
//! `undo.rs` executes every action with one call and feeds its outcome back
//! unchanged, so every ordering decision lives here, where
//! `protocol/kani_proofs.rs` proves it (shard `core-replace-protocol`):
//!
//! - the target is rewritten only after its undo body, its entry and the armed
//!   manifest are durable (P2 → P4 → P5);
//! - an entry is removed before its body, and a body whose entry could not be
//!   removed is kept, so no entry ever references a missing body;
//! - Undo writes only to a file whose current digest equals the replaced one.
//!
//! The durable writes themselves are `atomic_replace_stream` calls whose
//! crash safety the write-protocol shards prove; this module assumes their
//! theorems (a write that fails before its rename leaves the previous bytes).

/// One step of the apply protocol for one file (design D3, P0–P6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileAction {
    /// P0: take the target's write guard and its identity facts.
    AcquireTarget,
    /// P1: admit the undo body against the journal's free space.
    AdmitBody,
    /// P1: before the run's first body, remove any previous run's journal.
    PrepareJournal,
    /// P2, pass A: stream the target into its body while dry-running the edits.
    WriteBody,
    /// P3: admit the output against the target's free space.
    AdmitOutput,
    /// P4: write the entry durably.
    WriteEntry,
    /// P4: after the run's first entry, arm the incremental manifest.
    ArmJournal,
    /// P5, pass B: stream the body through the edits into the target.
    WriteTarget,
    /// Cleanup: remove the entry (always before its body).
    RemoveEntry,
    /// Cleanup: remove the body, once no entry references it.
    RemoveBody,
    /// P6: release the guard; the file ends here.
    Finish(FileEnd),
}

/// How one file's apply ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileEnd {
    /// The target holds the replaced bytes, with an active entry.
    Replaced {
        /// Whether the target's directory sync after the rename succeeded.
        durable: bool,
    },
    /// The target keeps its bytes; the reason is the shell's to report.
    NotReplaced {
        /// Whether the journal was left without this file's entry and body.
        /// A failed entry removal keeps the body, which cleanup removes later.
        cleaned: bool,
    },
    /// First pass only: the output is larger than `main`'s first-pass limit,
    /// so the file goes to the second pass. Nothing of it was kept.
    Deferred,
    /// No edit applied; nothing of the file was kept.
    Unchanged,
    /// Cancellation was observed; the target keeps its bytes. The entry and
    /// body this file wrote are removed, entry first, unless a removal failed:
    /// then both stay (or an orphan body does), never an entry without its
    /// body, and the cancelled run's journal delete or a later Undo, which
    /// finds the target already original, retires them.
    Cancelled,
}

/// The outcome of the shell's last action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileOutcome {
    /// The action succeeded.
    Done,
    /// The action failed and left nothing behind (a failed write is
    /// `BeforeRename`: the previous bytes are intact, the temp is removed).
    Failed,
    /// `WriteBody` only: the body was renamed into place but its directory
    /// sync failed, so the body may or may not survive a crash.
    BodyUnconfirmed,
    /// `WriteBody` only: the dry run found no edit to apply.
    NoChange,
    /// `AcquireTarget` or `WriteBody`: the source or output is larger than
    /// the first pass's limit.
    Defer,
    /// `WriteBody` or `WriteTarget`: cancellation was observed before rename.
    Cancelled,
    /// `WriteTarget` only: renamed, but the directory sync failed.
    RenamedUnconfirmed,
}

/// Per-run facts the protocol needs: whether this is the run's first body and
/// first entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RunJournalState {
    /// The previous run's journal was already removed.
    pub(crate) prepared: bool,
    /// The incremental manifest is already armed.
    pub(crate) armed: bool,
}

/// The apply protocol for one file.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReplaceFileProtocol {
    action: FileAction,
    journal: RunJournalState,
    /// The end the cleanup currently under way leads to.
    pending_end: FileEnd,
    /// The body exists (or may exist) and is this file's.
    body_written: bool,
    /// The entry exists (or may exist).
    entry_written: bool,
}

impl ReplaceFileProtocol {
    /// Start one file; the first action is always [`FileAction::AcquireTarget`].
    #[must_use]
    pub(crate) fn start(journal: RunJournalState) -> (Self, FileAction) {
        let protocol = Self {
            action: FileAction::AcquireTarget,
            journal,
            pending_end: FileEnd::NotReplaced { cleaned: true },
            body_written: false,
            entry_written: false,
        };
        (protocol, protocol.action)
    }

    /// The journal facts after this file, for the next file's [`Self::start`].
    #[must_use]
    pub(crate) fn journal(self) -> RunJournalState {
        self.journal
    }

    /// Feed the outcome of the last action and get the next one.
    pub(crate) fn step(&mut self, outcome: FileOutcome) -> FileAction {
        use FileAction as A;
        use FileOutcome as O;
        let next = match (self.action, outcome) {
            (A::AcquireTarget, O::Done) => A::AdmitBody,
            // First pass only: the source (at the target) or the output (at
            // the body) is larger than `main`'s first-pass limit, so the file
            // goes to the second pass untouched.
            (A::AcquireTarget | A::WriteBody, O::Defer) => A::Finish(FileEnd::Deferred),
            (A::AdmitBody, O::Done) if self.journal.prepared => A::WriteBody,
            (A::AdmitBody, O::Done) => A::PrepareJournal,
            (A::PrepareJournal, O::Done) => {
                self.journal.prepared = true;
                A::WriteBody
            }
            (A::WriteBody, O::Done) => {
                self.body_written = true;
                A::AdmitOutput
            }
            (A::WriteBody, O::BodyUnconfirmed) => {
                self.body_written = true;
                self.cleanup(FileEnd::NotReplaced { cleaned: true })
            }
            (A::WriteBody, O::NoChange) => A::Finish(FileEnd::Unchanged),
            (A::WriteBody | A::WriteTarget, O::Cancelled) => self.cleanup(FileEnd::Cancelled),
            (A::AdmitOutput, O::Done) => A::WriteEntry,
            (A::WriteEntry, O::Done) if self.journal.armed => {
                self.entry_written = true;
                A::WriteTarget
            }
            (A::WriteEntry, O::Done) => {
                self.entry_written = true;
                A::ArmJournal
            }
            (A::ArmJournal, O::Done) => {
                self.journal.armed = true;
                A::WriteTarget
            }
            (A::WriteTarget, O::Done) => A::Finish(FileEnd::Replaced { durable: true }),
            (A::WriteTarget, O::RenamedUnconfirmed) => {
                A::Finish(FileEnd::Replaced { durable: false })
            }
            (A::WriteEntry, _) => {
                // A failed entry write may have left a partial entry: remove it
                // before the body like any other.
                self.entry_written = true;
                self.cleanup(FileEnd::NotReplaced { cleaned: true })
            }
            (A::RemoveEntry, O::Done) => {
                self.entry_written = false;
                A::RemoveBody
            }
            (A::RemoveEntry, _) => {
                // The entry may still reference the body: keep both, so no
                // entry references a missing body. The target is unchanged,
                // so Undo treats the entry as already restored.
                A::Finish(match self.pending_end {
                    FileEnd::NotReplaced { .. } => FileEnd::NotReplaced { cleaned: false },
                    end => end,
                })
            }
            (A::RemoveBody, O::Done) => {
                self.body_written = false;
                A::Finish(self.pending_end)
            }
            (A::RemoveBody, _) => A::Finish(match self.pending_end {
                // An orphan body is benign and cleanup removes it (D9).
                FileEnd::NotReplaced { .. } => FileEnd::NotReplaced { cleaned: false },
                end => end,
            }),
            (A::Finish(end), _) => A::Finish(end),
            // Every other failure: undo whatever this file left, entry first.
            (_, _) => self.cleanup(FileEnd::NotReplaced { cleaned: true }),
        };
        self.action = next;
        next
    }

    fn cleanup(&mut self, end: FileEnd) -> FileAction {
        self.pending_end = end;
        if self.entry_written {
            FileAction::RemoveEntry
        } else if self.body_written {
            FileAction::RemoveBody
        } else {
            FileAction::Finish(end)
        }
    }
}

/// How the current bytes of an undo target compare with its entry (U1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CurrentContent {
    /// Length and digest equal the original's: already restored.
    Original,
    /// Length and digest equal the replaced bytes': restore it.
    Replaced,
    /// Anything else, a read failure or a file past the limit: keep it.
    Diverged,
}

/// One step of the Undo (or rollback) protocol for one entry (design D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UndoAction {
    /// U0: take the target's write guard and its identity facts.
    AcquireTarget,
    /// U1: stream-hash the current file.
    HashCurrent,
    /// Before the first restore write of an Undo in a later launch: sweep the
    /// recorded launch's temp files and re-arm the manifest with this launch.
    RearmJournal,
    /// U2: admit the restore against its raw need, never the reserve.
    AdmitRestore,
    /// U2: stream the body into the target, checking its digest and the facts.
    WriteRestore,
    /// U3: remove the entry.
    RemoveEntry,
    /// U3: remove the body, after its entry.
    RemoveBody,
    /// The entry ends here.
    Finish(UndoEnd),
}

/// How one entry's Undo ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UndoEnd {
    /// The target holds its original bytes. `retired` says whether the entry
    /// left the journal (a failed removal leaves an entry Undo recognises as
    /// restored).
    Restored {
        /// The entry was removed from the journal.
        retired: bool,
    },
    /// The target holds neither digest; nothing was written. Retryable.
    Diverged,
    /// A step failed before any write; the entry stays. Retryable.
    Failed,
}

/// The outcome of the Undo shell's last action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UndoOutcome {
    /// The action succeeded.
    Done,
    /// The action failed and left the target's bytes in place.
    Failed,
    /// `HashCurrent` only: what the current bytes are.
    Current(CurrentContent),
}

/// The Undo protocol for one entry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReplaceUndoProtocol {
    action: UndoAction,
    rearm_needed: bool,
    /// Whether to retire the entry from the journal after the restore.
    retire_entry: bool,
}

impl ReplaceUndoProtocol {
    /// Start one entry. `rearm_needed` is true until this Undo, running in a
    /// later launch than the manifest records, has re-armed it; `retire_entry`
    /// is false for an undo owner with no journal.
    #[must_use]
    pub(crate) fn start(rearm_needed: bool, retire_entry: bool) -> (Self, UndoAction) {
        let protocol = Self {
            action: UndoAction::AcquireTarget,
            rearm_needed,
            retire_entry,
        };
        (protocol, protocol.action)
    }

    /// Whether the manifest still needs re-arming after this entry.
    #[must_use]
    pub(crate) fn rearm_needed(self) -> bool {
        self.rearm_needed
    }

    /// Feed the outcome of the last action and get the next one.
    pub(crate) fn step(&mut self, outcome: UndoOutcome) -> UndoAction {
        use UndoAction as A;
        use UndoOutcome as O;
        let next = match (self.action, outcome) {
            (A::AcquireTarget, O::Done) => A::HashCurrent,
            (A::HashCurrent, O::Current(CurrentContent::Original)) | (A::WriteRestore, O::Done) => {
                self.retire()
            }
            (A::HashCurrent, O::Current(CurrentContent::Replaced)) if self.rearm_needed => {
                A::RearmJournal
            }
            (A::HashCurrent, O::Current(CurrentContent::Replaced)) => A::AdmitRestore,
            (A::HashCurrent, O::Current(CurrentContent::Diverged)) => A::Finish(UndoEnd::Diverged),
            (A::RearmJournal, O::Done) => {
                self.rearm_needed = false;
                A::AdmitRestore
            }
            (A::AdmitRestore, O::Done) => A::WriteRestore,
            (A::RemoveEntry, O::Done) => A::RemoveBody,
            (A::RemoveEntry, _) => A::Finish(UndoEnd::Restored { retired: false }),
            (A::RemoveBody, _) => A::Finish(UndoEnd::Restored { retired: true }),
            (A::Finish(end), _) => A::Finish(end),
            (_, _) => A::Finish(UndoEnd::Failed),
        };
        self.action = next;
        next
    }

    fn retire(self) -> UndoAction {
        if self.retire_entry {
            UndoAction::RemoveEntry
        } else {
            UndoAction::Finish(UndoEnd::Restored { retired: false })
        }
    }
}

/// The order a journal shrink removes one retired entry's files in: the entry
/// first, then its body, and the body only once the entry is gone (D3a, D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShrinkAction {
    /// Remove the entry JSON.
    RemoveEntry,
    /// Remove the body the entry referenced.
    RemoveBody,
    /// Done with this entry; `body_kept` when the entry removal failed.
    Finish {
        /// The body was kept because its entry may still reference it.
        body_kept: bool,
    },
}

/// Next shrink action after `action` ended with `succeeded`.
#[must_use]
pub(crate) fn shrink_step(action: ShrinkAction, succeeded: bool) -> ShrinkAction {
    match (action, succeeded) {
        (ShrinkAction::RemoveEntry, true) => ShrinkAction::RemoveBody,
        (ShrinkAction::RemoveEntry, false) => ShrinkAction::Finish { body_kept: true },
        (ShrinkAction::RemoveBody | ShrinkAction::Finish { .. }, _) => {
            ShrinkAction::Finish { body_kept: false }
        }
    }
}

#[cfg(kani)]
mod kani_proofs;
