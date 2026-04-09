# Known Data Loss Vectors

Calibration catalog for data-safety subagents. Each entry documents a confirmed or calibration-relevant data loss pattern with its location, the specific code that causes it, the user scenario, and a safe counterexample where one exists. Subagents use this to distinguish genuinely dangerous code from similar-looking safe patterns.

All paths are written as normalized suffixes relative to any `*/src/` root.
- `/repo/crates/lushtext-core/src/ui/window/session.rs` → `ui/window/session.rs`
- `packages/editor-core/src/services/session_service.rs` → `services/session_service.rs`

---

## Draft Persistence

### DI-1: Optimistic dirty-flag clear (CONFIRMED)
**Location**: `ui/window/session.rs` — `autosave_tick()` function
**Code**: `editor.set_draft_dirty(false)` is called BEFORE `spawn_blocking_then` that writes drafts. The `then` callback logs errors but does NOT call `set_draft_dirty(true)`.
**Scenario**: Disk full → `write_draft` fails → dirty flag stays false → user makes no edits in next 5s → next autosave skips tab → app closes → draft not on disk → content lost.
**Safe counterexample**: `save_file_async` in `ui/editor_page/mod.rs` calls `set_modified(false)` optimistically but DOES call `set_modified(true)` on error. That's the correct pattern.

### DI-3: Stale manifest path after rename (CONFIRMED)
**Location**: `ui/window/session.rs` — autosave tick, manifest upsert block
**Code**: `original_path` is read from `manifest.find_by_id(draft_id)` (the existing manifest entry) instead of from `editor.file_path()` (the live, post-rename path).
**Scenario**: User renames file in sidebar → `editor.file_path()` updated → autosave reads old path from manifest → writes manifest with old path → crash → draft orphaned from new path → `find_by_path` with new path returns None → draft not applied on next startup.

### DI-4: Concurrent manifest writes (CONFIRMED)
**Location**: `ui/window/session.rs`
- `autosave_tick()` uses `spawn_blocking_then` → calls `save_manifest`
- `delete_draft_by_id()` uses `std::thread::spawn` → calls `save_manifest`
**Code**: Both paths clone the in-memory manifest at call time and write it independently. No serialization between them.
**Scenario**: Tab close triggers `delete_draft_by_id` (thread A clones manifest after removal). Simultaneously, autosave fires (thread B clones manifest with removal already applied but also with new draft data). Thread A writes first, thread B overwrites. Or vice versa — either way, one write's changes are lost.

---

## Save and Close Flows

### CF-1: Destroy after save failure (CONFIRMED — CRITICAL)
**Location**: `ui/window/dialogs.rs` — `show_save_changes_dialog`, RESPONSE_SAVE handler
**Code**: Each `save_file_async` callback decrements a `pending` counter. When counter reaches 0, `cleanup_drafts_for_editors(&all_editors)` + `on_done(true)` runs REGARDLESS of individual save results.
**Scenario**: User has 3 unsaved tabs, clicks Save in close dialog. Tab 2's save fails (permission denied). Counter still reaches 0. Drafts deleted. Window destroyed. Tab 2's content: not saved, draft deleted. **Permanently lost.**
**Why critical**: This is the normal close flow — every user encounters it. The only protection is that saves rarely fail, but when they do, the failure is catastrophic.

### CF-2: Draft cleanup without save check (CONFIRMED — CRITICAL)
**Location**: Same as CF-1 — `cleanup_drafts_for_editors` called unconditionally
**Entangled with CF-1**: The draft deletion IS the data loss mechanism. If saves fail but drafts are preserved, the user can recover on next startup. The fix for CF-1 and CF-2 is the same: only delete drafts for files that were successfully saved.

### CF-4: Untitled tabs in save dialog (CONFIRMED)
**Location**: `ui/window/dialogs.rs` — the `if check.is_active() && editor.file_path().is_some()` guard
**Code**: Untitled tabs increment `pending_saves` only if `file_path` is `Some`. Since untitled tabs have `None`, they're excluded from the save count.
**UX issue**: The checkbox row IS shown for untitled tabs with "(new)" label. User can check it, believes save will happen, but it's silently skipped. Only `cleanup_drafts_for_editors` runs → draft deleted.

