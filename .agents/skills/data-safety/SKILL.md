---
name: data-safety
description: "Identify and fix data loss risks: draft persistence failures, save/close
  flow gaps, replace-all backup safety, session restore bugs, async concurrency hazards.
  Uses 5 parallel grep-based subagents with binary decision trees for deterministic
  results. Auto-invoked on .rs changes in ui/, services/, or model/. Also invocable
  as /data-safety for full codebase audit. Trigger whenever code touches: file I/O,
  buffer state, tab or window close, draft or session persistence, spawn_blocking_then,
  search/replace, or any async pattern that modifies application state. Every finding
  must be fixed immediately — never deferred."
---

# Data Safety Audit

Detect and fix code patterns that cause silent data loss. Data loss is the most severe bug class in a text editor — users who lose work never trust the app again.

## Determinism Contract

Every finding is anchored to a grep match, validated through a binary decision tree (each check resolves to SAFE or continue), and reported only when all conditions resolve to FLAG. Ambiguous matches are dropped silently — false positives erode trust. Multiple runs against the same code produce identical results.

## Definitions

- **Persistence operation**: Write to `$XDG_DATA_HOME/lushtext/` (drafts, session, workspaces, search history, saved searches) or GSettings.
- **Atomic write**: Temp file + flush + rename. Final path is always fully old or fully new.
- **Fire-and-forget cleanup**: `std::thread::spawn` that ONLY deletes temp files — no manifest or data writes.
- **Optimistic state clear**: Setting a flag to "clean" BEFORE the async operation completes.
- **Data loss (flaggable)**: User content (buffer text, file content, draft) permanently unrecoverable. Metadata loss (cursor position, window size) is not flagged.

## Severity

Assigned by trigger likelihood (impact is always data loss):

- **CRITICAL**: Normal usage any user would encounter.
- **HIGH**: Specific but realistic conditions in regular use.
- **MEDIUM**: Edge conditions requiring unusual timing or circumstances.

## Workflow

### 1. Scope

- **Auto-trigger**: Changed `.rs` files (from git diff or known changes). Filter to `src/ui/`, `src/services/`, `src/model/`. If none match → "No data-safety-relevant changes" → stop.
- **Manual** (`/data-safety`): All `.rs` files under those paths.

### 2. Context

Read `references/known-vectors.md` from the skill directory. Include the relevant domain section when building each subagent prompt — it provides calibration examples of real bugs vs. safe-looking code.

### 3. Dispatch

Launch subagents in parallel via the Agent tool. Skip any whose trigger paths have no files in scope. For manual invocation, dispatch all 5.

| Subagent | Trigger paths |
|---|---|
| `draft-integrity` | `services/draft_service.rs`, `ui/window/session.rs`, `ui/editor_page/**` |
| `close-flow` | `ui/window/imp.rs`, `ui/window/mod.rs`, `ui/window/dialogs.rs`, `ui/editor_page/mod.rs` |
| `atomic-write` | `services/**/*.rs`, `ui/window/session.rs` |
| `replace-safety` | `services/content_search.rs`, `ui/search_panel/**`, `ui/window/search.rs` |
| `restore-lifecycle` | `services/session_service.rs`, `ui/window/session.rs`, `ui/window/mod.rs` |

### 4. Subagent Prompts

