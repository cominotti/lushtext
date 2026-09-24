// -*- mode: Bluespec; -*-
// SPDX-License-Identifier: GPL-3.0-or-later
//
// T3: a design sketch of the N6 inter-process data-directory lock, in Quint.
// The same sketch as formal/evaluation/tlaplus/DataDirLock.tla; see its
// header for the protocol. DISPOSABLE EVALUATION MODEL, NOT A MAINTAINED
// PROPERTY, and not a design decision: N6 stays dormant until axiom A6 is
// breached, and its final core would be Kani-checked Rust.
//
// Two processes, A and B (the journal's first and second window), share one
// data directory and take flock(LOCK_EX | LOCK_NB) on a lock file at startup.
// The instance modules at the end fix the variant under test.
module data_dir_lock {
  /// "ReadOnly" or "Refuse": what a second instance does when the lock is taken.
  const Variant: str
  /// "flock" (released by the kernel on death), "pidfile" (an O_EXCL file a
  /// crash leaves behind), or "none" (today, K8).
  const LockKind: str
  /// Whether a read-only instance that wins the lock re-runs startup
  /// reconciliation, or keeps the journal view it built while read-only.
  const UpgradeReconciles: bool
  /// Run the T2 journal under the lock (safety) or drop it (liveness).
  const WithJournal: bool
  const MaxSteps: int
  const MaxCrashes: int

  import journal_defs(TwoProcesses = true, IdCount = 1).* from "./journal"

  type Mode = Down | Starting | Writer | ReadOnly
  pure val Procs = Set("A", "B")
  pure val NoOne = "nobody"
  pure def other(p: str): str = if (p == "A") "B" else "A"
  pure def isB(p: str): bool = p == "B"

  var mode: str -> Mode
  var lock: str
  var crashes: str -> int
  var jnl: Journal
  var steps: int
  /// Ghost: a read-only instance's unsaved edits were thrown away.
  var roDropped: bool

  // --- journal plumbing -------------------------------------------------------

  pure def asP(jj: Journal, p: str, a: Action, id: int, fault: bool): Journal = stepAs(jj, isB(p), a, id, fault)
  pure def gone(jj: Journal, p: str): Journal = asP(jj, p, Crash, 0, false)
  pure def writerStartup(jj: Journal, p: str): Journal = asP(jj, p, Startup, 0, false)

  /// A read-only instance opens the session from what disk shows, but writes
  /// nothing: it keeps Startup's editors and drops every disk effect.
  pure def readOnlyView(jj: Journal, p: str): Journal = {
    val started = writerStartup(jj, p)
    if (isB(p)) { ...jj, other: { ...started.other, trusted: false } }
    else { ...jj, editors: started.editors, running: true, trusted: false,
           lastReconciliationComplete: started.lastReconciliationComplete }
  }

  /// A read-only instance's edits exist only in its buffers (it may not write
  /// drafts), so exiting or upgrading with a reconciling restart drops them.
  pure def hasUnsavedEdits(jj: Journal, p: str): bool = {
    val editors = if (isB(p)) jj.other.editors else jj.editors
    WithJournal and Ids.exists(id => editors.get(id).dirty)
  }

  pure def upgrade(jj: Journal, p: str): Journal =
    if (UpgradeReconciles) writerStartup(gone(jj, p), p) else jj

  /// The journal before either process starts: the previous session with
  /// every `kani::any()` bit chosen.
  action init = {
    nondet entryPresent = Ids.setOfMaps(Set(true, false)).oneOf()
    nondet bodyPresent = Ids.setOfMaps(Set(true, false)).oneOf()
    nondet backingMoved = Ids.setOfMaps(Set(true, false)).oneOf()
    all {
      mode' = Procs.mapBy(_ => Down),
      lock' = NoOne,
      crashes' = Procs.mapBy(_ => 0),
      steps' = 0,
      roDropped' = false,
      jnl' = if (WithJournal) previousSession(entryPresent, bodyPresent, backingMoved)
             else previousSession(Ids.mapBy(_ => false), Ids.mapBy(_ => false), Ids.mapBy(_ => false)),
    }
  }

  // --- the lock protocol ------------------------------------------------------

