// SPDX-License-Identifier: GPL-3.0-or-later

//! A port of the Kani harness ENVIRONMENT in
//! `crates/lushtext-core/src/services/draft_service/kani_proofs.rs`.
//!
//! Every journal DECISION is a call to the real
//! `lushtext_core::services::draft_service::journal_core`, at exactly the
//! points `kani_proofs.rs` calls it. The differences from the Kani harness are
//! mechanical and listed here so a reviewer can check them one by one:
//!
//! - each `kani::any()` becomes an explicit argument (`actor`, `action`, `id`,
//!   `fault`, and the `RestoreEnding` chosen inside `RestoreApply`, restricted
//!   to the endings the harness's `kani::assume` admits);
//! - the previous-session bits become an enumerated set of initial disks
//!   ([`Journal::previous_session`] takes them as a parameter);
//! - the two inline `assert!`s (`WriteBody`: "a body was written without an
//!   entry"; `ExecCleanup`: S2) record sticky flags instead of panicking, so the
//!   checker can report them as properties;
//! - the invariant checks return `bool` instead of asserting;
//! - `Hash` is written by hand for [`Editor`], because the production
//!   `DeletionStep` does not derive `Hash`.

use core::hash::{Hash, Hasher};

use lushtext_core::model::draft::DraftManifestCompleteness;
use lushtext_core::services::draft_service::journal_core::{
    BodyFacts, BodyWriteDecision, CleanupBodyIdentity, CleanupEntryState, CleanupFacts,
    DeletionStep, EntryState, OrphanBodyDecision, RestoreDisposition, RestoreEnding,
    UntrustedCommitDisposition, body_write_decision, commit_authority, deletion_start,
    may_write_after_registration, next_deletion_step, orphan_body_decision, ownership,
    registration_required, startup_may_retire_stale, unapplied_restore_disposition,
    untrusted_commit_disposition,
};

/// Draft ids in the model.
pub const IDS: usize = 3;
/// Content ids: 0 plus one per pre-existing body and one per edit.
pub const CONTENTS: usize = 1 + IDS + MAX_EDITS;
/// Edits the model admits.
pub const MAX_EDITS: usize = 3;

pub type ContentId = u8;
pub type ContentSet = u16;

const fn bit(content: ContentId) -> ContentSet {
    1 << content
}

/// The `RestoreEnding`s the harness's `kani::assume` admits inside
/// `RestoreApply` (everything except `Stale` and `MissingBody`).
pub const CHOSEN_ENDINGS: [RestoreEnding; 6] = [
    RestoreEnding::Applied,
    RestoreEnding::Oversized,
    RestoreEnding::ReadFailed,
    RestoreEnding::EditedOver,
    RestoreEnding::InstallCancelled,
    RestoreEnding::Unavailable,
];

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct DiskEntry {
    pub present: bool,
    /// Backing-file version the entry's body was written against.
    pub backing: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Editor {
    pub open: bool,
    pub content: ContentId,
    /// `draft_dirty`: the buffer is ahead of the recovery body.
    pub dirty: bool,
    /// A body-write token minted by this pass's registration step.
    pub token: bool,
    /// Content written as a body and awaiting its manifest commit.
    pub written: Option<ContentId>,
    /// A restore of this id's body is pending: the user has not seen it.
    pub restore_pending: bool,
    /// Tombstone plus the next step of an in-progress deletion.
    pub deletion: Option<DeletionStep>,
    /// The deletion must preserve the body first (queued preservation).
    pub preserve_queued: bool,
    /// The window's manifest copy lists the id.
    pub known_entry: bool,
    /// An orphan-cleanup inspection saw this body content.
    pub cleanup_candidate: Option<ContentId>,
}

impl Hash for Editor {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.open.hash(state);
        self.content.hash(state);
        self.dirty.hash(state);
        self.token.hash(state);
        self.written.hash(state);
        self.restore_pending.hash(state);
        // `DeletionStep` is fieldless and `Copy` but does not derive `Hash`.
        self.deletion.map(|step| step as u8).hash(state);
        self.preserve_queued.hash(state);
        self.known_entry.hash(state);
        self.cleanup_candidate.hash(state);
    }
}

