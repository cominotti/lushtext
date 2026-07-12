---
name: data-safety
description: "Identify and fix data loss risks: draft persistence failures, save/close
  flow gaps, replace-all backup safety, session restore bugs, async concurrency hazards.
  Auto-invoked on .rs changes in Rust app code under any */src/ui, */src/services,
  or */src/model tree to provide scoped guidance only. Also invocable as
  $data-safety for the full deterministic five-domain audit. Trigger whenever
  code touches: file I/O, buffer state, tab or window close, draft or session
  persistence, spawn_blocking_then, search/replace, or any async pattern that
  modifies application state. Automatic mode must stay lightweight: no broad
  scans or audit dispatch. In explicit mode, report every confirmed finding and
  fix it when the user has authorized implementation; otherwise request that
  authority instead of mutating the repository."
---

# Data Safety

Guide and audit code patterns that can cause silent data loss. Data loss is the most severe bug class in a text editor — users who lose work never trust the app again. Automatic invocation gives scoped guidance for the current task; explicit `$data-safety` runs the full deterministic audit.

## Guidance Mode (automatic invocation)

Use this mode when the skill was triggered automatically by agents because the current task touches relevant Rust app code or data-safety-sensitive patterns.

- Stay scoped to the active task, diff, or already-open files.
- Do **not** run the explicit audit workflow below.
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
- For draft autosave, clear `draft_dirty` only after the matching snapshot and
  background write are accepted for the same editor generation. Failed draft
  file or manifest writes must leave the editor retryable.
- Keep multi-tab draft passes backpressured to one complete body at a time, and
  enforce the automatic-recovery limit inside the read itself rather than only
  through an earlier metadata probe. Aggregate-preload skips must retain compact
  lazy markers until slow session loads can enter the serialized restore queue.
- For draft orphan cleanup, separate bounded inspection from mutation, reload
  the latest trusted manifest, and merge only committed exact fingerprints.
  Body deletion must hold the stable target guard and recheck the inspected
  inode so an atomic autosave replacement survives the body/manifest gap.
- For Replace All undo, visible in-memory undo state can update immediately,
  but disk save/delete work must be generation-guarded so delayed persistence
  cannot resurrect stale backups or clear a newer one.
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

Use this mode only when the human explicitly invokes `$data-safety` or explicitly asks for a full data-safety audit. The determinism contract, subagent dispatch, fix policy, and report format below apply only to this mode.

## Determinism Contract

This contract applies only to explicit audit mode.

Make the audit reproducible by recording the exact revision and scope source,
the normalized file list, the patterns checked, and the fixed domain/report
ordering. Anchor confirmed findings to source evidence and walk the same binary
decision trees for every candidate. If the available code or runtime evidence
cannot resolve a branch, report an `UNRESOLVED` candidate with the missing
evidence and do not classify it as either safe or a finding. The same recorded
scope and evidence should produce the same ordering and classification; do not
promise identical results when the checkout, generated files, environment, or
available runtime evidence differs.

## Definitions

- **Persistence operation**: Write to `$XDG_DATA_HOME/lushtext/` (drafts, session, workspaces, search history, saved searches) or GSettings.
- **Atomic write**: Unique temp path in the target directory + temp-file flush/sync + rename + parent-directory sync through `services::filesystem::write`. Final path is always fully old or fully new, and the renamed directory entry is durable before success is reported on common Linux filesystems.
- **Fire-and-forget cleanup**: `std::thread::spawn` that ONLY deletes temp files — no manifest or data writes.
- **Optimistic state clear**: Setting a flag to "clean" BEFORE the async operation completes.
- **Data loss (flaggable)**: User content (buffer text, file content, draft) permanently unrecoverable. Metadata loss (cursor position, window size) is not flagged.

## Severity

Assigned by trigger likelihood (impact is always data loss):

- **CRITICAL**: Normal usage any user would encounter.
- **HIGH**: Specific but realistic conditions in regular use.
- **MEDIUM**: Edge conditions requiring unusual timing or circumstances.

## Explicit Audit Workflow

### 1. Scope

