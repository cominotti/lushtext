// SPDX-License-Identifier: GPL-3.0-or-later

//! The draft journal's verification environment, shared by the Kani
//! harnesses (`kani_proofs`) and the draft-service conformance driver
//! (`services::conformance::draft_service`).
//!
//! It is verification code, compiled only under `cfg(kani)`, `cfg(test)` and
//! the `test-utils` feature, and never in a shipping build. The model below is
//! the journal's **environment** — disk, editors, each window's in-memory
//! state, the per-application journal coordinator, and crashes — as
//! fixed-size arrays; every **decision** it takes is a call to the production
//! `journal_core` function that the service and the GTK drafts workflow call
//! at the same point. A harness failure is therefore a defect in the
//! decisions production makes, not in a second copy of them.
//!
//! The model takes the choices that happen inside it through [`Choices`]: the
//! harnesses answer with `kani::any()`, the conformance driver with what the
//! script or the real service decided. The action, the id and the fault of
//! each step are arguments of [`Journal::step`].
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

#![expect(
    clippy::cast_possible_truncation,
    clippy::needless_range_loop,
    clippy::unused_self,
    reason = "the model is written for CBMC: ids and windows index fixed arrays below \
              MAX_IDS and two, so their u8 casts cannot truncate, and plain index loops \
              and associated-constant methods keep the symbolic state small"
)]

use super::journal_core::{
    BodyFacts, BodyWriteDecision, CleanupBodyIdentity, CleanupEntryState, CleanupFacts,
    CommitWriteMode, DeletionStep, DraftClaim, DraftOpenDecision, EntryState,
    ExistingBodyReplacement, JournalLaneAdmission, JournalLaneHolder, OrphanBodyDecision,
    RegistrationCommit, RegistrationFacts, RegistrationRequirement, RestoreDisposition,
    RestoreEnding, StartupRestore, UntrustedCommitDisposition, WindowBodyFacts,
    body_write_decision, commit_authority, deletion_start, draft_open_decision,
    journal_lane_admission, may_write_after_registration, next_deletion_step, orphan_body_decision,
    ownership, registration_required, startup_may_retire_stale, startup_restore,
    unapplied_restore_disposition, untrusted_commit_disposition,
};
use lushtext_model::persistence::draft::DraftManifestCompleteness;

/// Where the model's own nondeterministic choices come from: the ones taken
/// inside the model rather than at a harness call site. The Kani harnesses
/// answer with `kani::any()` and `kani::assume` (`KaniChoices` in
/// `kani_proofs`), which is exactly the nondeterminism the model used to take
/// inline; the conformance driver answers with what the script or the real
/// service decided.
pub(crate) trait Choices {
    /// One previous-session layout bit (an entry, a body, a moved file).
    fn flag(&mut self) -> bool;
    /// The ending of a restore whose body is current, among those `admits`
    /// allows.
    fn restore_ending(&mut self, admits: impl Fn(RestoreEnding) -> bool) -> RestoreEnding;
}

/// The most draft ids any harness uses.
///
/// Kind: out: tool; a verification model scope.
pub(crate) const MAX_IDS: usize = 3;
/// Edits the model admits.
///
/// Kind: out: tool; a verification model scope.
pub(crate) const MAX_EDITS: usize = 3;
/// Content ids: 0 plus one per pre-existing body and one per edit.
///
/// Kind: out: tool; a verification model scope.
pub(crate) const CONTENTS: usize = 1 + MAX_IDS + MAX_EDITS;

pub(crate) type ContentId = u8;
pub(crate) type ContentSet = u16;

pub(crate) const fn bit(content: ContentId) -> ContentSet {
    1 << content
}

/// What a harness models, beyond its actor counts. A trait of associated
/// constants rather than a value, so every scope branch folds at compile time
/// and costs the solver nothing.
pub(crate) trait Scope {
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
    /// Whether an unconfirmed set-aside copy counts as preserved and succeeds
    /// (`false` in fixed production; `true` only in the D6 counterexample exhibit
    /// pinning why `set_aside::place` must reject `AfterRename` copies).
    const ALLOW_UNCONFIRMED_SET_ASIDE_COPY: bool;
}

/// I/O fault outcome for one journal step (`verify-shell-conformance-against-proven-cores` D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub(crate) enum Fault {
    /// Every filesystem and directory-sync call succeeded.
    None,
    /// A call failed before the step's durable effect took place on disk.
    BeforeEffect,
    /// The rename placed the file or manifest, then its directory sync failed:
    /// the effect is live on disk but unconfirmed (revertible on crash), and
    /// the shell returned an error so the window treats the step as failed.
    AfterEffect,
}

