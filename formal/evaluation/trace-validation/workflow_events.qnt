// -*- mode: Bluespec; -*-
// SPDX-License-Identifier: GPL-3.0-or-later
//
// E2: the automation event-log producer (the same spec as
// WorkflowEvents.tla). The generated workflow_events_traces.qnt replays
// each recorded smoke artifact as a `run`; `quint test` fails a run whose
// events the spec rejects. DISPOSABLE EVALUATION MODEL.
module workflow_events {
  type Event = { workflowId: str, phase: str, status: str, blocker: str, sequence: int }

  pure def blockersOf(w: str): Set[str] =
    if (w == "minimap-refresh") Set("minimap-refresh")
    else if (w == "search") Set("editor-search", "workspace-search", "command-palette-search", "replace-preview")
    else if (w == "file-load") Set("file-load")
    else if (w == "save") Set("save")
    else if (w == "session-restore") Set("session-restore")
    else if (w == "workspace-refresh") Set("workspace-tree-refresh")
    else if (w == "content-search") Set("workspace-search")
    else if (w == "replace-preview") Set("replace-preview")
    else Set()

  var open: Set[str]
  var seq: int
  var started: bool

  action init = all { open' = Set(), seq' = 0, started' = false }

  action emit(e: Event): bool = all {
    // The log may start after earlier events were dropped (capped).
    started implies e.sequence == seq + 1,
    or {
      all { e.phase == "started", not(open.contains(e.workflowId)), e.status == "running",
            blockersOf(e.workflowId).contains(e.blocker) },
      all { e.phase == "finished", open.contains(e.workflowId), e.status == "settled",
            e.blocker == "none" },
    },
    open' = if (e.phase == "started") open.union(Set(e.workflowId)) else open.exclude(Set(e.workflowId)),
    seq' = e.sequence,
    started' = true,
  }

  action step = all { open' = open, seq' = seq, started' = started }
}