- **Collect candidates**: In automatic mode, read changed `.rs` files from `git diff --name-only` or the known review context. In explicit mode, enumerate every tracked `.rs` file, normalize it as below, then retain every suffix under `ui/`, `services/`, or `model/` plus any other `.rs` file whose contents match a listed domain hint.
- **Normalize paths**: Normalize separators to `/`, strip any leading `./`, and derive a **path suffix alias** by removing everything through the last `/src/` when present.
  - `/repo/crates/lushtext-core/src/ui/window/drafts.rs` → `ui/window/drafts.rs`
  - `/repo/crates/lushtext-core/src/ui/window/session_persistence.rs` → `ui/window/session_persistence.rs`
  - `crates/lushtext-core/src/services/session_service.rs` → `services/session_service.rs`
  - `packages/editor-core/src/model/draft.rs` → `model/draft.rs`
- **Match on suffix aliases, not repo layout**: Trigger matching must work for absolute paths, repo-relative paths, crate-relative paths, and future crate moves/reorgs.
- **Relevant Rust app files**: Any normalized suffix under `ui/`, `services/`, or `model/` is in scope. If none match and no content-hint fallback below applies → "No data-safety-relevant changes" → stop.
- **Explicit `$data-safety`**: Use exactly the retained normalized list from the collection rule above for all five domain prompts; record it once in the audit evidence so reruns cannot silently broaden or narrow scope.

### 2. Context

Read `references/known-vectors.md` from the skill directory. Include the relevant domain section when building each subagent prompt — it provides calibration examples of real bugs vs. safe-looking code.

### 3. Dispatch

Audit the five domains below. Skip any whose normalized suffixes AND content
hints both miss, except that explicit `$data-safety` always covers all five.

When subagents are available, dispatch only leaf reviewers and respect the
runtime's total concurrency limit. A root agent in a four-slot runtime has at
most three child slots, so use these deterministic batches:

1. `draft-integrity`, `close-flow`, and `atomic-write` in parallel.
2. After all three finish and release their slots, `replace-safety` and
   `restore-lifecycle` in parallel.

If fewer child slots are available, preserve that order and use smaller
batches. If no child slot is available, run the same prompts and decision trees
locally, one domain at a time. Never claim independent or parallel validation
when it did not occur. Do not let reviewers spawn their own subagents.

| Subagent | Trigger suffixes | Content hints / fallback triggers |
|---|---|---|
| `draft-integrity` | `services/draft_service.rs`, `ui/window/drafts.rs`, `ui/window/documents.rs`, `ui/editor_page/**` | `set_draft_dirty`, `draft_dirty`, `write_draft`, `save_manifest`, `find_by_id`, `original_path`, `is_evicted`, `autosave_inflight`, `autosave_pending` |
| `close-flow` | `ui/window/imp.rs`, `ui/window/mod.rs`, `ui/window/dialogs.rs`, `ui/window/tabs.rs`, `ui/editor_page/mod.rs` | `close_page_finish`, `save_file_async`, `is_saving`, `SaveInProgress`, `cleanup_drafts`, `on_done(true)`, `.destroy()`, `search_panel.close()` |
| `atomic-write` | `ui/**/*.rs`, `services/**/*.rs` | `std::thread::spawn`, `spawn_blocking_then`, `json_store::save`, `save_manifest`, `filesystem::write`, `write_all`, `rename`, `sync_all`, `sync_data`, `release_slot` |
| `replace-safety` | `services/content_search/**`, `ui/search_panel/**`, `ui/window/search.rs` | `undo_backup`, `skip_paths`, `apply_replacements`, `original_line`, `content_mismatch` |
| `restore-lifecycle` | `services/session_service.rs`, `ui/window/session_persistence.rs`, `ui/window/drafts.rs`, `ui/window/mod.rs`, `ui/editor_page/mod.rs` | `load_completed_callback`, `timeout_add_local_once`, `preloaded_drafts`, `filter_existing_tabs`, direct path-existence probes, `set_restore_position`, `apply_restore_position`, `save_ordered` |

If a future feature moves code into a different crate or package but preserves the normalized suffix and/or hits the same content hints, the same subagent must still trigger.

### 4. Subagent Prompts

Each subagent: read assigned files, grep each pattern, walk the decision tree, report only FLAG results. Resolve `{files}` from normalized suffix matching first, then add any content-hint fallback files that belong to that subagent. The prompts below are complete — send each as-is to the Agent tool, replacing `{files}` with the resolved file list and `{known_vectors_section}` with the relevant section from `references/known-vectors.md`.

