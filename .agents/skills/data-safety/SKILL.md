---
name: data-safety
description: "Data-loss guard for LushText: drafts, save and close flows, session restore, Replace All undo, durable writes and async state changes. Applies automatically, as short scoped guidance, to Rust changes under ui/, services/, model/ or lushtext-model/ and to any code touching file I/O, buffer state, tab or window close, persistence, spawn_blocking_then or search/replace; invoke $data-safety by name for the full five-domain audit."
---

# Data Safety

Guide and audit code patterns that can cause silent data loss, the most severe bug class in a text editor. Automatic invocation gives scoped guidance for the current task; explicit `$data-safety` runs the full deterministic audit.

## Guidance Mode (automatic invocation)

Use this mode when the skill was triggered automatically by agents because the current task touches relevant Rust app code or data-safety-sensitive patterns.

- Stay scoped to the active task, diff, or already-open files.
- Do **not** run the explicit audit workflow.
- Do **not** dispatch audit subagents.
- Do **not** broad-scan the repo or inspect all in-scope Rust files.
- Do **not** emit the full audit report format, severity list, or fix-policy framing.
- Output concise guidance only: touched safety domains, must-preserve invariants, concrete pitfalls, and whether the human should explicitly run `$data-safety`.
- If you lack enough context to give confident guidance without expanding scope, stop at recommending explicit `$data-safety` rather than escalating automatically.

### Responsiveness guardrails

Automatic guidance must not create or encourage responsiveness regressions.

- Do not recommend synchronous file I/O, manifest scans, or broad grep passes on hot UI paths just because data safety is relevant.
- Do not suggest per-frame, per-animation-tick, per-keystroke, or per-notify safety validation. Safety checks on interactive paths must stay O(1) on the GTK main thread.
- For animations, timers, signal handlers, and rapid input flows, prefer existing project patterns such as `spawn_blocking_then`, generation counters, and success-gated state transitions.
- For save-time formatting rewrites, preserve the disk-to-buffer contract: if the bytes written to disk differ from the captured buffer snapshot, update the live buffer after the write succeeds before clearing the modified state, or leave the buffer modified.
- The draft invariants (`draft_dirty` clears only after same-id, same-generation acceptance; one body at a time; orphan deletion under the stable target guard with an inode recheck) are in `AGENTS.md`, "Loading, saving and recovery"; preserve them. In addition:
  - enforce the automatic-recovery limit inside the read itself rather than only through an earlier metadata probe;
  - aggregate-preload skips must retain compact lazy markers until slow session loads can enter the serialized restore queue;
  - draft orphan cleanup separates bounded inspection from mutation, reloads the latest trusted manifest, and merges only committed exact fingerprints.
- For Replace All undo, visible in-memory undo state can update immediately, but disk save/delete work must be generation-guarded so delayed persistence cannot resurrect stale backups or clear a newer one (journal lifetime: `ui/search_panel/AGENTS.md`).
- For path presence or kind checks, prefer `services::filesystem::metadata::exists` or `path_status`; reserve `file_facts` for workflows that also need canonical identity, byte size, or mtime.
- If deeper review would require repo-wide inspection, many-file greps, or parallel audit subagents, recommend explicit `$data-safety` instead of doing it automatically.
- When safety and responsiveness pull in different directions, preserve both: do not propose data-loss fixes that introduce UI jank, animation stutter, or "Application Not Responding" risks.

### Automatic-mode output

Present guidance as:

1. `Touched domains:` ...
2. `Protect these invariants:` ...
3. `Avoid:` ...
4. `Escalate to $data-safety?` Yes/No + one sentence

## Explicit Audit Mode (`$data-safety`)

Use this mode only when the human explicitly invokes `$data-safety` or explicitly asks for a full data-safety audit.

Read [references/audit.md](references/audit.md) first and follow it step by step. It holds the determinism contract, definitions, severity, scope and dispatch rules, the five domain prompts (sent verbatim, with only `{files}` and the calibration section substituted), aggregation, fix policy, verification expectations and the report format. Do not improvise prompts or decision trees from this summary.

Fix policy in one line: never relabel a confirmed finding as pre-existing; fix every finding when the user authorized implementation, otherwise report every finding and request that authority.
