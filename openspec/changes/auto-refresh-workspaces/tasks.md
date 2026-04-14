## 1. Watcher foundation

- [x] 1.1 Add a service-layer workspace watcher module and any required dependency wiring for recursive filesystem notifications on workspace roots.
- [x] 1.2 Implement debounced, section-friendly watcher events that coalesce bursty create/remove/rename activity into refresh requests.
- [x] 1.3 Add service-level tests covering root-directory watching, file-root watching, and event coalescing or failure handling.

## 2. Workspace-section refresh pipeline

- [x] 2.1 Add the new `Refresh` header button in `workspace-section.ui` immediately to the left of the existing replace-root button and wire it through `workspace_section` without changing the fixed-row layout contract.
- [x] 2.2 Add a single refresh controller to `LushtextWorkspaceSection` that both the manual button and automatic watcher events use, including debounce and in-flight generation guards.
- [x] 2.3 Implement path-scoped subtree reloads for affected loaded directories and a whole-section fallback for root-shape or ambiguous changes.
- [x] 2.4 Scope automatic watch targets to the currently materialized sidebar tree instead of recursively watching every descendant under broad configured roots.
- [x] 2.5 Filter watcher noise so access-only and content-only events do not trigger sidebar refresh.
- [x] 2.6 Reconcile refreshed child stores in place instead of clearing and repopulating them, so unchanged rows stay mounted.

## 3. State preservation and feedback

- [x] 3.1 Snapshot and restore drill-down scope, expanded rows, and selected paths across refreshes when those paths still exist.
- [x] 3.2 Start, update, and tear down workspace watcher subscriptions as section roots change, drill-down state changes, and widgets are disposed.
- [x] 3.3 Surface user-visible feedback for watcher startup/runtime failures and manual refresh failures while keeping the current tree usable.
- [x] 3.4 Defer watcher startup off the initial render path so workspaces and restored tabs appear before scoped watch setup begins.

## 4. Verification and follow-up

- [x] 4.1 Add widget tests for header button placement, manual refresh behavior, and preserved section state after refresh.
- [x] 4.2 Add integration or deterministic refresh tests that simulate external create/remove/rename events and verify the sidebar tree updates correctly.
- [x] 4.3 Regenerate dependency metadata if needed and update any relevant docs or architecture notes to describe automatic workspace refresh behavior.
- [x] 4.4 Verify broad-root startup no longer fails immediately on unreadable deep descendants and that collapsed roots are not re-expanded by manual refresh.
- [x] 4.5 Verify access-only watcher noise no longer triggers refresh and that subtree refreshes stay visually stable for create/remove/rename changes.
