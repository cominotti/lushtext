---------------------------- MODULE JournalLiveness ----------------------------
(***************************************************************************)
(* SPDX-License-Identifier: GPL-3.0-or-later                               *)
(*                                                                         *)
(* E9 (agent-added idea): the journal's liveness L1 as a real temporal     *)
(* property. Kani checks L1 bounded ("clean within k = 7 fault-free steps  *)
(* after a three-action prefix", a_dirty_editor_becomes_clean_without_     *)
(* faults). Here: every dirty open editor eventually becomes clean, under  *)
(* weak fairness of the fault-free autosave pass, for every behaviour, with *)
(* an adversarial environment that may fault, crash, save, discard, and    *)
(* move the backing file a bounded number of times, and edit (bounded by   *)
(* MaxEdits) at any time. DISPOSABLE EVALUATION MODEL.                     *)
(***************************************************************************)
EXTENDS Integers

CONSTANTS IdCount, EnvBudget

J == INSTANCE Journal WITH TwoProcesses <- FALSE, MaxSteps <- 0, j <- 0, n <- 0, last <- 0

Ids == 0..(IdCount - 1)

VARIABLES jl, budget
vars == <<jl, budget>>

Init ==
  \E entryPresent, bodyPresent, backingMoved \in [Ids -> BOOLEAN], startupFault \in BOOLEAN :
    /\ jl = J!StepJournal(J!PreviousSession(entryPresent, bodyPresent, backingMoved),
                          "Startup", 0, startupFault, "Applied")
    /\ budget = EnvBudget

\* The environment: any action, faults included, while its budget lasts.
Env ==
  /\ budget > 0
  /\ \E id \in Ids, fault \in BOOLEAN :
       \/ \E a \in J!Actions \ {"RestoreApply"} : jl' = J!StepJournal(jl, a, id, fault, "Applied")
       \/ \E e \in J!ChosenEndings : jl' = J!StepJournal(jl, "RestoreApply", id, fault, e)
  /\ budget' = budget - 1

\* The user keeps typing; MaxEdits bounds it inside the journal model.
Edit == \E id \in Ids : jl' = J!StepJournal(jl, "Edit", id, FALSE, "Applied") /\ UNCHANGED budget

\* The production pass order of the Kani liveness harness: resolve a pending
\* restore, drain the serialized deletion, then commit, write, register; and
\* restart a crashed process.
PassAction(e) ==
  IF e.restorePending THEN "RestoreApply"
  ELSE IF e.deletion # "None" THEN "DeletionStep"
  ELSE IF e.written # J!None THEN "Commit"
  ELSE IF e.token THEN "WriteBody"
  ELSE "Register"

Pass(id) ==
  /\ \E ending \in J!ChosenEndings :
       jl' = IF ~jl.running THEN J!StepJournal(jl, "Startup", 0, FALSE, "Applied")
             ELSE J!StepJournal(jl, PassAction(jl.editors[id]), id, FALSE, ending)
  /\ UNCHANGED budget

Next == Env \/ Edit \/ \E id \in Ids : Pass(id)

Spec == Init /\ [][Next]_vars
FairSpec == Spec /\ \A id \in Ids : WF_vars(Pass(id))

Dirty(id) == jl.editors[id].open /\ jl.editors[id].dirty

\* L1 without a bound.
DirtyEventuallyClean == \A id \in Ids : [](Dirty(id) ~> ~Dirty(id))
=============================================================================
