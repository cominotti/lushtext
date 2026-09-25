// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the draft journal core (`journal_core`).
//!
//! Compiled only under `cfg(kani)` (`make kani`). The model below is the
//! journal's **environment** — disk, editors, each window's in-memory state,
//! the per-application journal coordinator, and crashes — as fixed-size
//! arrays; every **decision** it takes is a call to the production
//! `journal_core` function that the service and the GTK drafts workflow call at
//! the same point. A harness failure is therefore a defect in the decisions
//! production makes, not in a second copy of them.
//!
//! ## Actors
//!
//! Two kinds of actor, named so that each harness states what it shares:
//!
//! - a **window** ([`Window`]) holds exactly the per-window production state:
//!   its editors, its manifest copy and authority, its restore holds (the
//!   editors' `restore_pending`), its tombstones (the editors' `deletion`), its
//!   in-flight journal work (`lane`, in the baseline scope), and its cleanup
//!   candidates;
//! - a **process** ([`Process`]) holds one or two windows plus what production
//!   makes process-wide: the manifest write lock and the stable target guard
//!   (every model action runs under both, so they need no state), and the
//!   draft journal coordinator (`ui/window/drafts/admission.rs`) — the one
//!   journal lane, whether the process already restored its session, and
//!   which window schedules orphan cleanup.
//!
//! Processes share only the disk. `Crash` and `Startup` act on a whole
//! process; every other action is one window's. Each step swaps the acting
//! process and window into slot 0 and back, so no access inside a step has a
//! symbolic array index: indexing by a nondeterministic process or window made
//! the two-process harness three times larger.
//!
//! ## Bounds (the small-scope hypothesis; see the programme record)
//!
//! Each harness states its own: the number of draft ids (one of which may be
//! untitled, the kind that needs no registration), at most [`MAX_EDITS`] edits
//! (so at most three new content generations), and a number of
//! nondeterministic actions after startup, each of which may be `Crash` or
//! `Startup`, and each I/O step of which may fail (a fault).
//!
//! Content is modelled by identity. Content `0` is "nothing to protect" (the
//! buffer equals its file); every edit mints a new content id whose ancestor
//! set records the work it contains, so "a newer body contains an older one"
//! is a fact of the model rather than an assumption.

use super::journal_core::{
    BodyFacts, BodyWriteDecision, CleanupBodyIdentity, CleanupEntryState, CleanupFacts,
    DeletionStep, DraftClaim, DraftOpenDecision, EntryState, JournalLaneAdmission,
    JournalLaneHolder, OrphanBodyDecision, RestoreDisposition, RestoreEnding, SetAsideNameStep,
    SetAsideSlot, StartupRestore, UntrustedCommitDisposition, body_write_decision,
    commit_authority, deletion_start, draft_open_decision, journal_lane_admission,
    may_write_after_registration, next_deletion_step, orphan_body_decision, ownership,
    registration_required, set_aside_name_step, startup_may_retire_stale, startup_restore,
    unapplied_restore_disposition, untrusted_commit_disposition,
};
use crate::model::draft::DraftManifestCompleteness;

/// The most draft ids any harness uses.
const MAX_IDS: usize = 3;
/// Edits the model admits.
const MAX_EDITS: usize = 3;
/// Content ids: 0 plus one per pre-existing body and one per edit.
const CONTENTS: usize = 1 + MAX_IDS + MAX_EDITS;

type ContentId = u8;
type ContentSet = u16;

const fn bit(content: ContentId) -> ContentSet {
    1 << content
}

/// What a harness models, beyond its actor counts. A trait of associated
/// constants rather than a value, so every scope branch folds at compile time
/// and costs the solver nothing.
trait Scope {
    /// Design D2 of `verify-multi-window-draft-journal`: the journal lane that
    /// keeps orphan cleanup out of a registration → write → commit pass and
    /// out of a deletion, missing-body entry removal in cleanup, a window's
    /// manifest copy refreshed from every commit it accepts, the reconciling
    /// commit rebuilding an untitled body's entry, and the window actions
    /// (`Open`, `CloseWindow`, `OpenWindow`). `false` is the K3 model as it
    /// was before that change, kept for the K8 pin.
    const EXTENDED: bool;
    /// The draft id that is untitled, if any.
    const UNTITLED: Option<usize>;
    /// The journal coordinator of `verify-multi-window-draft-journal`: one
    /// journal lane per process, cleanup inspecting the persisted manifest in
    /// the one window that schedules it, every commit mirrored into every
    /// window's manifest copy, once-per-process startup restore, and an open
    /// of a file another window has open redirected to that window. `false`
    /// is the baseline the two-window harness first ran against, where every
    /// one of those was per window.
    const PROCESS_JOURNAL: bool;
}