pub const CLOSED: Editor = Editor {
    open: false,
    content: 0,
    dirty: false,
    token: false,
    written: None,
    restore_pending: false,
    deletion: None,
    preserve_queued: false,
    known_entry: false,
    cleanup_candidate: None,
};

/// One LushText process's in-memory journal state (K8 runs two).
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct Window {
    pub editors: [Editor; IDS],
    pub running: bool,
    pub trusted: bool,
    pub last_reconciliation_complete: bool,
}

/// The previous session's disk for one id (the harness's three `kani::any()`
/// bits per id).
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct PreviousId {
    pub entry: bool,
    pub body: bool,
    pub backing_moved: bool,
}

impl PreviousId {
    /// All eight combinations, in a deterministic order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0u8..8).map(|bits| Self {
            entry: bits & 1 != 0,
            body: bits & 2 != 0,
            backing_moved: bits & 4 != 0,
        })
    }
}

/// One nondeterministic action of the environment.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Action {
    Edit,
    Register,
    WriteBody,
    Commit,
    Save,
    Discard,
    DeletionStep,
    Inspect,
    ExecCleanup,
    RestoreApply,
    ExternalMtime,
    Crash,
    Startup,
}

impl Action {
    pub const ALL: [Self; 13] = [
        Self::Edit,
        Self::Register,
        Self::WriteBody,
        Self::Commit,
        Self::Save,
        Self::Discard,
        Self::DeletionStep,
        Self::Inspect,
        Self::ExecCleanup,
        Self::RestoreApply,
        Self::ExternalMtime,
        Self::Crash,
        Self::Startup,
    ];
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Journal {
    /// A second process over the same data directory, parked while the
    /// active one acts (axiom A6 dropped); `None` models A6.
    pub other: Option<Window>,
    pub entry: [DiskEntry; IDS],
    pub body: [Option<ContentId>; IDS],
    pub backing: [u8; IDS],
    /// Content ids durably kept in the set-aside area (or local history).
    pub preserved: ContentSet,
    pub editors: [Editor; IDS],
    pub running: bool,
    pub trusted: bool,
    /// Ghost: the last reconciliation that set `trusted` was complete.
    pub last_reconciliation_complete: bool,
    /// Ghost: content the user was told is protected.
    pub accepted: ContentSet,
    /// Ghost: content the user resolved (saved to its file, or discarded).
    pub resolved: ContentSet,
    /// Ancestor sets: `ancestors[c]` holds every content `c` contains.
    pub ancestors: [ContentSet; CONTENTS],
    pub next_content: ContentId,
    pub edits: usize,
    /// Replaces `assert!(entry present)` in `WriteBody`.
    pub body_without_entry: bool,
    /// Replaces the S2 `assert!` in `ExecCleanup`.
    pub cleanup_unsafe: bool,
}

impl Journal {
    /// A previous session's disk: any id may have an entry, a body, both, or
    /// neither, and its file may have moved on.
    pub fn previous_session(previous: [PreviousId; IDS]) -> Self {
        let mut journal = Self {
            other: None,
            entry: [DiskEntry {
                present: false,
                backing: 0,
            }; IDS],
            body: [None; IDS],
            backing: [0; IDS],
            preserved: 0,
            editors: [CLOSED; IDS],
            running: false,
            trusted: false,
            last_reconciliation_complete: false,
            accepted: 0,
            resolved: 0,
            ancestors: [0; CONTENTS],
            next_content: 1 + IDS as ContentId,
            edits: 0,
            body_without_entry: false,
            cleanup_unsafe: false,
        };
        for id in 0..IDS {
            let content = 1 + id as ContentId;
            journal.ancestors[usize::from(content)] = bit(content);
            journal.entry[id] = DiskEntry {
                present: previous[id].entry,
                backing: 0,
            };
            if previous[id].body {
                journal.body[id] = Some(content);
                // Everything a previous session left on disk was promised.
                journal.accepted |= bit(content);
            }
            if previous[id].backing_moved {
                journal.backing[id] = 1;
            }
        }
        journal
    }