---

#### draft-integrity

> Audit the draft persistence lifecycle for data loss. Read these source files: {files}
>
> **Calibration context (known real bugs vs. safe patterns):**
> {known_vectors_section: Draft Persistence}
>
> **Patterns to check:**
>
> **DI-1: Optimistic dirty-flag clear**
> Grep: `set_draft_dirty(false)`
> Tree: Before `spawn_blocking_then` or `thread::spawn`? → No: SAFE. Error path calls `set_draft_dirty(true)` to restore? → Yes: SAFE.
> FLAG HIGH: "Dirty flag cleared before write; not restored on error. Write failure + no edits before next autosave tick = draft content lost."
>
> **DI-2: Empty buffer skipped in close-time flush**
> Grep: `is_empty()` in draft flush/save functions (look in `flush_dirty_drafts` or similar)
> Tree: Does the flush function skip tabs with empty text? → No: SAFE.
> FLAG MEDIUM: "Empty modified buffers skipped in flush. User's intentional empty state (type → select-all → delete) not persisted as draft."
>
> **DI-3: Manifest reads stale path after rename**
> Grep: `find_by_id` or `original_path` in autosave/draft-write context
> Tree: Does manifest upsert get `original_path` from `editor.file_path()` (live)? → Yes: SAFE. From existing manifest entry (stale)?
> FLAG HIGH: "After file rename, manifest retains old path. Crash between rename and autosave loses path association — draft orphaned from new path."
>
> **DI-4: Concurrent manifest writes from different thread types**
> Grep: `save_manifest` across all in-scope files
> Tree: Can `save_manifest` be called from both `spawn_blocking_then` AND `std::thread::spawn` paths? → No: SAFE. Serialization between callers (mutex, channel)? → Yes: SAFE.
> FLAG HIGH: "Manifest written from autosave (spawn_blocking_then) and draft delete (thread::spawn). Last-writer-wins race can silently lose manifest entries."
>
> **DI-4b: Overlapping draft autosave batches**
> Grep: `autosave_inflight` and `autosave_pending`
> Tree: Can a new autosave batch start while a prior batch is still writing drafts and manifest? → No, inflight guard present: SAFE. Edits during inflight mark pending and rerun after completion? → Yes: SAFE.
> FLAG HIGH: "Overlapping autosave batches write stale manifest snapshots out of order. Last writer wins and can drop newer draft entries."
>
> **DI-5: Evicted tab handling in draft flush**
> Grep: `is_evicted` in flush context
> Tree: Does flush skip evicted tabs? → No: SAFE. Can modified buffers be evicted? → Check the live editor memory policy snapshot and immediate candidate revalidation for active/modified/save/load/failure/path guards. Both snapshot and revalidation protect modified content: SAFE. Either guard missing or incorrect: FLAG CRITICAL.
>
> **Output**: For each FLAG: Pattern ID, severity, file:line_number, 3-5 line code snippet, one-sentence data loss scenario, one-sentence required fix. If no findings: "draft-integrity: CLEAN"

---

#### close-flow

