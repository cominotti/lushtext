----------------------------- MODULE DataDirLock -----------------------------
(***************************************************************************)
(* SPDX-License-Identifier: GPL-3.0-or-later                               *)
(*                                                                         *)
(* T3: a design sketch of the N6 inter-process data-directory lock         *)
(* (docs/next/formal-verification-next.md, N6; K8 in                       *)
(* docs/next/formal-verification.md). DISPOSABLE EVALUATION MODEL, NOT A   *)
(* MAINTAINED PROPERTY, and not a design decision: N6 stays dormant until  *)
(* axiom A6 is breached, and its final core would be Kani-checked Rust.    *)
(*                                                                         *)
(* Two LushText processes share one data directory. At startup a process  *)
(* takes flock(LOCK_EX | LOCK_NB) on a lock file there:                    *)
(*   - it succeeds: the process is the writer and reconciles the journal;  *)
(*   - it fails: Variant "ReadOnly" opens a read-only view and keeps       *)
(*     retrying the lock; Variant "Refuse" exits.                          *)
(* The kernel releases a flock when its holder dies (LockKind "flock").    *)
(* LockKind "pidfile" is the rejected alternative, an O_EXCL lock file a   *)
(* crash leaves behind; LockKind "none" is today's K8 world.               *)
(*                                                                         *)
(* A read-only instance that wins the lock upgrades either by a fresh      *)
(* startup reconciliation (UpgradeReconciles) or in place, keeping the     *)
(* journal view it built while read-only.                                  *)
(*                                                                         *)
(* WithJournal = TRUE runs the T2 journal (Journal.tla) under the lock for *)
(* the safety checks, bounded by MaxSteps journal actions; FALSE drops it  *)
(* so the state space is finite and unbounded for the liveness checks.     *)
(***************************************************************************)
EXTENDS Integers, FiniteSets

CONSTANTS Variant, LockKind, UpgradeReconciles, WithJournal, MaxSteps, IdCount, MaxCrashes

ASSUME Variant \in {"ReadOnly", "Refuse"}
ASSUME LockKind \in {"flock", "pidfile", "none"}

Procs == {"A", "B"}
NoOne == "nobody"
Other(p) == IF p = "A" THEN "B" ELSE "A"
\* Journal.tla's second window is process B (`actorB`).
IsB(p) == p = "B"

J == INSTANCE Journal WITH TwoProcesses <- TRUE, j <- 0, n <- 0, last <- 0

VARIABLES
  mode,     \* [Procs -> {"down", "starting", "writer", "readonly"}]
  lock,     \* who the lock file names: a process, or NoOne
  crashes,  \* [Procs -> Nat], bounded by MaxCrashes (finitely many crashes)
  jnl,      \* the T2 journal record (Journal.tla), when WithJournal
  steps,    \* journal actions taken, bounded by MaxSteps
  roDropped \* ghost: a read-only instance's unsaved edits were thrown away
vars == <<mode, lock, crashes, jnl, steps, roDropped>>

\* ---- journal plumbing -------------------------------------------------------

\* A process's in-memory window, as Journal.tla's step_as sees it.
Window(jj, p) == IF IsB(p) THEN jj.other ELSE [editors |-> jj.editors, running |-> jj.running,
                                                trusted |-> jj.trusted, lrc |-> jj.lrc]
\* Act on the journal as process p.
AsP(jj, p, a, id, fault, ending) == J!StepAs(jj, IsB(p), a, id, fault, ending)
\* A process died or exited: its in-memory state is gone (Journal's Crash).
Gone(jj, p) == AsP(jj, p, "Crash", 0, FALSE, "Applied")
\* The writer's startup: reconcile the disk and reopen the session.
WriterStartup(jj, p) == AsP(jj, p, "Startup", 0, FALSE, "Applied")

\* A read-only instance opens the session from what disk shows, but writes
\* nothing: it keeps Startup's editors and drops every disk effect.
ReadOnlyView(jj, p) ==
  LET started == WriterStartup(jj, p)
      w == Window(started, p)
      view == [w EXCEPT !.trusted = FALSE]
  IN IF IsB(p) THEN [jj EXCEPT !.other = view]
     ELSE [jj EXCEPT !.editors = view.editors, !.running = TRUE, !.trusted = FALSE, !.lrc = view.lrc]

\* A read-only instance's edits exist only in its buffers (it may not write
\* drafts), so exiting or upgrading with a reconciling restart drops them.
HasUnsavedEdits(jj, p) == WithJournal /\ \E id \in 0..(IdCount - 1) : Window(jj, p).editors[id].dirty

Upgrade(jj, p) == IF UpgradeReconciles THEN WriterStartup(Gone(jj, p), p) ELSE jj

\* The journal actions each mode may take. A read-only instance may edit its
\* buffers and nothing else; everything else writes the data directory.
WriterActions == J!Actions \ {"ExternalMtime"}
ReadOnlyActions == {"Edit"}

Init ==
  /\ mode = [p \in Procs |-> "down"]
  /\ lock = NoOne
  /\ crashes = [p \in Procs |-> 0]
  /\ steps = 0
  /\ roDropped = FALSE
  /\ IF WithJournal
     THEN \E entryPresent, bodyPresent, backingMoved \in [0..(IdCount - 1) -> BOOLEAN] :
            jnl = J!PreviousSession(entryPresent, bodyPresent, backingMoved)
     ELSE jnl = 0

\* ---- the lock protocol ------------------------------------------------------

Launch(p) ==
  /\ mode[p] = "down"
  /\ mode' = [mode EXCEPT ![p] = "starting"]
  /\ UNCHANGED <<lock, crashes, jnl, steps, roDropped>>

\* flock(LOCK_EX | LOCK_NB) succeeds (LockKind "none": there is no lock).
Acquire(p) ==
  /\ mode[p] = "starting"
  /\ lock = NoOne \/ LockKind = "none"
  /\ mode' = [mode EXCEPT ![p] = "writer"]
  /\ lock' = IF LockKind = "none" THEN lock ELSE p
  /\ jnl' = IF WithJournal THEN WriterStartup(jnl, p) ELSE jnl
  /\ UNCHANGED <<crashes, steps, roDropped>>

Busy(p) ==
  /\ mode[p] = "starting"
  /\ lock # NoOne /\ LockKind # "none"
  /\ IF Variant = "ReadOnly"
     THEN /\ mode' = [mode EXCEPT ![p] = "readonly"]
          /\ jnl' = IF WithJournal THEN ReadOnlyView(jnl, p) ELSE jnl
     ELSE /\ mode' = [mode EXCEPT ![p] = "down"]
          /\ UNCHANGED jnl
  /\ UNCHANGED <<lock, crashes, steps, roDropped>>

\* A read-only instance retries the lock (a timer, or an inotify on the file).
Retry(p) ==
  /\ mode[p] = "readonly"
  /\ lock = NoOne
  /\ mode' = [mode EXCEPT ![p] = "writer"]
  /\ lock' = p
  /\ jnl' = IF WithJournal THEN Upgrade(jnl, p) ELSE jnl
  /\ roDropped' = (roDropped \/ (UpgradeReconciles /\ HasUnsavedEdits(jnl, p)))
  /\ UNCHANGED <<crashes, steps>>

\* A clean exit closes the lock file descriptor, which releases the flock;
\* a pidfile implementation unlinks the file.
Exit(p) ==
  /\ mode[p] \in {"writer", "readonly"}
  /\ mode' = [mode EXCEPT ![p] = "down"]
  /\ lock' = IF lock = p THEN NoOne ELSE lock
  /\ jnl' = IF WithJournal THEN Gone(jnl, p) ELSE jnl
  /\ roDropped' = (roDropped \/ (mode[p] = "readonly" /\ HasUnsavedEdits(jnl, p)))
  /\ UNCHANGED <<crashes, steps>>

\* A crash: the kernel releases a flock; a pidfile stays behind.
Crash(p) ==
  /\ mode[p] # "down"
  /\ crashes[p] < MaxCrashes
  /\ mode' = [mode EXCEPT ![p] = "down"]
  /\ lock' = IF lock = p /\ LockKind = "flock" THEN NoOne ELSE lock
  /\ crashes' = [crashes EXCEPT ![p] = @ + 1]
  /\ jnl' = IF WithJournal THEN Gone(jnl, p) ELSE jnl
  /\ UNCHANGED <<steps, roDropped>>

\* One T2 journal action, allowed only in the process's mode.
JournalAct(p) ==
  /\ WithJournal
  /\ steps < MaxSteps
  /\ mode[p] \in {"writer", "readonly"}
  /\ \E id \in 0..(IdCount - 1), fault \in BOOLEAN :
       \/ \E a \in (IF mode[p] = "writer" THEN WriterActions ELSE ReadOnlyActions) \ {"Crash", "Startup", "RestoreApply"} :
            jnl' = AsP(jnl, p, a, id, fault, "Applied")
       \/ /\ mode[p] = "writer"
          /\ \E ending \in J!ChosenEndings : jnl' = AsP(jnl, p, "RestoreApply", id, fault, ending)
  /\ steps' = steps + 1
  /\ UNCHANGED <<mode, lock, crashes, roDropped>>

\* The backing file changes on disk (another editor, a checkout): an
\* environment event, whatever mode either process is in. A review (E7)
\* caught the first form, which allowed it only through a writer.
EnvMtime ==
  /\ WithJournal
  /\ steps < MaxSteps
  /\ \E id \in 0..(IdCount - 1) : jnl' = [jnl EXCEPT !.backing[id] = @ + 1]
  /\ steps' = steps + 1
  /\ UNCHANGED <<mode, lock, crashes, roDropped>>

Next == EnvMtime \/ \E p \in Procs :
  Launch(p) \/ Acquire(p) \/ Busy(p) \/ Retry(p) \/ Exit(p) \/ Crash(p) \/ JournalAct(p)

Spec == Init /\ [][Next]_vars

\* ---- fairness assumptions, stated separately so each can be dropped --------

\* The read-only instance's retry timer keeps firing.
RetryWF == \A p \in Procs : WF_vars(Retry(p))
RetrySF == \A p \in Procs : SF_vars(Retry(p))
\* A launched process gets through its startup.
StartupWF == \A p \in Procs : WF_vars(Acquire(p)) /\ WF_vars(Busy(p))
\* The user relaunches a refused instance.
RelaunchWF == \A p \in Procs : WF_vars(Launch(p))

SpecNoFairness == Spec
SpecRetryWF == Spec /\ StartupWF /\ RetryWF
SpecRetrySF == Spec /\ StartupWF /\ RetrySF
SpecRefuse == Spec /\ StartupWF /\ RelaunchWF

\* ---- safety -----------------------------------------------------------------

AtMostOneWriter == Cardinality({p \in Procs : mode[p] = "writer"}) <= 1
\* The lock file names the writer (except in the "none" world).
LockNamesWriter == LockKind # "none" => \A p \in Procs : mode[p] = "writer" => lock = p

JournalSafe == WithJournal => (J!S1Holds(jnl) /\ ~jnl.cleanupUnsafe /\ ~jnl.bodyWithoutEntry)
S1 == WithJournal => J!S1Holds(jnl)
NoBodyWithoutEntry == WithJournal => ~jnl.bodyWithoutEntry
Safety == AtMostOneWriter /\ LockNamesWriter /\ JournalSafe

\* An open design question, not a property any variant here satisfies: S1
\* protects committed drafts only, so it cannot see a read-only user's
\* typing thrown away by an exit or a reconciling upgrade (review E7).
NoReadOnlyEditsDropped == ~roDropped

\* No VIEW is used: an earlier `<<mode, lock, crashes, jnl>>` view dropped
\* the journal step bound, so TLC could keep a copy with more steps used and
\* discard a later copy with fewer, silently skipping in-bound behaviours
\* (breadth-first depth here also counts lock actions). A fresh-eyes review
\* (E7) caught it.

\* ---- liveness ---------------------------------------------------------------

\* No permanent read-only wedge: if the lock is free again and again, no
\* instance stays read-only forever.
NoReadOnlyWedge == \A p \in Procs :
  ([]<>(lock = NoOne)) => []<>(mode[p] # "readonly")

\* Once the other process is gone for good, a read-only instance does not
\* stay read-only: it becomes the writer, or exits or crashes itself. (Weaker
\* than "becomes the writer"; named for what it checks, after review E7.)
ReadOnlyEventuallyLeaves == \A p \in Procs :
  (<>[](mode[Other(p)] = "down")) => [](mode[p] = "readonly" => <>(mode[p] # "readonly"))

\* Refuse variant: once the other process is gone for good and the user keeps
\* relaunching, the instance eventually becomes the writer.
RefusedEventuallyWrites == \A p \in Procs :
  (<>[](mode[Other(p)] = "down")) => []<>(mode[p] = "writer")
=============================================================================