    /// Adds the parked second process of the K8 harness.
    pub fn with_second_process(mut self) -> Self {
        self.other = Some(Window {
            editors: [CLOSED; IDS],
            running: false,
            trusted: false,
            last_reconciliation_complete: false,
        });
        self
    }

    pub fn swap_windows(&mut self) {
        if let Some(other) = self.other.as_mut() {
            core::mem::swap(&mut other.editors, &mut self.editors);
            core::mem::swap(&mut other.running, &mut self.running);
            core::mem::swap(&mut other.trusted, &mut self.trusted);
            core::mem::swap(
                &mut other.last_reconciliation_complete,
                &mut self.last_reconciliation_complete,
            );
        }
    }

    /// One action by process `actor` (0, or 1 when a second process exists).
    pub fn step_as(
        &mut self,
        actor: bool,
        action: Action,
        id: usize,
        fault: bool,
        ending: Option<RestoreEnding>,
    ) {
        let swap = actor && self.other.is_some();
        if swap {
            self.swap_windows();
        }
        self.step(action, id, fault, ending);
        if swap {
            self.swap_windows();
        }
    }

    /// Whether `RestoreApply` by `actor` on `id` reaches the harness's
    /// `kani::any()` ending choice (the body is present and not stale).
    pub fn restore_choice_needed(&self, actor: bool, id: usize) -> bool {
        let editors = if actor && self.other.is_some() {
            self.other.as_ref().map(|other| other.editors).unwrap()
        } else {
            self.editors
        };
        let running = if actor {
            self.other.map_or(self.running, |other| other.running)
        } else {
            self.running
        };
        running
            && editors[id].restore_pending
            && self.body[id].is_some()
            && self.entry[id].backing == self.backing[id]
    }

    fn contains(&self, holder: ContentId, content: ContentId) -> bool {
        holder != 0 && self.ancestors[usize::from(holder)] & bit(content) != 0
    }

    /// Reconciliation is complete when every body has an entry.
    fn inventory_complete(&self) -> bool {
        self.inventory_complete_with(&self.entry)
    }

    /// Whether every body would have an entry if the manifest read `entries`.
    fn inventory_complete_with(&self, entries: &[DiskEntry; IDS]) -> bool {
        (0..IDS).all(|id| self.body[id].is_none() || entries[id].present)
    }

    /// The production window view.
    fn window_body_facts(&self, id: usize) -> BodyFacts {
        let editor = self.editors[id];
        BodyFacts::from_window_view(editor.known_entry, editor.restore_pending)
    }

    fn preserve_body(&mut self, id: usize) {
        if let Some(content) = self.body[id] {
            self.preserved |= bit(content);
        }
    }