/// Whether `fault` is meaningful for `action` (harnesses `kani::assume` away
/// `AfterEffect` on actions with no directory-sync-failure rename outcome).
#[cfg_attr(
    not(kani),
    expect(
        dead_code,
        reason = "called by Kani harnesses to prune redundant fault states"
    )
)]
pub(crate) const fn fault_admitted(action: Action, fault: Fault) -> bool {
    match fault {
        Fault::None | Fault::BeforeEffect => true,
        Fault::AfterEffect => matches!(
            action,
            Action::Register
                | Action::WriteBody
                | Action::Commit
                | Action::DeletionStep
                | Action::ExecCleanup
                | Action::RestoreApply
                | Action::Startup
        ),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DiskEntry {
    pub(crate) present: bool,
    /// Backing-file version the entry's body was written against.
    pub(crate) backing: u8,
    /// Changes on every write of the entry: the fingerprint cleanup compares.
    pub(crate) stamp: u8,
}

#[derive(Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Kani two-window draft-journal model facts"
)]
pub(crate) struct Editor {
    pub(crate) open: bool,
    pub(crate) content: ContentId,
    /// `draft_dirty`: the buffer is ahead of the recovery body.
    pub(crate) dirty: bool,
    /// A body-write token minted by this pass's registration step.
    pub(crate) token: bool,
    /// Content written as a body and awaiting its manifest commit.
    pub(crate) written: Option<ContentId>,
    /// A restore of this id's body is pending: the user has not seen it.
    pub(crate) restore_pending: bool,
    /// Tombstone plus the next step of an in-progress deletion.
    pub(crate) deletion: Option<DeletionStep>,
    /// The deletion must preserve the body first (queued preservation).
    pub(crate) preserve_queued: bool,
    /// Startup repaired this entry from the session after failing to read the
    /// persisted manifest, so its in-memory copy records an unproven mtime.
    pub(crate) unverified_mtime: bool,
}

pub(crate) const CLOSED: Editor = Editor {
    open: false,
    content: 0,
    dirty: false,
    token: false,
    written: None,
    restore_pending: false,
    deletion: None,
    preserve_queued: false,
    unverified_mtime: false,
};

/// What holds a journal lane.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaneUse {
    /// A registration → body write → commit pass for one id.
    Pass(u8),
    /// A serialized deletion of one id.
    Deletion(u8),
    /// An orphan-cleanup inspect/execute worker.
    Cleanup,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct LaneHolder {
    /// The holder's stable window identity.
    pub(crate) window: u8,
    pub(crate) lane_use: LaneUse,
}

/// One window's in-memory journal state.
#[derive(Clone, Copy)]
pub(crate) struct Window<const IDS: usize> {
    /// Stable identity inside its process (swaps move it with the window).
    pub(crate) id: u8,
    pub(crate) running: bool,
    pub(crate) editors: [Editor; IDS],
    /// The window's manifest copy: the stamp of the entry it lists per id.
    pub(crate) known: [Option<u8>; IDS],
    pub(crate) trusted: bool,
    /// Ghost: the last reconciliation that set `trusted` was complete.
    pub(crate) last_reconciliation_complete: bool,
    /// The window's own lane (the baseline scope).
    pub(crate) lane: Option<LaneUse>,
    /// Orphan bodies an inspection nominated, by the content it saw.
    pub(crate) orphan_candidate: [Option<ContentId>; IDS],
    /// Missing-body entries an inspection nominated, by fingerprint.
    pub(crate) missing_candidate: [Option<u8>; IDS],
}

impl<const IDS: usize> Window<IDS> {
    pub(crate) const fn stopped(id: u8) -> Self {
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

    pub(crate) fn reset(&mut self) {
        *self = Self::stopped(self.id);
    }
}

/// One LushText process: its windows and its journal coordinator.
#[derive(Clone, Copy)]
pub(crate) struct Process<const IDS: usize, const W: usize> {
    pub(crate) windows: [Window<IDS>; W],
    /// The process-wide lane (the process-journal scope).
    pub(crate) lane: Option<LaneHolder>,
    /// The process already ran its startup restore (the process-journal scope).
    pub(crate) restored: bool,
    /// The window that schedules orphan cleanup (the process-journal scope).
    pub(crate) cleanup_owner: Option<u8>,
}

impl<const IDS: usize, const W: usize> Process<IDS, W> {
    pub(crate) fn stopped() -> Self {
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

    pub(crate) fn reset(&mut self) {
        for window in &mut self.windows {
            window.reset();
        }
        self.lane = None;
        self.restored = false;
        self.cleanup_owner = None;
    }

    pub(crate) fn running(&self) -> bool {
        let mut running = false;
        for index in 0..W {
            running |= self.windows[index].running;
        }
        running
    }
}

pub(crate) struct Journal<S: Scope, const IDS: usize, const W: usize, const P: usize> {
    pub(crate) scope: core::marker::PhantomData<S>,
    pub(crate) processes: [Process<IDS, W>; P],
    pub(crate) entry: [DiskEntry; IDS],
    /// Last durably confirmed manifest entry array (what an unconfirmed
    /// manifest write reverts to on crash).
    pub(crate) confirmed_entry: [DiskEntry; IDS],
    pub(crate) manifest_unconfirmed: bool,
    pub(crate) body: [Option<ContentId>; IDS],
    /// Last durably confirmed body per slot (what an unconfirmed write or
    /// unsynced unlink reverts to on crash).
    pub(crate) confirmed_body: [Option<ContentId>; IDS],
    pub(crate) body_unconfirmed: [bool; IDS],
    pub(crate) backing: [u8; IDS],
    /// Content ids currently visible as preserved in the set-aside area.
    pub(crate) preserved: ContentSet,
    /// Set-aside placements confirmed by a successful `set-aside/` sync.
    pub(crate) confirmed_preserved: ContentSet,
    /// Set-aside copies whose `set-aside/` sync failed (`AfterEffect`): live
    /// on disk, not counted as preserved until confirmed or kept at crash.
    pub(crate) unconfirmed_copy: [Option<ContentId>; IDS],
    /// Set-aside moves whose `set-aside/` sync may be unconfirmed: counted as
    /// placed, and if reverted on crash they return to `drafts/`.
    pub(crate) unconfirmed_move: [Option<ContentId>; IDS],
    /// Ghost: content the user was told is protected (accepted by a commit, or
    /// found on disk at the first startup).
    pub(crate) accepted: ContentSet,
    /// Ghost: content the user resolved (saved to its file, or discarded).
    pub(crate) resolved: ContentSet,
    /// Ancestor sets: `ancestors[c]` holds every content `c` contains.
    pub(crate) ancestors: [ContentSet; CONTENTS],
    pub(crate) next_content: ContentId,
    pub(crate) edits: usize,
}

/// One nondeterministic action of the environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub(crate) enum Action {
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
    #[cfg_attr(
        not(any(test, kani)),
        expect(dead_code, reason = "only Kani and unit tests construct OpenWindow")
    )]
    OpenWindow,
    Crash,
    Startup,
}