### CF-5: Undo backup cleared on close attempt (CONFIRMED)
**Location**: `ui/window/imp.rs` — `close_request` implementation
**Code**: `self.search_panel.close()` is called BEFORE checking `modified_editors()` and showing the save dialog.
**Scenario**: User does Replace All → tries to close window → `search_panel.close()` destroys undo backup → save dialog appears → user cancels → window stays open but Replace All undo is gone.

---

## Atomic Writes and Concurrency

### AW-2: Raw thread::spawn for persistence (CONFIRMED)
**Location**: `ui/window/session.rs` — `delete_draft_by_id()`
**Code**: `std::thread::spawn` calling `delete_draft_file` + `save_manifest`
**Why not fire-and-forget**: `save_manifest` writes the FULL manifest to disk. This is a persistence operation.
**Safe counterexample**: Temp file cleanup after failed inline-create in sidebar uses `std::thread::spawn` for JUST deleting a temp file. That IS fire-and-forget cleanup and is acceptable.

### AW-3: Panic slot leak (CONFIRMED — LOW LIKELIHOOD)
**Location**: `services/async_task.rs` — `spawn_blocking_then` implementation
**Code**: `release_slot()` is called inside the spawned thread after `work()` completes. No `catch_unwind` wraps the work closure. No Drop guard ensures slot release.
**Scenario**: If a work closure panics (e.g., serde panic on corrupt data), `release_slot()` is never called. ACTIVE_THREADS stays incremented. After 8 such panics, all `spawn_blocking_then` calls enter the 50ms retry loop permanently.
**Likelihood**: Very low — work closures are simple I/O. But possible with corrupt files.

---

## Replace Operations

### RS-1: In-memory undo backup (CONFIRMED)
**Location**: `ui/search_panel` imp struct — `undo_backup: RefCell<HashMap<PathBuf, Vec<u8>>>`
**Code**: Backup populated during Replace All, stored only in widget state. Cleared by: new search, panel close, app exit, window close.
**Scenario**: User does Replace All across 50 files → app crashes → backup gone → original content permanently lost. Only git or external backups can recover.
**Safe counterexample**: Draft persistence saves content to DISK every 5 seconds. Replace backup should follow similar persistence pattern.

### RS-2: Stale skip_paths (CONFIRMED — NARROW WINDOW)
**Location**: `ui/window/search.rs` — `replace_callback` closure
**Code**: `skip_paths` is built from `editor.is_modified()` on the main thread, then passed to `spawn_blocking_then`. Between snapshot and background execution, a user could save a tab (clearing `is_modified`).
**Window**: Typically very short (milliseconds for small codebases). Wider for large codebases where `apply_replacements` takes seconds.

---

## Restore Lifecycle

### RL-1: Retry-dependent draft recovery (CALIBRATION — CURRENT CODE PARTIALLY SAFE)
**Location**: `ui/editor_page/mod.rs` — `load_completed_callback`
**Code**: Callback is set in `open_document()` and consumed via `.take()` only in `load_file_async`'s success callback. On error, the callback remains stored and current code surfaces `_Retry`, which re-runs `load_file_async`.
**Nuance**: This means the current code is SAFE for the narrow "callback dropped on first error" failure mode. The remaining risk is lifecycle-based: if the user abandons or closes the failed tab, the callback never fires and the draft can later be deleted by orphan cleanup without ever being reapplied.
**Calibration takeaway**: Do NOT flag code just because the callback fires only on success. Flag when the error path drops recovery state, omits any retry/recovery path, or later cleanup can delete the draft without another recovery route.

### RL-3: Session filter drops unavailable files (CONFIRMED)
**Location**: `services/session_service.rs` — `filter_existing_tabs()`
**Code**: `path.exists()` stat check → false for NFS/slow mount → tab removed → session re-saved without it.
**Scenario**: Laptop undocked, NFS share unavailable → session restore drops all NFS-backed tabs → session saved → dock again → tabs permanently gone from session. Draft files (if any) survive as orphans for one restart cycle.