    pub fn step(&mut self, action: Action, id: usize, fault: bool, ending: Option<RestoreEnding>) {
        if !self.running && !matches!(action, Action::Startup) {
            return;
        }
        let editor = self.editors[id];
        match action {
            Action::Edit => {
                if editor.open && self.edits < MAX_EDITS {
                    let content = self.next_content;
                    self.next_content += 1;
                    self.edits += 1;
                    self.ancestors[usize::from(content)] =
                        bit(content) | self.ancestors[usize::from(editor.content)];
                    let editor = &mut self.editors[id];
                    editor.content = content;
                    editor.dirty = true;
                    editor.token = false;
                }
            }
            Action::Register => {
                if !editor.open || !editor.dirty || editor.token || editor.deletion.is_some() {
                    return;
                }
                let needed = registration_required(true, editor.known_entry, self.trusted);
                match body_write_decision(ownership(self.window_body_facts(id)), needed) {
                    BodyWriteDecision::Hold => {}
                    BodyWriteDecision::Write | BodyWriteDecision::PreserveThenWrite => {
                        self.editors[id].token = true;
                    }
                    BodyWriteDecision::RegisterFirst => {
                        if fault {
                            return;
                        }
                        // The service's own ownership check against disk.
                        let superseded =
                            self.entry[id].present && self.entry[id].backing != self.backing[id];
                        let owner = ownership(BodyFacts {
                            body_present: self.body[id].is_some(),
                            entry: if !self.entry[id].present {
                                EntryState::Absent
                            } else if superseded {
                                EntryState::Superseded
                            } else {
                                EntryState::Current
                            },
                            restore_pending: false,
                            backing_stale: false,
                        });
                        if body_write_decision(owner, false) == BodyWriteDecision::PreserveThenWrite
                        {
                            self.preserve_body(id);
                        }
                        let complete = self.inventory_complete();
                        let insert = !self.entry[id].present;
                        let committed = if complete {
                            true
                        } else {
                            untrusted_commit_disposition(true, true)
                                == UntrustedCommitDisposition::Additive
                        };
                        if !committed {
                            return;
                        }
                        if insert {
                            self.entry[id] = DiskEntry {
                                present: true,
                                backing: self.backing[id],
                            };
                        }
                        let completeness = if complete {
                            DraftManifestCompleteness::Complete
                        } else {
                            DraftManifestCompleteness::Partial
                        };
                        self.trusted = commit_authority(completeness, complete).is_trusted();
                        if self.trusted {
                            self.last_reconciliation_complete = true;
                        }
                        let editor = &mut self.editors[id];
                        editor.known_entry = true;
                        editor.token = may_write_after_registration(true, true);
                    }
                }
            }
            Action::WriteBody => {
                if !editor.open || !editor.dirty || !editor.token {
                    return;
                }
                let needed = registration_required(true, editor.known_entry, self.trusted);
                if body_write_decision(ownership(self.window_body_facts(id)), needed)
                    == BodyWriteDecision::Hold
                {
                    return;
                }
                self.editors[id].token = false;
                if fault {
                    return;
                }
                // The token is the proof: no body without an entry.
                if !self.entry[id].present {
                    self.body_without_entry = true;
                }
                self.body[id] = Some(editor.content);
                self.editors[id].written = Some(editor.content);
            }
            Action::Commit => {
                let Some(written) = editor.written else {
                    return;
                };
                self.editors[id].written = None;
                if fault {
                    return;
                }
                let complete = self.inventory_complete();
                if !complete {
                    // A reconciling commit that cannot prove completeness fails.
                    self.trusted = false;
                    return;
                }
                self.entry[id] = DiskEntry {
                    present: true,
                    backing: self.backing[id],
                };
                self.trusted =
                    commit_authority(DraftManifestCompleteness::Complete, true).is_trusted();
                self.last_reconciliation_complete = true;
                if self.body[id] == Some(written) {
                    self.accepted |= bit(written);
                    if editor.content == written {
                        self.editors[id].dirty = false;
                    }
                }
            }
            Action::Save | Action::Discard => {
                if !editor.open || editor.deletion.is_some() {
                    return;
                }
                // The user resolves exactly the work they can see.
                self.resolved |= self.ancestors[usize::from(editor.content)];
                if matches!(action, Action::Save) {
                    self.backing[id] = self.backing[id].wrapping_add(1);
                }
                let start = deletion_start(ownership(self.window_body_facts(id)));
                let editor = &mut self.editors[id];
                editor.dirty = false;
                editor.token = false;
                editor.written = None;
                editor.content = 0;
                editor.known_entry = false;
                editor.preserve_queued = start == DeletionStep::Preserve;
                editor.deletion = Some(start);
            }
            Action::DeletionStep => self.run_deletion_step(id, fault),
            Action::Inspect => {
                if self.trusted && !editor.known_entry && self.body[id].is_some() {
                    self.editors[id].cleanup_candidate = self.body[id];
                }
            }
            Action::ExecCleanup => {
                let Some(inspected) = editor.cleanup_candidate else {
                    return;
                };
                self.editors[id].cleanup_candidate = None;
                let facts = CleanupFacts {
                    manifest_trusted: self.trusted,
                    path_matches: true,
                    entry: if self.entry[id].present {
                        CleanupEntryState::Referenced
                    } else {
                        CleanupEntryState::Unreferenced
                    },
                    // Body writes hold the same guard, so none is in flight.
                    write_guard_held: true,
                    identity: match self.body[id] {
                        None => CleanupBodyIdentity::Missing,
                        Some(content) if content == inspected => CleanupBodyIdentity::Inspected,
                        Some(_) => CleanupBodyIdentity::Changed,
                    },
                };
                if orphan_body_decision(facts) == OrphanBodyDecision::Delete && !fault {
                    // S2: the deleted body is unreferenced and revalidated.
                    if !(!self.entry[id].present && self.body[id] == Some(inspected)) {
                        self.cleanup_unsafe = true;
                    }
                    self.body[id] = None;
                }
            }
            Action::RestoreApply => {
                if !editor.restore_pending {
                    return;
                }
                let ending = match self.body[id] {
                    None => RestoreEnding::MissingBody,
                    Some(_) if self.entry[id].backing != self.backing[id] => RestoreEnding::Stale,
                    Some(_) => {
                        let ending = ending.expect("RestoreApply needs a chosen ending here");
                        assert!(!matches!(
                            ending,
                            RestoreEnding::Stale | RestoreEnding::MissingBody
                        ));
                        ending
                    }
                };
                self.editors[id].restore_pending = false;
                match unapplied_restore_disposition(ending) {
                    RestoreDisposition::Nothing => {
                        if ending == RestoreEnding::Applied
                            && let Some(content) = self.body[id]
                        {
                            let editor = &mut self.editors[id];
                            editor.content = content;
                            editor.dirty = false;
                        }
                    }
                    RestoreDisposition::PreserveCopy => {
                        if !fault {
                            self.preserve_body(id);
                        } else {
                            // The hold stays until the copy exists.
                            self.editors[id].restore_pending = true;
                        }
                    }
                    RestoreDisposition::PreserveThenRetire => {
                        let editor = &mut self.editors[id];
                        editor.preserve_queued = true;
                        editor.known_entry = false;
                        editor.deletion = Some(DeletionStep::Preserve);
                    }
                }
            }
            Action::ExternalMtime => {
                self.backing[id] = self.backing[id].wrapping_add(1);
            }
            Action::Crash => {
                self.running = false;
                self.trusted = false;
                self.editors = [CLOSED; IDS];
            }
            Action::Startup => {
                if !self.running {
                    self.startup(fault);
                }
            }
        }
    }