impl Action {
    /// Every action, in declaration order: the conformance driver picks
    /// among the enabled ones by index.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "read only by unit tests; property driver queries enabled_steps"
        )
    )]
    pub(crate) const ALL: [Self; 16] = [
        Self::Edit,
        Self::Open,
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
        Self::CloseWindow,
        Self::OpenWindow,
        Self::Crash,
        Self::Startup,
    ];
}

impl<S: Scope, const IDS: usize, const W: usize, const P: usize> Journal<S, IDS, W, P> {
    /// A previous session's disk, chosen nondeterministically: any id may have
    /// an entry, a body, both, or neither, and its file may have moved on.
    pub(crate) fn previous_session(choices: &mut impl Choices) -> Self {
        let empty_entry = DiskEntry {
            present: false,
            backing: 0,
            stamp: 0,
        };
        let mut journal = Self {
            scope: core::marker::PhantomData,
            processes: [Process::stopped(); P],
            entry: [empty_entry; IDS],
            confirmed_entry: [empty_entry; IDS],
            manifest_unconfirmed: false,
            body: [None; IDS],
            confirmed_body: [None; IDS],
            body_unconfirmed: [false; IDS],
            backing: [0; IDS],
            preserved: 0,
            confirmed_preserved: 0,
            unconfirmed_copy: [None; IDS],
            unconfirmed_move: [None; IDS],
            accepted: 0,
            resolved: 0,
            ancestors: [0; CONTENTS],
            next_content: 1 + IDS as ContentId,
            edits: 0,
        };
        for id in 0..IDS {
            let content = 1 + id as ContentId;
            journal.ancestors[usize::from(content)] = bit(content);
            journal.entry[id].present = choices.flag();
            if choices.flag() {
                journal.body[id] = Some(content);
                // Everything a previous session left on disk was promised.
                journal.accepted |= bit(content);
            }
            if !journal.untitled(id) && choices.flag() {
                journal.backing[id] = 1;
            }
        }
        journal.confirmed_entry = journal.entry;
        journal.confirmed_body = journal.body;
        journal
    }

    pub(crate) fn untitled(&self, id: usize) -> bool {
        S::UNTITLED == Some(id)
    }

    /// Whether `id` is currently open in any running window of the process.
    fn open_in_any_window(&self, id: usize) -> bool {
        for window in 0..W {
            let win = &self.processes[0].windows[window];
            if win.running && win.editors[id].open {
                return true;
            }
        }
        false
    }

    /// Reconcile unregistered file-backed bodies into `set-aside/` (moving
    /// unattributable ones, or copying and rebuilding stale entries for tabs
    /// open in the session), matching `recoverable_draft_inventory_with_candidate`.
    fn reconcile_unregistered_bodies(
        &mut self,
        choices: &mut impl Choices,
        after: &mut [DiskEntry; IDS],
        inserting_id: Option<usize>,
        allow_session_attribution: bool,
        fault: Fault,
    ) {
        for other in 0..IDS {
            if Some(other) == inserting_id {
                continue;
            }
            if !self.untitled(other) && self.body[other].is_some() && !after[other].present {
                if allow_session_attribution && self.open_in_any_window(other) {
                    if let Some(content) = self.body[other]
                        && (fault != Fault::BeforeEffect || choices.flag())
                    {
                        self.confirmed_preserved |= bit(content);
                        after[other] = DiskEntry {
                            present: true,
                            backing: u8::MAX,
                            stamp: after[other].stamp.wrapping_add(1),
                        };
                    }
                } else if fault != Fault::BeforeEffect || choices.flag() {
                    let moved = self.body[other].take();
                    self.confirmed_body[other] = None;
                    self.body_unconfirmed[other] = false;
                    self.unconfirmed_move[other] = moved;
                }
            }
        }
        self.recompute_preserved();
    }

    /// The acting window (slot 0 of the acting process, which is slot 0).
    pub(crate) fn me(&self) -> &Window<IDS> {
        &self.processes[0].windows[0]
    }

    pub(crate) fn me_mut(&mut self) -> &mut Window<IDS> {
        &mut self.processes[0].windows[0]
    }

    /// Reconciliation is complete when every file-backed body has an entry.
    /// An untitled body needs none: reconciliation rebuilds its entry.
    pub(crate) fn inventory_complete(&self) -> bool {
        self.inventory_complete_with(&self.entry)
    }