Each subagent: read assigned files, grep each pattern, walk the decision tree, report only FLAG results. The prompts below are complete — send each as-is to the Agent tool, replacing `{files}` with the resolved file list and `{known_vectors_section}` with the relevant section from `references/known-vectors.md`.

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
> **DI-5: Evicted tab handling in draft flush**
> Grep: `is_evicted` in flush context
> Tree: Does flush skip evicted tabs? → No: SAFE. Can modified buffers be evicted? → Check `maybe_evict_background_tabs` for `!is_modified()` guard. Guard present and correctly checks `is_modified()`: SAFE. Guard missing or incorrect: FLAG CRITICAL.
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
> **CF-3: close_page_finish behind failing weak refs**
> Grep: `close_page_finish` with `upgrade()` nearby
> Tree: Called through weak ref upgrade? → No: skip. If upgrade returns None, close_page_finish skipped entirely?
> FLAG MEDIUM: "Window destroyed during close dialog → close_page_finish never called → tab stuck in inhibited state."
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
> Grep: `fs::write` or `write_all` in functions that write to persistent paths
> Tree: Writing to a persistent path (under `data_dir()` or similar)? → No: SAFE. Uses temp file + rename pattern? → Yes: SAFE.
> FLAG HIGH: "Direct write to persistent file without atomic temp+rename. Crash during write corrupts file."
>
> **AW-2: Persistence I/O via raw thread::spawn**
> Grep: `std::thread::spawn`
> Tree: Closure performs file I/O? → No: SAFE. Fire-and-forget cleanup only (temp file delete, no manifest/data writes)? → Yes: SAFE. Writes to files that `spawn_blocking_then` also writes to? → No: SAFE.
> FLAG HIGH: "Persistence I/O via raw thread bypasses concurrency guard. Races with spawn_blocking_then writes to same file."
>
> **AW-3: Panic leaks concurrency slot**
> Grep: `release_slot` in `async_task.rs` implementation
> Tree: Slot release in a Drop guard or catch_unwind wrapper? → Yes: SAFE. Panic in work closure can leave ACTIVE_THREADS permanently incremented?
> FLAG MEDIUM: "Panic in work closure leaks concurrency slot. After 8 panics, all background I/O stalls permanently."
>
> **AW-4: Temp file not flushed before rename**
> Grep: `rename` in atomic write functions (e.g., `json_store::save`, `write_draft`)
> Tree: Is `flush()`, `sync_all()`, or `sync_data()` called on the temp file before rename? → Yes: SAFE.
> FLAG HIGH: "No flush before rename. Power loss → renamed file may contain partial/zero data from filesystem write-back cache."
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
> **RS-2: Stale skip_paths snapshot across async boundary**
> Grep: `skip_paths` in replace/replacement context
> Tree: Set of "modified tabs to skip" built on main thread and sent to background thread? → No: skip. Can source data (tab modified state) change between snapshot and background use?
> FLAG MEDIUM: "skip_paths snapshot becomes stale during background Replace All. File saved after snapshot but before background write is overwritten."
>
> **RS-3: Partial multi-file replacement without rollback**
> Grep: `apply_replacements` — look for loop structure and cancel token checks
> Tree: Replacement loop interruptible (cancel token, error break)? → No: SAFE. Already-processed files rolled back from backup on interruption? → Yes: SAFE.
> FLAG HIGH: "Cancel mid-Replace All → some files replaced, others not. No automatic rollback for already-written files."
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
> Tree: Callback consumed only in success path of load? → Continue. Also consumed or handled in error path? → Yes: SAFE. On retry (calling load_file_async again), is the original callback still in the RefCell (never taken on error)? → Yes: SAFE (retry will fire it). Callback taken/dropped on error without applying draft?
> FLAG HIGH: "Load error drops draft recovery callback without applying draft. No retry mechanism → draft not applied."
>
> **RL-2: Timed cleanup races slow operations**
> Grep: `timeout_add_local_once` with `clear` or `take` on cached data (e.g., preloaded_drafts)
> Tree: Timer unconditionally clears cached data? → No: SAFE. Operations can outlast the timer duration? → Fallback re-reads from disk AND fallback fires regardless of callback state? → Yes: SAFE.
> FLAG MEDIUM: "Timed cleanup evicts preloaded drafts while slow loads still in progress."
>
> **RL-3: Session filter drops unavailable files permanently**
> Grep: `filter_existing_tabs` or `path.exists()` in session load context
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
3. **Unified report**: Use the format below.

### 6. Fix Policy

Every finding must be fixed in the current work stream. This aligns with `.agents/rules/preexisting-blockers.md`.

Do not defer, document as known, skip as pre-existing, or downgrade severity. If a fix requires a design change (e.g., disk-based undo backup for RS-1), implement the design change.

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
    **File**: `ui/window/session.rs:408`
    ...

    ### Clean Domains
    - atomic-write: CLEAN
    - restore-lifecycle: CLEAN
