---------------------------- MODULE WorkflowEvents ----------------------------
(***************************************************************************)
(* SPDX-License-Identifier: GPL-3.0-or-later                               *)
(*                                                                         *)
(* E2: trace validation of the automation event log. The producer spec: the *)
(* bounded, content-free GetWorkflowEvents log (docs/automation.md) records *)
(* readiness-state changes as `started` / `finished` phases per workflow   *)
(* id, with a gapless sequence. WorkflowEventsTrace.tla checks that every   *)
(* recorded smoke artifact is a behaviour of this spec. DISPOSABLE         *)
(* EVALUATION MODEL; it reads existing artifacts only.                     *)
(***************************************************************************)
EXTENDS Integers, Sequences

\* The readiness blocker a `started` event may name, per workflow id
\* (docs/automation-reference.md, readiness blockers). A `finished` event
\* names none.
BlockersOf(w) ==
  CASE w = "minimap-refresh" -> {"minimap-refresh"}
    [] w = "search" -> {"editor-search", "workspace-search", "command-palette-search", "replace-preview"}
    [] w = "file-load" -> {"file-load"}
    [] w = "save" -> {"save"}
    [] w = "session-restore" -> {"session-restore"}
    [] w = "workspace-refresh" -> {"workspace-tree-refresh"}
    [] w = "content-search" -> {"workspace-search"}
    [] w = "replace-preview" -> {"replace-preview"}
    [] OTHER -> {}

\* One event, given the set of workflows the log shows running and the last
\* sequence number: the event is accepted, and its successor state follows.
Accepts(open, seq, e) ==
  /\ e.sequence = seq + 1
  /\ \/ /\ e.phase = "started" /\ e.workflow_id \notin open
        /\ e.status = "running" /\ e.blocker \in BlockersOf(e.workflow_id)
     \/ /\ e.phase = "finished" /\ e.workflow_id \in open
        /\ e.status = "settled" /\ e.blocker = "none"

Opened(open, e) ==
  IF e.phase = "started" THEN open \cup {e.workflow_id} ELSE open \ {e.workflow_id}
=============================================================================
