// -*- mode: Bluespec; -*-
// SPDX-License-Identifier: GPL-3.0-or-later
//
// T2 scale: the draft journal, one and two processes, re-expressed in Quint.
//
// DISPOSABLE EVALUATION MODEL, NOT A MAINTAINED PROPERTY. The maintained
// proofs are the Kani harnesses in
// crates/lushtext-core/src/services/draft_service/kani_proofs.rs, which drive
// the real decision core crates/lushtext-core/src/services/draft_service/journal_core.rs.
// Section 1 transliterates every journal_core decision the harness calls;
// section 2 transliterates the harness's environment (`Journal`, `step`,
// `step_as`, `startup`, the invariants) function for function, as pure
// functions over one record, so a trace maps one-to-one onto a Kani step.
//
// Known answers (docs/next/formal-verification.md, phase 4 and K8):
//   - one process (`journal_invariants_hold_under_crashes`, 8 actions):
//     S1-S4 and the two inline assertions hold;
//   - two processes (`a_second_writer_breaks_the_journal_invariants`):
//     at 8 actions after both startups both S1 ("accepted work lost") and
//     "a body was written without an entry" fail; at 6 only the latter.
//
// The instance modules at the end fix the bounds (`MaxSteps`) and the
// process count (`TwoProcesses`).
module journal_defs {
  const TwoProcesses: bool
  /// The harness's `IDS = 3`; T3 reuses these definitions with fewer ids.
  const IdCount: int

  // --- Bounds (kani_proofs.rs) ---------------------------------------------

  val Ids = 0.to(IdCount - 1)
  pure val MaxEdits = 3
  /// `None` for an `Option<ContentId>`: content ids start at 0, and 0 is
  /// "nothing to protect", so -1 is free.
  pure val NoContent = -1

  // --- 1. journal_core decisions -------------------------------------------

  type EntryState = Absent | Current | Superseded
  type BodyFacts = { bodyPresent: bool, entry: EntryState, restorePending: bool, backingStale: bool }
  type BodyOwner = Nothing | Unseen | OtherVersion | Unregistered | JournalBody
  type BodyWriteDecision = Hold | RegisterFirst | PreserveThenWrite | Write
  type UntrustedCommitDisposition = Additive | Refuse
  type DeletionStep = Preserve | DeleteBody | RemoveEntry | DeletionDone | Stopped
  type RestoreEnding = Applied | Stale | Oversized | ReadFailed | EditedOver | InstallCancelled | Unavailable | MissingBody
  type RestoreDisposition = NothingToDo | PreserveCopy | PreserveThenRetire
  type CleanupEntryState = Unreferenced | Referenced
  type CleanupBodyIdentity = Missing | Inspected | Changed | NotRegularFile | Uncertain
  type CleanupFacts = {
    manifestTrusted: bool, pathMatches: bool, entry: CleanupEntryState,
    writeGuardHeld: bool, identity: CleanupBodyIdentity,
  }
  type OrphanBodyDecision = Delete | AlreadyAbsent | Retain
  type Completeness = Complete | Partial

  /// `ownership`.
  pure def ownership(f: BodyFacts): BodyOwner =
    if (not(f.bodyPresent)) Nothing
    else if (f.restorePending) Unseen
    else if (f.backingStale or f.entry == Superseded) OtherVersion
    else if (f.entry == Absent) Unregistered
    else JournalBody

  /// `must_preserve_before_replacing`.
  pure def mustPreserveBeforeReplacing(o: BodyOwner): bool = o == Unseen or o == OtherVersion

  /// `registration_required`.
  pure def registrationRequired(fileBacked: bool, registered: bool, trusted: bool): bool =
    fileBacked and (not(registered) or not(trusted))

  /// `may_write_after_registration`.
  pure def mayWriteAfterRegistration(required: bool, committed: bool): bool = not(required) or committed

  /// `body_write_decision`.
  pure def bodyWriteDecision(o: BodyOwner, needed: bool): BodyWriteDecision =
    if (o == Unseen) Hold
    else if (needed) RegisterFirst
    else if (o == OtherVersion) PreserveThenWrite
    else Write

  /// `untrusted_commit_disposition`.
  pure def untrustedCommitDisposition(insertOnly: bool, replacementAllowed: bool): UntrustedCommitDisposition =
    if (insertOnly and replacementAllowed) Additive else Refuse

