---
title: 'Optimize draft restore latency on startup'
type: 'perf'
created: '2026-04-04'
status: 'done'
baseline_commit: '458cb5c'
context:
  - '.agents/AGENTS.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** The "Draft Changes Restored" yellow bar takes ~1 second to appear after launching LushText. The draft content reaches the UI through 3 serial `spawn_blocking_then` round-trips: (1) manifest+session+orphan cleanup, (2) per-tab file load, (3) per-tab draft read. Each hop adds thread spawn overhead + one main-loop iteration latency (~16ms). Orphan cleanup (`N × stat + readdir + write`) also blocks the critical path unnecessarily.

**Approach:** Pre-read all draft content in the initial background task (eliminating hop #3) and defer orphan cleanup to after tab restoration. Reduces the serial chain from 3 hops to 2 with less I/O in hop #1.

## Boundaries & Constraints

**Always:** Keep the `load_completed_callback` gate — draft content must be applied after file load completes to prevent the file-read callback from overwriting draft content. Preserve the fallback to background draft read for non-startup `check_draft_on_open` calls (e.g., files opened after startup).

**Ask First:** Any change to the `spawn_blocking_then` concurrency model or `MAX_CONCURRENT_SPAWNS`.

**Never:** Change GTK info bar animation timing. Add an async runtime. Remove the `check_draft_on_open` fallback path (it's needed for files opened after session restore).

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Tab with preloaded draft | Session tab with matching manifest entry | Draft applied in load_completed_callback, no background task #3 | N/A |
| Tab without draft | Session tab, no manifest match | Normal file load, no draft apply | N/A |
| Draft file unreadable | Manifest entry exists but file missing | Skip silently during batch preload, tab opens without draft | Log at warn level |
| Orphan cleanup deferred | Stale drafts at startup | Cleanup runs ~2s after restore, no user-visible impact | Log errors, never block startup |
| Post-startup file open | New file opened after session restore | Preloaded map is empty, falls back to background draft read | Same as current |

</frozen-after-approval>

## Code Map

- `crates/lushtext-core/src/ui/window/imp.rs` -- Window imp struct, needs `preloaded_drafts` field
- `crates/lushtext-core/src/ui/window/session.rs` -- `load_session_and_drafts`, `check_draft_on_open`, `check_draft_by_id`, `restore_tabs`
- `crates/lushtext-core/src/services/draft_service.rs` -- `draft_id_for_path`, `read_draft` (called in batch), `cleanup_orphans`

## Tasks & Acceptance

**Execution:**
- [x] `crates/lushtext-core/src/ui/window/imp.rs` -- Add `preloaded_drafts: RefCell<HashMap<String, String>>` to imp struct (draft_id → content). Defaults to empty via `RefCell::default()`.
- [x] `crates/lushtext-core/src/ui/window/session.rs` -- Rework `load_session_and_drafts`: remove `cleanup_orphans` + `save_manifest` from background task. After loading manifest+session+filter, iterate session tabs to compute draft_ids (file-backed via `draft_id_for_path`, untitled via `tab.draft_id`), check manifest for matches, batch-read drafts into `HashMap<String, String>`. Return `(DraftManifest, SessionData, HashMap)`. Store preloaded_drafts on window. Call `schedule_orphan_cleanup()` after `restore_tabs`.
- [x] `crates/lushtext-core/src/ui/window/session.rs` -- Modify `check_draft_on_open`: try `preloaded_drafts.borrow_mut().remove(draft_id)` first. If found, apply draft content immediately (no spawn). Fall back to background read only when not preloaded. Same pattern for `check_draft_by_id`.
- [x] `crates/lushtext-core/src/ui/window/session.rs` -- Add `schedule_orphan_cleanup()`: `glib::timeout_add_local_once(2s)` wrapping `spawn_blocking_then` with `cleanup_orphans` + `save_manifest`. Update the in-memory manifest on completion.
- [x] Tests: No existing tests break (draft_service and session_service functions unchanged, widget tests don't exercise draft restoration timing). No new tests required — the optimization is an internal implementation detail with identical external behavior. Verify with `make test`.

**Acceptance Criteria:**
- Given a session with draft-backed tabs, when LushText launches, then the "Draft Changes Restored" bar appears with only 2 `spawn_blocking_then` round-trips in the critical path (not 3)
- Given a session with draft-backed tabs, when LushText launches, then orphan cleanup runs after tabs are visually restored
- Given a file opened after startup, when `check_draft_on_open` is called, then it falls back to background draft read (preloaded map is empty)

## Verification

**Commands:**
- `make test` -- expected: all tests pass
- `make check` -- expected: no clippy warnings, formatting clean

**Manual checks (if no CLI):**
- Launch app with 1+ draft tabs, observe "Draft Changes Restored" bar appears noticeably faster
- Verify orphan cleanup still runs (check `tracing` output or observe drafts dir cleanup after ~2s)

## Spec Change Log

- **Review finding: preloaded_drafts clear timing.** The initial `clear()` at end of `restore_tabs` would have run before async file-load callbacks fired, defeating the preloaded fast path for file-backed tabs. Moved clear to `schedule_orphan_cleanup` (T+2s), after all file loads complete. KEEP: the preloaded → consume → clear lifecycle.
- **Review finding: orphan cleanup manifest race.** Wholesale manifest replacement in the cleanup callback could overwrite concurrent mutations (autosave). Changed to merge-back pattern: compute removed IDs, apply only those removals to the live manifest. Skipped disk save — next autosave persists. KEEP: merge-back pattern for deferred cleanup.
- **Review finding: mtime validation.** Pre-existing: drafts applied without checking file mtime. Documented in `docs/next/draft-mtime-validation.md` for future work.

## Suggested Review Order

**Core optimization — batch preload**

- Entry point: reworked background task now batch-reads all drafts alongside manifest+session
  [`session.rs:157`](../../crates/lushtext-core/src/ui/window/session.rs#L157)

- New `preloaded_drafts` field on window imp struct
  [`imp.rs:93`](../../crates/lushtext-core/src/ui/window/imp.rs#L93)

**Consumer changes — preloaded-first pattern**

- `check_draft_on_open` tries preloaded map before background read
  [`session.rs:425`](../../crates/lushtext-core/src/ui/window/session.rs#L425)

- `check_draft_by_id` same pattern for untitled tabs
  [`session.rs:257`](../../crates/lushtext-core/src/ui/window/session.rs#L257)

- Extracted `apply_draft` shared helper
  [`session.rs:289`](../../crates/lushtext-core/src/ui/window/session.rs#L289)

**Deferred orphan cleanup — merge-back pattern**

- 2s delayed cleanup with ID-based merge instead of wholesale replace
  [`session.rs:303`](../../crates/lushtext-core/src/ui/window/session.rs#L303)

**Tests**

- Merge-back preserves concurrent manifest additions
  [`draft.rs:275`](../../crates/lushtext/tests/integration/draft.rs#L275)

- Batch preload reads matching drafts / skips missing
  [`draft.rs:371`](../../crates/lushtext/tests/integration/draft.rs#L371)

- Preloaded draft consumed by check_draft_by_id
  [`window.rs:1072`](../../crates/lushtext/tests/widget/window.rs#L1072)

**Documentation**

- Future work: draft mtime conflict detection
  [`draft-mtime-validation.md`](../../docs/next/draft-mtime-validation.md)
