-------------------------- MODULE MC_JournalApalache --------------------------
(* Apalache wrapper for Journal.tla (T2): the typed declarations Apalache    *)
(* needs, then INSTANCE Journal. Disposable evaluation model.                *)
(* Run: apalache-mc check --cinit=CInitK8 --init=Init --next=Next            *)
(*        --inv=Invariants --length=6 MC_JournalApalache.tla                 *)
EXTENDS Integers

\* The type aliases live in Journal.tla, next to the operators they type.

CONSTANTS
  \* @type: Int;
  MaxSteps,
  \* @type: Bool;
  TwoProcesses,
  \* @type: Int;
  IdCount

VARIABLES
  \* @type: $journal;
  j,
  \* @type: Int;
  n,
  \* @type: {actorB: Bool, act: Str, id: Int, fault: Bool, ending: Str};
  last

INSTANCE Journal

\* The Kani harness bounds. Apalache's --length bounds the steps, so MaxSteps
\* only has to be at least that.
CInitK3 == MaxSteps = 100 /\ TwoProcesses = FALSE /\ IdCount = 3
CInitK8 == MaxSteps = 100 /\ TwoProcesses = TRUE /\ IdCount = 3
================================================================================
