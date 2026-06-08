## Context

The canonical workspace specs now define a workspace as a named, ordered folder set that may contain zero folders. The remaining rough edges are in the sidebar's presentation layer:

- `create_new_workspace()` still opens a folder picker titled `Open Folder`.
- `handle_new_workspace()` still derives the workspace name from the selected folder path.
- A one-folder workspace can render the top-level configured folder as a synthetic `Files` row.
- Workspace sections already have code-level group expand/collapse helpers and a header double-click gesture, but that behavior is hidden and operates on folder-row expansion rather than presenting a discoverable workspace-section body collapse.
- Folder drag-and-drop already uses stable workspace and folder IDs, but it does not give a strong visible cue that the operation is only reordering top-level workspace folder memberships.

This change is intentionally narrower than the folder-set redefinition. It refines how users create, read, and reorder workspace folders without changing the workspace JSON format, search/palette de-duplication semantics, notes/browser behavior, or folder-note identity.

## Goals / Non-Goals

**Goals:**

- Make `New Workspace` create a named zero-folder workspace through a name-entry modal.
- Keep `Add Folder` as the only folder-picker workflow for workspace membership.
- Show actual configured workspace folders as actual folder rows, including in the one-folder case.
- Remove the synthetic `Files` top-level row/icon from workspace folder membership presentation.
- Add an explicit workspace-section collapse affordance that hides a workspace's folder body while keeping the header controls available.
- Make drag-and-drop reorder visually explicit with a top-level folder drag affordance and above/below insertion feedback.
- Ensure invalid drop targets never look valid and never mutate workspace state.
- Preserve the existing non-pointer reorder path for keyboard and assistive users.
- Add focused tests that prove the UI behavior, state mutation, persistence path, and no-filesystem-mutation contract.

**Non-Goals:**

- Do not introduce a new workspace persistence format or migration.
- Do not add workspace-level drag-and-drop reordering. Only folders inside a workspace are reorderable.
- Do not de-duplicate the sidebar tree. Overlapping workspace folders continue to render literally in the sidebar.
- Do not persist workspace-section collapsed/expanded UI state in the workspace folder-set JSON payload.
- Do not rename the command-palette `Files` mode or change command-palette source-group ordering.
- Do not change notes/browser folder-set behavior. Existing folder-note and document-row de-duplication semantics remain in force.
- Do not create, delete, move, copy, or rename user files as part of folder reorder.
- Do not add an external drag-and-drop, modal, or reorder dependency.

## Decisions

### 1. Split workspace creation from folder membership

`New Workspace` should present a Libadwaita dialog with a single workspace-name entry and `Cancel`/`Create` responses. The create response trims the entry text, rejects empty names, creates a workspace with an empty folder list, selects the new workspace as the shared current scope, rebuilds or appends the section, persists, and notifies workspace-aware consumers.

The folder picker moves out of the creation path. Users add folder membership only through the workspace section's `Add Folder` action, which keeps duplicate canonical-folder checks scoped to the target workspace.

Names remain display labels, not identity. The stable `WorkspaceId` continues to identify workspaces. This change should not silently rewrite a user's provided name for uniqueness; duplicate-name policy can be revisited as a separate UX decision if it becomes confusing in the selector.

Alternative considered: keep the current folder picker but ask for a name afterward. That still teaches the old concept, where a workspace begins life as one folder. The model now permits zero folders, so creation should start from the workspace itself.

### 2. Preserve an empty workspace as a useful section

After creating a workspace, the section should render immediately with its header controls and the existing explicit empty-folder-set state. The visible path for the next action is the section's `Add Folder` button, not a chained folder dialog.

The existing scope behavior remains correct: the new workspace is selected immediately even though it has no folders, and workspace-aware consumers see empty folder coverage until a folder is added.

Alternative considered: keep users in `All workspaces` after creating an empty workspace. That would make the new section easy to miss and would weaken the mental model that the selector controls the current shared workspace scope.

### 3. Treat top-level folder rows as real folder identities