  /// `deletion_start`.
  pure def deletionStart(o: BodyOwner): DeletionStep =
    if (mustPreserveBeforeReplacing(o)) Preserve else DeleteBody

  /// `next_deletion_step`.
  pure def nextDeletionStep(s: DeletionStep, succeeded: bool): DeletionStep = match s {
    | Preserve => if (succeeded) DeleteBody else Stopped
    | DeleteBody => if (succeeded) RemoveEntry else Stopped
    | RemoveEntry => if (succeeded) DeletionDone else Stopped
    | DeletionDone => DeletionDone
    | Stopped => Stopped
  }

  /// `startup_may_retire_stale`.
  pure def startupMayRetireStale(trusted: bool): bool = trusted

  /// `unapplied_restore_disposition`.
  pure def unappliedRestoreDisposition(e: RestoreEnding): RestoreDisposition = match e {
    | Stale => PreserveThenRetire
    | Oversized => PreserveCopy
    | ReadFailed => PreserveCopy
    | EditedOver => PreserveCopy
    | InstallCancelled => PreserveCopy
    | Unavailable => PreserveCopy
    | Applied => NothingToDo
    | MissingBody => NothingToDo
  }

  /// `orphan_body_decision`, collapsed to the decision (the retention reason
  /// is not observed by the harness).
  pure def orphanBodyDecision(f: CleanupFacts): OrphanBodyDecision =
    if (not(f.manifestTrusted)) Retain
    else if (not(f.pathMatches)) Retain
    else if (f.entry == Referenced) Retain
    else if (not(f.writeGuardHeld)) Retain
    else match f.identity {
      | Missing => AlreadyAbsent
      | Inspected => Delete
      | Changed => Retain
      | NotRegularFile => Retain
      | Uncertain => Retain
    }

  /// `commit_authority(..).is_trusted()`.
  pure def commitAuthorityTrusted(c: Completeness, durablyWritten: bool): bool =
    c == Complete and durablyWritten

  // --- 2. The harness's environment (kani_proofs.rs) ------------------------

  type DiskEntry = { present: bool, backing: int }
  type Deletion = NoDeletion | Deleting(DeletionStep)
  type Editor = {
    open: bool,
    content: int,
    dirty: bool,
    token: bool,
    written: int,
    restorePending: bool,
    deletion: Deletion,
    preserveQueued: bool,
    knownEntry: bool,
    cleanupCandidate: int,
  }
  type Window = { editors: int -> Editor, running: bool, trusted: bool, lastReconciliationComplete: bool }
  type Journal = {
    hasOther: bool,
    other: Window,
    entry: int -> DiskEntry,
    body: int -> int,
    backing: int -> int,
    preserved: Set[int],
    editors: int -> Editor,
    running: bool,
    trusted: bool,
    lastReconciliationComplete: bool,
    accepted: Set[int],
    resolved: Set[int],
    ancestors: int -> Set[int],
    nextContent: int,
    edits: int,
    // The inline `assert!` in `WriteBody`: "a body was written without an entry".
    bodyWithoutEntry: bool,
    // The inline S2 `assert!` in `ExecCleanup`.
    cleanupUnsafe: bool,
  }

  type Action =
    | Edit | Register | WriteBody | Commit | Save | Discard | DeletionStepAction
    | Inspect | ExecCleanup | RestoreApply(RestoreEnding) | ExternalMtime | Crash | Startup

  pure val Closed: Editor = {
    open: false, content: 0, dirty: false, token: false, written: NoContent,
    restorePending: false, deletion: NoDeletion, preserveQueued: false,
    knownEntry: false, cleanupCandidate: NoContent,
  }
  pure val ClosedEditors: int -> Editor = Ids.mapBy(_ => Closed)
  pure val ClosedWindow: Window = {
    editors: ClosedEditors, running: false, trusted: false, lastReconciliationComplete: false,
  }

  pure def hasBody(j: Journal, id: int): bool = j.body.get(id) != NoContent

  pure def setEditor(j: Journal, id: int, e: Editor): Journal = { ...j, editors: j.editors.set(id, e) }

  /// `swap_windows`.
  pure def swapWindows(j: Journal): Journal =
    if (not(j.hasOther)) j
    else {
      ...j,
      editors: j.other.editors,
      running: j.other.running,
      trusted: j.other.trusted,
      lastReconciliationComplete: j.other.lastReconciliationComplete,
      other: {
        editors: j.editors, running: j.running, trusted: j.trusted,
        lastReconciliationComplete: j.lastReconciliationComplete,
      },
    }

