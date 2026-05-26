## Context

The completed `simplify-notes-entry-points` change made the `Notes` menu dynamic so the bookmark toggle can read `Add Bookmark` or `Remove Bookmark`. The current implementation rebuilds the menu model from `refresh_notes_menu_state()`, and the button also refreshes state from its `notify::active` handler when the user opens the menu.

That creates a fragile activation path: clicking `Notes` can mark the `GtkMenuButton` active, trigger a refresh, replace the `menu-model`, and cancel or invalidate the popup GTK was about to show. The bug is a popup lifecycle issue, not a note-storage issue.

## Goals / Non-Goals

**Goals:**

- Make clicking the visible `Notes` button reliably open the menu popup.
- Preserve the dynamic bookmark toggle label and existing sensitivity behavior.
- Avoid replacing the `GtkMenuButton` menu model while GTK is opening or showing its popover.
- Add regression coverage that exercises the actual popup/open path.

**Non-Goals:**

- Do not change the final simplified `Notes` menu contents.
- Do not change bookmark, document-note, workspace-note, or range-note persistence.
- Do not change command-palette or shortcut actions.
- Do not introduce new note workflows.

## Decisions

### 1. Do not rebuild the menu model from the `active` notification path

The menu should be ready before the user clicks it. State changes that affect menu contents already flow through tab selection, workspace-scope refreshes, bookmark changes, annotation changes, and cursor mark updates. The `notify::active` handler should not rebuild or replace the menu model while GTK is opening the popover.

Alternative considered: keep the active handler and delay the rebuild with an idle callback. That still risks the menu changing while open and makes behavior timing-dependent.

### 2. Keep dynamic labeling in normal state refreshes

The bookmark label can still be rebuilt from `refresh_notes_menu_state()` as long as that refresh happens before activation or after state changes outside the popup-open path. If the implementation needs extra safety, it can avoid calling `set_menu_model()` when the label and structure are unchanged.

Alternative considered: replace dynamic labels with a static `Toggle Bookmark`. That would avoid rebuild pressure, but it would lose the user-facing clarity introduced by the simplification work.

### 3. Test the popup contract directly

Existing tests inspect menu labels and action sensitivity, but that does not prove GTK can actually open the popover. Add a widget test that creates a visible context for the `Notes` button, invokes the menu button's popup/open path, and asserts that the button becomes active or its popover is visible.

Alternative considered: only test that the menu model exists. That is the gap that allowed this regression through.

## Risks / Trade-offs

- [Menu label can be stale if no state refresh fires before opening] -> Preserve existing refresh calls on cursor movement, bookmark changes, tab switches, and workspace-scope changes; add a focused test for label state separately from popup opening.
- [Avoiding refresh on active could hide a last-moment cursor update] -> Cursor mark changes already call `refresh_notes_menu_state()`, so the popup path should not need a second rebuild.
- [Widget popup visibility can be display-sensitive] -> Use the existing headless widget runner and assert stable GTK state (`active`/popover visible) rather than fragile pixel geometry.
