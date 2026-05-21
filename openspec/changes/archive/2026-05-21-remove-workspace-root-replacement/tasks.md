## 1. Sidebar Header UI

- [x] 1.1 Remove the workspace-section replace-root button from `resources/ui/workspace-section.ui` so `Refresh` is the only right-side header button.
- [x] 1.2 Remove the replace/add-folder template child, click wiring, callback field, and notification methods from `workspace_section`.
- [x] 1.3 Update refresh button state and tooltip logic so it no longer manages replacement/add-folder button state.
- [x] 1.4 Remove the sidebar replace-root dialog flow and section callback forwarding that invokes it.

## 2. Workspace Model And Persistence

- [x] 2.1 Remove `WorkspacesFile::replace_root` and any tests that assert in-place root replacement.
- [x] 2.2 Add or update tests that cover the supported remove-and-add workflow for using a different root.
- [x] 2.3 Confirm current-scope fallback and latest-state persistence behavior still pass after replacement removal.

## 3. Tests And UI Contracts

- [x] 3.1 Update workspace-section widget tests to assert `Refresh` is the rightmost header-control button.
- [x] 3.2 Update symbolic-icon tests so sidebar controls cover New Workspace, Refresh, drill-down back, and Focus Folder without `Replace Workspace Root`.
- [x] 3.3 Remove or rewrite add-folder/replace-root callback tests that no longer match the product surface.
- [x] 3.4 Run focused workspace model, sidebar widget, and persistence tests affected by the change.

## 4. Documentation And Spec Alignment

- [x] 4.1 Update root `AGENTS.md` sidebar and workspace-note guidance to remove replace-root wording.
- [x] 4.2 Update code comments and service docs that describe `Replace Workspace Root` as supported behavior.
- [x] 4.3 Search for `Replace Workspace Root`, `replace root`, `replace_root`, and `add_folder_button`, then remove or rewrite every stale reference.

## 5. Verification

- [x] 5.1 Run `openspec validate remove-workspace-root-replacement --strict`.
- [x] 5.2 Run `cargo fmt --all`.
- [x] 5.3 Run `cargo test --workspace`.
- [x] 5.4 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.5 Run `./scripts/run-widget-tests.sh --auto`.
