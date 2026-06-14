## 1. Tab-State Projection

- [x] 1.1 Define a window-owned sidebar row-state snapshot for open file identities and the active file identity, using existing file path and canonical identity data where available.
- [x] 1.2 Add a sidebar API that accepts the row-state snapshot and forwards it to every mounted workspace section without exposing window internals to `workspace_section/`.
- [x] 1.3 Add section-local storage and centralized predicates for open-file, active-file, and ordinary file-tree row presentation.

## 2. Row Rendering

- [x] 2.1 Update workspace-section row setup/bind/unbind to include stable open/active indicator presentation without changing row height, disclosure layout, or label ellipsizing.
- [x] 2.2 Scope CSS so ordinary clicked rows do not retain persistent selected-row fill while hover, press, and keyboard focus remain visible inside the workspace file tree.
- [x] 2.3 Ensure open/active indicators apply only to file rows and are reset for folders, placeholders, empty states, and recycled row widgets.

## 3. Synchronization Hooks

- [x] 3.1 Refresh sidebar row-state projection after tab open, duplicate-tab focus, selected-page changes, tab detach/close, session restore, and first-load failure cleanup.
- [x] 3.2 Refresh sidebar row-state projection after Save As, sidebar rename path updates, and file/folder delete closures.
- [x] 3.3 Run an explicit realized-row resync after row-state updates and after folder load, refresh, collapse/expand, scope filter hide/show, and row recycling transitions.

## 4. Widget Coverage

- [x] 4.1 Add widget coverage proving pointer-clicked folder and file rows do not keep misleading persistent row emphasis after hover/press ends.
- [x] 4.2 Add widget coverage proving keyboard focus and Space-to-peek still work after selection paint is decoupled from internal selection.
- [x] 4.3 Add widget coverage for open and active indicators across open, tab switch, close, first-load failure, Save As, sidebar rename, delete, and session restore flows.
- [x] 4.4 Add widget coverage for overlapping folder trees, row recycling, hidden/shown workspace scopes, empty workspaces, no-workspace state, long/deep labels, dense trees, and constrained sidebar width.

## 5. Verification

- [x] 5.1 Run `cargo fmt --all -- --check`.
- [x] 5.2 If Blueprint or generated UI files changed, run `make blueprint-generate` and `make check-blueprint`.
- [x] 5.3 Run targeted widget tests covering workspace sidebar row states.
- [x] 5.4 Run `make test-widget-headless` and treat any `FLAKY:` output as blocking.
- [x] 5.5 Run `make visual-geometry-smoke` if CSS or rendered sidebar geometry changes need screenshot-level proof.
- [x] 5.6 Run `openspec validate refine-workspace-sidebar-row-states --strict`.