> Audit save/close/shutdown flows for data loss. Read these source files: {files}
>
> **Calibration context:**
> {known_vectors_section: Save and Close Flows}
>
> **Patterns to check:**
>
> **CF-1: Destroy/proceed after save without checking result**
> Grep: `.destroy()` and `on_done(true)` near save callbacks
> Tree: Inside a `save_file_async` callback? → No: skip. Checks `Result` before proceeding to destroy/on_done? → Yes: SAFE.
> FLAG CRITICAL: "Window destroyed after save failure. Draft cleanup also runs — both file save and draft backup lost."
>
> **CF-2: Draft cleanup without save verification**
> Grep: `cleanup_drafts` near save completion paths
> Tree: Called after save operations? → No: skip. Conditional on ALL saves succeeding? → Yes: SAFE.
> FLAG CRITICAL: "Drafts deleted regardless of save result. Failed save + deleted draft = content permanently gone."
>
> **CF-4: Untitled tabs silently excluded from save dialog**
> Grep: `file_path().is_some()` in save-dialog pending-saves logic
> Tree: Untitled tabs (file_path == None) excluded from pending count? → No: SAFE. User can still check their checkbox in the dialog?
> FLAG HIGH: "Untitled tab shows save checkbox but save silently skips it. User believes content will be saved — it is discarded."
>
> **CF-5: Undo backup cleared before close confirmation**
> Grep: `search_panel` method calls in `close_request` body
> Tree: Does close_request close the search panel before showing save dialog? → No: SAFE. If user cancels close, is the undo backup restored?
> FLAG HIGH: "Replace All undo backup destroyed on close attempt. Cancelling close doesn't restore it — undo permanently lost."
>
> **CF-6: Close while a save is already in flight**
> Grep: `is_saving` or `SaveInProgress` near tab/window close paths
> Tree: Can a user close a tab/window while `save_file_async` is still writing? → No, close is cancelled with warning: SAFE. Duplicate save returns `SaveInProgress` and leaves the editor open? → Yes: SAFE.
> FLAG HIGH: "Close proceeds while the editor is read-only and save is in flight. A failure can happen after the tab/window is gone, leaving no recovery path."
>
> **Output**: Same format as draft-integrity.

---

#### atomic-write

> Audit file I/O for atomicity and concurrency safety. Read these source files: {files}
>
> **Calibration context:**
> {known_vectors_section: Atomic Writes and Concurrency}
>
> **Patterns to check:**
>
> **AW-1: Non-atomic write for persistent data**
> Grep: direct raw writes, `write_all`, or public non-durable filesystem write helpers in functions that write to persistent paths
> Tree: Writing to a persistent path (under `data_dir()` or similar)? → No: SAFE. Uses `filesystem::write::atomic_replace`, `atomic_replace_stream`, or another durable temp-file sync + rename + parent-directory sync pattern? → Yes: SAFE.
> FLAG HIGH: "Direct write to persistent file without atomic temp+rename. Crash during write corrupts file."
>
> **AW-2: Persistence I/O via raw thread::spawn**
> Grep: `std::thread::spawn`
> Tree: Closure performs file I/O? → No: SAFE. Fire-and-forget cleanup only (temp file delete, no manifest/data writes)? → Yes: SAFE. Writes to files that `spawn_blocking_then` also writes to? → No: SAFE.
> FLAG HIGH: "Persistence I/O via raw thread bypasses concurrency guard. Races with spawn_blocking_then writes to same file."
>
> **AW-3: Worker slot leak or premature release**
> Grep: `SlotGuard`, `ACTIVE_THREADS`, and `release_slot_count` in `crates/gtk-lush/tasks/src/lib.rs`
> Tree: Slot release protected by an RAII guard that survives worker panic? → Yes: continue. Successful result keeps the slot held until the GLib main-loop callback consumes the result? → Yes: SAFE. Panic in work closure can leave ACTIVE_THREADS incremented, or large results are released before the UI consumes them?
> FLAG MEDIUM: "Background worker slot can leak or release before result consumption. Panics may stall future I/O, or saturated loads may exceed the intended memory/backpressure cap."
>
> **AW-4: Temp file, metadata, or renamed directory entry not flushed before completion**
> Grep: `rename` in atomic write functions (e.g., `json_store::save`, `write_draft`)
> Tree: Is content flushed before metadata is applied? → Yes: continue. Is required metadata applied before the final temp-file sync? → Yes: continue. Is `sync_all()` or `sync_data()` called on the temp file after metadata and before rename? → No: FLAG. After successful rename, is the parent directory synced (for example through `filesystem::write::sync_parent_dir`)? → Yes: SAFE.
> FLAG HIGH: "Atomic write missing final temp-file sync after metadata or parent-directory sync. Power loss on ext4/XFS/Btrfs can lose the new bytes, required metadata, or renamed directory entry."
>
> **AW-5: Shared temp path for concurrent writers**
> Grep: temp path construction near atomic writes (`with_extension("tmp")`, `.tmp`, or private unique-temp helpers)
> Tree: Does each atomic write use a collision-resistant temp name in the final directory through `filesystem::write`? → Yes: SAFE. Can two saves of the same final path use the same temp path concurrently?
> FLAG HIGH: "Concurrent writers share one temp path. One writer can rename or delete the other's temp file, causing failed saves or stale bytes."
>
> **AW-6: Write coordination tied to replaceable destination inode**
> Grep: `flock`, `File::open`, or lock files near editor save / Replace All coordination
> Tree: Does coordination key the stable resolved target path (canonical target for existing files/symlinks, canonical parent + file name for missing files)? → Yes: SAFE. Does the guard require opening the destination read-write? → No: SAFE.
> FLAG MEDIUM: "Write coordination locks the old destination inode or requires read-write access. Atomic rename replaces the inode, so save/replace operations can interleave or fail to coordinate valid read-only targets."
>
> **Output**: Same format as draft-integrity.

