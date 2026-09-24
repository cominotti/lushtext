------------------------------- MODULE LockProof -------------------------------
(***************************************************************************)
(* SPDX-License-Identifier: GPL-3.0-or-later                               *)
(*                                                                         *)
(* E6: one TLAPS lemma. The lock layer of DataDirLock.tla (T3) without the *)
(* journal, with the at-most-one-writer invariant proved for every         *)
(* behaviour, any number of steps and any number of crashes: the unbounded *)
(* claim a model checker (TLC, Apalache, Kani) only checks to a bound.     *)
(* DISPOSABLE EVALUATION MODEL, NOT A MAINTAINED PROPERTY.                 *)
(*                                                                         *)
(* Check: tlapm --cleanfp LockProof.tla                                    *)
(***************************************************************************)
EXTENDS Naturals, TLAPS

CONSTANTS Variant, LockKind

ASSUME VariantOK == Variant \in {"ReadOnly", "Refuse"}
\* The "none" world (today, K8) has no lock and is excluded on purpose.
ASSUME KindOK == LockKind \in {"flock", "pidfile"}

Procs == {"A", "B"}
NoOne == "nobody"
Modes == {"down", "starting", "writer", "readonly"}

VARIABLES mode, lock, crashes
vars == <<mode, lock, crashes>>

Init ==
  /\ mode = [p \in Procs |-> "down"]
  /\ lock = NoOne
  /\ crashes = [p \in Procs |-> 0]

Launch(p) ==
  /\ mode[p] = "down"
  /\ mode' = [mode EXCEPT ![p] = "starting"]
  /\ UNCHANGED <<lock, crashes>>

Acquire(p) ==
  /\ mode[p] = "starting"
  /\ lock = NoOne
  /\ mode' = [mode EXCEPT ![p] = "writer"]
  /\ lock' = p
  /\ UNCHANGED crashes

Busy(p) ==
  /\ mode[p] = "starting"
  /\ lock # NoOne
  /\ mode' = [mode EXCEPT ![p] = IF Variant = "ReadOnly" THEN "readonly" ELSE "down"]
  /\ UNCHANGED <<lock, crashes>>

Retry(p) ==
  /\ mode[p] = "readonly"
  /\ lock = NoOne
  /\ mode' = [mode EXCEPT ![p] = "writer"]
  /\ lock' = p
  /\ UNCHANGED crashes

Exit(p) ==
  /\ mode[p] \in {"writer", "readonly"}
  /\ mode' = [mode EXCEPT ![p] = "down"]
  /\ lock' = IF lock = p THEN NoOne ELSE lock
  /\ UNCHANGED crashes

\* Unbounded crashes: the bound DataDirLock needs for its liveness checks is
\* not needed for safety.
Crash(p) ==
  /\ mode[p] # "down"
  /\ mode' = [mode EXCEPT ![p] = "down"]
  /\ lock' = IF lock = p /\ LockKind = "flock" THEN NoOne ELSE lock
  /\ crashes' = [crashes EXCEPT ![p] = @ + 1]

Next == \E p \in Procs :
  Launch(p) \/ Acquire(p) \/ Busy(p) \/ Retry(p) \/ Exit(p) \/ Crash(p)

Spec == Init /\ [][Next]_vars

TypeOK ==
  /\ mode \in [Procs -> Modes]
  /\ lock \in Procs \cup {NoOne}
  /\ crashes \in [Procs -> Nat]

\* The inductive strengthening: the lock names every writer.
Inv == TypeOK /\ \A p \in Procs : mode[p] = "writer" => lock = p

AtMostOneWriter == \A p, q \in Procs : mode[p] = "writer" /\ mode[q] = "writer" => p = q

LEMMA InitInv == Init => Inv
  BY DEF Init, Inv, TypeOK, Procs, NoOne, Modes

LEMMA NextInv == Inv /\ [Next]_vars => Inv'
  <1> SUFFICES ASSUME Inv, [Next]_vars PROVE Inv'
    OBVIOUS
  <1> USE DEF Inv, TypeOK, Procs, NoOne, Modes
  <1>1. ASSUME NEW p \in Procs, Launch(p) PROVE Inv'
    BY <1>1 DEF Launch
  <1>2. ASSUME NEW p \in Procs, Acquire(p) PROVE Inv'
    BY <1>2 DEF Acquire
  <1>3. ASSUME NEW p \in Procs, Busy(p) PROVE Inv'
    BY <1>3, VariantOK DEF Busy
  <1>4. ASSUME NEW p \in Procs, Retry(p) PROVE Inv'
    BY <1>4 DEF Retry
  <1>5. ASSUME NEW p \in Procs, Exit(p) PROVE Inv'
    BY <1>5 DEF Exit
  <1>6. ASSUME NEW p \in Procs, Crash(p) PROVE Inv'
    BY <1>6, KindOK DEF Crash
  <1>7. CASE UNCHANGED vars
    BY <1>7 DEF vars
  <1> QED
    BY <1>1, <1>2, <1>3, <1>4, <1>5, <1>6, <1>7 DEF Next

LEMMA InvSafe == Inv => AtMostOneWriter
  BY DEF Inv, AtMostOneWriter

THEOREM Safe == Spec => []AtMostOneWriter
  <1>1. Spec => []Inv
    BY InitInv, NextInv, PTL DEF Spec
  <1> QED
    BY <1>1, InvSafe, PTL
=============================================================================