    fn run_deletion_step(&mut self, id: usize, fault: bool) {
        let Some(step) = self.editors[id].deletion else {
            return;
        };
        let succeeded = !fault;
        match step {
            DeletionStep::Preserve => {
                if succeeded {
                    self.preserve_body(id);
                    self.editors[id].preserve_queued = false;
                }
            }
            DeletionStep::DeleteBody => {
                if succeeded {
                    self.body[id] = None;
                }
            }
            DeletionStep::RemoveEntry => {
                if succeeded {
                    let mut after = self.entry;
                    after[id].present = false;
                    if self.inventory_complete_with(&after) {
                        self.entry = after;
                        self.trusted = commit_authority(DraftManifestCompleteness::Complete, true)
                            .is_trusted();
                        self.last_reconciliation_complete = true;
                    } else {
                        self.trusted = false;
                    }
                }
            }
            DeletionStep::Done | DeletionStep::Stopped => {}
        }
        let next = next_deletion_step(step, succeeded);
        // A stopped deletion keeps its tombstone and retries from the start.
        self.editors[id].deletion = match next {
            DeletionStep::Done => None,
            DeletionStep::Stopped => Some(if self.editors[id].preserve_queued {
                DeletionStep::Preserve
            } else {
                DeletionStep::DeleteBody
            }),
            other => Some(other),
        };
    }

