## Why

The command palette is currently efficient for keyboard-only use, but its mode selector is not discoverable for mouse users because it can only be changed with Tab. File results also mix sources in a way that makes already-open documents harder to return to and does not clearly communicate whether the workspace bucket reflects the selected workspace or the aggregate workspace scope.

## What Changes

- Replace the command palette's mode label with a mouse-usable selector for `All`, `Files`, and `Commands`, while preserving Tab as a keyboard shortcut for cycling modes.
- Present file-oriented palette results in labeled source groups:
  - `Open Tabs` first for matching open file-backed tabs.
  - `Selected Workspace` when the sidebar scope targets one workspace.
  - `All Workspaces` when the sidebar scope is the aggregate `All workspaces` scope.
- In `All` mode, present groups in this order: `Open Tabs`, workspace-scope files, then `Commands`.
- Deduplicate file paths across source groups so an open tab does not also appear in the workspace bucket.
- Keep command execution and file activation behavior unchanged after a result is selected.

## Capabilities

### New Capabilities

- `command-palette-source-groups`: Defines command palette mode selection and grouped result presentation across open tabs, current workspace scope files, and commands.

### Modified Capabilities

- `workspace-scope`: Clarifies that the command palette's workspace file bucket follows the current shared workspace scope, while open file-backed tabs are available as a separate active-document source before workspace-indexed results.

## Impact

- Affected UI: command palette template, result row factory, mode selector interaction, result grouping presentation, and keyboard/mouse behavior.
- Affected window integration: command palette needs a snapshot of open file-backed tabs and current workspace scope metadata in addition to the existing file index.
- Affected services/models: palette search may need source-aware result grouping while preserving existing fuzzy matching and command registry behavior.
- Affected tests: command palette widget tests, workspace-scope integration tests, and any expectations around palette mode labels/placeholders.