#[derive(Clone, Copy)]
struct DiskEntry {
    present: bool,
    /// Backing-file version the entry's body was written against.
    backing: u8,
    /// Changes on every write of the entry: the fingerprint cleanup compares.
    stamp: u8,
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
};

/// What holds a journal lane.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LaneUse {
    /// A registration → body write → commit pass for one id.
    Pass(u8),
    /// A serialized deletion of one id.
    Deletion(u8),
    /// An orphan-cleanup inspect/execute worker.
    Cleanup,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct LaneHolder {
    /// The holder's stable window identity.
    window: u8,
    lane_use: LaneUse,
}

/// One window's in-memory journal state.
#[derive(Clone, Copy)]
struct Window<const IDS: usize> {
    /// Stable identity inside its process (swaps move it with the window).
    id: u8,
    running: bool,
    editors: [Editor; IDS],
    /// The window's manifest copy: the stamp of the entry it lists per id.
    known: [Option<u8>; IDS],
    trusted: bool,
    /// Ghost: the last reconciliation that set `trusted` was complete.
    last_reconciliation_complete: bool,
    /// The window's own lane (the baseline scope).
    lane: Option<LaneUse>,
    /// Orphan bodies an inspection nominated, by the content it saw.
    orphan_candidate: [Option<ContentId>; IDS],
    /// Missing-body entries an inspection nominated, by fingerprint.
    missing_candidate: [Option<u8>; IDS],
}

impl<const IDS: usize> Window<IDS> {
    const fn stopped(id: u8) -> Self {
        Self {
            id,
            running: false,
            editors: [CLOSED; IDS],
            known: [None; IDS],
            trusted: false,
            last_reconciliation_complete: false,
            lane: None,
            orphan_candidate: [None; IDS],
            missing_candidate: [None; IDS],
        }
    }

    fn reset(&mut self) {
        *self = Self::stopped(self.id);
    }
}

/// One LushText process: its windows and its journal coordinator.
#[derive(Clone, Copy)]
struct Process<const IDS: usize, const W: usize> {
    windows: [Window<IDS>; W],
    /// The process-wide lane (the process-journal scope).
    lane: Option<LaneHolder>,
    /// The process already ran its startup restore (the process-journal scope).
    restored: bool,
    /// The window that schedules orphan cleanup (the process-journal scope).
    cleanup_owner: Option<u8>,
}

impl<const IDS: usize, const W: usize> Process<IDS, W> {
    fn stopped() -> Self {
        let mut windows = [Window::stopped(0); W];
        for (index, window) in windows.iter_mut().enumerate() {
            window.id = index as u8;
        }
        Self {
            windows,
            lane: None,
            restored: false,
            cleanup_owner: None,
        }
    }

    fn reset(&mut self) {
        for window in &mut self.windows {
            window.reset();
        }
        self.lane = None;
        self.restored = false;
        self.cleanup_owner = None;
    }

    fn running(&self) -> bool {
        let mut running = false;
        for index in 0..W {
            running |= self.windows[index].running;
        }
        running
    }
}

struct Journal<S: Scope, const IDS: usize, const W: usize, const P: usize> {
    scope: core::marker::PhantomData<S>,
    processes: [Process<IDS, W>; P],
    entry: [DiskEntry; IDS],
    body: [Option<ContentId>; IDS],
    backing: [u8; IDS],
    /// Content ids durably kept in the set-aside area (or local history).
    preserved: ContentSet,
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
    Open,
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
    CloseWindow,
    OpenWindow,
    Crash,
    Startup,
}

impl<S: Scope, const IDS: usize, const W: usize, const P: usize> Journal<S, IDS, W, P> {
    /// A previous session's disk, chosen nondeterministically: any id may have
    /// an entry, a body, both, or neither, and its file may have moved on.
    fn previous_session() -> Self {
        let mut journal = Self {
            scope: core::marker::PhantomData,
            processes: [Process::stopped(); P],
            entry: [DiskEntry {
                present: false,
                backing: 0,
                stamp: 0,
            }; IDS],
            body: [None; IDS],
            backing: [0; IDS],
            preserved: 0,
            accepted: 0,
            resolved: 0,
            ancestors: [0; CONTENTS],
            next_content: 1 + IDS as ContentId,
            edits: 0,
        };
        for id in 0..IDS {
            let content = 1 + id as ContentId;
            journal.ancestors[usize::from(content)] = bit(content);
            journal.entry[id].present = kani::any();
            if kani::any() {
                journal.body[id] = Some(content);
                // Everything a previous session left on disk was promised.
                journal.accepted |= bit(content);
            }
            if !journal.untitled(id) && kani::any() {
                journal.backing[id] = 1;
            }
        }
        journal
    }

    fn untitled(&self, id: usize) -> bool {
        S::UNTITLED == Some(id)
    }

    /// The acting window (slot 0 of the acting process, which is slot 0).
    fn me(&self) -> &Window<IDS> {
        &self.processes[0].windows[0]
    }