  action launch(p: str): bool = all {
    mode.get(p) == Down,
    mode' = mode.set(p, Starting),
    lock' = lock, crashes' = crashes, jnl' = jnl, steps' = steps, roDropped' = roDropped,
  }

  /// flock(LOCK_EX | LOCK_NB) succeeds ("none": there is no lock).
  action acquire(p: str): bool = all {
    mode.get(p) == Starting,
    lock == NoOne or LockKind == "none",
    mode' = mode.set(p, Writer),
    lock' = if (LockKind == "none") lock else p,
    jnl' = if (WithJournal) writerStartup(jnl, p) else jnl,
    crashes' = crashes, steps' = steps, roDropped' = roDropped,
  }

  action busy(p: str): bool = all {
    mode.get(p) == Starting,
    lock != NoOne and LockKind != "none",
    mode' = mode.set(p, if (Variant == "ReadOnly") ReadOnly else Down),
    jnl' = if (WithJournal and Variant == "ReadOnly") readOnlyView(jnl, p) else jnl,
    lock' = lock, crashes' = crashes, steps' = steps, roDropped' = roDropped,
  }

  /// A read-only instance retries the lock (a timer, or an inotify on the file).
  action retry(p: str): bool = all {
    mode.get(p) == ReadOnly,
    lock == NoOne,
    mode' = mode.set(p, Writer),
    lock' = p,
    jnl' = if (WithJournal) upgrade(jnl, p) else jnl,
    roDropped' = (roDropped or (UpgradeReconciles and hasUnsavedEdits(jnl, p))),
    crashes' = crashes, steps' = steps,
  }

  /// A clean exit closes the descriptor (flock) or unlinks the file (pidfile).
  action exit(p: str): bool = all {
    mode.get(p) == Writer or mode.get(p) == ReadOnly,
    mode' = mode.set(p, Down),
    lock' = if (lock == p) NoOne else lock,
    jnl' = if (WithJournal) gone(jnl, p) else jnl,
    roDropped' = (roDropped or (mode.get(p) == ReadOnly and hasUnsavedEdits(jnl, p))),
    crashes' = crashes, steps' = steps,
  }

  /// A crash: the kernel releases a flock; a pidfile stays behind.
  action crash(p: str): bool = all {
    mode.get(p) != Down,
    crashes.get(p) < MaxCrashes,
    mode' = mode.set(p, Down),
    lock' = if (lock == p and LockKind == "flock") NoOne else lock,
    crashes' = crashes.set(p, crashes.get(p) + 1),
    jnl' = if (WithJournal) gone(jnl, p) else jnl,
    steps' = steps, roDropped' = roDropped,
  }

  /// One T2 journal action; a read-only instance may only edit its buffers.
  action journalAct(p: str): bool = all {
    WithJournal,
    steps < MaxSteps,
    mode.get(p) == Writer or mode.get(p) == ReadOnly,
    nondet id = Ids.oneOf()
    nondet fault = Set(true, false).oneOf()
    nondet a = (if (mode.get(p) == Writer) Actions.exclude(Set(Crash, Startup, ExternalMtime)) else Set(Edit)).oneOf()
    jnl' = asP(jnl, p, a, id, fault),
    steps' = steps + 1,
    mode' = mode, lock' = lock, crashes' = crashes, roDropped' = roDropped,
  }

  /// The backing file changes on disk: an environment event, whatever mode
  /// either process is in (review E7 caught a writer-only first form).
  action envMtime = all {
    WithJournal,
    steps < MaxSteps,
    nondet id = Ids.oneOf()
    jnl' = { ...jnl, backing: jnl.backing.set(id, jnl.backing.get(id) + 1) },
    steps' = steps + 1,
    mode' = mode, lock' = lock, crashes' = crashes, roDropped' = roDropped,
  }

  action step = any {
    envMtime,
    nondet p = Procs.oneOf()
    any { launch(p), acquire(p), busy(p), retry(p), exit(p), crash(p), journalAct(p) },
  }

  // --- safety -------------------------------------------------------------------