  /// `contains`.
  pure def holds(j: Journal, holder: int, content: int): bool =
    holder != 0 and holder != NoContent and j.ancestors.get(holder).contains(content)

  /// `inventory_complete_with`.
  pure def inventoryCompleteWith(j: Journal, entries: int -> DiskEntry): bool =
    Ids.forall(id => j.body.get(id) == NoContent or entries.get(id).present)

  /// `inventory_complete`.
  pure def inventoryComplete(j: Journal): bool = inventoryCompleteWith(j, j.entry)

  /// `window_body_facts`: `BodyFacts::from_window_view`.
  pure def windowBodyFacts(j: Journal, id: int): BodyFacts = {
    val e = j.editors.get(id)
    {
      bodyPresent: e.restorePending or e.knownEntry,
      entry: if (e.knownEntry) Current else Absent,
      restorePending: e.restorePending,
      backingStale: false,
    }
  }

  /// `preserve_body`.
  pure def preserveBody(j: Journal, id: int): Journal =
    if (hasBody(j, id)) { ...j, preserved: j.preserved.union(Set(j.body.get(id))) } else j

  pure def doEdit(j: Journal, id: int): Journal = {
    val e = j.editors.get(id)
    if (e.open and j.edits < MaxEdits) {
      val c = j.nextContent
      val j1 = {
        ...j,
        nextContent: c + 1,
        edits: j.edits + 1,
        ancestors: j.ancestors.set(c, Set(c).union(j.ancestors.get(e.content))),
      }
      setEditor(j1, id, { ...e, content: c, dirty: true, token: false })
    } else j
  }

  pure def doRegister(j: Journal, id: int, fault: bool): Journal = {
    val e = j.editors.get(id)
    if (not(e.open) or not(e.dirty) or e.token or e.deletion != NoDeletion) j
    else {
      val needed = registrationRequired(true, e.knownEntry, j.trusted)
      val decision = bodyWriteDecision(ownership(windowBodyFacts(j, id)), needed)
      if (decision == Hold) j
      else if (decision == Write or decision == PreserveThenWrite) setEditor(j, id, { ...e, token: true })
      else if (fault) j
      else {
        // RegisterFirst: the service's own ownership check against disk.
        val en = j.entry.get(id)
        val superseded = en.present and en.backing != j.backing.get(id)
        val owner = ownership({
          bodyPresent: hasBody(j, id),
          entry: if (not(en.present)) Absent else if (superseded) Superseded else Current,
          restorePending: false,
          backingStale: false,
        })
        val j1 = if (bodyWriteDecision(owner, false) == PreserveThenWrite) preserveBody(j, id) else j
        val complete = inventoryComplete(j1)
        val insert = not(en.present)
        val committed = if (complete) true else untrustedCommitDisposition(true, true) == Additive
        if (not(committed)) j1
        else {
          val j2 = if (insert) { ...j1, entry: j1.entry.set(id, { present: true, backing: j1.backing.get(id) }) } else j1
          val completeness = if (complete) Complete else Partial
          val trusted = commitAuthorityTrusted(completeness, complete)
          val j3 = {
            ...j2,
            trusted: trusted,
            lastReconciliationComplete: if (trusted) true else j2.lastReconciliationComplete,
          }
          setEditor(j3, id, { ...e, knownEntry: true, token: mayWriteAfterRegistration(true, true) })
        }
      }
    }
  }

  pure def doWriteBody(j: Journal, id: int, fault: bool): Journal = {
    val e = j.editors.get(id)
    if (not(e.open) or not(e.dirty) or not(e.token)) j
    else {
      val needed = registrationRequired(true, e.knownEntry, j.trusted)
      if (bodyWriteDecision(ownership(windowBodyFacts(j, id)), needed) == Hold) j
      else {
        val j1 = setEditor(j, id, { ...e, token: false })
        if (fault) j1
        else {
          // The token is the proof: no body without an entry.
          val j2 = { ...j1, bodyWithoutEntry: j1.bodyWithoutEntry or not(j1.entry.get(id).present) }
          val j3 = { ...j2, body: j2.body.set(id, e.content) }
          setEditor(j3, id, { ...j3.editors.get(id), written: e.content })
        }
      }
    }
  }