    /// Whether every file-backed body would have an entry if the manifest read
    /// `entries`.
    pub(crate) fn inventory_complete_with(&self, entries: &[DiskEntry; IDS]) -> bool {
        let mut complete = true;
        for id in 0..IDS {
            if !self.untitled(id) && self.body[id].is_some() && !entries[id].present {
                complete = false;
            }
        }
        complete
    }

    /// Whether the backing file changed since the entry's body was written.
    pub(crate) fn stale(&self, id: usize) -> bool {
        !self.untitled(id) && self.entry[id].backing != self.backing[id]
    }

    /// The production window view, so the model checks the construction the
    /// window uses rather than a copy of it.
    pub(crate) fn window_body_facts(&self, id: usize) -> BodyFacts {
        let me = self.me();
        BodyFacts::from_window_view(WindowBodyFacts {
            registered: me.known[id].is_some(),
            restore_pending: me.editors[id].restore_pending,
        })
    }

    fn recompute_preserved(&mut self) {
        let mut mask = self.confirmed_preserved;
        for id in 0..IDS {
            if let Some(content) = self.unconfirmed_move[id] {
                mask |= bit(content);
            }
            if S::ALLOW_UNCONFIRMED_SET_ASIDE_COPY
                && let Some(content) = self.unconfirmed_copy[id]
            {
                mask |= bit(content);
            }
        }
        self.preserved = mask;
    }

    fn confirm_drafts_dir(&mut self) {
        self.confirmed_entry = self.entry;
        self.manifest_unconfirmed = false;
        self.confirmed_body = self.body;
        self.body_unconfirmed = [false; IDS];
    }

    fn confirm_set_aside_dir(&mut self) {
        for id in 0..IDS {
            if let Some(content) = self.unconfirmed_copy[id].take() {
                self.confirmed_preserved |= bit(content);
            }
            if let Some(content) = self.unconfirmed_move[id].take() {
                self.confirmed_preserved |= bit(content);
            }
        }
        self.recompute_preserved();
    }

    pub(crate) fn preserve_body(&mut self, id: usize, fault: Fault) -> bool {
        let Some(content) = self.body[id] else {
            return true;
        };
        match fault {
            Fault::None => {
                self.confirmed_preserved |= bit(content);
                self.confirm_set_aside_dir();
                true
            }
            Fault::BeforeEffect => false,
            Fault::AfterEffect => {
                self.unconfirmed_copy[id] = Some(content);
                self.recompute_preserved();
                S::ALLOW_UNCONFIRMED_SET_ASIDE_COPY
            }
        }
    }

    pub(crate) fn write_entry(&mut self, id: usize) {
        self.entry[id] = DiskEntry {
            present: true,
            backing: self.backing[id],
            stamp: self.entry[id].stamp.wrapping_add(1),
        };
    }

    // --- the journal lane --------------------------------------------------

