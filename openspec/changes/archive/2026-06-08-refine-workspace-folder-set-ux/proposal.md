## Why

The folder-set workspace model is now correct, but a few sidebar interactions still feel like the old single-folder workflow: creating a workspace starts with a folder picker, a one-folder workspace can hide membership behind a synthetic `Files` row, expanded workspace sections can create avoidable vertical clutter, and drag-and-drop reordering does not clearly communicate that it is only changing sidebar order. This change tightens the user-facing workspace UX so the new concept reads as "a named workspace containing zero or more folders" from the first click.

## What Changes

- Change `New Workspace` from a folder-first flow into a name-first modal flow. Creating a workspace asks for the workspace name, creates a zero-folder workspace, selects it immediately, and leaves folder addition to the section's add-folder affordance.
- Keep folder addition separate from workspace creation. `Add Folder` remains the only folder-picker flow and appends a selected folder to an existing workspace after the existing duplicate checks.
- Remove the synthetic `Files` folder-row presentation for one-folder workspaces. Workspace sections must expose the real configured top-level folder row or rows, using actual folder names/paths and folder terminology.
- Do not rename the sidebar surface to `Files & Folders`. The sidebar's folder-set area should use precise folder membership language where a label is needed, while the existing workspace selector and section headers carry workspace identity.
- Add an explicit collapse/expand affordance to each workspace section header. Collapsing a workspace hides that section's folder body for a cleaner sidebar while keeping the workspace label, add-folder, refresh, and context-menu controls reachable.
- Refine drag-and-drop folder reorder UX so it is visibly reorder-only: top-level workspace folder rows have an explicit drag affordance, valid drops show a horizontal insertion indicator above or below the target row, and invalid child/file rows do not show a drop target.
- Ensure folder reorder never performs filesystem moves, copies, or path mutations. It only changes the persisted order of workspace folder memberships.
- Preserve the non-pointer reorder path, such as Move Up and Move Down, and require it to use the same persisted reorder logic as drag-and-drop.
- Add focused widget, service, and regression tests for the name-first creation flow, real folder labels, visual DnD affordances, invalid drops, no filesystem mutation, and constrained sidebar geometry.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `workspace-sidebar-shell`: refine workspace creation, top-level folder row presentation, workspace-section collapse, and drag-and-drop folder reorder affordances for the folder-set sidebar.

## Impact

- Affected UI code: workspace sidebar creation dialogs, workspace-section templates, workspace header collapse controls, folder row presentation, folder reorder drag-and-drop, context actions, status/feedback copy, and sidebar CSS if a disclosure icon, drop insertion indicator, or drag handle style is needed.
- Affected model/service behavior: no new persistence format is required; implementation may need a sidebar-level helper to create an empty named workspace and reuse the existing folder add/reorder persistence paths.
- Affected tests: GTK widget tests for sidebar flows, workspace-section collapse, and DnD presentation; workspace manager or sidebar state tests for empty named workspace creation; focused filesystem-boundary tests proving reorder does not mutate files; and terminology/label regression tests proving the synthetic `Files` row is gone.
- Dependencies: no new runtime dependency is expected.