  pure def doCommit(j: Journal, id: int, fault: bool): Journal = {
    val e = j.editors.get(id)
    if (e.written == NoContent) j
    else {
      val written = e.written
      val j1 = setEditor(j, id, { ...e, written: NoContent })
      if (fault) j1
      else if (not(inventoryComplete(j1))) { ...j1, trusted: false }
      else {
        val j2 = {
          ...j1,
          entry: j1.entry.set(id, { present: true, backing: j1.backing.get(id) }),
          trusted: commitAuthorityTrusted(Complete, true),
          lastReconciliationComplete: true,
        }
        if (j2.body.get(id) == written) {
          val j3 = { ...j2, accepted: j2.accepted.union(Set(written)) }
          if (e.content == written) setEditor(j3, id, { ...j3.editors.get(id), dirty: false }) else j3
        } else j2
      }
    }
  }

  pure def doSaveOrDiscard(j: Journal, id: int, save: bool): Journal = {
    val e = j.editors.get(id)
    if (not(e.open) or e.deletion != NoDeletion) j
    else {
      // The user resolves exactly the work they can see.
      val j1 = { ...j, resolved: j.resolved.union(j.ancestors.get(e.content)) }
      val j2 = if (save) { ...j1, backing: j1.backing.set(id, j1.backing.get(id) + 1) } else j1
      val start = deletionStart(ownership(windowBodyFacts(j2, id)))
      setEditor(j2, id, {
        ...e,
        dirty: false, token: false, written: NoContent, content: 0, knownEntry: false,
        preserveQueued: start == Preserve,
        deletion: Deleting(start),
      })
    }
  }

  /// `run_deletion_step`.
  pure def doDeletionStep(j: Journal, id: int, fault: bool): Journal = {
    match j.editors.get(id).deletion {
      | NoDeletion => j
      | Deleting(s) =>
        val succeeded = not(fault)
        val j1 = match s {
          | Preserve =>
            if (succeeded) {
              val jp = preserveBody(j, id)
              setEditor(jp, id, { ...jp.editors.get(id), preserveQueued: false })
            } else j
          | DeleteBody => if (succeeded) { ...j, body: j.body.set(id, NoContent) } else j
          | RemoveEntry =>
            if (succeeded) {
              // A reconciling commit: the removal lands only with a complete
              // inventory, and then the journal is trusted.
              val after = j.entry.set(id, { ...j.entry.get(id), present: false })
              if (inventoryCompleteWith(j, after))
                { ...j, entry: after, trusted: commitAuthorityTrusted(Complete, true), lastReconciliationComplete: true }
              else { ...j, trusted: false }
            } else j
          | DeletionDone => j
          | Stopped => j
        }
        val e1 = j1.editors.get(id)
        val following = nextDeletionStep(s, succeeded)
        // A stopped deletion keeps its tombstone and retries from the start.
        val deletion = match following {
          | DeletionDone => NoDeletion
          | Stopped => Deleting(if (e1.preserveQueued) Preserve else DeleteBody)
          | _ => Deleting(following)
        }
        setEditor(j1, id, { ...e1, deletion: deletion })
    }
  }

  pure def doInspect(j: Journal, id: int): Journal = {
    val e = j.editors.get(id)
    if (j.trusted and not(e.knownEntry) and hasBody(j, id)) setEditor(j, id, { ...e, cleanupCandidate: j.body.get(id) })
    else j
  }

  pure def doExecCleanup(j: Journal, id: int, fault: bool): Journal = {
    val e = j.editors.get(id)
    if (e.cleanupCandidate == NoContent) j
    else {
      val inspected = e.cleanupCandidate
      val j1 = setEditor(j, id, { ...e, cleanupCandidate: NoContent })
      val b = j1.body.get(id)
      val facts = {
        manifestTrusted: j1.trusted,
        pathMatches: true,
        entry: if (j1.entry.get(id).present) Referenced else Unreferenced,
        // Body writes hold the same guard, so none is in flight.
        writeGuardHeld: true,
        identity: if (b == NoContent) Missing else if (b == inspected) Inspected else Changed,
      }
      if (orphanBodyDecision(facts) == Delete and not(fault)) {
        // S2: the deleted body is unreferenced and revalidated.
        val safe = not(j1.entry.get(id).present) and b == inspected
        { ...j1, cleanupUnsafe: j1.cleanupUnsafe or not(safe), body: j1.body.set(id, NoContent) }
      } else j1
    }
  }