    fn me_mut(&mut self) -> &mut Window<IDS> {
        &mut self.processes[0].windows[0]
    }

    fn contains(&self, holder: ContentId, content: ContentId) -> bool {
        holder != 0 && self.ancestors[usize::from(holder)] & bit(content) != 0
    }

    /// Reconciliation is complete when every file-backed body has an entry.
    /// An untitled body needs none: reconciliation rebuilds its entry.
    fn inventory_complete(&self) -> bool {
        self.inventory_complete_with(&self.entry)
    }

    /// Whether every file-backed body would have an entry if the manifest read
    /// `entries`.
    fn inventory_complete_with(&self, entries: &[DiskEntry; IDS]) -> bool {
        let mut complete = true;
        for id in 0..IDS {
            if !self.untitled(id) && self.body[id].is_some() && !entries[id].present {
                complete = false;
            }
        }
        complete
    }

    /// Whether the backing file changed since the entry's body was written.
    fn stale(&self, id: usize) -> bool {
        !self.untitled(id) && self.entry[id].backing != self.backing[id]
    }

    /// The production window view, so the model checks the construction the
    /// window uses rather than a copy of it.
    fn window_body_facts(&self, id: usize) -> BodyFacts {
        let me = self.me();
        BodyFacts::from_window_view(me.known[id].is_some(), me.editors[id].restore_pending)
    }

    fn preserve_body(&mut self, id: usize) {
        if let Some(content) = self.body[id] {
            self.preserved |= bit(content);
        }
    }

    fn write_entry(&mut self, id: usize) {
        self.entry[id] = DiskEntry {
            present: true,
            backing: self.backing[id],
            stamp: self.entry[id].stamp.wrapping_add(1),
        };
    }

    // --- the journal lane --------------------------------------------------

    /// What holds the lane the acting window uses, as it sees it.
    fn lane_holder(&self) -> Option<LaneHolder> {
        if !S::EXTENDED {
            return None;
        }
        if S::PROCESS_JOURNAL {
            self.processes[0].lane
        } else {
            self.me().lane.map(|lane_use| LaneHolder {
                window: self.me().id,
                lane_use,
            })
        }
    }

    fn set_lane(&mut self, lane_use: Option<LaneUse>) {
        if !S::EXTENDED {
            return;
        }
        if S::PROCESS_JOURNAL {
            let window = self.me().id;
            self.processes[0].lane = lane_use.map(|lane_use| LaneHolder { window, lane_use });
        } else {
            self.me_mut().lane = lane_use;
        }
    }

    /// Whether the acting window may run `lane_use` now: it already holds the
    /// lane for exactly that work, or `journal_lane_admission` admits it.
    fn lane_admits(&self, lane_use: LaneUse) -> bool {
        let holder = match self.lane_holder() {
            None => JournalLaneHolder::Nobody,
            Some(holder) if holder.window == self.me().id => {
                if holder.lane_use == lane_use {
                    return true;
                }
                JournalLaneHolder::ThisWindow
            }
            Some(_) => JournalLaneHolder::OtherWindow,
        };
        journal_lane_admission(holder) == JournalLaneAdmission::Admit
    }

    /// Release the lane when the acting window holds it for `lane_use`.
    fn release_lane(&mut self, lane_use: LaneUse) {
        if self.lane_holder()
            == Some(LaneHolder {
                window: self.me().id,
                lane_use,
            })
        {
            self.set_lane(None);
        }
    }

    // --- manifest copies ------------------------------------------------------

    /// The acting window adopts the persisted manifest, minus the ids it is
    /// deleting (its tombstones): `accept_draft_manifest_commit`, and in the
    /// process-journal scope every other window of the process too
    /// (`adopt_peer_draft_manifest`), with the same authority.
    fn accept_commit(&mut self, trusted: bool) {
        let entry = self.entry;
        let mirror = S::PROCESS_JOURNAL;
        for index in 0..W {
            let window = &mut self.processes[0].windows[index];
            if !window.running || (index > 0 && !mirror) {
                continue;
            }
            for id in 0..IDS {
                window.known[id] = if window.editors[id].deletion.is_some() || !entry[id].present {
                    None
                } else {
                    Some(entry[id].stamp)
                };
            }
            window.trusted = trusted;
            if trusted {
                window.last_reconciliation_complete = true;
            }
        }
    }

    /// A failed commit revokes the acting window's authority, and in the
    /// process-journal scope every window's (`reject_draft_manifest_authority`).
    fn revoke_trust(&mut self) {
        let mirror = S::PROCESS_JOURNAL;
        for index in 0..W {
            if index == 0 || mirror {
                self.processes[0].windows[index].trusted = false;
            }
        }
    }

    // --- one step -------------------------------------------------------------

