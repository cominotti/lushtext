## 1. Reorder Handle Projection

- [x] 1.1 Extract or centralize the workspace-folder reorderability check so row binding and explicit synchronization use the same rule.
- [x] 1.2 Reapply reorder-handle visibility and sensitivity to realized `GtkListView` rows after workspace folder membership changes.
- [x] 1.3 Invoke the synchronization from top-level workspace folder load, incremental add, remove, reorder, and in-place refresh paths without forcing unnecessary full section rebuilds.
- [x] 1.4 Ensure the synchronization leaves drag shields inert outside an active reorder drag and clears stale insertion or valid-drop state when a row becomes non-reorderable.
- [x] 1.5 Confirm the implementation does not change persisted workspace JSON, workspace scope semantics, folder-note identity, or filesystem contents.

## 2. Widget Test Matrix

- [x] 2.1 Add or refine widget-test helpers that locate realized folder rows and drag handles by path without depending on stale recycled row instances.
- [x] 2.2 Cover empty workspace sections and first-folder add: no reorder handles, empty/folder body state is readable, and workspace header actions remain reachable.
- [x] 2.3 Cover the reported live transition: adding a second folder updates both visible top-level rows immediately, without sidebar hide/show, scope switch, manual refresh, or restart.
- [x] 2.4 Cover reverse transitions: removing from two folders to one hides the remaining handle, and removing all folders leaves no fake rows, handles, insertion indicators, or valid-drop state.
- [x] 2.5 Cover reorder completion through drag/drop test seams and Move Up/Move Down: visible order matches persisted order and all remaining top-level rows keep correct handle visibility.
- [x] 2.6 Cover descendant file rows, descendant directory rows, drill-down rows, and row recycling so reorder handles and insertion state never leak to non-top-level rows.
- [x] 2.7 Cover section collapse/expand and workspace scope filter hide/show after add, remove, and reorder so handles reflect the current folder set when the section is visible again.
- [x] 2.8 Cover dense and awkward sidebar states: many folders, long names, overlapping folder paths, and constrained width keep handles, labels, context menus, add-folder, refresh, and header actions reachable with no horizontal scrollbar.
- [x] 2.9 Cover invalid reorder targets after synchronization: child rows, drill-down rows, one-folder workspaces, empty sections, and cross-workspace rows reject drops without insertion feedback or order mutation.
- [x] 2.10 Keep interaction assertions behavior-focused: disclosure, activation, file peek, context menus, focus-folder, and inline rename must still work through their existing paths after synchronization.

## 3. Verification

- [x] 3.1 Run the smallest relevant widget tests for `workspace_section` through the headless widget runner; rerun any new or changed test in isolation until it is stable.
- [x] 3.2 Run `make test-widget-headless` and treat any `FLAKY:` output as a blocker to investigate, not as a clean pass.
- [x] 3.3 Run `cargo fmt --all -- --check`.
- [x] 3.4 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 3.5 Run `openspec validate stabilize-workspace-folder-reorder-handles --strict`.
- [x] 3.6 Run `git diff --check`.