  /// `RestoreApply`. `chosen` is the harness's `kani::any()` ending, used
  /// only when the body exists and is not stale.
  pure def doRestoreApply(j: Journal, id: int, fault: bool, chosen: RestoreEnding): Journal = {
    val e = j.editors.get(id)
    if (not(e.restorePending)) j
    else {
      val ending =
        if (not(hasBody(j, id))) MissingBody
        else if (j.entry.get(id).backing != j.backing.get(id)) Stale
        else chosen
      val e1 = { ...e, restorePending: false }
      val j1 = setEditor(j, id, e1)
      match unappliedRestoreDisposition(ending) {
        | NothingToDo =>
          if (ending == Applied and hasBody(j1, id)) setEditor(j1, id, { ...e1, content: j1.body.get(id), dirty: false })
          else j1
        | PreserveCopy =>
          if (not(fault)) preserveBody(j1, id)
          // The hold stays until the copy exists; production retries the
          // copy on each autosave tick.
          else setEditor(j1, id, { ...e1, restorePending: true })
        | PreserveThenRetire =>
          setEditor(j1, id, { ...e1, preserveQueued: true, knownEntry: false, deletion: Deleting(Preserve) })
      }
    }
  }

  /// `startup`: reconcile, then reopen the session.
  pure def startup(j: Journal, fault: bool): Journal = {
    // Reconciliation sets unattributable bodies aside (moves them).
    val setAside = Ids.filter(id => hasBody(j, id) and not(j.entry.get(id).present))
    val j1 = {
      ...j,
      running: true,
      preserved: j.preserved.union(setAside.map(id => j.body.get(id))),
      body: j.body.keys().mapBy(id => if (setAside.contains(id)) NoContent else j.body.get(id)),
    }
    val complete = inventoryComplete(j1)
    val completeness = if (complete) Complete else Partial
    val trusted = commitAuthorityTrusted(completeness, not(fault))
    val j2 = {
      ...j1,
      trusted: trusted,
      lastReconciliationComplete: if (trusted) complete else j1.lastReconciliationComplete,
    }
    {
      ...j2,
      editors: Ids.mapBy(id =>
        if (not(j2.entry.get(id).present)) j2.editors.get(id)
        else {
          val opened = { ...Closed, open: true, knownEntry: true }
          if (not(hasBody(j2, id))) opened
          else if (j2.entry.get(id).backing != j2.backing.get(id) and startupMayRetireStale(j2.trusted))
            { ...opened, knownEntry: false, preserveQueued: true, deletion: Deleting(Preserve) }
          else { ...opened, restorePending: true }
        }
      ),
    }
  }

  /// `step`.
  pure def stepJournal(j: Journal, a: Action, id: int, fault: bool): Journal =
    if (not(j.running) and a != Startup) j
    else match a {
      | Edit => doEdit(j, id)
      | Register => doRegister(j, id, fault)
      | WriteBody => doWriteBody(j, id, fault)
      | Commit => doCommit(j, id, fault)
      | Save => doSaveOrDiscard(j, id, true)
      | Discard => doSaveOrDiscard(j, id, false)
      | DeletionStepAction => doDeletionStep(j, id, fault)
      | Inspect => doInspect(j, id)
      | ExecCleanup => doExecCleanup(j, id, fault)
      | RestoreApply(ending) => doRestoreApply(j, id, fault, ending)
      | ExternalMtime => { ...j, backing: j.backing.set(id, j.backing.get(id) + 1) }
      | Crash => { ...j, running: false, trusted: false, editors: ClosedEditors }
      | Startup => if (not(j.running)) startup(j, fault) else j
    }

  /// `step_as`.
  pure def stepAs(j: Journal, actorB: bool, a: Action, id: int, fault: bool): Journal =
    if (actorB and j.hasOther) swapWindows(stepJournal(swapWindows(j), a, id, fault))
    else stepJournal(j, a, id, fault)

  // --- Invariants ------------------------------------------------------------

