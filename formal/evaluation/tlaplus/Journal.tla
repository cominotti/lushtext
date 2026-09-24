------------------------------- MODULE Journal -------------------------------
(***************************************************************************)
(* SPDX-License-Identifier: GPL-3.0-or-later                               *)
(*                                                                         *)
(* T2 scale: the draft journal, one and two processes, in plain TLA+.      *)
(*                                                                         *)
(* DISPOSABLE EVALUATION MODEL, NOT A MAINTAINED PROPERTY. The maintained  *)
(* proofs are the Kani harnesses in                                        *)
(* crates/lushtext-core/src/services/draft_service/kani_proofs.rs over the *)
(* real decision core journal_core.rs. Part 1 transliterates each          *)
(* journal_core decision the harness calls; part 2 transliterates the      *)
(* harness environment (Journal, step, step_as, startup, invariants) as    *)
(* operators over one record, so a TLC trace maps onto Kani steps.         *)
(*                                                                         *)
(* MaxSteps bounds the loop like the harness's STEPS / SECOND_WRITER_STEPS;*)
(* TwoProcesses = TRUE is K8; IdCount = 3 is the harness's IDS.           *)
(***************************************************************************)
EXTENDS Integers, FiniteSets

CONSTANTS MaxSteps, TwoProcesses, IdCount

\* The harness's IDS = 3; T3 reuses these operators with fewer ids.
(*
  Apalache type aliases (comments to TLC). E5 added these and the operator
  annotations below so Apalache's Snowcat can type the record parameters.
  @typeAlias: editor = {open: Bool, content: Int, dirty: Bool, token: Bool,
    written: Int, restorePending: Bool, deletion: Str, preserveQueued: Bool,
    knownEntry: Bool, cleanupCandidate: Int};
  @typeAlias: window = {editors: Int -> $editor, running: Bool, trusted: Bool, lrc: Bool};
  @typeAlias: journal = {hasOther: Bool, other: $window,
    entry: Int -> {present: Bool, backing: Int}, body: Int -> Int,
    backing: Int -> Int, preserved: Set(Int), editors: Int -> $editor,
    running: Bool, trusted: Bool, lrc: Bool, accepted: Set(Int),
    resolved: Set(Int), ancestors: Int -> Set(Int), nextContent: Int,
    edits: Int, bodyWithoutEntry: Bool, cleanupUnsafe: Bool};
*)
Journal_aliases == TRUE

Ids == 0..(IdCount - 1)
MaxEdits == 3
None == -1

\* ---- 1. journal_core ---------------------------------------------------

\* @type: ({bodyPresent: Bool, entry: Str, restorePending: Bool, backingStale: Bool}) => Str;
Ownership(f) ==
  IF ~f.bodyPresent THEN "Nothing"
  ELSE IF f.restorePending THEN "Unseen"
  ELSE IF f.backingStale \/ f.entry = "Superseded" THEN "OtherVersion"
  ELSE IF f.entry = "Absent" THEN "Unregistered"
  ELSE "Journal"

MustPreserveBeforeReplacing(o) == o \in {"Unseen", "OtherVersion"}
RegistrationRequired(fileBacked, registered, trusted) == fileBacked /\ (~registered \/ ~trusted)
MayWriteAfterRegistration(required, committed) == ~required \/ committed

BodyWriteDecision(o, needed) ==
  IF o = "Unseen" THEN "Hold"
  ELSE IF needed THEN "RegisterFirst"
  ELSE IF o = "OtherVersion" THEN "PreserveThenWrite"
  ELSE "Write"

UntrustedCommitDisposition(insertOnly, replacementAllowed) ==
  IF insertOnly /\ replacementAllowed THEN "Additive" ELSE "Refuse"

DeletionStart(o) == IF MustPreserveBeforeReplacing(o) THEN "Preserve" ELSE "DeleteBody"

NextDeletionStep(s, ok) ==
  CASE s = "Preserve" -> IF ok THEN "DeleteBody" ELSE "Stopped"
    [] s = "DeleteBody" -> IF ok THEN "RemoveEntry" ELSE "Stopped"
    [] s = "RemoveEntry" -> IF ok THEN "Done" ELSE "Stopped"
    [] OTHER -> s

StartupMayRetireStale(trusted) == trusted

UnappliedRestoreDisposition(e) ==
  CASE e = "Stale" -> "PreserveThenRetire"
    [] e \in {"Oversized", "ReadFailed", "EditedOver", "InstallCancelled", "Unavailable"} -> "PreserveCopy"
    [] OTHER -> "Nothing"

\* Collapsed to the decision; the retention reason is not observed.
\* @type: ({manifestTrusted: Bool, pathMatches: Bool, entry: Str, writeGuardHeld: Bool, identity: Str}) => Str;
OrphanBodyDecision(f) ==
  IF ~f.manifestTrusted \/ ~f.pathMatches \/ f.entry = "Referenced" \/ ~f.writeGuardHeld
  THEN "Retain"
  ELSE CASE f.identity = "Missing" -> "AlreadyAbsent"
         [] f.identity = "Inspected" -> "Delete"
         [] OTHER -> "Retain"

CommitAuthorityTrusted(completeness, durable) == completeness = "Complete" /\ durable

\* ---- 2. the harness environment ------------------------------------------

Closed == [open |-> FALSE, content |-> 0, dirty |-> FALSE, token |-> FALSE,
           written |-> None, restorePending |-> FALSE, deletion |-> "None",
           preserveQueued |-> FALSE, knownEntry |-> FALSE, cleanupCandidate |-> None]
ClosedEditors == [id \in Ids |-> Closed]
ClosedWindow == [editors |-> ClosedEditors, running |-> FALSE, trusted |-> FALSE, lrc |-> FALSE]

\* @type: ($journal, Int) => Bool;
HasBody(j, id) == j.body[id] # None
\* @type: ($journal, Int, $editor) => $journal;
SetEditor(j, id, e) == [j EXCEPT !.editors[id] = e]

\* @type: ($journal) => $journal;
SwapWindows(j) ==
  IF ~j.hasOther THEN j
  ELSE [j EXCEPT !.editors = j.other.editors, !.running = j.other.running,
                 !.trusted = j.other.trusted, !.lrc = j.other.lrc,
                 !.other = [editors |-> j.editors, running |-> j.running,
                            trusted |-> j.trusted, lrc |-> j.lrc]]

\* @type: ($journal, Int, Int) => Bool;
Holds(j, holder, c) == holder \notin {0, None} /\ c \in j.ancestors[holder]

\* @type: ($journal, Int -> {present: Bool, backing: Int}) => Bool;
InventoryCompleteWith(j, entries) == \A id \in Ids : j.body[id] = None \/ entries[id].present
\* @type: ($journal) => Bool;
InventoryComplete(j) == InventoryCompleteWith(j, j.entry)

\* @type: ($journal, Int) => {bodyPresent: Bool, entry: Str, restorePending: Bool, backingStale: Bool};
WindowBodyFacts(j, id) ==
  LET e == j.editors[id] IN
  [bodyPresent |-> e.restorePending \/ e.knownEntry,
   entry |-> IF e.knownEntry THEN "Current" ELSE "Absent",
   restorePending |-> e.restorePending, backingStale |-> FALSE]

\* @type: ($journal, Int) => $journal;
PreserveBody(j, id) == IF HasBody(j, id) THEN [j EXCEPT !.preserved = @ \cup {j.body[id]}] ELSE j

\* @type: ($journal, Int) => $journal;
DoEdit(j, id) ==
  LET e == j.editors[id] IN
  IF e.open /\ j.edits < MaxEdits THEN
    LET c == j.nextContent IN
    [j EXCEPT !.nextContent = c + 1, !.edits = @ + 1,
              !.ancestors[c] = {c} \cup j.ancestors[e.content],
              !.editors[id] = [e EXCEPT !.content = c, !.dirty = TRUE, !.token = FALSE]]
  ELSE j

\* @type: ($journal, Int, Bool) => $journal;
DoRegister(j, id, fault) ==
  LET e == j.editors[id]
      needed == RegistrationRequired(TRUE, e.knownEntry, j.trusted)
      decision == BodyWriteDecision(Ownership(WindowBodyFacts(j, id)), needed)
  IN
  IF ~e.open \/ ~e.dirty \/ e.token \/ e.deletion # "None" THEN j
  ELSE IF decision = "Hold" THEN j
  ELSE IF decision \in {"Write", "PreserveThenWrite"} THEN SetEditor(j, id, [e EXCEPT !.token = TRUE])
  ELSE IF fault THEN j
  ELSE
    \* RegisterFirst: the service's own ownership check against disk.
    LET en == j.entry[id]
        superseded == en.present /\ en.backing # j.backing[id]
        owner == Ownership([bodyPresent |-> HasBody(j, id),
                            entry |-> IF ~en.present THEN "Absent" ELSE IF superseded THEN "Superseded" ELSE "Current",
                            restorePending |-> FALSE, backingStale |-> FALSE])
        j1 == IF BodyWriteDecision(owner, FALSE) = "PreserveThenWrite" THEN PreserveBody(j, id) ELSE j
        complete == InventoryComplete(j1)
        insert == ~en.present
        committed == IF complete THEN TRUE ELSE UntrustedCommitDisposition(TRUE, TRUE) = "Additive"
        j2 == IF insert THEN [j1 EXCEPT !.entry[id] = [present |-> TRUE, backing |-> j1.backing[id]]] ELSE j1
        trusted == CommitAuthorityTrusted(IF complete THEN "Complete" ELSE "Partial", complete)
        j3 == [j2 EXCEPT !.trusted = trusted, !.lrc = IF trusted THEN TRUE ELSE @]
    IN IF ~committed THEN j1
       ELSE SetEditor(j3, id, [e EXCEPT !.knownEntry = TRUE, !.token = MayWriteAfterRegistration(TRUE, TRUE)])

\* @type: ($journal, Int, Bool) => $journal;
DoWriteBody(j, id, fault) ==
  LET e == j.editors[id]
      needed == RegistrationRequired(TRUE, e.knownEntry, j.trusted)
  IN
  IF ~e.open \/ ~e.dirty \/ ~e.token THEN j
  ELSE IF BodyWriteDecision(Ownership(WindowBodyFacts(j, id)), needed) = "Hold" THEN j
  ELSE
    LET j1 == SetEditor(j, id, [e EXCEPT !.token = FALSE]) IN
    IF fault THEN j1
    ELSE \* The token is the proof: no body without an entry.
      [j1 EXCEPT !.bodyWithoutEntry = @ \/ ~j1.entry[id].present,
                 !.body[id] = e.content,
                 !.editors[id].written = e.content]

\* @type: ($journal, Int, Bool) => $journal;
DoCommit(j, id, fault) ==
  LET e == j.editors[id] IN
  IF e.written = None THEN j
  ELSE
    LET written == e.written
        j1 == SetEditor(j, id, [e EXCEPT !.written = None])
    IN
    IF fault THEN j1
    ELSE IF ~InventoryComplete(j1) THEN [j1 EXCEPT !.trusted = FALSE]
    ELSE
      LET j2 == [j1 EXCEPT !.entry[id] = [present |-> TRUE, backing |-> j1.backing[id]],
                           !.trusted = CommitAuthorityTrusted("Complete", TRUE), !.lrc = TRUE]
      IN IF j2.body[id] = written
         THEN LET j3 == [j2 EXCEPT !.accepted = @ \cup {written}] IN
              IF e.content = written THEN [j3 EXCEPT !.editors[id].dirty = FALSE] ELSE j3
         ELSE j2

\* @type: ($journal, Int, Bool) => $journal;
DoSaveOrDiscard(j, id, save) ==
  LET e == j.editors[id] IN
  IF ~e.open \/ e.deletion # "None" THEN j
  ELSE
    \* The user resolves exactly the work they can see.
    LET j1 == [j EXCEPT !.resolved = @ \cup j.ancestors[e.content]]
        j2 == IF save THEN [j1 EXCEPT !.backing[id] = @ + 1] ELSE j1
        start == DeletionStart(Ownership(WindowBodyFacts(j2, id)))
    IN SetEditor(j2, id, [e EXCEPT !.dirty = FALSE, !.token = FALSE, !.written = None,
                                   !.content = 0, !.knownEntry = FALSE,
                                   !.preserveQueued = (start = "Preserve"), !.deletion = start])

\* @type: ($journal, Int, Bool) => $journal;
DoDeletionStep(j, id, fault) ==
  LET s == j.editors[id].deletion
      ok == ~fault
  IN
  IF s = "None" THEN j
  ELSE
    LET j1 ==
          CASE s = "Preserve" ->
                 IF ok THEN LET jp == PreserveBody(j, id) IN [jp EXCEPT !.editors[id].preserveQueued = FALSE]
                 ELSE j
            [] s = "DeleteBody" -> IF ok THEN [j EXCEPT !.body[id] = None] ELSE j
            [] s = "RemoveEntry" ->
                 IF ok THEN
                   \* A reconciling commit: it lands only with a complete inventory.
                   LET after == [j.entry EXCEPT ![id].present = FALSE] IN
                   IF InventoryCompleteWith(j, after)
                   THEN [j EXCEPT !.entry = after, !.trusted = CommitAuthorityTrusted("Complete", TRUE), !.lrc = TRUE]
                   ELSE [j EXCEPT !.trusted = FALSE]
                 ELSE j
            [] OTHER -> j
        e1 == j1.editors[id]
        following == NextDeletionStep(s, ok)
        \* A stopped deletion keeps its tombstone and retries from the start.
        deletion == CASE following = "Done" -> "None"
                      [] following = "Stopped" -> IF e1.preserveQueued THEN "Preserve" ELSE "DeleteBody"
                      [] OTHER -> following
    IN SetEditor(j1, id, [e1 EXCEPT !.deletion = deletion])

\* @type: ($journal, Int) => $journal;
DoInspect(j, id) ==
  IF j.trusted /\ ~j.editors[id].knownEntry /\ HasBody(j, id)
  THEN [j EXCEPT !.editors[id].cleanupCandidate = j.body[id]]
  ELSE j

\* @type: ($journal, Int, Bool) => $journal;
DoExecCleanup(j, id, fault) ==
  LET e == j.editors[id] IN
  IF e.cleanupCandidate = None THEN j
  ELSE
    LET inspected == e.cleanupCandidate
        j1 == [j EXCEPT !.editors[id].cleanupCandidate = None]
        b == j1.body[id]
        facts == [manifestTrusted |-> j1.trusted, pathMatches |-> TRUE,
                  entry |-> IF j1.entry[id].present THEN "Referenced" ELSE "Unreferenced",
                  writeGuardHeld |-> TRUE,
                  identity |-> IF b = None THEN "Missing" ELSE IF b = inspected THEN "Inspected" ELSE "Changed"]
    IN IF OrphanBodyDecision(facts) = "Delete" /\ ~fault
       THEN [j1 EXCEPT !.cleanupUnsafe = @ \/ ~(~j1.entry[id].present /\ b = inspected),
                       !.body[id] = None]
       ELSE j1

\* @type: ($journal, Int, Bool, Str) => $journal;
DoRestoreApply(j, id, fault, chosen) ==
  LET e == j.editors[id] IN
  IF ~e.restorePending THEN j
  ELSE
    LET ending == IF ~HasBody(j, id) THEN "MissingBody"
                  ELSE IF j.entry[id].backing # j.backing[id] THEN "Stale"
                  ELSE chosen
        e1 == [e EXCEPT !.restorePending = FALSE]
        j1 == SetEditor(j, id, e1)
        d == UnappliedRestoreDisposition(ending)
    IN CASE d = "Nothing" ->
              IF ending = "Applied" /\ HasBody(j1, id)
              THEN SetEditor(j1, id, [e1 EXCEPT !.content = j1.body[id], !.dirty = FALSE])
              ELSE j1
         [] d = "PreserveCopy" ->
              IF ~fault THEN PreserveBody(j1, id)
              \* The hold stays until the copy exists.
              ELSE SetEditor(j1, id, [e1 EXCEPT !.restorePending = TRUE])
         [] d = "PreserveThenRetire" ->
              SetEditor(j1, id, [e1 EXCEPT !.preserveQueued = TRUE, !.knownEntry = FALSE, !.deletion = "Preserve"])

\* @type: ($journal, Bool) => $journal;
Startup(j, fault) ==
  LET setAside == {id \in Ids : HasBody(j, id) /\ ~j.entry[id].present}
      j1 == [j EXCEPT !.running = TRUE,
                      !.preserved = @ \cup {j.body[id] : id \in setAside},
                      !.body = [id \in Ids |-> IF id \in setAside THEN None ELSE j.body[id]]]
      complete == InventoryComplete(j1)
      trusted == CommitAuthorityTrusted(IF complete THEN "Complete" ELSE "Partial", ~fault)
      j2 == [j1 EXCEPT !.trusted = trusted, !.lrc = IF trusted THEN complete ELSE @]
      opened == [Closed EXCEPT !.open = TRUE, !.knownEntry = TRUE]
  IN [j2 EXCEPT !.editors =
        [id \in Ids |->
          IF ~j2.entry[id].present THEN j2.editors[id]
          ELSE IF ~HasBody(j2, id) THEN opened
          ELSE IF j2.entry[id].backing # j2.backing[id] /\ StartupMayRetireStale(j2.trusted)
               THEN [opened EXCEPT !.knownEntry = FALSE, !.preserveQueued = TRUE, !.deletion = "Preserve"]
          ELSE [opened EXCEPT !.restorePending = TRUE]]]

\* `step`. `a` is an action name; `ending` is used only by RestoreApply.
\* @type: ($journal, Str, Int, Bool, Str) => $journal;
StepJournal(j, a, id, fault, ending) ==
  IF ~j.running /\ a # "Startup" THEN j
  ELSE CASE a = "Edit" -> DoEdit(j, id)
         [] a = "Register" -> DoRegister(j, id, fault)
         [] a = "WriteBody" -> DoWriteBody(j, id, fault)
         [] a = "Commit" -> DoCommit(j, id, fault)
         [] a = "Save" -> DoSaveOrDiscard(j, id, TRUE)
         [] a = "Discard" -> DoSaveOrDiscard(j, id, FALSE)
         [] a = "DeletionStep" -> DoDeletionStep(j, id, fault)
         [] a = "Inspect" -> DoInspect(j, id)
         [] a = "ExecCleanup" -> DoExecCleanup(j, id, fault)
         [] a = "RestoreApply" -> DoRestoreApply(j, id, fault, ending)
         [] a = "ExternalMtime" -> [j EXCEPT !.backing[id] = @ + 1]
         [] a = "Crash" -> [j EXCEPT !.running = FALSE, !.trusted = FALSE, !.editors = ClosedEditors]
         [] a = "Startup" -> IF ~j.running THEN Startup(j, fault) ELSE j

\* @type: ($journal, Bool, Str, Int, Bool, Str) => $journal;
StepAs(j, actorB, a, id, fault, ending) ==
  IF actorB /\ j.hasOther THEN SwapWindows(StepJournal(SwapWindows(j), a, id, fault, ending))
  ELSE StepJournal(j, a, id, fault, ending)

\* @type: (Int -> Bool, Int -> Bool, Int -> Bool) => $journal;
PreviousSession(entryPresent, bodyPresent, backingMoved) ==
  [hasOther |-> TwoProcesses, other |-> ClosedWindow,
   entry |-> [id \in Ids |-> [present |-> entryPresent[id], backing |-> 0]],
   body |-> [id \in Ids |-> IF bodyPresent[id] THEN 1 + id ELSE None],
   backing |-> [id \in Ids |-> IF backingMoved[id] THEN 1 ELSE 0],
   preserved |-> {}, editors |-> ClosedEditors, running |-> FALSE, trusted |-> FALSE, lrc |-> FALSE,
   \* Everything a previous session left on disk was promised.
   accepted |-> {1 + id : id \in {i \in Ids : bodyPresent[i]}},
   resolved |-> {},
   ancestors |-> [c \in 0..(IdCount + MaxEdits) |-> IF c \in 1..IdCount THEN {c} ELSE {}],
   nextContent |-> 1 + IdCount, edits |-> 0, bodyWithoutEntry |-> FALSE, cleanupUnsafe |-> FALSE]

\* ---- invariants ------------------------------------------------------------

\* @type: ($journal) => Bool;
S1Holds(j) ==
  \A c \in 1..(j.nextContent - 1) :
    (c \in j.accepted /\ c \notin j.resolved) =>
      LET onDisk == \/ c \in j.preserved
                    \/ \E id \in Ids : j.body[id] # None /\ Holds(j, j.body[id], c)
                    \/ \E kept \in j.preserved : Holds(j, kept, c)
          inEditor == \/ j.running /\ \E id \in Ids : j.editors[id].open /\ Holds(j, j.editors[id].content, c)
                      \/ j.hasOther /\ j.other.running
                         /\ \E id \in Ids : j.other.editors[id].open /\ Holds(j, j.other.editors[id].content, c)
      IN onDisk \/ inEditor

\* @type: ($journal) => Bool;
DeleteOrdered(j) == \A id \in Ids : j.editors[id].deletion # "None" => ~(HasBody(j, id) /\ ~j.entry[id].present)
\* @type: ($journal) => Bool;
TrustSound(j) == (j.running /\ j.trusted) => (j.lrc /\ InventoryComplete(j))

\* ---- the state machine ------------------------------------------------------

VARIABLES j, n, last
vars == <<j, n, last>>

Actions == {"Edit", "Register", "WriteBody", "Commit", "Save", "Discard", "DeletionStep",
            "Inspect", "ExecCleanup", "RestoreApply", "ExternalMtime", "Crash", "Startup"}
\* The harness's kani::any() ending, excluding the two it derives itself.
ChosenEndings == {"Applied", "Oversized", "ReadFailed", "EditedOver", "InstallCancelled", "Unavailable"}

Init ==
  \E entryPresent, bodyPresent, backingMoved \in [Ids -> BOOLEAN], startupFault \in BOOLEAN :
    LET previous == PreviousSession(entryPresent, bodyPresent, backingMoved) IN
    /\ j = IF TwoProcesses
           \* K8: both startups without faults, A then B.
           THEN StepAs(StepAs(previous, FALSE, "Startup", 0, FALSE, "Applied"), TRUE, "Startup", 0, FALSE, "Applied")
           ELSE StepJournal(previous, "Startup", 0, startupFault, "Applied")
    /\ n = 0
    /\ last = [actorB |-> FALSE, act |-> "Startup", id |-> 0, fault |-> ~TwoProcesses /\ startupFault, ending |-> "-"]

StepWith(actorB, a, id, fault, ending) ==
  /\ n < MaxSteps
  /\ j' = StepAs(j, actorB, a, id, fault, ending)
  /\ n' = n + 1
  /\ last' = [actorB |-> actorB, act |-> a, id |-> id, fault |-> fault,
              ending |-> IF a = "RestoreApply" THEN ending ELSE "-"]

Next ==
  \E actorB \in (IF TwoProcesses THEN BOOLEAN ELSE {FALSE}), id \in Ids, fault \in BOOLEAN :
    \/ \E a \in Actions \ {"RestoreApply"} : StepWith(actorB, a, id, fault, "Applied")
    \/ \E ending \in ChosenEndings : StepWith(actorB, "RestoreApply", id, fault, ending)

Spec == Init /\ [][Next]_vars

\* TLC fingerprints states by this VIEW, dropping only `last`, which is trace
\* decoration. The step counter `n` must stay in the view: without it TLC
\* keeps the first copy of a journal it meets, and with several workers (or
\* any search that is not strictly level by level) that copy may carry a
\* larger `n` than a later one, whose remaining in-bound steps would then
\* never be explored. A fresh-eyes review (E7) caught the unsound first form.
JournalView == <<j, n>>

S1 == S1Holds(j)
S2 == ~j.cleanupUnsafe
S3 == DeleteOrdered(j) /\ (j.hasOther => DeleteOrdered(SwapWindows(j)))
S4 == TrustSound(j) /\ (j.hasOther => TrustSound(SwapWindows(j)))
NoBodyWithoutEntry == ~j.bodyWithoutEntry
Invariants == S1 /\ S2 /\ S3 /\ S4 /\ NoBodyWithoutEntry
=============================================================================
