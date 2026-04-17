## Context

LushText currently exposes bookmark and annotation workflows through the primary menu in `resources/ui/window.ui`, alongside app-wide controls, file actions, print, view options, and custom theme and zoom widgets. The underlying workflow code is already split cleanly: `crates/lushtext-core/src/ui/window/notes.rs` distinguishes saved-document actions from workspace-scope browse and export flows, while `actions.rs` already registers stable window actions and shortcuts for those commands.

The design problem is therefore not storage, sidecar identity, or feature semantics. It is shell organization. The current primary menu is overloaded, and the notes actions are mixed into a surface that the GNOME HIG reserves primarily for app-wide actions. The solution should improve discoverability and grouping without changing the bookmark and annotation data model or duplicating workflow logic.

The first implementation pass also exposed a narrower shell-contract gap: the artifacts established that `Notes` should become a secondary menu, but they never said where it must sit relative to `Main Menu`. In the current UI, that omission allows `Notes` to render to the right of the hamburger, which weakens the GNOME menu hierarchy instead of clarifying it.

## Goals / Non-Goals

**Goals:**
- Move bookmark and annotation commands out of the primary menu and into a dedicated secondary menu in the header bar.
- Keep the app-wide `Main Menu` as the outermost end-aligned menu and place `Notes` immediately to its left when both are rendered in the header bar.
- Organize note workflows by scope so current-document commands and workspace-wide commands read as distinct groups.
- Reuse the existing `win.*` note actions, dialogs, shortcuts, and command-palette entries rather than inventing parallel workflows.
- Make the menu surface reflect real availability through visibility and sensitivity, so users are not funneled into avoidable error messages from the menu itself.

**Non-Goals:**
- Redesign bookmark or annotation persistence, sidecar identity, export format, or browse-dialog behavior.
- Introduce new bookmark or annotation actions beyond reorganizing the existing workflows.
- Rework the broader main-menu structure outside the note-related removals needed for this change.
- Resolve the separate GNOME HIG follow-up about placing the primary menu above the sidebar instead of in the header bar.

## Decisions

### 1. Add a single `Notes` secondary menu to the header bar

LushText will add one dedicated secondary menu for bookmark and annotation workflows, implemented as a separate header-bar `GtkMenuButton` instead of another section inside the primary menu.

This matches the GNOME HIG split between primary menus for app-wide actions and secondary menus for actions tied to the current view or content item. It also avoids nested submenus inside the primary menu, which would make the menu harder to scan and would move in the opposite direction of the requested simplification.

While the primary menu remains in the header bar, it keeps ownership of the far-right edge. `Notes` becomes the secondary menu immediately to its left, so the visual hierarchy remains "contextual menu, then app-wide menu" instead of allowing the contextual menu to appear outside the app-wide one.

Alternatives considered:
- Keep the primary menu and just rename or reshuffle the notes section: cheaper, but it leaves the main structural problem in place.
- Add a nested `Notes` submenu inside the primary menu: saves header-bar space, but conflicts with HIG guidance against nested submenus.
- Add separate `Bookmarks` and `Annotations` menu buttons: too much header-bar weight for two tightly related features.
- Place `Notes` to the right of `Main Menu`: visually possible, but it reverses the expected GNOME menu hierarchy by putting a contextual menu outside the app-wide one.

### 2. Organize the menu by action scope, not by storage type

The `Notes` menu will use two sections:
- Current document: `Toggle Bookmark`, `Edit Bookmark Label…`, `Add Annotation…`, `Edit Annotation…`
- Workspace: `Browse Bookmarks…`, `Browse Annotations…`, `Export Annotations…`

Grouping by scope mirrors the way the code already behaves in `notes.rs`: some workflows operate on the active saved editor, while others operate on the current workspace scope. This keeps the menu understandable even for users who do not yet know which actions require a saved document and which act across the workspace.

Alternatives considered:
- Group by feature (`Bookmarks` section, `Annotations` section): visually tidy, but it mixes active-document and workspace actions in each section.
- Flatten all note actions into one list: simplest markup, but the current problem is that the actions already read as an undifferentiated block.

### 3. Reflect availability through menu state instead of dead-end clicks

The `Notes` menu surface will be context-aware:
- The menu button is shown only when the window can surface at least one note workflow, meaning there is an active editor or a workspace scope.
- Menu items use sensitivity to reflect actionability from the menu surface.
- Saved-file actions are insensitive when the active document does not have a stable file path.
- Cursor-specific edit actions can become insensitive when the current cursor is not on an eligible bookmark or annotation.
- Workspace actions are insensitive when no workspace scope exists.

This keeps the UI aligned with GNOME guidance to prefer insensitive unavailable actions over sending users into predictable warnings. Existing guards in the workflow code remain important for shortcuts, command-palette invocation, and other call sites outside the menu itself.

Alternatives considered:
- Leave all menu items clickable and rely on status messages: lowest implementation cost, but preserves unnecessary friction in the reorganized surface.
- Hide individual invalid menu items instead of disabling them: produces a shorter menu, but makes the feature set feel unstable and harder to learn.

### 4. Preserve workflow semantics and integration points

This change will reuse the existing note actions, browse dialogs, annotation editor, export flow, shortcuts, and command-palette commands. The main behavioral delta is where the actions are presented and how the header-bar/menu state is synchronized.

That keeps the change low risk: the data model, services, and persistence rules remain unchanged, and the implementation can focus on shell wiring, menu models, and tests.

Alternatives considered:
- Introduce a new shell-specific notes controller or rewrite the note actions around a fresh abstraction: unnecessary for a UI-only reorganization and would add risk without clear user benefit.

## Risks / Trade-offs

- [Another header-bar control could crowd narrow layouts] → Keep the change to one menu button, place it with the other end-aligned controls, and avoid splitting notes into multiple buttons.
- [Template declaration order can diverge from rendered button order] → Treat the menu placement as a rendered-shell contract and add widget coverage that checks `Notes` never appears to the right of `Main Menu`.
- [Context-aware sensitivity can drift from the real workflow guards] → Reuse existing action state where possible and add targeted widget coverage for saved-file, cursor-context, and workspace-scope transitions.
- [The primary menu remains broader than the GNOME ideal even after removing notes] → Treat this change as a focused cleanup and leave broader main-menu reform as a separate follow-up.
- [Hiding the button when nothing is actionable can reduce discoverability in the empty state] → Accept that trade-off because a dead or fully disabled secondary menu is more confusing than helpful before the user opens a document or adds a workspace.

## Migration Plan

No data migration is required. The rollout is a UI-only reorganization:
1. Add the new `Notes` secondary menu button and menu model.
2. Keep `Main Menu` as the outermost end-aligned menu and place `Notes` immediately to its left when both are visible.
3. Move note-related menu items out of the primary menu.
4. Wire menu visibility and action sensitivity to current editor and workspace state.
5. Update widget and integration tests to cover the new structure and the rendered header-bar order.

Rollback is straightforward: restore the primary-menu items and remove the `Notes` secondary menu wiring. Bookmark and annotation data, sidecars, and workspace exports are unaffected either way.

## Open Questions

None.