    /// One action by window `w` of process `p` (`Crash` and `Startup` act on
    /// the whole process). At most two processes and two windows.
    fn step(&mut self, p: usize, w: usize, action: Action, id: usize, fault: bool) {
        let swap_process = P > 1 && p == 1;
        let swap_window = W > 1 && w == 1;
        if swap_process {
            self.processes.swap(0, 1);
        }
        if swap_window {
            self.processes[0].windows.swap(0, 1);
        }
        self.act(action, id, fault);
        if swap_window {
            self.processes[0].windows.swap(0, 1);
        }
        if swap_process {
            self.processes.swap(0, 1);
        }
    }

    fn act(&mut self, action: Action, id: usize, fault: bool) {
        match action {
            Action::Crash => {
                self.processes[0].reset();
                return;
            }
            Action::Startup => {
                if !self.processes[0].running() {
                    self.startup(fault);
                    let owner = self.me().id;
                    let process = &mut self.processes[0];
                    process.restored = true;
                    process.cleanup_owner = Some(owner);
                }
                return;
            }
            Action::OpenWindow => {
                if S::EXTENDED && self.processes[0].running() && !self.me().running {
                    self.open_window(fault);
                }
                return;
            }
            _ => {}
        }
        if !self.me().running {
            return;
        }
        let editor = self.me().editors[id];
        match action {
            Action::Edit => {
                if editor.open && self.edits < MAX_EDITS {
                    let content = self.next_content;
                    self.next_content += 1;
                    self.edits += 1;
                    self.ancestors[usize::from(content)] =
                        bit(content) | self.ancestors[usize::from(editor.content)];
                    let editor = &mut self.me_mut().editors[id];
                    editor.content = content;
                    editor.dirty = true;
                    editor.token = false;
                    // The pass's captured snapshot is no longer current.
                    if editor.written.is_none() {
                        self.release_lane(LaneUse::Pass(id as u8));
                    }
                }
            }
            Action::Open => self.open_editor(id),
            Action::Register => self.register(id, fault),
            Action::WriteBody => {
                if !editor.open || !editor.dirty || !editor.token {
                    return;
                }
                let needed = registration_required(
                    !self.untitled(id),
                    self.me().known[id].is_some(),
                    self.me().trusted,
                );
                self.me_mut().editors[id].token = false;
                if body_write_decision(ownership(self.window_body_facts(id)), needed)
                    == BodyWriteDecision::Hold
                    || fault
                {
                    self.release_lane(LaneUse::Pass(id as u8));
                    return;
                }
                // The token is the proof: no file-backed body without an entry.
                assert!(
                    self.untitled(id) || self.entry[id].present,
                    "a body was written without an entry"
                );
                self.body[id] = Some(editor.content);
                self.me_mut().editors[id].written = Some(editor.content);
            }
            Action::Commit => self.commit(id, fault),
            Action::Save | Action::Discard => {
                if !editor.open || editor.deletion.is_some() {
                    return;
                }
                // The user resolves exactly the work they can see.
                self.resolved |= self.ancestors[usize::from(editor.content)];
                if matches!(action, Action::Save) && !self.untitled(id) {
                    self.backing[id] = self.backing[id].wrapping_add(1);
                }
                let start = deletion_start(ownership(self.window_body_facts(id)));
                self.release_lane(LaneUse::Pass(id as u8));
                let me = self.me_mut();
                me.known[id] = None;
                let editor = &mut me.editors[id];
                editor.dirty = false;
                editor.token = false;
                editor.written = None;
                editor.content = 0;
                editor.preserve_queued = start == DeletionStep::Preserve;
                editor.deletion = Some(start);
            }
            Action::DeletionStep => self.run_deletion_step(id, fault),
            Action::Inspect => self.inspect(id),
            Action::ExecCleanup => self.exec_cleanup(id, fault),
            Action::RestoreApply => self.restore_apply(id, fault),
            Action::ExternalMtime => {
                if !self.untitled(id) {
                    self.backing[id] = self.backing[id].wrapping_add(1);
                }
            }
            Action::CloseWindow => self.close_window(),
            Action::Crash | Action::Startup | Action::OpenWindow => {}
        }
    }

    /// `open_document` for a file (or a new untitled tab): the window restores
    /// a draft its manifest copy lists.
    fn open_editor(&mut self, id: usize) {
        if !S::EXTENDED || self.me().editors[id].open {
            return;
        }
        let mut open_elsewhere = false;
        for window in 1..W {
            let window = &self.processes[0].windows[window];
            open_elsewhere |= window.running && window.editors[id].open;
        }
        if self.untitled(id) {
            // A new untitled tab gets a fresh id: only one nobody holds.
            let mut open_in_other_process = false;
            for process in 1..P {
                for window in 0..W {
                    open_in_other_process |=
                        self.processes[process].windows[window].editors[id].open;
                }
            }
            let in_use = self.entry[id].present
                || self.body[id].is_some()
                || open_elsewhere
                || open_in_other_process;
            if in_use {
                return;
            }
        }
        if S::PROCESS_JOURNAL {
            // `documents.rs`: an open of a file another window has open
            // presents that window's tab instead.
            let claim = if open_elsewhere {
                DraftClaim::OtherWindow
            } else {
                DraftClaim::Unclaimed
            };
            if draft_open_decision(claim) == DraftOpenDecision::PresentOwner {
                return;
            }
        }
        let restore = self.me().known[id].is_some();
        let editor = &mut self.me_mut().editors[id];
        *editor = CLOSED;
        editor.open = true;
        editor.restore_pending = restore;
    }

