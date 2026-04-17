## Why

LushText's primary menu now mixes app-wide controls, document actions, view options, and seven bookmark and annotation commands in one long surface. That makes the notes workflows harder to discover and pushes the primary menu away from the GNOME HIG guidance that app-level menus stay concise, grouped, and focused on app-wide actions while document-specific actions move into a secondary menu.

## What Changes

- Introduce a dedicated document-scoped `Notes` secondary menu in the header bar for bookmark and annotation workflows.
- Keep `Main Menu` as the outermost end-aligned header-bar menu while `Notes` appears immediately to its left whenever both are visible in the header bar.
- Move bookmark and annotation actions out of the primary menu and regroup them by scope inside the new `Notes` menu.
- Keep current-document note actions separate from workspace-scope browse and export actions so the menu reads clearly and mirrors the real behavior of each workflow.
- Preserve existing shortcuts, command-palette entries, saved-file guards, and workspace-scope behavior while improving menu placement and labeling.
- Keep the primary menu focused on app-wide and general window actions instead of adding nested note submenus.

## Capabilities

### New Capabilities
- `document-notes-menu`: Expose bookmark and annotation workflows through a GNOME-native secondary menu that is scoped to the active document and organized by action scope.

### Modified Capabilities
None.

## Impact

- Affected UI resources and shell wiring in `resources/ui/window.ui` and `crates/lushtext-core/src/ui/window/`.
- Header-bar behavior, rendered menu placement, and menu-state updates for saved-file and workspace-aware note actions.
- Widget and integration coverage for menu organization, visibility, and action availability.
- No new external dependencies or storage-format changes.
