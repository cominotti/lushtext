## Why

LushText's workspace behavior has grown in useful directions, but the current contract is split between model-era assumptions and newer sidebar UI behavior. The app still carries multi-root and file-root workspace semantics in persistence and some downstream consumers, while the sidebar, refresh flows, and shell affordances increasingly behave as if a workspace is one navigable root with one current scope. That mismatch makes the feature harder to reason about, harder to spec accurately, and harder to evolve without regressions.

This change defines the current product direction explicitly: a workspace is a real, app-wide concept with a single root, and workspace selection must drive every workspace-aware surface consistently. It also captures the intentional empty-sidebar shell so the retroactive spec matches the experience LushText already wants to present.

## What Changes

- **BREAKING** redefine a workspace as one named root directory instead of a collection of mixed directory and file roots.
- **BREAKING** rework any remaining multi-root or standalone-file workspace behavior so the canonical contract is single-root only.
- Promote workspace selection from a sidebar-local visibility filter into the app-wide workspace scope used by search, file indexing and palette results, note and export workflows, and other workspace-aware actions.
- Capture the workspace sidebar shell as an intentional product contract, including the fixed top affordance row, the empty-sidebar state when no workspaces exist, per-workspace sections, refresh and replace-root header actions, and drill-down navigation.
- Align workspace persistence and restore behavior with the new product contract so stored state, restored selection, and visible shell behavior do not contradict each other.
- Keep file peek unchanged and out of scope for this change so the new contract stays focused on workspace identity, scope, and sidebar shell behavior.

## Capabilities

### New Capabilities
- `workspace-sidebar-shell`: Defines the user-facing sidebar contract for empty state, fixed top affordance, single-root workspace sections, section header actions, and drill-down navigation.
- `workspace-scope`: Defines the app-wide current workspace scope, including how explicit workspace selection and the aggregate `All workspaces` scope affect search, indexing, palette results, and workspace-aware workflows.

### Modified Capabilities
- `workspace-state-persistence`: Change persisted workspace state from mixed-root collections to single-root workspaces, and align restored workspace selection and empty-shell behavior with the user-facing contract.
- `workspace-tree-refresh`: Align refresh behavior and section-scoped tree expectations with the single-root workspace model and the canonical sidebar-shell contract.

## Impact

- Affected code:
  - `crates/lushtext-core/src/model/workspace.rs`
  - `crates/lushtext-core/src/services/workspace_manager.rs`
  - `crates/lushtext-core/src/ui/sidebar/**`
  - `crates/lushtext-core/src/ui/window/search.rs`
  - `crates/lushtext-core/src/ui/window/focus_indexing.rs`
  - `crates/lushtext-core/src/ui/window/notes.rs`
  - related widget and integration tests under `crates/lushtext/tests/**`
- Affected systems:
  - workspace persistence and restore
  - sidebar layout and section orchestration
  - global workspace-aware scoping for search, palette, and note/export flows
- Spec impact:
  - adds `workspace-sidebar-shell`
  - adds `workspace-scope`
  - revises `workspace-state-persistence`
  - revises `workspace-tree-refresh`
