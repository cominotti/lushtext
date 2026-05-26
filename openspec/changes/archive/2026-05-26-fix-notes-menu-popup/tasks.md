## 1. Menu Activation Fix

- [x] 1.1 Remove or neutralize the `Notes` menu `notify::active` refresh path that rebuilds the menu model during popup opening.
- [x] 1.2 Preserve normal `refresh_notes_menu_state()` updates for tab, workspace, bookmark, annotation, and cursor-state changes.
- [x] 1.3 Avoid unnecessary `set_menu_model()` calls when the existing menu already represents the current bookmark-toggle label, if needed to keep popup activation stable.

## 2. Regression Coverage

- [x] 2.1 Add a widget test that makes the `Notes` button visible, opens it through the actual menu-button popup path, and asserts the popup opens.
- [x] 2.2 Extend the popup-opening test to cover the dynamic `Add Bookmark` and `Remove Bookmark` label states.
- [x] 2.3 Keep existing label, ordering, action-sensitivity, shortcut, and contextual-entry tests green.

## 3. Verification

- [x] 3.1 Run the targeted headless widget tests for `Notes` menu behavior.
- [x] 3.2 Run `make check`.
- [x] 3.3 Run `openspec validate fix-notes-menu-popup --strict`.
