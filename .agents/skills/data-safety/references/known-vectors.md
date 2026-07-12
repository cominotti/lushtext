# Known Data Loss Vectors

## Table of Contents

- [Draft Persistence](#draft-persistence)
- [Save and Close Flows](#save-and-close-flows)
- [Atomic Writes and Concurrency](#atomic-writes-and-concurrency)
- [Replace Operations](#replace-operations)
- [Restore Lifecycle](#restore-lifecycle)

Calibration catalog for data-safety subagents. Each entry documents a confirmed or calibration-relevant data loss pattern with its location, the specific code that causes it, the user scenario, and a safe counterexample where one exists. Subagents use this to distinguish genuinely dangerous code from similar-looking safe patterns.

All paths are written as normalized suffixes relative to any `*/src/` root.
- `/repo/crates/lushtext-core/src/ui/window/drafts.rs` → `ui/window/drafts.rs`
- `/repo/crates/lushtext-core/src/ui/window/session_persistence.rs` → `ui/window/session_persistence.rs`
- `packages/editor-core/src/services/session_service.rs` → `services/session_service.rs`

---

## Draft Persistence

### DI-1: Optimistic dirty-flag clear (CONFIRMED HISTORICAL)
**Former location**: `ui/window/session.rs`; current workflow owner: `ui/window/drafts.rs` — `autosave_tick()` function
**Old code**: `editor.set_draft_dirty(false)` was called BEFORE `spawn_blocking_then` that writes drafts. The `then` callback logged errors but did NOT call `set_draft_dirty(true)`.
**Scenario**: Disk full → `write_draft` fails → dirty flag stays false → user makes no edits in next 5s → next autosave skips tab → app closes → draft not on disk → content lost.
**Current guardrail**: Draft autosave keeps batches serialized with inflight/pending state and restores dirty state on write failure. `save_file_async` must not clear `buffer.modified` until the durable write succeeds.

### DI-3: Stale manifest path after rename (CONFIRMED HISTORICAL)
**Former location**: `ui/window/session.rs`; current workflow owner: `ui/window/drafts.rs` — autosave tick, manifest upsert block
**Old code**: `original_path` was read from `manifest.find_by_id(draft_id)` (the existing manifest entry) instead of from `editor.file_path()` (the live, post-rename path).
**Scenario**: User renames file in sidebar → `editor.file_path()` updated → autosave reads old path from manifest → writes manifest with old path → crash → draft orphaned from new path → `find_by_path` with new path returns None → draft not applied on next startup.

### DI-4: Concurrent manifest writes (CONFIRMED HISTORICAL)
**Former location**: `ui/window/session.rs`; current workflow owner: `ui/window/drafts.rs`
- `autosave_tick()` uses `spawn_blocking_then` → calls `save_manifest`
- `delete_draft_by_id()` uses `std::thread::spawn` → calls `save_manifest`
**Old code**: Both paths cloned the in-memory manifest at call time and wrote it independently. No serialization between them.
**Scenario**: Tab close triggers `delete_draft_by_id` (thread A clones manifest after removal). Simultaneously, autosave fires (thread B clones manifest with removal already applied but also with new draft data). Thread A writes first, thread B overwrites. Or vice versa — either way, one write's changes are lost.
**Current guardrail**: Persistent draft writes should go through the guarded async path; overlapping autosave batches must be serialized with a pending rerun when edits arrive mid-flight.

---

## Save and Close Flows

### CF-1/CF-2/CF-4/CF-5: Close-flow loss cluster (CONFIRMED HISTORICAL)
**Former location**: `ui/window/dialogs.rs` and `ui/window/imp.rs`
**Old failure**: Close completion ignored individual save failures, deleted drafts unconditionally, silently skipped selected untitled rows, and closed search state before the user confirmed window closure.
**Current guardrails**: The close coordinator records failed saves, preserves recovery drafts for unsuccessful or untitled work, blocks completion while selected untitled tabs still require an explicit save path, and closes search state only after confirmation succeeds. Re-audit these invariants at their current owners instead of reporting the historical defects as present.

---

## Atomic Writes and Concurrency

### AW-2: Raw thread::spawn for persistence (CONFIRMED HISTORICAL)
**Former location**: `ui/window/session.rs`; current workflow owner: `ui/window/drafts.rs` — `delete_draft_by_id()`
**Old code**: `std::thread::spawn` called `delete_draft_file` + `save_manifest`
**Why not fire-and-forget**: `save_manifest` writes the FULL manifest to disk. This is a persistence operation.
**Current guardrail**: Draft deletion uses `spawn_blocking_then` plus manifest update helpers so it stays under the concurrency guard and merges against the current on-disk manifest. Temp file cleanup after failed inline-create in sidebar uses `std::thread::spawn` for JUST deleting a temp file. That IS fire-and-forget cleanup and is acceptable.

### AW-3: Worker slot leak or premature release (CONFIRMED HISTORICAL)
**Former location**: `services/async_task.rs`; current implementation: `crates/gtk-lush/tasks/src/lib.rs` — `spawn_blocking_then`
**Old code**: `release_slot()` was called inside the spawned thread after `work()` completed. No `catch_unwind` or Drop guard ensured slot release, and saturated work rechecked capacity through a timer loop.
**Old scenario**: If a work closure panicked, `ACTIVE_THREADS` stayed incremented. After enough panics, all `spawn_blocking_then` calls entered the retry loop permanently.
**Current guardrail**: `gtk-lush-tasks` owns worker accounting with an RAII `SlotGuard`, holds the slot until the GLib main-loop callback consumes the result, and queues saturated work in a main-thread FIFO woken by slot release.

### AW-4: Missing final temp metadata sync or parent-directory sync after atomic rename (CONFIRMED HISTORICAL)
**Location**: `services/json_store.rs`, `services/draft_service.rs`, `services/editor_io.rs`, `services/content_search/replace.rs`, `services/local_history_service.rs`
**Old code**: Atomic write helpers flushed and `sync_all()`ed the temp file before the final rename, but returned immediately after rename without syncing the containing directory. A later partial hardening still synced temp content before applying metadata, leaving chmod/chown/xattr/ACL mutations outside the final temp-file durability proof. Local-history migration also renamed/copy-then-removed snapshot files without making the destination entry durable first.
**Scenario**: Power loss after rename on ext4, XFS, or Btrfs can preserve the synced temp-file bytes while losing the directory entry update or required destination metadata. The app may restart with the old JSON state, old draft file, old saved document, widened or missing file metadata, missing Replace All rollback state, or a broken local-history lineage.
**Current guardrail**: `filesystem::write::atomic_replace` probes metadata before temp creation, creates overwrite temps with no wider standard permissions than the destination, applies required metadata before the final temp-file sync, and syncs the parent directory after successful rename. `filesystem::write::copy_file_durable()` uses source metadata for cross-filesystem fallback copies and removes the source only after the destination write and destination parent sync complete.

### AW-5: Shared temp path for concurrent writers (CONFIRMED HISTORICAL)
**Location**: `services/json_store.rs`, `services/draft_service.rs`, `services/editor_io.rs`, `services/content_search/replace.rs`
**Old code**: Several atomic writers derived temp paths only from the final file name, such as `.tmp`, so concurrent writes to the same final path shared one temp file.
**Scenario**: Two saves of the same file overlap. Writer A syncs and renames the temp path while writer B is still writing, or writer B recreates the same temp path after writer A opened it. The final rename can fail or persist stale bytes.
**Current guardrail**: Use `services::filesystem::write` durable entry points so each writer gets a collision-resistant temp path in the final directory.

### AW-6: Destination-inode lock does not coordinate atomic replace (CONFIRMED HISTORICAL)
**Location**: `services/durable_write.rs`, `services/editor_io.rs`, `services/content_search/replace.rs`
**Old code**: Save/replace coordination used a lock on the current destination inode. Atomic rename replaces that inode, so later writes could coordinate with an inode that was no longer the live target. The lock also required opening the destination read-write, which can fail for read-only files that are still replaceable through a writable parent directory.
**Scenario**: An editor save and Replace All target the same file through different path spellings or a symlink. One operation replaces the inode while the other is still keyed to the old inode, letting reads/writes interleave. A valid read-only destination can also fail coordination before the atomic-replace path has a chance to run.
**Current guardrail**: Resolve the stable write target before coordination: existing files and symlinks use the canonical target path, and missing files use the canonical parent plus file name. Editor save, Save As, Replace All, and undo all acquire this process-local target guard before reading or writing bytes, without opening the destination read-write.

---

## Replace Operations

### RS-1: In-memory undo backup (CONFIRMED HISTORICAL)
**Former location**: `ui/search_panel` widget state
**Old failure**: Replace All kept rollback bytes only in memory, so a crash or close discarded the only recovery copy.
**Current guardrail**: `services/search_backup.rs` persists a generation-guarded undo journal before mutation and retains ambiguous post-rename failures. Visible widget state is only a projection of that durable recovery evidence.

### RS-2: Modified-target snapshot coordination (CALIBRATION — CURRENT CODE SAFE)
**Current workflow owner**: `ui/window/search.rs` plus `services/content_search/replace.rs`
**Why the old flag was wrong**: A path modified when `skip_paths` is captured remains in the immutable skip set even if the user saves it later; it cannot become newly eligible inside that operation. Editor saves and replacements also coordinate through the same stable-target guard.
**Calibration takeaway**: Flag only a target that can become newly modified after capture and then bypass both shared target coordination and the replacement content check. Do not claim that saving an already-skipped path makes it overwriteable.

---

## Restore Lifecycle

### RL-1: Retry-dependent draft recovery (CALIBRATION — CURRENT CODE PARTIALLY SAFE)
**Location**: `ui/editor_page/mod.rs` — `load_completed_callback`
**Code**: Callback is set in `open_document()` and consumed via `.take()` only in `load_file_async`'s success callback. On error, the callback remains stored and current code surfaces `_Retry`, which re-runs `load_file_async`.
**Nuance**: This means the current code is SAFE for the narrow "callback dropped on first error" failure mode. The remaining risk is lifecycle-based: if the user abandons or closes the failed tab, the callback never fires and the draft can later be deleted by orphan cleanup without ever being reapplied.
**Calibration takeaway**: Do NOT flag code just because the callback fires only on success. Flag when the error path drops recovery state, omits any retry/recovery path, or later cleanup can delete the draft without another recovery route.

### RL-3: Session filter drops unavailable files (CONFIRMED HISTORICAL)
**Former location**: `services/session_service.rs` — removed `filter_existing_tabs()`
**Old code**: direct path-existence stat check → false for NFS/slow mount → tab removed → session re-saved without it.
**Scenario**: Laptop undocked, NFS share unavailable → session restore drops all NFS-backed tabs → session saved → dock again → tabs permanently gone from session. Draft files (if any) survive as orphans for one restart cycle.
**Current guardrail**: Startup restore must load `session.json` as-is and preserve temporarily unavailable file-backed tabs. Do not reintroduce a service API that filters session tabs with `Path::exists`.
