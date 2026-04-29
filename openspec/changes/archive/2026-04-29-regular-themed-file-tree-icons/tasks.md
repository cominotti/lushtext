## 1. Icon Presentation Helper

- [x] 1.1 Add a small workspace-section icon presentation helper that classifies placeholder rows, directory rows, and file rows without changing `FileTreeItem`'s persisted data model.
- [x] 1.2 Use regular themed `folder` for directory rows and keep the existing symbolic information/status icon for placeholder rows.
- [x] 1.3 Derive file-row icons from GIO path-based content-type inference and fall back to regular `text-x-generic` when inference or themed lookup is unusable.
- [x] 1.4 Ensure icon derivation avoids file-content reads and blocking filesystem work during list-item binding.

## 2. Workspace Section Row Binding

- [x] 2.1 Update the workspace-section list-item factory to bind file-tree content rows through the new icon presentation helper.
- [x] 2.2 Preserve symbolic icons for sidebar controls and non-content affordances such as New Workspace, Refresh, Replace Workspace Root, drill-down back, Focus Folder, and placeholder/status rows.
- [x] 2.3 Preserve existing row layout, input handling, expansion, selection, file peek, context menus, inline rename, refresh reconciliation, and workspace filtering behavior.

## 3. Tests

- [x] 3.1 Add helper-level tests for placeholder, directory, known file type, and unknown file type icon presentation.
- [x] 3.2 Add or update widget coverage that verifies realized file-tree rows use regular content icons while sidebar controls or placeholder/status rows remain symbolic.
- [x] 3.3 Add regression coverage that row activation/selection or file peek still works after regular icon binding.

## 4. Verification

- [x] 4.1 Run `openspec validate regular-themed-file-tree-icons`.
- [x] 4.2 Run targeted Rust/widget tests covering the icon presentation helper and workspace-section file-tree behavior.
- [x] 4.3 Run `make check` or the nearest repo-standard formatting and lint gate needed for the touched files.
