# Session Time Travel (Local History)

## Status: Implemented MVP

## Summary

LushText now ships a narrow local-history MVP for saved, file-backed documents. The feature sits
between short-lived undo and full version control: it automatically records
restore points, lets users browse them in a GTK-native history browser, and
restores earlier text safely without writing directly to disk.

## MVP Scope

- Saved, file-backed documents only
- Automatic snapshot capture at meaningful edit boundaries
- Adaptive history browser with snapshot list plus read-only preview
- Restore into the active buffer, not directly to disk
- Immediate undo path after restore
- Canonical-path identity with in-app rename migration
- Large-file-aware capture limits

## Recommended UX

- Trigger local history from the main menu, command palette, `Ctrl+Alt+L`, the
  sidebar file context menu, or the editor content context menu for the active
  saved document
- Open an adaptive `AdwDialog` browser rather than reusing the narrow properties
  pane
- Use a snapshot list sorted newest-first with a read-only preview of the
  selected snapshot
- Provide `Restore` and `Copy` actions in the browser
- On narrow windows, adapt into a navigation flow instead of squeezing list and
  preview side by side
- Keep explicit inner spacing inside the preview text surface so the snapshot
  content never sits flush against the frame edge

## Snapshot Model

- Store full UTF-8 text snapshots, not diffs, for the MVP
- Use stable canonical-path identity instead of `DefaultHasher`
- Keep one local-history lineage per saved document
- Skip duplicate snapshots when the candidate content matches the latest stored
  snapshot
- Keep fixed retention caps in code for the MVP rather than exposing user-tuned
  storage controls yet

## Capture Policy

- Capture a baseline snapshot when a clean saved document first becomes dirty in
  an editing cycle
- Capture additional snapshots no more than once every 5 minutes while the
  document remains modified
- Capture a snapshot after each successful save
- Keep all snapshot I/O off the GTK main thread

## Restore Safety

- Before restoring a selected snapshot, first store the current buffer as a
  fresh safety snapshot
- Apply the selected historical text to the editor buffer
- Mark the editor modified after restore so the user still chooses whether to
  save it
- Surface an immediate undo path instead of requiring a confirmation dialog for
  every restore

## Identity and Path Behavior

- In-app rename of a file or parent directory should migrate the document's
  local-history lineage
- `Save As` should start a new lineage for the new path instead of merging the
  previous history automatically

## Large-File Policy

- Up to 10 MB: full MVP behavior
- Above 10 MB and up to 50 MB: save-boundary snapshots only
- Above 50 MB: local history unavailable

These thresholds intentionally align with LushText's existing large-file safety
policy instead of inventing a separate one for local history.

## Suggested Follow-Ups

- Diff and compare UI for selected snapshots
- Local history for untitled documents
- Workspace-wide history browser across multiple files
- Richer retention and storage controls
- Additional timeline metadata, filtering, and search

## Notes

- This feature is not a substitute for git or other version-control tools
- The MVP intentionally favors a robust, GTK-native recovery workflow over a
  feature-rich compare experience
- Current storage lives under `$XDG_DATA_HOME/lushtext/local-history/`