  /// S1: every accepted, unresolved content is recoverable.
  pure def acceptanceDurable(j: Journal): bool =
    0.to(j.nextContent - 1).forall(c =>
      (c >= 1 and j.accepted.contains(c) and not(j.resolved.contains(c))) implies {
        val onDisk = j.preserved.contains(c)
          or Ids.exists(id => j.body.get(id) != NoContent and holds(j, j.body.get(id), c))
          or j.preserved.exists(kept => holds(j, kept, c))
        val inEditor = (j.running and Ids.exists(id => j.editors.get(id).open and holds(j, j.editors.get(id).content, c)))
          or (j.hasOther and j.other.running
              and Ids.exists(id => j.other.editors.get(id).open and holds(j, j.other.editors.get(id).content, c)))
        onDisk or inEditor
      }
    )

  /// S3 for the active window.
  pure def deleteOrdered(j: Journal): bool =
    Ids.forall(id => j.editors.get(id).deletion != NoDeletion implies not(hasBody(j, id) and not(j.entry.get(id).present)))

  /// S4 for the active window.
  pure def trustSound(j: Journal): bool =
    (j.running and j.trusted) implies (j.lastReconciliationComplete and inventoryComplete(j))

  /// `assert_invariants`, split so a violation names the property.
  pure def s1(j: Journal): bool = acceptanceDurable(j)
  pure def s3(j: Journal): bool = deleteOrdered(j) and (not(j.hasOther) or deleteOrdered(swapWindows(j)))
  pure def s4(j: Journal): bool = trustSound(j) and (not(j.hasOther) or trustSound(swapWindows(j)))

  pure val RestoreEndingsChosen = Set(Applied, Oversized, ReadFailed, EditedOver, InstallCancelled, Unavailable)
  pure val PlainActions = Set(Edit, Register, WriteBody, Commit, Save, Discard, DeletionStepAction,
                              Inspect, ExecCleanup, ExternalMtime, Crash, Startup)
  pure val Actions = PlainActions.union(RestoreEndingsChosen.map(e => RestoreApply(e)))

  /// `previous_session` for one choice of its `kani::any()` bits.
  pure def previousSession(entryPresent: int -> bool, bodyPresent: int -> bool, backingMoved: int -> bool): Journal = {
    hasOther: TwoProcesses,
    other: ClosedWindow,
    entry: Ids.mapBy(id => { present: entryPresent.get(id), backing: 0 }),
    body: Ids.mapBy(id => if (bodyPresent.get(id)) 1 + id else NoContent),
    backing: Ids.mapBy(id => if (backingMoved.get(id)) 1 else 0),
    preserved: Set(),
    editors: ClosedEditors,
    running: false,
    trusted: false,
    lastReconciliationComplete: false,
    // Everything a previous session left on disk was promised.
    accepted: Ids.filter(id => bodyPresent.get(id)).map(id => 1 + id),
    resolved: Set(),
    ancestors: 0.to(IdCount + MaxEdits).mapBy(c => if (c >= 1 and c <= IdCount) Set(c) else Set()),
    nextContent: 1 + IdCount,
    edits: 0,
    bodyWithoutEntry: false,
    cleanupUnsafe: false,
  }
}

/// The harness loop over `journal_defs`: its state variables, bounds, and
/// properties.
module journal {
  const MaxSteps: int
  const TwoProc: bool
  const NIds: int
  import journal_defs(TwoProcesses = TwoProc, IdCount = NIds).*

  // --- State machine -----------------------------------------------------------

  var j: Journal
  /// Actions taken after the startup(s); the harness's loop counter.
  var n: int
  /// For trace readability: the last action, its actor, id, and fault.
  var last: { actorB: bool, act: Action, id: int, fault: bool }

  /// The previous session followed by the startup(s) before the loop.
  action initFrom(previous: Journal, startupFault: bool): bool = all {
    j' = if (TwoProcesses)
           // K8: both startups without faults, A then B.
           stepAs(stepAs(previous, false, Startup, 0, false), true, Startup, 0, false)
         else stepJournal(previous, Startup, 0, startupFault),
    n' = 0,
    last' = { actorB: false, act: Startup, id: 0, fault: if (TwoProcesses) false else startupFault },
  }

  action init = {
    nondet entryPresent = Ids.setOfMaps(Set(true, false)).oneOf()
    nondet bodyPresent = Ids.setOfMaps(Set(true, false)).oneOf()
    nondet backingMoved = Ids.setOfMaps(Set(true, false)).oneOf()
    nondet startupFault = Set(true, false).oneOf()
    initFrom(previousSession(entryPresent, bodyPresent, backingMoved), startupFault)
  }

