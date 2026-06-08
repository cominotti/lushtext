## Context

The window already has a dedicated `Notes` menu button, menu-only `notes-*` actions, and a unified `Browse Notes…` dialog. The browser can list workspace-scoped rows, supplemental saved open-tab rows, or an explicit empty state when no browsable notes exist. The current header menu visibility is narrower than that browser capability: it is hidden unless an active editor exists or the current workspace scope exposes folders.

This creates a mismatch after the last tab closes. The user can still reasonably want to browse folder notes, and the browser already knows how to explain an empty notes set, but the primary visual entry point disappears.

## Goals / Non-Goals

**Goals:**

- Make the `Notes` header-bar button visible as a stable window-level notes entry point whenever the header bar is visible.
- Keep `Browse Notes…` enabled from the header menu with no open tabs and with no restored workspace folders.
- Preserve sensitivity for document-specific and concrete-workspace-specific menu items.
- Add widget coverage for no-tab and no-workspace no-tab states.

**Non-Goals:**

- Do not change sidecar schemas, note migration, note browser search, or note browser row grouping.
- Do not add a second notes index to the command palette.
- Do not move note commands back into the primary menu.
- Do not create notes, bookmarks, workspaces, or document-note sidecars merely by opening an empty browser.

## Decisions

### Treat the header menu as a window-level entry point

The `Notes` menu button should be visible when the header bar is visible, independent of active editor and workspace-folder count. This matches the existing browser behavior: `show_notes_dialog()` can produce useful results from workspace folders, saved open tabs, or a clear empty dialog.

Alternative considered: keep hiding the button when there are no tabs and no workspace folders. That keeps the header slightly quieter, but it creates an inconsistent mental model where `Browse Notes…` is sometimes an app-level feature and sometimes invisible based on unrelated tab state.

### Keep action sensitivity at the row level

The menu should keep its existing sections and labels, but the rows express applicability:

- `Browse Notes…` remains enabled.
- `Add Bookmark` / `Remove Bookmark` and `Open Document Note…` require an active saved document.
- `Open Folder Note…` requires a concrete workspace folder target.

Alternative considered: hide unavailable rows. Keeping the rows visible but insensitive makes the menu stable and avoids layout/section churn when tabs open, close, or workspace scope changes.

### Reuse the existing empty Notes browser

No new empty-state flow is needed. When no workspace folders, open-tab note rows, bookmarks, folder notes, or document notes exist, `Browse Notes…` should open the existing `No notes yet` browser state.

Alternative considered: disable `Browse Notes…` with no data. That saves one click, but it blocks the only visible explanation of why the notes surface is empty.

## Risks / Trade-offs

- [Risk] A permanently visible `Notes` menu adds one stable header item even for brand-new windows with no notes. → Mitigation: the menu remains compact and the empty browser explains that notes will appear after bookmarks or notes are saved.
- [Risk] Users may try disabled document/workspace rows from an empty window. → Mitigation: sensitivity accurately communicates which workflows need a saved document or concrete workspace, while `Browse Notes…` remains the enabled fallback.
- [Risk] Updating visibility during tab close could miss the last-tab transition. → Mitigation: add widget coverage that closes the last tab and asserts the menu remains visible with `Browse Notes…` enabled.
