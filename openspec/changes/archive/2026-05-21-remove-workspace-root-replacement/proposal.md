## Why

The workspace sidebar currently exposes two ways to point at a different folder: replacing a workspace root in place, or removing the workspace and adding a new one. Keeping both paths makes workspace identity harder to explain and adds an extra sidecar/persistence concept that users do not need.

## What Changes

- **BREAKING** Remove the `Replace Workspace Root` affordance from each workspace-section header.
- Keep the per-section `Refresh` button and place it in the header slot currently occupied by the replace-root button.
- Make root changes an explicit remove-and-add workflow: users remove the old workspace and create a new workspace for a different folder.
- Remove the model and sidebar flow that mutates an existing workspace ID to point at a new root.
- Update workspace-note, persistence, refresh, icon, and agent documentation so they no longer describe root replacement as supported behavior.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `workspace-sidebar-shell`: Workspace section headers expose only the section-local `Refresh` control plus the existing header context menu actions, with no replace-root control.
- `workspace-tree-refresh`: Manual refresh remains available, but its placement is now the rightmost workspace-section header control rather than a control positioned beside replace-root.
- `workspace-state-persistence`: Persisted workspace state is edited through add, remove, rename, and scope changes only; replacing a root in place is no longer a supported mutation.
- `workspace-notes`: Workspace-note identity follows canonical roots across remove/re-add and root directory renames, without a special replace-root case.
- `regular-file-tree-icons`: Symbolic sidebar-control wording no longer lists `Replace Workspace Root` as an available control.

## Impact

- Affected UI: `resources/ui/workspace-section.ui` and the workspace-section header wiring.
- Affected sidebar code: `crates/lushtext-core/src/ui/sidebar/**`, especially replace-root dialog and callback plumbing.
- Affected model/tests: `crates/lushtext-core/src/model/workspace.rs`, workspace persistence tests, and workspace-section widget tests.
- Affected docs/specs: root `AGENTS.md`, workspace sidebar/refresh/persistence/notes/icon specs, and any comments that describe replace-root behavior.
- No new dependencies or storage migrations are expected.
