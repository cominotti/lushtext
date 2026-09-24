------------------------- MODULE WorkflowEventsTrace -------------------------
(* Trace validation: TLC picks one recorded trace and replays it event by     *)
(* event through WorkflowEvents!Accepts. A trace the spec rejects leaves no   *)
(* successor before its end, which TLC reports as a deadlock; accepting every *)
(* trace means every state with events left has a successor. The traces live *)
(* in the generated WorkflowEventsTraces.tla (gen_traces.py).                 *)
EXTENDS Integers, Sequences, WorkflowEvents, WorkflowEventsTraces

VARIABLES t, i, open, seq

Init == /\ t \in DOMAIN Traces /\ i = 0 /\ open = {}
        \* The log may start after earlier events were dropped (capped).
        /\ seq = IF Len(Traces[t]) > 0 THEN Traces[t][1].sequence - 1 ELSE 0

Next == /\ i < Len(Traces[t])
        /\ LET e == Traces[t][i + 1] IN
           /\ Accepts(open, seq, e)
           /\ open' = Opened(open, e)
           /\ seq' = e.sequence
           /\ i' = i + 1
           /\ t' = t

Done == i = Len(Traces[t])
\* The end of every trace is a legal stopping point.
Terminal == Done /\ UNCHANGED <<t, i, open, seq>>
Spec == Init /\ [][Next \/ Terminal]_<<t, i, open, seq>>

\* Checked as an invariant: a state with no enabled Next is the end of its trace.
Accepted == ENABLED Next \/ Done
=============================================================================
