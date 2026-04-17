## 1. Header Bar Notes Menu

- [x] 1.1 Add a dedicated `Notes` secondary menu button and menu model to the window header bar, and remove bookmark/annotation entries from the primary menu resource.
- [x] 1.2 Organize the `Notes` menu into current-document and workspace sections with the exact bookmark and annotation actions defined by the spec.

## 2. Context-Aware Menu State

- [x] 2.1 Wire `Notes` menu button visibility to the active editor and workspace-scope state so the button only appears when at least one notes workflow is available.
- [x] 2.2 Update note-related action sensitivity so saved-file requirements, cursor-specific edit eligibility, and workspace-scope requirements are reflected directly in the `Notes` menu.
- [x] 2.3 Keep the existing note dialogs, exports, shortcuts, and command-palette entries working after the menu reorganization.

## 3. Verification

- [x] 3.1 Add or update widget coverage for the header-bar `Notes` menu, grouped menu contents, and the removal of note actions from the primary menu.
- [x] 3.2 Add or update interaction coverage for unsaved documents, saved documents, missing cursor-note context, and missing workspace scope so menu sensitivity matches the spec.
- [x] 3.3 Run targeted tests and a live menu smoke test to verify the new organization and action availability behave correctly in the app shell.

## 4. Header-Bar Ordering Follow-Up

- [x] 4.1 Adjust the header-bar shell so `Main Menu` remains the outermost end-aligned menu and `Notes` appears immediately to its left whenever both are visible.
- [x] 4.2 Add or update widget coverage that asserts the rendered header-bar order prevents `Notes` from appearing to the right of `Main Menu`.
- [x] 4.3 Re-run the relevant window widget coverage after the ordering fix so the new placement contract is explicitly verified.