    /// Reconcile, then reopen the session: every id with an entry has a tab.
    pub fn startup(&mut self, fault: bool) {
        self.running = true;
        for id in 0..IDS {
            if self.body[id].is_some() && !self.entry[id].present {
                self.preserve_body(id);
                self.body[id] = None;
            }
        }
        let complete = self.inventory_complete();
        let completeness = if complete {
            DraftManifestCompleteness::Complete
        } else {
            DraftManifestCompleteness::Partial
        };
        self.trusted = commit_authority(completeness, !fault).is_trusted();
        if self.trusted {
            self.last_reconciliation_complete = complete;
        }
        for id in 0..IDS {
            if !self.entry[id].present {
                continue;
            }
            let mut editor = CLOSED;
            editor.open = true;
            editor.known_entry = true;
            if self.body[id].is_some() {
                if self.entry[id].backing != self.backing[id]
                    && startup_may_retire_stale(self.trusted)
                {
                    editor.known_entry = false;
                    editor.preserve_queued = true;
                    editor.deletion = Some(DeletionStep::Preserve);
                } else {
                    editor.restore_pending = true;
                }
            }
            self.editors[id] = editor;
        }
    }

    /// S1: every accepted, unresolved content is recoverable.
    pub fn acceptance_durability_holds(&self) -> bool {
        for content in 1..self.next_content {
            if self.accepted & bit(content) == 0 || self.resolved & bit(content) != 0 {
                continue;
            }
            let on_disk = self.preserved & bit(content) != 0
                || (0..IDS)
                    .any(|id| self.body[id].is_some_and(|body| self.contains(body, content)))
                || (1..self.next_content)
                    .any(|kept| self.preserved & bit(kept) != 0 && self.contains(kept, content));
            let in_editor = (self.running
                && (0..IDS).any(|id| {
                    self.editors[id].open && self.contains(self.editors[id].content, content)
                }))
                || self.other.is_some_and(|other| {
                    other.running
                        && (0..IDS).any(|id| {
                            other.editors[id].open
                                && self.contains(other.editors[id].content, content)
                        })
                });
            if !(on_disk || in_editor) {
                return false;
            }
        }
        true
    }

    /// S3 for the active window.
    fn delete_ordering_holds_active(&self) -> bool {
        (0..IDS).all(|id| {
            self.editors[id].deletion.is_none()
                || !(self.body[id].is_some() && !self.entry[id].present)
        })
    }

    /// S4 for the active window.
    fn trust_holds_active(&self) -> bool {
        !(self.running && self.trusted)
            || (self.last_reconciliation_complete && self.inventory_complete())
    }

    /// `assert_invariants` checks S3 and S4 on the parked window too.
    fn for_both_windows(&self, check: fn(&Self) -> bool) -> bool {
        if !check(self) {
            return false;
        }
        if self.other.is_some() {
            let mut swapped = self.clone();
            swapped.swap_windows();
            return check(&swapped);
        }
        true
    }

    /// S3: delete intent never coexists with a present body and a missing entry.
    pub fn delete_ordering_holds(&self) -> bool {
        self.for_both_windows(Self::delete_ordering_holds_active)
    }

    /// S4: a trusted journal follows a complete reconciliation and holds no
    /// body without an entry.
    pub fn trust_holds(&self) -> bool {
        self.for_both_windows(Self::trust_holds_active)
    }

    /// S1 to S4 and `NoBodyWithoutEntry` together: the conjunction of the
    /// properties `JournalModel` checks one by one.
    pub fn all_invariants_hold(&self) -> bool {
        self.acceptance_durability_holds()
            && !self.cleanup_unsafe
            && self.delete_ordering_holds()
            && self.trust_holds()
            && !self.body_without_entry
    }
}