    fn register(&mut self, id: usize, fault: bool) {
        let editor = self.me().editors[id];
        if !editor.open || !editor.dirty || editor.token || editor.deletion.is_some() {
            return;
        }
        if !self.lane_admits(LaneUse::Pass(id as u8)) {
            // The existing mark-pending admission: a later tick retries.
            return;
        }
        let trusted = self.me().trusted;
        let needed =
            registration_required(!self.untitled(id), self.me().known[id].is_some(), trusted);
        match body_write_decision(ownership(self.window_body_facts(id)), needed) {
            BodyWriteDecision::Hold => {}
            BodyWriteDecision::Write | BodyWriteDecision::PreserveThenWrite => {
                self.set_lane(Some(LaneUse::Pass(id as u8)));
                self.me_mut().editors[id].token = true;
            }
            BodyWriteDecision::RegisterFirst => {
                if fault {
                    return;
                }
                self.set_lane(Some(LaneUse::Pass(id as u8)));
                // The service's own ownership check against disk.
                let superseded = self.entry[id].present && self.stale(id);
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
                if body_write_decision(owner, false) == BodyWriteDecision::PreserveThenWrite {
                    self.preserve_body(id);
                }
                let complete = self.inventory_complete();
                let insert = !self.entry[id].present;
                let committed = complete
                    || untrusted_commit_disposition(true, true)
                        == UntrustedCommitDisposition::Additive;
                if !committed {
                    self.release_lane(LaneUse::Pass(id as u8));
                    return;
                }
                if insert {
                    self.write_entry(id);
                }
                let completeness = if complete {
                    DraftManifestCompleteness::Complete
                } else {
                    DraftManifestCompleteness::Partial
                };
                let now_trusted = commit_authority(completeness, complete).is_trusted();
                if now_trusted && S::EXTENDED {
                    self.accept_commit(true);
                } else {
                    // The additive path mirrors only the inserted entry, and
                    // leaves the journal untrusted.
                    let stamp = self.entry[id].stamp;
                    let mirror = S::PROCESS_JOURNAL;
                    for index in 0..W {
                        let window = &mut self.processes[0].windows[index];
                        if (index == 0 || mirror) && window.known[id].is_none() {
                            window.known[id] = Some(stamp);
                        }
                    }
                    if now_trusted {
                        let me = self.me_mut();
                        me.trusted = true;
                        me.last_reconciliation_complete = true;
                    } else {
                        self.revoke_trust();
                    }
                }
                self.me_mut().editors[id].token = may_write_after_registration(true, true);
            }
        }
    }

    fn commit(&mut self, id: usize, fault: bool) {
        let editor = self.me().editors[id];
        let Some(written) = editor.written else {
            return;
        };
        self.me_mut().editors[id].written = None;
        self.release_lane(LaneUse::Pass(id as u8));
        if fault {
            return;
        }
        if S::EXTENDED {
            // A reconciling commit rebuilds the entry of an untitled body
            // that has none.
            for other in 0..IDS {
                if self.untitled(other) && self.body[other].is_some() && !self.entry[other].present
                {
                    self.write_entry(other);
                }
            }
        }
        if !self.inventory_complete() {
            // A reconciling commit that cannot prove completeness fails.
            self.revoke_trust();
            return;
        }
        self.write_entry(id);
        let trusted = commit_authority(DraftManifestCompleteness::Complete, true).is_trusted();
        if S::EXTENDED {
            self.accept_commit(trusted);
        } else {
            let stamp = self.entry[id].stamp;
            let me = self.me_mut();
            me.known[id] = Some(stamp);
            me.trusted = trusted;
            me.last_reconciliation_complete = true;
        }
        // Production accepts the generation its commit carried; the K3 model
        // also required the body to still be the one written, which only the
        // lane guarantees.
        if S::EXTENDED || self.body[id] == Some(written) {
            self.accepted |= bit(written);
            if editor.content == written {
                self.me_mut().editors[id].dirty = false;
            }
        }
    }