  /// One loop iteration with its choices fixed (for `run` replays).
  action stepWith(actorB: bool, a: Action, id: int, fault: bool): bool = all {
    n < MaxSteps,
    j' = stepAs(j, actorB, a, id, fault),
    n' = n + 1,
    last' = { actorB: actorB, act: a, id: id, fault: fault },
  }

  action step = all {
    n < MaxSteps,
    nondet actorB = (if (TwoProcesses) Set(false, true) else Set(false)).oneOf()
    nondet a = Actions.oneOf()
    nondet id = Ids.oneOf()
    nondet fault = Set(true, false).oneOf()
    stepWith(actorB, a, id, fault),
  }

  val S1 = s1(j)
  val S2 = not(j.cleanupUnsafe)
  val S3 = s3(j)
  val S4 = s4(j)
  val NoBodyWithoutEntry = not(j.bodyWithoutEntry)
  val invariants = S1 and S2 and S3 and S4 and NoBodyWithoutEntry
}

// --- Instances: the bounds under test --------------------------------------

/// `journal_invariants_hold_under_crashes`: one process, 8 actions.
module k3_8 { import journal(MaxSteps = 8, TwoProc = false, NIds = 3).* }
module k3_10 { import journal(MaxSteps = 10, TwoProc = false, NIds = 3).* }
/// `a_second_writer_breaks_the_journal_invariants` at the CI pin (6) and at
/// the programme record's 8.
module k8_6 { import journal(MaxSteps = 6, TwoProc = true, NIds = 3).* }
module k8_8 { import journal(MaxSteps = 8, TwoProc = true, NIds = 3).* }
module k8_10 { import journal(MaxSteps = 10, TwoProc = true, NIds = 3).* }
module k8_12 { import journal(MaxSteps = 12, TwoProc = true, NIds = 3).* }

/// The two K8 counterexamples decoded in the programme record, replayed
/// step for step (`quint test --main k8_traces`), plus a one-process control.
module k8_traces {
  import journal(MaxSteps = 8, TwoProc = true, NIds = 3).*
  import journal_defs(TwoProcesses = true, IdCount = 3).*

  /// Id 0 has a manifest entry and no body; ids 1 and 2 have neither.
  pure val EntryOnly0 = previousSession(
    Map(0 -> true, 1 -> false, 2 -> false),
    Map(0 -> false, 1 -> false, 2 -> false),
    Map(0 -> false, 1 -> false, 2 -> false),
  )
  pure val A = false
  pure val B = true

  /// S1, accepted work lost: A edits, writes, and commits; A crashes; B,
  /// whose tab still shows the older content, discards and deletes the body.
  run s1LossTest =
    initFrom(EntryOnly0, false)
      .then(stepWith(A, Edit, 0, false))
      .then(stepWith(A, Register, 0, false))
      .then(stepWith(A, WriteBody, 0, false))
      .then(stepWith(A, Commit, 0, false))
      .expect(S1 and j.accepted.contains(4))
      .then(stepWith(A, Crash, 0, false))
      .then(stepWith(B, Discard, 0, false))
      .then(stepWith(B, DeletionStepAction, 0, false))
      .expect(not(S1))

  /// A body written without an entry, the 8-action form recorded in the
  /// programme record (two filler steps included).
  run bodyWithoutEntry8Test =
    initFrom(EntryOnly0, false)
      .then(stepWith(B, Edit, 0, false))
      .then(stepWith(A, Discard, 0, false))
      .then(stepWith(B, Register, 0, false))
      .then(stepWith(A, DeletionStepAction, 0, false))
      .then(stepWith(B, ExternalMtime, 0, false))
      .then(stepWith(B, RestoreApply(Applied), 0, false))
      .then(stepWith(A, DeletionStepAction, 0, false))
      .expect(NoBodyWithoutEntry)
      .then(stepWith(B, WriteBody, 0, false))
      .expect(not(NoBodyWithoutEntry))

  /// The same without the two fillers: six actions, the CI pin.
  run bodyWithoutEntry6Test =
    initFrom(EntryOnly0, false)
      .then(stepWith(B, Edit, 0, false))
      .then(stepWith(A, Discard, 0, false))
      .then(stepWith(B, Register, 0, false))
      .then(stepWith(A, DeletionStepAction, 0, false))
      .then(stepWith(A, DeletionStepAction, 0, false))
      .then(stepWith(B, WriteBody, 0, false))
      .expect(not(NoBodyWithoutEntry))
}
