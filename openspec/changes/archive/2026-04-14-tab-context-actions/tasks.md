## 1. Tab menu scaffolding

- [x] 1.1 Add a dedicated `ui/window/tabs.rs` workflow module and wire `mod.rs`, `imp.rs`, and `window.ui` or menu setup so `AdwTabView` exposes a native context menu for tab pages.
- [x] 1.2 Register window actions for `Pin` or `Unpin`, `Close All Tabs to the Right`, `Close Other Tabs`, `Move Left`, and `Move Right`, and refresh the clicked-page target plus enabled state when the menu opens.
- [x] 1.3 Keep the menu labels and target-page behavior correct for background tabs so actions operate on the clicked tab without regressing the current selected-page workflow.

## 2. Pin state and session persistence

- [x] 2.1 Extend `SessionTab` with backward-compatible pin metadata and add unit coverage for deserializing older session data without that field.
- [x] 2.2 Update session collection and restore so pinned tabs and unpinned tabs each keep their saved relative order while restoring pinned tabs ahead of unpinned tabs.
- [x] 2.3 Trigger debounced session saves when tab pin state or tab order changes, not only on selection and close events.

## 3. Safe bulk-close behavior

- [x] 3.1 Implement helpers that compute eligible bulk-close targets from the clicked tab, current tab order, and pin state while excluding pinned tabs from the bulk-close set.
- [x] 3.2 Reuse `show_save_changes_dialog()` and `save_editors_for_close()` to confirm bulk-close operations once before closing any modified target tabs.
- [x] 3.3 Close resolved target pages in a stable order that preserves existing cleanup behavior for drafts, monitors, `open_paths`, status refresh, and session updates.

## 4. Directional movement and pin actions

- [x] 4.1 Implement `Pin` or `Unpin` so pinning updates the tab's segment placement immediately and keeps pinned tabs grouped at the leading edge.
- [x] 4.2 Implement `Move Left` and `Move Right` with segment-boundary guards so pinned tabs move only among pinned tabs and unpinned tabs move only among unpinned tabs.
- [x] 4.3 Add any focused shell feedback or indicator updates needed so pinned state and boundary-disabled actions stay understandable from the tab strip and context menu.

## 5. Verification and contract alignment

- [x] 5.1 Add unit coverage for session persistence and target-set computation across pinned and unpinned combinations.
- [x] 5.2 Add widget or integration coverage for context-menu targeting, bulk-close confirmation behavior, and move-action boundary states.
- [x] 5.3 Update nearby module-map docs or guidance files if the new window workflow module changes the documented shell structure, then run the relevant Rust and GTK test targets.