    fn run_deletion_step(&mut self, id: usize, fault: bool) {
        let Some(step) = self.me().editors[id].deletion else {
            return;
        };
        if !self.lane_admits(LaneUse::Deletion(id as u8)) {
            return;
        }
        self.set_lane(Some(LaneUse::Deletion(id as u8)));
        let succeeded = !fault;
        match step {
            DeletionStep::Preserve => {
                if succeeded {
                    self.preserve_body(id);
                    self.me_mut().editors[id].preserve_queued = false;
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
                        let trusted = commit_authority(DraftManifestCompleteness::Complete, true)
                            .is_trusted();
                        if S::EXTENDED {
                            self.accept_commit(trusted);
                        } else {
                            let me = self.me_mut();
                            me.trusted = trusted;
                            me.last_reconciliation_complete = true;
                        }
                    } else {
                        self.revoke_trust();
                    }
                }
            }
            DeletionStep::Done | DeletionStep::Stopped => {}
        }
        let next = next_deletion_step(step, succeeded);
        let editor = &mut self.me_mut().editors[id];
        // A stopped deletion keeps its tombstone and retries from the start.
        editor.deletion = match next {
            DeletionStep::Done => None,
            DeletionStep::Stopped => Some(if editor.preserve_queued {
                DeletionStep::Preserve
            } else {
                DeletionStep::DeleteBody
            }),
            other => Some(other),
        };
        if matches!(next, DeletionStep::Done | DeletionStep::Stopped) {
            self.release_lane(LaneUse::Deletion(id as u8));
        }
    }

    /// `inspect_orphan_cleanup_from`: orphan bodies and missing-body entries.
    /// The baseline inspects the window's manifest copy; the process journal
    /// inspects the persisted manifest (`inspect_orphan_cleanup_against_persisted`)
    /// and only in the window that schedules cleanup for the process.
    fn inspect(&mut self, id: usize) {
        if !self.me().trusted {
            return;
        }
        if !S::EXTENDED {
            if self.me().known[id].is_none() && self.body[id].is_some() {
                self.me_mut().orphan_candidate[id] = self.body[id];
            }
            return;
        }
        if S::PROCESS_JOURNAL {
            let me = self.me().id;
            let process = &mut self.processes[0];
            match process.cleanup_owner {
                Some(owner) if owner != me => return,
                Some(_) => {}
                None => process.cleanup_owner = Some(me),
            }
        }
        if !self.lane_admits(LaneUse::Cleanup) {
            return;
        }
        self.set_lane(Some(LaneUse::Cleanup));
        let persisted = S::PROCESS_JOURNAL;
        let body = self.body;
        let entry = self.entry;
        let me = self.me_mut();
        for id in 0..IDS {
            let listed = if persisted {
                entry[id].present.then_some(entry[id].stamp)
            } else {
                me.known[id]
            };
            me.orphan_candidate[id] = None;
            me.missing_candidate[id] = None;
            match (listed, body[id]) {
                (None, Some(content)) => me.orphan_candidate[id] = Some(content),
                (Some(stamp), None) => me.missing_candidate[id] = Some(stamp),
                _ => {}
            }
        }
    }

    /// `execute_orphan_cleanup`: revalidate against the latest persisted
    /// manifest under the lock and the target guard, then delete.
    fn exec_cleanup(&mut self, id: usize, fault: bool) {
        if !S::EXTENDED {
            self.exec_cleanup_one(id, fault);
            return;
        }
        if self.lane_holder()
            != Some(LaneHolder {
                window: self.me().id,
                lane_use: LaneUse::Cleanup,
            })
        {
            return;
        }
        for id in 0..IDS {
            self.exec_cleanup_one(id, false);
        }
        // Missing-body entries go in one manifest write, which may fail.
        if !fault {
            let mirror = S::PROCESS_JOURNAL;
            for id in 0..IDS {
                if let Some(stamp) = self.me().missing_candidate[id]
                    && self.entry[id].present
                    && self.entry[id].stamp == stamp
                    && self.body[id].is_none()
                {
                    self.entry[id].present = false;
                    // `merge_committed_orphan_removals`, and in the process
                    // journal `mirror_orphan_removals_to_peers`.
                    for index in 0..W {
                        if index == 0 || mirror {
                            self.processes[0].windows[index].known[id] = None;
                        }
                    }
                }
            }
        }
        self.me_mut().missing_candidate = [None; IDS];
        self.release_lane(LaneUse::Cleanup);
    }

    fn exec_cleanup_one(&mut self, id: usize, fault: bool) {
        let Some(inspected) = self.me().orphan_candidate[id] else {
            return;
        };
        self.me_mut().orphan_candidate[id] = None;
        let facts = CleanupFacts {
            manifest_trusted: self.me().trusted,
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

    fn restore_apply(&mut self, id: usize, fault: bool) {
        if !self.me().editors[id].restore_pending {
            return;
        }
        let ending = match self.body[id] {
            None => RestoreEnding::MissingBody,
            Some(_) if self.stale(id) => RestoreEnding::Stale,
            Some(_) => {
                let ending: RestoreEnding = kani::any();
                kani::assume(!matches!(
                    ending,
                    RestoreEnding::Stale | RestoreEnding::MissingBody
                ));
                ending
            }
        };
        self.me_mut().editors[id].restore_pending = false;
        match unapplied_restore_disposition(ending) {
            RestoreDisposition::Nothing => {
                if ending == RestoreEnding::Applied
                    && let Some(content) = self.body[id]
                {
                    let editor = &mut self.me_mut().editors[id];
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
                    self.me_mut().editors[id].restore_pending = true;
                }
            }
            RestoreDisposition::PreserveThenRetire => {
                let me = self.me_mut();
                me.known[id] = None;
                let editor = &mut me.editors[id];
                editor.preserve_queued = true;
                editor.deletion = Some(DeletionStep::Preserve);
            }
        }
    }

    /// The window's close, after its close flush drained: nothing dirty,
    /// pending, or in flight is left in it (the flush drives the same
    /// register / write / commit and deletion actions the model already has,
    /// and a close with a pending restore stays retryable instead).
    fn close_window(&mut self) {
        if !S::EXTENDED {
            return;
        }
        let me = *self.me();
        let mut busy = false;
        for id in 0..IDS {
            let editor = me.editors[id];
            busy |= editor.dirty
                || editor.token
                || editor.written.is_some()
                || editor.restore_pending
                || editor.deletion.is_some();
        }
        let holds_lane = self
            .lane_holder()
            .is_some_and(|holder| holder.window == me.id);
        if busy || holds_lane {
            return;
        }
        let process = &mut self.processes[0];
        process.windows[0].reset();
        if process.cleanup_owner == Some(me.id) {
            process.cleanup_owner = None;
        }
        if !process.running() {
            // The last window closed: the application exits.
            process.reset();
        }
    }

    /// A second window of a running process (`LushtextWindow::new` while
    /// another window exists). The baseline runs its own startup restore; the
    /// process journal starts it empty with the process's manifest copy
    /// (`adopt_peer_draft_records`).
    fn open_window(&mut self, fault: bool) {
        let restored = self.processes[0].restored;
        if !S::PROCESS_JOURNAL || startup_restore(restored) == StartupRestore::RestoreSession {
            self.startup(fault);
            return;
        }
        // With two windows the peer is the other slot.
        let peer = self.processes[0].windows[W - 1];
        if W < 2 || !peer.running {
            return;
        }
        let me = self.me_mut();
        me.running = true;
        me.known = peer.known;
        me.trusted = peer.trusted;
        me.last_reconciliation_complete = peer.last_reconciliation_complete;
    }

    /// Reconcile, then reopen the session: every id with an entry has a tab.
    fn startup(&mut self, fault: bool) {
        self.me_mut().running = true;
        // Reconciliation sets unattributable file-backed bodies aside (moves
        // them) and rebuilds an untitled body's entry, so the inventory it
        // publishes is complete.
        for id in 0..IDS {
            if self.body[id].is_some() && !self.entry[id].present {
                if self.untitled(id) {
                    self.write_entry(id);
                } else {
                    self.preserve_body(id);
                    self.body[id] = None;
                }
            }
        }
        let complete = self.inventory_complete();
        let completeness = if complete {
            DraftManifestCompleteness::Complete
        } else {
            DraftManifestCompleteness::Partial
        };
        let trusted = commit_authority(completeness, !fault).is_trusted();
        {
            let me = self.me_mut();
            me.trusted = trusted;
            if trusted {
                me.last_reconciliation_complete = complete;
            }
        }
        for id in 0..IDS {
            if !self.entry[id].present {
                continue;
            }
            let mut editor = CLOSED;
            editor.open = true;
            let mut known = Some(self.entry[id].stamp);
            if self.body[id].is_some() {
                if self.stale(id) && startup_may_retire_stale(trusted) {
                    known = None;
                    editor.preserve_queued = true;
                    editor.deletion = Some(DeletionStep::Preserve);
                } else {
                    editor.restore_pending = true;
                }
            }
            let me = self.me_mut();
            me.editors[id] = editor;
            me.known[id] = known;
        }
    }

    // --- invariants -------------------------------------------------------------

    /// S1: every accepted, unresolved content is recoverable — from a body on
    /// disk, a preserved copy, or an open editor's buffer in a running window.
    fn assert_acceptance_durability(&self) {
        // Everything some holder contains: a preserved copy, a body on disk, or
        // an open editor's buffer in a running window. A content is
        // recoverable exactly when it is in this set (the ancestor sets make
        // "contains" a union), so S1 is one mask test.
        let mut recoverable: ContentSet = 0;
        for kept in 1..CONTENTS {
            if self.preserved & bit(kept as ContentId) != 0 {
                recoverable |= self.ancestors[kept];
            }
        }
        for id in 0..IDS {
            if let Some(body) = self.body[id]
                && body != 0
            {
                recoverable |= self.ancestors[usize::from(body)];
            }
        }
        for process in 0..P {
            for window in 0..W {
                let window = &self.processes[process].windows[window];
                if !window.running {
                    continue;
                }
                for id in 0..IDS {
                    let editor = window.editors[id];
                    if editor.open && editor.content != 0 {
                        recoverable |= self.ancestors[usize::from(editor.content)];
                    }
                }
            }
        }
        assert!(
            self.accepted & !self.resolved & !recoverable == 0,
            "S1: accepted work became unrecoverable"
        );
    }

    /// S3: delete intent never coexists with a present file-backed body and a
    /// missing entry.
    fn assert_delete_ordering(&self) {
        for process in 0..P {
            for window in 0..W {
                let window = &self.processes[process].windows[window];
                for id in 0..IDS {
                    if window.editors[id].deletion.is_some() && !self.untitled(id) {
                        assert!(
                            !(self.body[id].is_some() && !self.entry[id].present),
                            "S3: a deleting id has a body but no entry"
                        );
                    }
                }
            }
        }
    }

    /// S4: a trusted window follows a complete reconciliation, and the journal
    /// it trusts holds no file-backed body without an entry.
    fn assert_trust(&self) {
        for process in 0..P {
            for window in 0..W {
                let window = &self.processes[process].windows[window];
                if window.running && window.trusted {
                    assert!(
                        window.last_reconciliation_complete,
                        "S4: trust without a complete reconciliation"
                    );
                    assert!(
                        self.inventory_complete(),
                        "S4: a trusted journal has an unexplained body"
                    );
                }
            }
        }
    }

    /// One owner per draft id: no id is open in two windows of one process.
    /// Checked only for the process journal, whose claims promise it.
    fn assert_single_owner(&self) {
        if !S::PROCESS_JOURNAL || W < 2 {
            return;
        }
        for process in 0..P {
            for id in 0..IDS {
                let mut owners = 0;
                for window in 0..W {
                    let window = &self.processes[process].windows[window];
                    if window.running && window.editors[id].open {
                        owners += 1;
                    }
                }
                assert!(owners <= 1, "a draft id is open in two windows");
            }
        }
    }

    fn assert_invariants(&self) {
        self.assert_acceptance_durability();
        self.assert_delete_ordering();
        self.assert_trust();
        self.assert_single_owner();
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
}

/// The single-window scope: two file-backed ids and one untitled id (id 2),
/// with the journal lane, missing-body entry removal, and the window actions.
struct SingleWindowScope;

impl Scope for SingleWindowScope {
    const EXTENDED: bool = true;
    const UNTITLED: Option<usize> = Some(2);
    const PROCESS_JOURNAL: bool = true;
}

/// Nondeterministic actions after the first startup.
const STEPS: usize = 8;

/// S1–S4 over every sequence of `STEPS` actions, faults, crashes, and
/// restarts after the first startup, for one window of one process.
#[kani::proof]
#[kani::unwind(9)]
fn journal_invariants_hold_under_crashes() {
    let mut journal = Journal::<SingleWindowScope, 3, 1, 1>::previous_session();
    journal.step(0, 0, Action::Startup, 0, kani::any());
    journal.assert_invariants();
    for _ in 0..STEPS {
        journal.step(0, 0, kani::any(), any_below(3), kani::any());
        journal.assert_invariants();
    }
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
    let mut journal = Journal::<SingleWindowScope, 3, 1, 1>::previous_session();
    journal.step(0, 0, Action::Startup, 0, false);
    for _ in 0..3 {
        journal.step(0, 0, kani::any(), any_below(3), kani::any());
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
        journal.step(0, 0, action, other, false);
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
        journal.step(0, 0, action, id, false);
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
    let mut journal = Journal::<S, 2, 2, 1>::previous_session();
    journal.step(0, 0, Action::Startup, 0, kani::any());
    journal.assert_invariants();
    for _ in 0..steps {
        journal.step(0, any_below(2), kani::any(), any_below(2), kani::any());
        journal.assert_invariants();
    }
}

/// S1–S4, and one owning window per draft id, over every sequence of
/// `TWO_WINDOW_STEPS` actions by either of two windows of one process (one
/// file-backed and one untitled id, at most three edits), with faults, window
/// opens and closes, crashes, and restarts.
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
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(9)]
fn a_second_writer_breaks_the_journal_invariants() {
    let mut journal = Journal::<K3Scope, 3, 1, 2>::previous_session();
    journal.step(0, 0, Action::Startup, 0, false);
    journal.step(1, 0, Action::Startup, 0, false);
    journal.assert_invariants();
    for _ in 0..SECOND_WRITER_STEPS {
        journal.step(any_below(2), 0, kani::any(), any_below(3), kani::any());
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
