## 1. Notes Menu State

- [x] 1.1 Update `refresh_notes_menu_state()` so the header-bar `Notes` menu remains visible whenever the header bar is visible, including no-tab and no-workspace windows.
- [x] 1.2 Keep `notes-show-notes` enabled independently from active-editor and workspace-folder availability.
- [x] 1.3 Preserve existing sensitivity for `notes-toggle-bookmark`, `notes-open-document-note`, and `notes-open-folder-note`.
- [x] 1.4 Preserve the existing menu model, sections, dynamic bookmark label, primary-menu exclusion, and popup stability behavior.

## 2. Widget Coverage

- [x] 2.1 Add coverage proving a restored-workspace window keeps the `Notes` menu visible and `Browse Notes…` enabled after the last tab closes.
- [x] 2.2 Add coverage proving an empty no-workspace, no-tab window still shows the `Notes` menu with `Browse Notes…` enabled.
- [x] 2.3 Add coverage proving activating `Browse Notes…` from the no-workspace no-tab state opens the existing empty Notes browser and does not materialize fake note rows.
- [x] 2.4 Keep existing coverage for concrete workspace, aggregate scope, saved-document, untitled-document, popup placement, and dynamic bookmark-label states passing.

## 3. Validation

- [x] 3.1 Run `cargo fmt --all`.
- [x] 3.2 Run focused widget tests for the Notes menu and Notes browser states.
- [x] 3.3 Run `openspec validate make-notes-menu-window-scoped --strict`.
- [x] 3.4 Run `git diff --check`.