The folder-set list should not substitute a synthetic row named `Files` for the configured folder. Top-level folder rows should display the real folder's normal display name and expose the configured path through tooltip, accessibility metadata, context menu targeting, or equivalent inspection affordance. One-folder and multi-folder workspaces should follow the same row model.

A generic `Folders` label can be used only as non-interactive section copy or accessibility grouping if needed. It must not become a fake tree row, a context-menu target, or a drag target. `Files & Folders` should not be introduced because it blurs the distinction between workspace membership folders and the files contained by those folders.

Alternative considered: rename `Files` to `Files & Folders`. That avoids one misleading word but keeps a fake row that hides the actual configured folder. The better fix is to show the real folder.

### 4. Make workspace-section collapse explicit

Each workspace header should become a disclosure row for that workspace section's body. The affordance should sit near the workspace name, use a familiar chevron/disclosure icon, and expose clear accessible labels such as `Collapse Workspace` and `Expand Workspace`.

Collapsing a workspace section hides the section body: the folder tree, empty-folder-set label, and any drill-down body/header presentation. It does not hide the workspace header, add-folder button, refresh button, or workspace context menu. It also does not mutate folder membership, folder order, individual folder-row expansion state, current shared workspace scope, search scope, command-palette behavior, notes/browser scope, or filesystem content.

This collapse state should be held as UI state on the sidebar/section and preserved across ordinary in-window section rebuilds where the same `WorkspaceId` still exists. It should not be written into the workspace-state payload in this change; app restart may restore sections expanded unless a later preference/persistence change decides otherwise.

The current header double-click toggle can either be removed or made secondary to the explicit control, but the explicit chevron must be the discoverable path. If double-click remains, it should perform the same section-body collapse/expand behavior rather than a separate "expand every folder row" behavior that teaches a second model.

Alternative considered: keep only the current double-click behavior that expands or collapses all top-level folder rows. That saves UI space, but it is undiscoverable and does not reduce clutter for a workspace whose top-level folder rows themselves are the clutter. A section-level body collapse better matches the workspace header as the user's organizing unit.

### 5. Make DnD reorder-only by construction and presentation

Drag-and-drop should remain model-state driven: payloads carry stable workspace and folder IDs, and the reorder callback mutates the in-memory `WorkspacesFile` order before persisting through the existing latest-state-wins pipeline.

The row presentation should make the operation unambiguous:

- A top-level workspace folder row exposes a drag handle or equivalent reorder affordance.
- Drag initiation is limited to that affordance or to another explicit top-level-folder reorder surface.
- Drop targets are valid only on top-level workspace folder rows in the same workspace and outside drill-down mode.
- Drag motion over a valid row shows one horizontal insertion indicator above or below the target row based on pointer position.
- Drag hover is presentation-only. While a workspace-folder reorder drag is active, row-surface drop targets should own hover before `GtkTreeExpander` sees it, including over descendant rows, expander regions, invalid targets, and other non-drop rows. Hover must not expand or collapse any `GtkTreeListRow`, materialize child stores, focus or drill down into folders, change selection, or restart workspace watches because of hover-induced expansion.
- Invalid rows, descendant file/directory rows, placeholders, empty states, section headers, and other workspaces show no insertion indicator and reject the drop.
- The UI never presents a centered "drop into this folder" state for workspace folder reorder.
- The insertion indicator should be a dedicated single-line presentation surface: a smooth rounded horizontal line at the before/after edge. The row must not show a filled rectangular accent area, duplicate overlapping line, GTK row drop highlight, or any other feedback that can read as "drop into this folder".

The existing Move Up/Move Down actions remain the non-pointer path and must call the same reorder/persist/notify path as DnD.

Alternative considered: allow dragging from the whole top-level row. That is simpler, but it competes with expand, activate, context-menu, peek, and drill-down interactions. A visible reorder affordance makes the gesture discoverable without making ordinary tree interaction feel dangerous.

### 6. Keep filesystem operations out of reorder

