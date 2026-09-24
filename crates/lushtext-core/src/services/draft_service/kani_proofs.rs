// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the draft journal core (`journal_core`).
//!
//! Compiled only under `cfg(kani)` (`make kani`). The model below is the
//! journal's **environment** — disk, editors, the window's in-memory state, and
//! crashes — as fixed-size arrays; every **decision** it takes is a call to the
//! production `journal_core` function that the service and the GTK drafts
//! workflow call at the same point. A harness failure is therefore a defect in
//! the decisions production makes, not in a second copy of them.
//!
//! Bounds (the small-scope hypothesis; see the programme record):
//!
//! - `IDS = 3` draft ids, all file-backed (the case that needs registration);
//! - at most `MAX_EDITS = 3` edits, so at most three new content generations;
//! - `STEPS = 8` nondeterministic actions after startup, each of which may be
//!   `Crash` or `Startup`, and each I/O step may fail (a fault).
//!
//! Content is modelled by identity. Content `0` is "nothing to protect" (the
//! buffer equals its file); every edit mints a new content id whose ancestor
//! set records the work it contains, so "a newer body contains an older one"
//! is a fact of the model rather than an assumption.

use super::journal_core::{
    BodyFacts, BodyWriteDecision, CleanupBodyIdentity, CleanupEntryState, CleanupFacts,
    DeletionStep, EntryState, OrphanBodyDecision, RestoreDisposition, RestoreEnding,
    SetAsideNameStep, SetAsideSlot, UntrustedCommitDisposition, body_write_decision,
    commit_authority, deletion_start, may_write_after_registration, next_deletion_step,
    orphan_body_decision, ownership, registration_required, set_aside_name_step,
    startup_may_retire_stale, unapplied_restore_disposition, untrusted_commit_disposition,
};
use crate::model::draft::DraftManifestCompleteness;

/// Draft ids in the model.
const IDS: usize = 3;
/// Content ids: 0 plus one per pre-existing body and one per edit.
const CONTENTS: usize = 1 + IDS + MAX_EDITS;
/// Edits the model admits.
const MAX_EDITS: usize = 3;
/// Nondeterministic actions after the first startup.
const STEPS: usize = 8;

type ContentId = u8;
type ContentSet = u16;

const fn bit(content: ContentId) -> ContentSet {
    1 << content
}

#[derive(Clone, Copy)]
struct DiskEntry {
    present: bool,
    /// Backing-file version the entry's body was written against.
    backing: u8,
}

#[derive(Clone, Copy)]
struct Editor {
    open: bool,
    content: ContentId,
    /// `draft_dirty`: the buffer is ahead of the recovery body.
    dirty: bool,
    /// A body-write token minted by this pass's registration step.
    token: bool,
    /// Content written as a body and awaiting its manifest commit.
    written: Option<ContentId>,
    /// A restore of this id's body is pending: the user has not seen it.
    restore_pending: bool,
    /// Tombstone plus the next step of an in-progress deletion.
    deletion: Option<DeletionStep>,
    /// The deletion must preserve the body first (queued preservation).
    preserve_queued: bool,
    /// The window's manifest copy lists the id.
    known_entry: bool,
    /// An orphan-cleanup inspection saw this body content.
    cleanup_candidate: Option<ContentId>,
}

