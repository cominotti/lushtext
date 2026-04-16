## 1. Local-history persistence and identity

- [x] 1.1 Add the local-history model and service layer for stable canonical-path identity, snapshot metadata, deduplicated full-text snapshot storage, retention pruning, and background-safe list/load/save helpers.
- [x] 1.2 Reuse the existing note-sidecar rename strategy so local-history data migrates after in-app file and directory renames while Save As starts a fresh lineage.
- [x] 1.3 Add service-level tests for snapshot deduplication, timestamp ordering, retention pruning, and rename migration behavior.

## 2. Automatic snapshot capture and restore safety

- [x] 2.1 Hook file-backed editors into local-history capture boundaries: first dirty transition, five-minute modified-session cadence, and successful saves, while keeping capture work off the GTK main thread.
- [x] 2.2 Apply the existing large-file thresholds to local history so files above 10 MB fall back to save-boundary capture and files above 50 MB disable local history entirely.
- [x] 2.3 Implement the restore safety flow so restoring first records the current buffer as a safety snapshot, then replaces the buffer, marks it modified, and exposes an immediate undo path.

## 3. GTK-native local-history browser

- [x] 3.1 Add a local-history browse surface built as an adaptive GTK/Libadwaita dialog with snapshot list, empty state, and read-only preview for the active saved document.
- [x] 3.2 Wire window actions, command/menu entry points, and availability state so only eligible saved documents can open local history.
- [x] 3.3 Add snapshot-level actions for Restore and Copy without introducing MVP diff or workspace-wide history browsing.

## 4. Verification and documentation

- [x] 4.1 Add GTK/widget or integration coverage for action availability, adaptive browser behavior, restore/undo flow, and empty-state handling.
- [x] 4.2 Add focused window/service coverage for baseline capture, periodic capture cadence, save-boundary capture, and large-file gating.
- [x] 4.3 Update `docs/next/session-time-travel.md` to match the MVP contract and explicitly record the deferred follow-ups: diff UI, untitled history, workspace-wide history browsing, richer retention controls, and timeline metadata polish.

## 5. Discoverability and layout polish

- [x] 5.1 Increase the local-history dialog's default size, add a keyboard shortcut, and update shortcut/help documentation so the feature is easier to reach.
- [x] 5.2 Add `Local History` to the sidebar file context menu for eligible saved files.
- [x] 5.3 Add `Local History` to the editor content context menu for eligible saved files using the native text-view extra-menu path.
- [x] 5.4 Add focused verification for the new entry points and update the permanent UI rule set so dialog-contained text surfaces always provide explicit inner spacing.
