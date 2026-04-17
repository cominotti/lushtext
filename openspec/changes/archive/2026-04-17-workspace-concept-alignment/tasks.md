## 1. Single-root workspace model and persistence

- [x] 1.1 Replace the persisted workspace shape with a single-root directory contract plus an explicit persisted workspace-scope selection that can represent either one workspace or `All workspaces`.
- [x] 1.2 Implement legacy workspace normalization so older multi-root and standalone-file workspace data is converted into single-root workspaces safely before rendering or re-saving.
- [x] 1.3 Update workspace persistence and integration tests for single-root restore, empty-shell restore, scope fallback to `All workspaces`, and latest-state-wins persistence.

## 2. Shared workspace scope plumbing

- [x] 2.1 Move current workspace scope out of sidebar-local filter state and into an explicit shared window-level scope that the sidebar selector reflects and edits.
- [x] 2.2 Add clear scope-aware and structure-aware sidebar APIs or callbacks so downstream consumers can react separately to scope changes and workspace-structure changes.
- [x] 2.3 Define the create or unlist or replace-root flows around the shared scope contract so creating a workspace selects it, replacing a root preserves selection, and removing the selected workspace falls back to `All workspaces`.

## 3. Sidebar shell and section alignment

- [x] 3.1 Rework the sidebar shell so the no-workspace state intentionally renders only the fixed top affordance row and no placeholder workspace section.
- [x] 3.2 Align workspace sections to the single-root contract by removing any remaining multi-root or file-root assumptions from section creation, replace-root flows, and section-local state.
- [x] 3.3 Keep the current section-level affordances coherent under the new contract, including the fixed selector row, section header actions, and drill-down or back navigation behavior.
- [x] 3.4 Update workspace-tree refresh and watcher assumptions so each section refreshes exactly one persisted root lineage and normalized sibling workspaces stay independent.

## 4. Workspace-aware consumer updates and verification

- [x] 4.1 Update search-panel scoping and file-palette indexing to honor the shared workspace scope instead of always aggregating all workspace roots.
- [x] 4.2 Update workspace-aware note, bookmark, annotation, and export flows to honor the same shared workspace scope.
- [x] 4.3 Add widget and integration coverage for scope selection, empty-shell behavior, scope restore or fallback, normalized legacy workspaces, and scope-aware search or palette results.
- [x] 4.4 Update nearby docs and contracts such as `README.md` or `AGENTS.md` if they still describe workspaces as multi-root collections or describe the selector as a sidebar-only filter.