---

#### replace-safety

> Audit Replace All for backup, undo, and partial-replacement safety. Read these source files: {files}
>
> **Calibration context:**
> {known_vectors_section: Replace Operations}
>
> **Patterns to check:**
>
> **RS-1: Undo backup stored only in memory**
> Grep: `undo_backup` and backup storage type (look for `RefCell<HashMap` or similar on imp struct)
> Tree: Backup stored only in widget state (RefCell/Cell on imp struct)? → Persisted to disk? → Yes: SAFE. Cleared on panel close, app exit, or new search?
> FLAG HIGH: "Replace All undo backup is in-memory only. Crash or close after Replace All = original file content permanently lost."
>
> **RS-1b: Undo journal persisted after file mutation**
> Grep: `apply_replacements` and backup persistence calls (`search_backup::save`, `set_undo_backup`, or similar)
> Tree: Is the durable undo journal written before the first file rename? → Yes: SAFE. If journal persistence fails, does Replace All abort before mutating that file?
> FLAG HIGH: "Replace All mutates files before the undo journal is durable. Crash or backup-save failure after mutation loses the only rollback copy."
>
> **RS-1c: Undo journal dropped after ambiguous write failure**
> Grep: `sync_parent_dir` or post-`rename` durability errors in the Replace All write path
> Tree: Can the write helper return an error after `rename` may already have replaced the destination? → No: skip. Does the backup entry stay durable for that path? → Yes: SAFE.
> FLAG MEDIUM: "Post-rename fsync failure removes the undo journal for a file that may already contain replaced bytes."
>
> **RS-2: Stale skip_paths snapshot across async boundary**
> Grep: `skip_paths` in replace/replacement context
> Tree: Set of modified targets is captured on the main thread and sent to background work? → No: skip. Is every captured modified path excluded immutably for the whole operation, and do editor saves plus replacement acquire the same stable-target guard? → Yes: continue. Can a path absent from the captured set acquire unsaved in-memory changes before replacement? → No: SAFE. If yes, do shared coordination, a freshness/content check, and durable recovery evidence prevent or recover an overwrite? → Yes: SAFE.
> FLAG MEDIUM: "Replace All can overwrite a target whose live editor became modified after the safety snapshot without shared coordination or content validation."
>
> **RS-3: Partial multi-file replacement without rollback**
> Grep: `apply_replacements` — inspect backup persistence, the mutation loop, error exits, and recovery reporting
> Tree: Is a durable pre-mutation undo entry accepted for each file before its rename? → No: FLAG. On cancellation, process interruption, or a mid-loop I/O failure, does the journal retain every already-mutated file and expose a bounded recovery/undo path? → Yes: SAFE. Does any exit delete or invalidate recovery evidence for files that may already contain replacements?
> FLAG HIGH: "Replace All can leave already-mutated files without durable, discoverable rollback evidence after interruption or a mid-loop failure."
>
> **RS-4: TOCTOU gap in replacement validation**
> Grep: `original_line` or `content_mismatch` in replace path
> Tree: Validates file content matches expected state before writing? → No: FLAG immediately. Between validation and atomic write, can file change externally? File locked between validate and write? → Yes: SAFE.
> FLAG MEDIUM: "Gap between line validation and write. External edit during replace applied to wrong content."
>
> **Output**: Same format as draft-integrity.

---

#### restore-lifecycle