  val AtMostOneWriter = Procs.filter(p => mode.get(p) == Writer).size() <= 1
  val LockNamesWriter = LockKind != "none" implies Procs.forall(p => mode.get(p) == Writer implies lock == p)
  val JournalSafe = WithJournal implies (s1(jnl) and not(jnl.cleanupUnsafe) and not(jnl.bodyWithoutEntry))
  val Safety = AtMostOneWriter and LockNamesWriter and JournalSafe
  /// An open design question, not a property any variant satisfies: S1
  /// protects committed drafts only, so it cannot see a read-only user's
  /// typing thrown away by an exit or a reconciling upgrade (review E7).
  val NoReadOnlyEditsDropped = not(roDropped)

  // --- fairness and liveness ------------------------------------------------------

  val vars = (mode, lock, crashes, jnl, steps, roDropped)

  temporal RetryWF = Procs.forall(p => weakFair(retry(p), vars))
  temporal RetrySF = Procs.forall(p => strongFair(retry(p), vars))
  temporal StartupWF = Procs.forall(p => weakFair(acquire(p), vars) and weakFair(busy(p), vars))
  temporal RelaunchWF = Procs.forall(p => weakFair(launch(p), vars))

  /// No permanent read-only wedge.
  temporal NoReadOnlyWedge = Procs.forall(p =>
    always(eventually(lock == NoOne)) implies always(eventually(mode.get(p) != ReadOnly)))

  /// Once the other process is gone for good, a read-only instance does not
  /// stay read-only: it becomes the writer, or exits or crashes itself.
  temporal ReadOnlyEventuallyLeaves = Procs.forall(p =>
    eventually(always(mode.get(other(p)) == Down))
      implies always(mode.get(p) == ReadOnly implies eventually(mode.get(p) != ReadOnly)))

  /// Refuse variant: once the other process is gone for good and the user
  /// keeps relaunching, the instance becomes the writer again and again.
  temporal RefusedEventuallyWrites = Procs.forall(p =>
    eventually(always(mode.get(other(p)) == Down)) implies always(eventually(mode.get(p) == Writer)))

  // Each check pairs a property with the fairness it assumes; `quint verify
  // --temporal` takes a property name, so the implication is written out.
  temporal liveRoSf = (StartupWF and RetrySF) implies (NoReadOnlyWedge and ReadOnlyEventuallyLeaves)
  temporal liveRoWfWedge = (StartupWF and RetryWF) implies NoReadOnlyWedge
  temporal liveRoWfEventual = (StartupWF and RetryWF) implies ReadOnlyEventuallyLeaves
  temporal liveRoNoFairness = ReadOnlyEventuallyLeaves
  temporal liveRefuse = (StartupWF and RelaunchWF) implies RefusedEventuallyWrites
  temporal liveRefuseNoFairness = RefusedEventuallyWrites
}

// --- Instances: the variants under test ---------------------------------------

module safety_ro_reconcile { import data_dir_lock(Variant = "ReadOnly", LockKind = "flock", UpgradeReconciles = true, WithJournal = true, MaxSteps = 6, MaxCrashes = 2).* }
module safety_refuse { import data_dir_lock(Variant = "Refuse", LockKind = "flock", UpgradeReconciles = true, WithJournal = true, MaxSteps = 6, MaxCrashes = 2).* }
module safety_pidfile { import data_dir_lock(Variant = "ReadOnly", LockKind = "pidfile", UpgradeReconciles = true, WithJournal = true, MaxSteps = 6, MaxCrashes = 2).* }
module safety_ro_in_place { import data_dir_lock(Variant = "ReadOnly", LockKind = "flock", UpgradeReconciles = false, WithJournal = true, MaxSteps = 6, MaxCrashes = 2).* }
module safety_no_lock { import data_dir_lock(Variant = "ReadOnly", LockKind = "none", UpgradeReconciles = true, WithJournal = true, MaxSteps = 6, MaxCrashes = 2).* }
module live_ro { import data_dir_lock(Variant = "ReadOnly", LockKind = "flock", UpgradeReconciles = true, WithJournal = false, MaxSteps = 0, MaxCrashes = 2).* }
module live_pidfile { import data_dir_lock(Variant = "ReadOnly", LockKind = "pidfile", UpgradeReconciles = true, WithJournal = false, MaxSteps = 0, MaxCrashes = 2).* }
module live_refuse { import data_dir_lock(Variant = "Refuse", LockKind = "flock", UpgradeReconciles = true, WithJournal = false, MaxSteps = 0, MaxCrashes = 2).* }