Reorder must not call filesystem mutation helpers. No file or directory should be created, deleted, moved, renamed, copied, or otherwise modified when a folder is reordered. The only durable change is the workspace metadata order.

Tests should use filesystem fixtures to create real folders and sentinel files, perform reorder through the same public/test entry points as the UI, and assert that all paths and sentinel contents remain in place afterward.

Alternative considered: rely on code review because the current reorder path is already state-only. The visual bug is specifically that DnD can look like a filesystem move, so regression tests need to prove the contract explicitly.

## Risks / Trade-offs

- [Risk] A name-first modal adds a second step before users can browse files. Mitigation: immediately select the new workspace and keep the section's `Add Folder` action visible in the empty state.
- [Risk] Duplicate workspace names can make the selector ambiguous. Mitigation: continue using stable workspace IDs for identity and leave name uniqueness as a separate product decision.
- [Risk] DnD indicators on virtualized `GtkListView` rows can leave stale CSS on recycled row widgets. Mitigation: clear indicator classes/state on leave, drop, cancel, and list-item unbind.
- [Risk] Row-level drop targets can interact poorly with `GtkTreeExpander` and expand folder rows while the pointer hovers during drag. Mitigation: track active workspace-folder reorder drags, let inert row-surface drop targets accept hover for every row during the drag, keep `TreeExpander` targetability stable, and guard expansion/watch side effects so drag hover is inert except for insertion indicator changes.
- [Risk] Painting the entire overlay widget as the insertion indicator can render as a filled accent rectangle plus a line when GTK allocates more height than intended. Mitigation: render the indicator as a transparent outer surface with one fixed-height rounded line, or use an equivalent dedicated drawing/separator widget that cannot paint the surrounding allocation.
- [Risk] Section collapse can accidentally be conflated with folder-row expansion and lose the user's prior tree state. Mitigation: keep workspace-section collapsed state separate from `GtkTreeListRow::expanded` state and add tests that expanding the section restores the previous tree presentation.
- [Risk] A new disclosure icon can crowd the workspace header beside Add Folder and Refresh. Mitigation: place it near the label, keep it compact, preserve label ellipsizing, and cover constrained widths with widget tests.
- [Risk] A drag handle can reduce room for long folder labels. Mitigation: use a compact icon, preserve ellipsizing, keep tooltips/full paths, and cover constrained widths with widget tests.
- [Risk] Visual DnD tests can be brittle under headless GTK. Mitigation: test semantic CSS/state helpers and public row-affordance visibility where possible, with a focused screenshot or pixel check only if the harness already supports it reliably.
- [Risk] Tightening drag initiation can make existing broad-row drag tests obsolete. Mitigation: update tests to use the new explicit handle or direct test helper for absolute-index reorder logic.

## Migration Plan

1. Add a sidebar helper for creating an empty named workspace, reusing the existing persist, rebuild, scope-select, and notification pipeline.
2. Replace the `New Workspace` folder picker with a name-entry modal that calls the empty-workspace helper after validation.
3. Keep `show_add_folder_dialog()` as the folder picker for existing workspace sections.
4. Remove the synthetic `Files` presentation path and icon constant, and ensure top-level folder rows use normal folder presentation.
5. Add an explicit workspace-section disclosure control and separate section-body collapsed state from individual folder-row expansion state.
6. Add a drag affordance and single-line insertion indicator state to top-level folder rows, own active reorder hover at the row surface before expanders can react, suppress tree expansion side effects during active reorder hover, and clear indicator state on invalid targets and row recycling.
7. Preserve Move Up/Move Down action behavior and route DnD through the existing absolute-index reorder callback.
8. Add the focused tests described in `tasks.md` and run the OpenSpec and code validation ladder.

Rollback is straightforward because no user data format changes. Reverting the implementation restores the old folder-first creation and row presentation without needing metadata conversion.

## Open Questions

None for this proposal. Command palette and notes/browser behavior are intentionally left as-is because the existing folder-set contracts already cover scope resolution, duplicate file suppression, and folder-note targeting.
