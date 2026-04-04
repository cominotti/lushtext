# Draft Restore: mtime Conflict Detection

## Status: Deferred

## Problem

When a draft is restored on startup, `check_draft_on_open` applies the draft content
unconditionally — it does not check whether the backing file's mtime has changed since
the draft was saved. The `DraftEntry.original_mtime_secs` field exists and is populated
by autosave, but is never consulted during restore.

If a user edits a file externally (in another editor) after LushText exits with unsaved
changes, the next launch applies the stale draft over the externally-modified file
content. The file monitor will separately show "File Has Changed on Disk", but the
draft content is already in the buffer — the user sees both bars simultaneously, which
is confusing.

## Proposed Behavior

During `check_draft_on_open` (or the batch preload in `load_session_and_drafts`),
compare the file's current mtime against `DraftEntry.original_mtime_secs`:

- **mtime unchanged**: Apply draft normally (file hasn't been modified externally).
- **mtime changed**: Skip draft restore. Show a different info bar message like
  "Draft discarded — file was modified externally" or silently discard. Delete the
  stale draft.
- **mtime unavailable** (file deleted, permission error): Skip draft, existing error
  handling covers file-not-found.

## Scope Notes

- Only affects file-backed tabs. Untitled tabs (`check_draft_by_id`) have no backing
  file and no mtime to compare.
- The mtime check should happen in the background thread (stat syscall can be slow on
  NFS/FUSE), not on the main thread.
- Consider whether "draft discarded" should be a user-visible notification or silent.
  GNOME Text Editor silently discards external changes — but it doesn't have drafts.

## Discovered During

Review of `spec-optimize-draft-restore-latency` (2026-04-04). Pre-existing behavior,
not a regression.
