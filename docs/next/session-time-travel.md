# Session Time Travel (Local History)

## Status: Proposed

## Description
Automatic periodic snapshots of file states that let users browse and restore previous
versions of any file from their editing session. A lightweight local history between
"undo" (lost when the session ends) and "git" (requires explicit knowledge). Presented
as a simple timeline UI — not branches, not diffs, just "go back to how this file
looked 15 minutes ago."

## Current State
- Draft persistence saves unsaved buffer content to `$XDG_DATA_HOME/lushtext/drafts/`
  every 30 seconds for modified files
- Draft manifest (`manifest.json`) tracks metadata per draft (original path, mtime)
- Only the latest draft is kept — no history
- Undo history lives in GtkSourceView's `GtkSourceUndoManager` and is lost on tab close

## Motivation
Writers and casual users lose work constantly because undo history is ephemeral and
version control is a foreign concept. Even developers occasionally wish they could see
"what this file looked like an hour ago" without git archaeology. The gap between
per-session undo and full VCS is vast — local history fills it with zero user effort.

## Implementation Plan

### Phase 1: Snapshot Storage (services/local_history.rs)
1. Storage location: `$XDG_DATA_HOME/lushtext/history/<file_hash>/`
2. Each snapshot is a plain text file named `<timestamp_epoch_ms>.txt`
3. `LocalHistoryService` with methods:
   - `save_snapshot(path: &Path, content: &str) -> Result<()>`
   - `list_snapshots(path: &Path) -> Result<Vec<SnapshotEntry>>`
   - `load_snapshot(path: &Path, timestamp: u64) -> Result<String>`
   - `cleanup_old_snapshots()` — retention policy enforcement
4. File identification uses the same `DefaultHasher` → hex string as drafts
5. Snapshots are taken:
   - On every explicit save (`Ctrl+S`)
   - On file open (snapshot the original state)
   - Periodically (every 5 minutes for modified buffers, alongside draft saves)

### Phase 2: Retention Policy
1. Keep all snapshots from the last 24 hours
2. Keep hourly snapshots for the last 7 days
3. Keep daily snapshots for the last 30 days
4. Delete everything older than 30 days
5. Per-file cap: 200 snapshots (safety net for pathological cases)
6. Global storage cap: 500MB — oldest snapshots pruned first when exceeded
7. Cleanup runs at startup (same pattern as draft orphan cleanup)

### Phase 3: Timeline UI (ui/local_history/)
1. New `LushtextLocalHistory` widget — a side panel or dialog
2. Triggered via command palette ("Show Local History") or menu
3. Timeline view: vertical list of snapshot entries sorted newest-first
4. Each entry shows: relative time ("5 minutes ago"), absolute timestamp, file size delta
5. Selecting a snapshot shows a read-only diff view against current buffer content
6. "Restore" button replaces current buffer with snapshot content (marks as modified)
7. "Copy to Clipboard" as a non-destructive alternative

### Phase 4: Diff View
1. Use GtkSourceView's built-in line marks or gutter renderer for inline diff markers
2. Alternatively, a simple side-by-side two-pane view using two `GtkSourceView` widgets
3. Highlight added lines (green), removed lines (red), changed lines (yellow)
4. For MVP: a single `GtkSourceView` showing the snapshot content (no diff), with a
   "Compare" button that opens a basic two-pane view

## Architecture Considerations
- Snapshots are stored as full file copies, not diffs. This is simpler, faster to
  restore, and storage is cheap. Delta compression could be added later if storage
  becomes a concern.
- The `file_hash` directory naming allows files to be renamed/moved without losing
  history — but makes history discovery for moved files non-trivial. Consider storing
  a `paths.json` that maps hashes to known paths for reverse lookup.
- Snapshot I/O must be fully async (`spawn_blocking_then`) to avoid blocking the UI,
  especially for large files.
- The retention policy cleanup should be incremental (process N files per startup, not
  all at once) to avoid slow startup on large history stores.

## Dependencies
- Existing draft infrastructure (services/draft_service.rs) — similar patterns
- Existing `spawn_blocking_then` for async I/O
- New UI widget for timeline/history browsing
- Optional: diff library (`similar` crate) for inline diff computation

## Risks
- Storage growth on machines with limited disk space. The 500MB global cap and 30-day
  retention mitigate this, but users editing many large files could still be surprised.
  A preferences toggle to disable local history is advisable.
- Performance of snapshot listing for files with many snapshots. The filesystem-based
  approach (one file per snapshot) should handle 200 entries fine, but a SQLite backend
  might be needed if the feature scales beyond simple file history.
- Users may confuse local history with version control and neglect proper VCS. The UI
  should include a subtle note that local history is not a substitute for git.