const CLOSED: Editor = Editor {
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
#[derive(Clone, Copy)]
struct Window {
    editors: [Editor; IDS],
    running: bool,
    trusted: bool,
    last_reconciliation_complete: bool,
}

struct Journal {
    /// A second process over the same data directory, parked while the
    /// active one acts (axiom A6 dropped); `None` models A6.
    other: Option<Window>,
    entry: [DiskEntry; IDS],
    body: [Option<ContentId>; IDS],
    backing: [u8; IDS],
    /// Content ids durably kept in the set-aside area (or local history).
    preserved: ContentSet,
    editors: [Editor; IDS],
    running: bool,
    trusted: bool,
    /// Ghost: the last reconciliation that set `trusted` was complete.
    last_reconciliation_complete: bool,
    /// Ghost: content the user was told is protected (accepted by a commit, or
    /// found on disk at the first startup).
    accepted: ContentSet,
    /// Ghost: content the user resolved (saved to its file, or discarded).
    resolved: ContentSet,
    /// Ancestor sets: `ancestors[c]` holds every content `c` contains.
    ancestors: [ContentSet; CONTENTS],
    next_content: ContentId,
    edits: usize,
}

/// One nondeterministic action of the environment.
#[derive(Clone, Copy, kani::Arbitrary)]
enum Action {
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

impl Journal {
    /// A previous session's disk, chosen nondeterministically: any id may have
    /// an entry, a body, both, or neither, and its file may have moved on.
    fn previous_session() -> Self {
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
        };
        for id in 0..IDS {
            let content = 1 + id as ContentId;
            journal.ancestors[usize::from(content)] = bit(content);
            journal.entry[id] = DiskEntry {
                present: kani::any(),
                backing: 0,
            };
            if kani::any() {
                journal.body[id] = Some(content);
                // Everything a previous session left on disk was promised.
                journal.accepted |= bit(content);
            }
            if kani::any() {
                journal.backing[id] = 1;
            }
        }
        journal
    }

    fn swap_windows(&mut self) {
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
    fn step_as(&mut self, actor: bool, action: Action, id: usize, fault: bool) {
        let swap = actor && self.other.is_some();
        if swap {
            self.swap_windows();
        }
        self.step(action, id, fault);
        if swap {
            self.swap_windows();
        }
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

    /// The production window view, so the model checks the construction the
    /// window uses rather than a copy of it.
    fn window_body_facts(&self, id: usize) -> BodyFacts {
        let editor = self.editors[id];
        BodyFacts::from_window_view(editor.known_entry, editor.restore_pending)
    }

    fn preserve_body(&mut self, id: usize) {
        if let Some(content) = self.body[id] {
            self.preserved |= bit(content);
        }
    }

    fn step(&mut self, action: Action, id: usize, fault: bool) {
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
                assert!(
                    self.entry[id].present,
                    "a body was written without an entry"
                );
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
                    assert!(!self.entry[id].present && self.body[id] == Some(inspected));
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
                        let ending: RestoreEnding = kani::any();
                        kani::assume(!matches!(
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
                            // The hold stays until the copy exists; production
                            // retries the copy on each autosave tick.
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
                    // A reconciling commit: the removal lands only with a
                    // complete inventory, and then the journal is trusted.
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
    fn startup(&mut self, fault: bool) {
        self.running = true;
        // Reconciliation sets unattributable bodies aside (moves them), so the
        // inventory it publishes is complete.
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

    /// S1: every accepted, unresolved content is recoverable — from a body on
    /// disk, a preserved copy, or (while running) an open editor's buffer.
    fn assert_acceptance_durability(&self) {
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
            assert!(
                on_disk || in_editor,
                "S1: accepted work became unrecoverable"
            );
        }
    }

    /// S3: delete intent never coexists with a present body and a missing entry.
    fn assert_delete_ordering(&self) {
        for id in 0..IDS {
            if self.editors[id].deletion.is_some() {
                assert!(
                    !(self.body[id].is_some() && !self.entry[id].present),
                    "S3: a deleting id has a body but no entry"
                );
            }
        }
    }

    /// S4: a trusted journal follows a complete reconciliation and holds no
    /// body without an entry.
    fn assert_trust(&self) {
        if self.running && self.trusted {
            assert!(
                self.last_reconciliation_complete,
                "S4: trust without a complete reconciliation"
            );
            assert!(
                self.inventory_complete(),
                "S4: a trusted journal has an unexplained body"
            );
        }
    }

    fn assert_invariants(&mut self) {
        self.assert_acceptance_durability();
        self.assert_delete_ordering();
        self.assert_trust();
        if self.other.is_some() {
            self.swap_windows();
            self.assert_delete_ordering();
            self.assert_trust();
            self.swap_windows();
        }
    }
}

fn any_id() -> usize {
    let id: usize = kani::any();
    kani::assume(id < IDS);
    id
}

/// S1–S4 over every sequence of `STEPS` actions, faults, crashes, and
/// restarts after the first startup.
#[kani::proof]
#[kani::unwind(9)]
fn journal_invariants_hold_under_crashes() {
    let mut journal = Journal::previous_session();
    journal.step(Action::Startup, 0, kani::any());
    journal.assert_invariants();
    for _ in 0..STEPS {
        journal.step(kani::any(), any_id(), kani::any());
        journal.assert_invariants();
    }
}

/// Fault-free steps that bound L1: the smallest `k` that covers the longest
/// fault-free path to clean — a pending restore that turns out stale (1), its
/// preserve / delete-body / remove-entry retirement (3), then register, write,
/// and commit (3). `a_dirty_editor_may_need_seven_steps` shows six is too few.
const LIVENESS_STEPS: usize = 7;

/// Whether, after any three-action prefix (faults allowed) and then only
/// fault-free steps of the production pass order, a dirty open editor is clean
/// within `steps` steps.
fn dirty_editor_becomes_clean_within(steps: usize) -> bool {
    let mut journal = Journal::previous_session();
    journal.step(Action::Startup, 0, false);
    for _ in 0..3 {
        journal.step(kani::any(), any_id(), kani::any());
    }
    let id = any_id();
    kani::assume(journal.running && journal.editors[id].open && journal.editors[id].dirty);
    for _ in 0..steps {
        let editor = journal.editors[id];
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
        journal.step(action, id, false);
    }
    !journal.editors[id].dirty
}

/// L1: without I/O faults, a dirty open editor becomes clean within
/// `LIVENESS_STEPS` steps of the autosave pass, after any prefix of actions.
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

/// Actions after both startups in the K8 harness. Six still reach a
/// counterexample (a body written without an entry: one process registers and
/// writes while the other discards and deletes the same id); eight steps, which
/// also reach the S1 loss recorded in the programme record, took 17 minutes and
/// about 18 GB — too much for a CI runner, and more than a `should_panic`
/// harness needs.
const SECOND_WRITER_STEPS: usize = 6;

/// K8: drop axiom A6 (one process per data directory). A second LushText
/// process runs its own editors and in-memory journal over the same disk.
/// Kani reports which of S1–S4 fail; the programme record keeps the
/// counterexamples and the decision. Kept as `should_panic`: the day this
/// passes, A6 is no longer needed and the record must change.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(9)]
fn a_second_writer_breaks_the_journal_invariants() {
    let mut journal = Journal::previous_session();
    journal.other = Some(Window {
        editors: [CLOSED; IDS],
        running: false,
        trusted: false,
        last_reconciliation_complete: false,
    });
    journal.step_as(false, Action::Startup, 0, false);
    journal.step_as(true, Action::Startup, 0, false);
    journal.assert_invariants();
    for _ in 0..SECOND_WRITER_STEPS {
        journal.step_as(kani::any(), kani::any(), any_id(), kani::any());
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