    /// What holds the lane the acting window uses, as it sees it.
    pub(crate) fn lane_holder(&self) -> Option<LaneHolder> {
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

    pub(crate) fn set_lane(&mut self, lane_use: Option<LaneUse>) {
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
    pub(crate) fn lane_admits(&self, lane_use: LaneUse) -> bool {
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
    pub(crate) fn release_lane(&mut self, lane_use: LaneUse) {
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
    /// (`adopt_draft_manifest`), with the same authority.
    pub(crate) fn accept_commit(&mut self, trusted: bool) {
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
                if trusted {
                    window.editors[id].unverified_mtime = false;
                }
            }
            window.trusted = trusted;
            if trusted {
                window.last_reconciliation_complete = true;
            }
        }
    }

    /// A failed commit revokes the acting window's authority, and in the
    /// process-journal scope every window's (`reject_draft_manifest_authority`).
    pub(crate) fn revoke_trust(&mut self) {
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
    pub(crate) fn step(
        &mut self,
        choices: &mut impl Choices,
        p: usize,
        w: usize,
        action: Action,
        id: usize,
        fault: Fault,
    ) {
        let swap_process = P > 1 && p == 1;
        let swap_window = W > 1 && w == 1;
        if swap_process {
            self.processes.swap(0, 1);
        }
        if swap_window {
            self.processes[0].windows.swap(0, 1);
        }
        self.act(choices, action, id, fault);
        if swap_window {
            self.processes[0].windows.swap(0, 1);
        }
        if swap_process {
            self.processes.swap(0, 1);
        }
    }

    pub(crate) fn act(
        &mut self,
        choices: &mut impl Choices,
        action: Action,
        id: usize,
        fault: Fault,
    ) {
        match action {
            Action::Crash => {
                self.processes[0].reset();
                if self.manifest_unconfirmed {
                    if choices.flag() {
                        self.confirmed_entry = self.entry;
                    } else {
                        self.entry = self.confirmed_entry;
                    }
                    self.manifest_unconfirmed = false;
                }
                for slot in 0..IDS {
                    if self.body_unconfirmed[slot] {
                        if choices.flag() {
                            self.confirmed_body[slot] = self.body[slot];
                        } else {
                            self.body[slot] = self.confirmed_body[slot];
                        }
                        self.body_unconfirmed[slot] = false;
                    }
                    if let Some(content) = self.unconfirmed_copy[slot].take()
                        && choices.flag()
                    {
                        self.confirmed_preserved |= bit(content);
                    }
                    if let Some(content) = self.unconfirmed_move[slot].take() {
                        if choices.flag() || self.body[slot].is_some() {
                            self.confirmed_preserved |= bit(content);
                        } else {
                            self.body[slot] = Some(content);
                            self.confirmed_body[slot] = Some(content);
                            self.body_unconfirmed[slot] = false;
                        }
                    }
                }
                self.recompute_preserved();
                return;
            }
            Action::Startup => {
                if !self.processes[0].running() {
                    self.startup(choices, fault);
                    let owner = self.me().id;
                    let process = &mut self.processes[0];
                    process.restored = true;
                    process.cleanup_owner = Some(owner);
                }
                return;
            }
            Action::OpenWindow => {
                if S::EXTENDED && self.processes[0].running() && !self.me().running {
                    self.open_window(choices, fault);
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
            Action::Register => self.register(choices, id, fault),
            Action::WriteBody => {
                if !editor.open || !editor.dirty || !editor.token {
                    return;
                }
                let needed = registration_required(RegistrationFacts {
                    file_backed: !self.untitled(id),
                    registered_in_known_manifest: self.me().known[id].is_some(),
                    known_manifest_trusted: self.me().trusted,
                });
                self.me_mut().editors[id].token = false;
                if body_write_decision(ownership(self.window_body_facts(id)), needed)
                    == BodyWriteDecision::Hold
                    || fault == Fault::BeforeEffect
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
                if fault == Fault::AfterEffect {
                    self.body_unconfirmed[id] = true;
                    self.release_lane(LaneUse::Pass(id as u8));
                } else {
                    self.confirm_drafts_dir();
                    self.me_mut().editors[id].written = Some(editor.content);
                }
            }
            Action::Commit => self.commit(choices, id, fault),
            Action::Save | Action::Discard => {
                if !editor.open || editor.deletion.is_some() {
                    return;
                }
                // The user resolves exactly the work they can see.
                self.resolved |= self.ancestors[usize::from(editor.content)];
                if matches!(action, Action::Save) && !self.untitled(id) {
                    self.backing[id] = self.backing[id].wrapping_add(1);
                }
                if !self.untitled(id) && self.me().known[id].is_none() && !editor.restore_pending {
                    self.release_lane(LaneUse::Pass(id as u8));
                    let editor = &mut self.me_mut().editors[id];
                    editor.dirty = false;
                    editor.token = false;
                    editor.written = None;
                    editor.content = 0;
                    return;
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
            Action::DeletionStep => self.run_deletion_step(choices, id, fault),
            Action::Inspect => self.inspect(id),
            Action::ExecCleanup => self.exec_cleanup(id, fault),
            Action::RestoreApply => self.restore_apply(choices, id, fault),
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
    pub(crate) fn open_editor(&mut self, id: usize) {
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

    pub(crate) fn register(&mut self, choices: &mut impl Choices, id: usize, fault: Fault) {
        let editor = self.me().editors[id];
        if !editor.open || !editor.dirty || editor.token || editor.deletion.is_some() {
            return;
        }
        if !self.lane_admits(LaneUse::Pass(id as u8)) {
            // The existing mark-pending admission: a later tick retries.
            return;
        }
        let trusted = self.me().trusted;
        let needed = registration_required(RegistrationFacts {
            file_backed: !self.untitled(id),
            registered_in_known_manifest: self.me().known[id].is_some(),
            known_manifest_trusted: trusted,
        });
        match body_write_decision(ownership(self.window_body_facts(id)), needed) {
            BodyWriteDecision::Hold => {}
            BodyWriteDecision::Write | BodyWriteDecision::PreserveThenWrite => {
                self.set_lane(Some(LaneUse::Pass(id as u8)));
                self.me_mut().editors[id].token = true;
            }
            BodyWriteDecision::RegisterFirst => {
                if S::EXTENDED
                    && !self.untitled(id)
                    && !self.entry[id].present
                    && self.body[id].is_some()
                {
                    if fault != Fault::BeforeEffect || choices.flag() {
                        let moved = self.body[id].take();
                        self.confirmed_body[id] = None;
                        self.body_unconfirmed[id] = false;
                        self.unconfirmed_move[id] = moved;
                        self.recompute_preserved();
                    }
                    if self.body[id].is_some() {
                        self.revoke_trust();
                        return;
                    }
                }
                let mut after = self.entry;
                if S::EXTENDED {
                    self.reconcile_unregistered_bodies(choices, &mut after, Some(id), true, fault);
                }
                if fault == Fault::BeforeEffect {
                    let additive_ok = S::EXTENDED && choices.flag();
                    if !additive_ok {
                        self.revoke_trust();
                        return;
                    }
                    self.set_lane(Some(LaneUse::Pass(id as u8)));
                    if !self.entry[id].present {
                        self.write_entry(id);
                    }
                    let stamp = self.entry[id].stamp;
                    let mirror = S::PROCESS_JOURNAL;
                    for index in 0..W {
                        let window = &mut self.processes[0].windows[index];
                        if (index == 0 || mirror) && window.known[id].is_none() {
                            window.known[id] = Some(stamp);
                        }
                    }
                    self.revoke_trust();
                    self.me_mut().editors[id].token = may_write_after_registration(
                        RegistrationRequirement::Required,
                        RegistrationCommit::Committed,
                    );
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
                    let _ = self.preserve_body(id, Fault::None);
                }
                let complete = self.inventory_complete_with(&after);
                let insert = !after[id].present;
                let committed = complete
                    || untrusted_commit_disposition(
                        CommitWriteMode::InsertOnly,
                        ExistingBodyReplacement::Allowed,
                    ) == UntrustedCommitDisposition::Additive;
                if !committed {
                    self.release_lane(LaneUse::Pass(id as u8));
                    return;
                }
                if insert {
                    after[id] = DiskEntry {
                        present: true,
                        backing: self.backing[id],
                        stamp: after[id].stamp.wrapping_add(1),
                    };
                }
                if S::EXTENDED && complete {
                    for other in 0..IDS {
                        if other != id
                            && self.untitled(other)
                            && self.body[other].is_some()
                            && !after[other].present
                        {
                            after[other] = DiskEntry {
                                present: true,
                                backing: self.backing[other],
                                stamp: after[other].stamp.wrapping_add(1),
                            };
                        }
                    }
                }
                self.entry = after;
                if fault == Fault::AfterEffect {
                    self.manifest_unconfirmed = true;
                    self.revoke_trust();
                    self.release_lane(LaneUse::Pass(id as u8));
                    return;
                }
                self.confirm_drafts_dir();
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
                self.me_mut().editors[id].token = may_write_after_registration(
                    RegistrationRequirement::Required,
                    RegistrationCommit::Committed,
                );
            }
        }
    }

    pub(crate) fn commit(&mut self, choices: &mut impl Choices, id: usize, fault: Fault) {
        let editor = self.me().editors[id];
        let Some(written) = editor.written else {
            return;
        };
        self.me_mut().editors[id].written = None;
        self.release_lane(LaneUse::Pass(id as u8));
        let mut after = self.entry;
        if S::EXTENDED {
            self.reconcile_unregistered_bodies(choices, &mut after, None, true, fault);
        }
        if fault == Fault::BeforeEffect {
            self.revoke_trust();
            return;
        }
        if S::EXTENDED {
            // A reconciling commit rebuilds the entry of an untitled body
            // that has none.
            for other in 0..IDS {
                if self.untitled(other) && self.body[other].is_some() && !after[other].present {
                    after[other] = DiskEntry {
                        present: true,
                        backing: self.backing[other],
                        stamp: after[other].stamp.wrapping_add(1),
                    };
                }
            }
        }
        if !self.inventory_complete_with(&after) {
            // A reconciling commit that cannot prove completeness fails.
            self.revoke_trust();
            return;
        }
        after[id] = DiskEntry {
            present: true,
            backing: self.backing[id],
            stamp: after[id].stamp.wrapping_add(1),
        };
        self.entry = after;
        if fault == Fault::AfterEffect {
            self.manifest_unconfirmed = true;
            self.revoke_trust();
            return;
        }
        self.confirm_drafts_dir();
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

    pub(crate) fn run_deletion_step(
        &mut self,
        choices: &mut impl Choices,
        id: usize,
        fault: Fault,
    ) {
        let Some(step) = self.me().editors[id].deletion else {
            return;
        };
        if !self.lane_admits(LaneUse::Deletion(id as u8)) {
            return;
        }
        self.set_lane(Some(LaneUse::Deletion(id as u8)));
        let succeeded = match step {
            DeletionStep::Preserve => {
                let ok = self.preserve_body(id, fault);
                if ok {
                    self.me_mut().editors[id].preserve_queued = false;
                }
                ok
            }
            DeletionStep::DeleteBody => {
                if fault == Fault::BeforeEffect {
                    false
                } else {
                    self.body[id] = None;
                    self.body_unconfirmed[id] = true;
                    true
                }
            }
            DeletionStep::RemoveEntry => {
                let mut after = self.entry;
                after[id].present = false;
                if S::EXTENDED {
                    self.reconcile_unregistered_bodies(choices, &mut after, Some(id), true, fault);
                }
                if fault == Fault::BeforeEffect {
                    self.revoke_trust();
                    false
                } else {
                    // A reconciling commit: the removal lands only with a
                    // complete inventory, and then the journal is trusted.
                    if S::EXTENDED {
                        for other in 0..IDS {
                            if other != id
                                && self.untitled(other)
                                && self.body[other].is_some()
                                && !after[other].present
                            {
                                after[other] = DiskEntry {
                                    present: true,
                                    backing: self.backing[other],
                                    stamp: after[other].stamp.wrapping_add(1),
                                };
                            }
                        }
                    }
                    if self.inventory_complete_with(&after) {
                        self.entry = after;
                        if fault == Fault::AfterEffect {
                            self.manifest_unconfirmed = true;
                            self.revoke_trust();
                            false
                        } else {
                            self.confirm_drafts_dir();
                            let trusted =
                                commit_authority(DraftManifestCompleteness::Complete, true)
                                    .is_trusted();
                            if S::EXTENDED {
                                self.accept_commit(trusted);
                            } else {
                                let me = self.me_mut();
                                me.trusted = trusted;
                                me.last_reconciliation_complete = true;
                            }
                            true
                        }
                    } else {
                        self.revoke_trust();
                        false
                    }
                }
            }
            DeletionStep::Done | DeletionStep::Stopped => true,
        };
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
    pub(crate) fn inspect(&mut self, id: usize) {
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
    pub(crate) fn exec_cleanup(&mut self, id: usize, fault: Fault) {
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
        if fault == Fault::BeforeEffect {
            // `load_trusted_manifest_for_cleanup` failed before any orphan deletion
            // or manifest write; release the cleanup lane and leave disk unchanged.
            self.me_mut().orphan_candidate = [None; IDS];
            self.me_mut().missing_candidate = [None; IDS];
            self.release_lane(LaneUse::Cleanup);
            return;
        }
        for id in 0..IDS {
            self.exec_cleanup_one(id, Fault::None);
        }
        // Missing-body entries go in one manifest write, which may fail.
        let mut any_missing_removed = false;
        for id in 0..IDS {
            if let Some(stamp) = self.me().missing_candidate[id]
                && self.entry[id].present
                && self.entry[id].stamp == stamp
                && self.body[id].is_none()
            {
                self.entry[id].present = false;
                any_missing_removed = true;
            }
        }
        if any_missing_removed {
            if fault == Fault::AfterEffect {
                self.manifest_unconfirmed = true;
                self.revoke_trust();
            } else {
                self.confirm_drafts_dir();
                let mirror = S::PROCESS_JOURNAL;
                for id in 0..IDS {
                    if !self.entry[id].present {
                        for index in 0..W {
                            if index == 0 || mirror {
                                self.processes[0].windows[index].known[id] = None;
                            }
                        }
                    }
                }
            }
        }
        self.me_mut().missing_candidate = [None; IDS];
        self.release_lane(LaneUse::Cleanup);
    }

    pub(crate) fn exec_cleanup_one(&mut self, id: usize, fault: Fault) {
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
        if orphan_body_decision(facts) == OrphanBodyDecision::Delete && fault != Fault::BeforeEffect
        {
            // S2: the deleted body is unreferenced and revalidated.
            assert!(!self.entry[id].present && self.body[id] == Some(inspected));
            self.body[id] = None;
            self.body_unconfirmed[id] = true;
        }
    }

    pub(crate) fn restore_apply(&mut self, choices: &mut impl Choices, id: usize, fault: Fault) {
        if !self.me().editors[id].restore_pending {
            return;
        }
        let ending = if self.stale(id) || self.me().editors[id].unverified_mtime {
            RestoreEnding::Stale
        } else {
            match self.body[id] {
                None => RestoreEnding::MissingBody,
                // Every other ending is the window's or the read's: the
                // environment chooses.
                Some(_) => choices.restore_ending(|ending| {
                    !matches!(ending, RestoreEnding::Stale | RestoreEnding::MissingBody)
                }),
            }
        };
        let editor = &mut self.me_mut().editors[id];
        editor.restore_pending = false;
        editor.unverified_mtime = false;
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
                if !self.preserve_body(id, fault) {
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
    pub(crate) fn close_window(&mut self) {
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
    pub(crate) fn open_window(&mut self, choices: &mut impl Choices, fault: Fault) {
        let restored = self.processes[0].restored;
        if !S::PROCESS_JOURNAL || startup_restore(restored) == StartupRestore::RestoreSession {
            self.startup(choices, fault);
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
    pub(crate) fn startup(&mut self, choices: &mut impl Choices, fault: Fault) {
        self.me_mut().running = true;
        // Reconciliation sets unattributable file-backed bodies aside (moves
        // them) and rebuilds an untitled body's entry, so the inventory it
        // publishes is complete. When `BeforeEffect` fails directory traversal
        // or a `move_in`, an unattributable body may remain in `drafts/`.
        let mut rebuilt_untitled = false;
        let mut after = self.entry;
        self.reconcile_unregistered_bodies(choices, &mut after, None, false, fault);
        self.entry = after;
        for id in 0..IDS {
            if self.untitled(id)
                && self.body[id].is_some()
                && !self.entry[id].present
                && fault != Fault::BeforeEffect
            {
                self.write_entry(id);
                rebuilt_untitled = true;
            }
        }
        let complete = self.inventory_complete();
        let completeness = if complete {
            DraftManifestCompleteness::Complete
        } else {
            DraftManifestCompleteness::Partial
        };
        let persisted_ok = fault == Fault::None;
        if rebuilt_untitled {
            if fault == Fault::AfterEffect {
                self.manifest_unconfirmed = true;
            } else if persisted_ok {
                self.confirm_drafts_dir();
            }
        }
        let trusted = commit_authority(completeness, persisted_ok).is_trusted();
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
            if self.body[id].is_some() || self.stale(id) {
                if self.stale(id) && startup_may_retire_stale(trusted) {
                    known = None;
                    editor.preserve_queued = true;
                    editor.deletion = Some(DeletionStep::Preserve);
                } else {
                    editor.restore_pending = true;
                    if fault == Fault::BeforeEffect
                        && !self.untitled(id)
                        && self.body[id].is_some()
                        && !self.stale(id)
                    {
                        editor.unverified_mtime = choices.flag();
                    }
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
    pub(crate) fn acceptance_is_durable(&self) -> bool {
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
        self.accepted & !self.resolved & !recoverable == 0
    }

    /// S3: delete intent never coexists with a present file-backed body and a
    /// missing entry.
    pub(crate) fn delete_ordering_holds(&self) -> bool {
        let mut holds = true;
        for process in 0..P {
            for window in 0..W {
                let window = &self.processes[process].windows[window];
                for id in 0..IDS {
                    if window.editors[id].deletion.is_some() && !self.untitled(id) {
                        holds &= !(self.body[id].is_some() && !self.entry[id].present);
                    }
                }
            }
        }
        holds
    }

    /// S4: a trusted window follows a complete reconciliation, and the journal
    /// it trusts holds no file-backed body without an entry.
    pub(crate) fn trust_is_founded(&self) -> (bool, bool) {
        let mut reconciled = true;
        let mut explained = true;
        for process in 0..P {
            for window in 0..W {
                let window = &self.processes[process].windows[window];
                if window.running && window.trusted {
                    reconciled &= window.last_reconciliation_complete;
                    explained &= self.inventory_complete();
                }
            }
        }
        (reconciled, explained)
    }

    /// One owner per draft id: no id is open in two windows of one process.
    /// Checked only for the process journal, whose claims promise it.
    pub(crate) fn single_owner_holds(&self) -> bool {
        if !S::PROCESS_JOURNAL || W < 2 {
            return true;
        }
        let mut holds = true;
        for process in 0..P {
            for id in 0..IDS {
                let mut owners = 0;
                for window in 0..W {
                    let window = &self.processes[process].windows[window];
                    if window.running && window.editors[id].open {
                        owners += 1;
                    }
                }
                holds &= owners <= 1;
            }
        }
        holds
    }

    /// S1, S3, S4 and single ownership, each asserted with its own message.
    /// The Kani harnesses call this after every step; the conformance driver
    /// calls it as a cross-check that the lockstep stayed inside the proved
    /// envelope. S2 is asserted where it applies, in `exec_cleanup_one`.
    pub(crate) fn assert_invariants(&self) {
        assert!(
            self.acceptance_is_durable(),
            "S1: accepted work became unrecoverable"
        );
        assert!(
            self.delete_ordering_holds(),
            "S3: a deleting id has a body but no entry"
        );
        let (reconciled, explained) = self.trust_is_founded();
        assert!(reconciled, "S4: trust without a complete reconciliation");
        assert!(explained, "S4: a trusted journal has an unexplained body");
        assert!(
            self.single_owner_holds(),
            "a draft id is open in two windows"
        );
    }
}

/// The single-window scope: two file-backed ids and one untitled id (id 2),
/// with the journal lane, missing-body entry removal, and the window actions.
pub(crate) struct SingleWindowScope;

impl Scope for SingleWindowScope {
    const EXTENDED: bool = true;
    const UNTITLED: Option<usize> = Some(2);
    const PROCESS_JOURNAL: bool = true;
    const ALLOW_UNCONFIRMED_SET_ASIDE_COPY: bool = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Choices fixed for a unit test: an empty previous session, and every
    /// restore applied.
    struct Fixed;

    impl Choices for Fixed {
        fn flag(&mut self) -> bool {
            false
        }

        fn restore_ending(&mut self, admits: impl Fn(RestoreEnding) -> bool) -> RestoreEnding {
            assert!(admits(RestoreEnding::Applied));
            RestoreEnding::Applied
        }
    }

    #[test]
    fn every_action_keeps_the_invariants_from_a_fresh_start() {
        for action in Action::ALL {
            for fault in [Fault::None, Fault::BeforeEffect, Fault::AfterEffect] {
                let mut journal =
                    Journal::<SingleWindowScope, 3, 1, 1>::previous_session(&mut Fixed);
                journal.step(&mut Fixed, 0, 0, Action::Startup, 0, Fault::None);
                journal.step(&mut Fixed, 0, 0, Action::Open, 0, Fault::None);
                journal.step(&mut Fixed, 0, 0, action, 0, fault);
                journal.assert_invariants();
            }
        }
    }

    #[test]
    fn an_edit_is_registered_written_and_committed_clean() {
        let mut journal = Journal::<SingleWindowScope, 3, 1, 1>::previous_session(&mut Fixed);
        journal.step(&mut Fixed, 0, 0, Action::Startup, 0, Fault::None);
        journal.assert_invariants();
        for action in [
            Action::Open,
            Action::Edit,
            Action::Register,
            Action::WriteBody,
            Action::Commit,
        ] {
            journal.step(&mut Fixed, 0, 0, action, 0, Fault::None);
            journal.assert_invariants();
        }
        let editor = journal.me().editors[0];
        assert!(editor.open && !editor.dirty);
        assert!(journal.entry[0].present);
        assert_eq!(journal.body[0], Some(editor.content));
    }

    #[test]
    fn a_crash_after_the_body_write_keeps_the_body_and_its_entry() {
        let mut journal = Journal::<SingleWindowScope, 3, 1, 1>::previous_session(&mut Fixed);
        journal.step(&mut Fixed, 0, 0, Action::Startup, 0, Fault::None);
        for action in [
            Action::Open,
            Action::Edit,
            Action::Register,
            Action::WriteBody,
        ] {
            journal.step(&mut Fixed, 0, 0, action, 0, Fault::None);
        }
        journal.step(&mut Fixed, 0, 0, Action::Crash, 0, Fault::None);
        journal.step(&mut Fixed, 0, 0, Action::Startup, 0, Fault::None);
        journal.assert_invariants();
        assert!(journal.me().editors[0].restore_pending);
        journal.step(&mut Fixed, 0, 0, Action::RestoreApply, 0, Fault::None);
        journal.assert_invariants();
        assert!(!journal.me().editors[0].restore_pending);
    }
}