> Audit session and draft restore for silent data loss during startup. Read these source files: {files}
>
> **Calibration context:**
> {known_vectors_section: Restore Lifecycle}
>
> **Patterns to check:**
>
> **RL-1: Draft recovery callback fires only on success**
> Grep: `load_completed_callback` usage (set, take, fire)
> Tree: Callback consumed only in success path of load? → Continue. On error, is the callback preserved for a user-initiated retry or equivalent recovery path? → Yes: SAFE for RL-1. Callback taken/dropped on error without applying draft OR no recovery path exists?
> FLAG HIGH: "Load error drops draft recovery state without applying draft. Failed open leaves no retry/recovery path, so draft content is stranded."
>
> **RL-2: Timed cleanup races slow operations**
> Grep: `timeout_add_local_once` with `clear` or `take` on cached data (e.g., preloaded_drafts)
> Tree: Timer unconditionally clears cached data? → No: SAFE. Operations can outlast the timer duration? → Fallback re-reads from disk AND fallback fires regardless of callback state? → Yes: SAFE.
> FLAG MEDIUM: "Timed cleanup evicts preloaded drafts while slow loads still in progress."
>
> **RL-3: Session filter drops unavailable files permanently**
> Grep: `filter_existing_tabs` or direct path-existence probes in session load context
> Tree: Filters out non-existent files? → No: skip. Filtered session re-saved to disk? → No (original preserved): SAFE. Removed entries preserved anywhere (backup file, log)? → Yes: SAFE.
> FLAG HIGH: "Temporarily unavailable files (NFS, unmounted) permanently removed from session. Re-saved without them."
>
> **RL-4: Cursor/scroll position lost on load retry**
> Grep: `set_restore_position` or `apply_restore_position`
> Tree: Positions stored independently of load callback (e.g., on EditorPage struct)? → Yes: SAFE. Available for retry after failure? → Yes: SAFE.
> FLAG MEDIUM: "Cursor positions cleared or overwritten on load failure, lost on retry."
>
> **Output**: Same format as draft-integrity.

---

### 5. Aggregate

1. **Deduplicate**: Same file:line flagged by two subagents → keep higher severity, more specific pattern ID.
2. **Sort**: CRITICAL → HIGH → MEDIUM. Within a level, by file path.
3. **Preserve unresolved candidates**: Deduplicate them by pattern and location,
   then sort by domain, file path, and line. A domain with an unresolved
   candidate is not `CLEAN`.
4. **Unified report**: Use the format below.

### 6. Fix Policy

This policy applies only to explicit audit mode.

Never suppress, downgrade, or relabel a confirmed finding as merely
pre-existing. Apply the repository's no-preexisting-blockers rule within the
authority granted by the current request:

- If the user asked to fix, implement, or complete the audited work, fix every
  finding in the same work stream, including necessary design changes.
- If the user asked only for an audit, review, or explanation, keep the pass
  read-only. Report every finding and ask for implementation authority.
- If a fix would require a materially different external action or scope, stop
  at that boundary and request direction; do not silently broaden authority.

A finding is unresolved until fixed and verified. Lack of write authority
changes the next action, not the severity or verdict.

### 7. Verification Expectations

These expectations apply only to explicit audit mode.

Every fix must also add or update regression coverage when the harness can reasonably exercise it.

- **Service/data-path fixes**: Prefer `crates/lushtext/tests/integration/` or existing service-unit tests.
- **Widget/close/restore/replace flows**: Prefer `crates/lushtext/tests/widget/`.
- **If a full regression test is not practical**: add the narrowest automated assertion available and state what remains manual.
- **Verification report**: Name the tests/checks that prove the fix, not just the code change.

## Report Format

Present findings as:

    ## Data Safety Audit Report

    ### Findings

    #### [CRITICAL] CF-1: Window destroy after save failure
    **File**: `ui/window/dialogs.rs:258`
    **Code**: (3-5 line snippet)
    **Impact**: Save fails during close → drafts deleted → content permanently lost
    **Fix**: Track per-save results. Only cleanup/destroy when ALL saves succeed.

    #### [HIGH] DI-1: Draft dirty flag not restored on write error
    **File**: `ui/window/drafts.rs:408`
    ...

    ### Unresolved Candidates
    - RS-3 — `services/content_search/replace.rs:NN`: could not determine
      whether the validation-to-write interval holds a stable target guard.
      Needed evidence: guard acquisition and release path.

    ### Clean Domains
    - atomic-write: CLEAN
    - restore-lifecycle: CLEAN
