## Context

LushText already has the major note primitives in place: saved-file bookmarks, saved-file range notes, one document note per saved file, and one workspace note per concrete workspace root. The current shell exposes them through a dedicated `Notes` secondary menu and a unified `Browse Notes...` dialog, but the menu still lists most operations directly and `Browse Notes...` omits bookmarks.

The design problem is therefore presentation and discoverability, not storage. We should keep the existing sidecar identity rules, save/rename behavior, markdown note editor surfaces, and `AdwSidebar`-based browser shell while making the top-level Notes surface easier to understand.

## Goals / Non-Goals

**Goals:**

- Make the header-bar `Notes` menu short, shallow, and task-oriented.
- Make `Browse Notes...` the single workspace-scoped discovery surface for bookmarks, workspace notes, document notes, and range notes.
- Keep `Notes` immediately left of `Main Menu` and keep note commands out of the primary menu.
- Preserve existing keyboard shortcuts and command-palette commands for fast users, including direct bookmark/range-note edit commands.
- Add contextual entry points where the object being acted on is obvious.
- Update widget coverage so menu labels, browser sections, selection/open behavior, and shortcuts documentation remain synchronized.

**Non-Goals:**

- Do not change bookmark, document-note, workspace-note, or range-note persistence formats.
- Do not change Save As or in-app rename identity semantics.
- Do not replace the main workspace file tree with `AdwSidebar`.
- Do not introduce a broad Document Activity or Inspector pane.
- Do not remove direct actions from shortcuts or command palette unless a later review shows they are redundant.
- Do not add inline rendered range notes or clickable annotation affordances in this change.

## Decisions

### 1. Treat the header menu as an entry-point menu, not a complete command list

The `Notes` menu should expose the routes users need most:

- `Browse Notes...`
- `Add Bookmark` or `Remove Bookmark`
- `Add Range Note...`
- `Open Document Note...`
- `Open Workspace Note...`
- `Export Range Notes...`

This removes low-context edit commands from the header bar while keeping creation, browsing, and major note surfaces easy to discover. `Edit Bookmark Label...`, `Edit Range Note...`, and bookmark navigation stay available through shortcuts and command palette, and can be exposed in contextual menus where eligibility is clear.

Alternative considered: keep the current complete menu but reorder it. That keeps every command visible, but still forces users to understand every note type before choosing a task.

### 2. Keep `Browse Notes...` as the umbrella browser and add a Bookmarks section

The unified browser should add a `Bookmarks` section to the existing `Workspace Notes`, `Document Notes`, and `Range Notes` sections. Bookmark rows should use the same workspace-scope filtering as the current bookmark browser, and the Open action should jump to the bookmarked file and line.

Bookmarks do not have note body text, so the preview pane should show an explicit bookmark metadata state instead of pretending there is rendered markdown. The preview should identify the label or fallback line title, workspace, file path, and line number. Search should match bookmark label, file metadata, workspace metadata, and line number.

Alternative considered: rename the menu item to `Browse Notes and Bookmarks...`. That is more literal, but longer and less clean; making bookmarks visible inside the browser resolves the mismatch while preserving the simple umbrella label.

### 3. Preserve direct expert routes outside the header menu

Removing edit-only items from the header menu should not remove workflows from the application. Existing plain `win.*` actions remain available for shortcuts and command palette. Menu-only `win.notes-*` actions can be reduced to the items still shown in the `Notes` menu.

This keeps the simplified menu friendly for discovery while preserving efficient paths for users who already know the commands.

Alternative considered: remove the direct `Browse Bookmarks`, `Edit Bookmark Label`, and `Edit Range Note` commands everywhere. That would be tidier on paper, but it would remove useful keyboard and command-palette affordances.

### 4. Add contextual entry points where scope is obvious

Context menus should expose note workflows only where the target is already clear:

- Sidebar file rows can offer `Open Document Note...` for files.
- Workspace headers can offer `Open Workspace Note...` for that concrete workspace root.
- Editor content context can offer current-line/range actions such as `Add Range Note...`, `Edit Range Note...`, `Add/Remove Bookmark`, and `Edit Bookmark Label...` when applicable.

These contextual entries should route through the same window actions or helper workflows as the header menu, not duplicate persistence or editor mutation logic.

Alternative considered: rely only on the header menu and command palette. That keeps implementation smaller, but it misses the GNOME pattern that secondary actions belong near the object they affect.

### 5. Keep browser ownership in `notes.rs`

`notes.rs` should remain the workflow owner for assembling bookmark, workspace-note, document-note, and range-note browser entries. A new `NotesBrowserEntry::Bookmark` variant can reuse `bookmark_service::list_workspace_bookmarks` and route `Open` through the existing editor-at-line helper.

The browser should continue to use `AdwDialog` plus `AdwNavigationSplitView` and `AdwSidebar`. Pointer activation should continue to preview/select only; explicit `Open` remains the editing/jump affordance.

Alternative considered: keep the standalone bookmarks browser and embed it as a separate dialog route. That preserves current code, but it leaves the core “Notes” grouping problem unresolved.

## Risks / Trade-offs

- [Bookmarks in a Notes browser can blur terminology] -> Give bookmarks their own section and preview metadata so users can distinguish markers from note bodies.
- [Removing edit items from the header menu may hide commands] -> Keep shortcuts, command palette entries, and contextual menus as the expert/actionable routes.
- [Bookmark previews do not have markdown body text] -> Render an explicit metadata state instead of an empty markdown preview.
- [Menu-only action state can drift from plain action guards] -> Keep workflow guards on the plain actions and add focused widget tests for menu sensitivity.
- [Adding contextual menus can duplicate logic] -> Route through existing `notes.rs` helpers and keep persistence in existing services.
- [Browser search can become less predictable with mixed item types] -> Match visible row text plus hidden backing metadata for every entry type, including bookmark labels and line numbers.

## Migration Plan

No data migration is required. The rollout is a UI and workflow-surface change:

1. Update the `Notes` menu model and menu-only actions to match the simplified entry-point list.
2. Add dynamic bookmark add/remove menu labeling and sensitivity.
3. Add bookmark entries to the unified Notes browser and remove the header menu's standalone `Browse Bookmarks...` item.
4. Add contextual entry points for document notes, workspace notes, and eligible editor note actions.
5. Update keyboard-shortcuts documentation for the existing `Edit Range Note` shortcut.
6. Update widget and integration coverage for the simplified menu, unified browser sections, contextual routes, and shortcut parity.

Rollback is local: restore the old menu labels/items and keep the standalone bookmark browser action as the header menu route. Existing sidecar data remains valid either way.

## Open Questions

None.
