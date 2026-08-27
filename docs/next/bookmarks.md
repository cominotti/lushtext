# Bookmarks: Shipped Scope and Future Work

## Status: Shipped

LushText ships saved-file bookmarks as a lightweight navigation aid. The core
user promise is in place:

- bookmarks never modify the source file
- bookmarks persist outside version-controlled content
- bookmarks restore automatically when a saved file reopens

## Shipped Scope

- Per-file bookmark gutter indicators backed by `GtkSourceMark`
- Optional bookmark labels
- `F2` / `Shift+F2` next/previous navigation inside the active file
- Searchable workspace bookmark browser
- Save-file-only workflow with explicit feedback for untitled tabs

### Lifecycle Guarantees

- Bookmark sidecars migrate across **in-app sidebar renames**
- **Save As** starts a new note identity instead of copying the old one
- Normal open flows and session restore reload sidecars automatically

## First-Release Limitations

- Notes still use **path-based identity**, so external filesystem moves or copies
  performed outside LushText behave like a new file identity.

## Good Next Steps

1. Add richer sidecar reconciliation for externally moved or heavily rewritten
   files, likely by combining path identity with content fingerprints.

   **This is a confirmed, reproducible data-reachability defect, not only a
   feature idea.** Slot 5a's data-safety pass recorded it as finding **H-7**
   (HIGH, needs-decision). Sidecars are keyed only by an FNV hash of the
   canonical path (`model/sidecar_identity.rs:34-39`), and sidecar migration runs
   from exactly one place — the in-app sidebar rename callback. The filesystem
   watcher classifies external renames for tree refresh only and calls no
   `move_path_tree`, **even though it already carries both paths**
   (`services/workspace_watch.rs:73`, `:481-483`). So a `mv`, a `git mv`, or a
   file-manager move makes the note unreachable from every UI surface, silently
   and permanently.

   Path-keying is itself the *safer* choice — inode-keying would lose notes on
   every external atomic save — which is why this is a product decision rather
   than a mechanical bug, and why slot 5a deliberately did not take it silently.
   Two candidate closes: correlate the watcher's `RenameMode::Both` into the same
   ledger-tracked migration the in-app rename uses, or add an orphaned-notes
   recovery affordance so an unreachable sidecar can be re-homed by the user.
2. Consider a command-palette note mode if bookmark counts grow enough that the
   dedicated browse dialogs feel too separated from file search.
