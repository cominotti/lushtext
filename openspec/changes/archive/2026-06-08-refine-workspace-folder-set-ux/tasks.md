## 1. Name-First Workspace Creation

- [x] 1.1 Add or reuse a model/service helper that creates `WorkspaceConfig { id, name, folders: [] }`, selects the new workspace scope, and keeps workspace identity based on `WorkspaceId` rather than name.
- [x] 1.2 Replace `LushtextSidebar::create_new_workspace()` in `crates/lushtext-core/src/ui/sidebar/dialogs.rs` with a Libadwaita name-entry modal titled `New Workspace`.
- [x] 1.3 Validate the modal entry by trimming whitespace, rejecting empty names, keeping `Create` recoverable when invalid, and leaving cancel as a no-op.
- [x] 1.4 Replace `handle_new_workspace(path)` in `crates/lushtext-core/src/ui/sidebar/workspaces.rs` with a name-based empty-workspace creation path that persists, rebuilds or appends the new section, refreshes the selector, selects the new workspace, and notifies scope/structure consumers.
- [x] 1.5 Keep `show_add_folder_dialog()` as the only folder-picker path for workspace membership and ensure it still appends folders through the existing duplicate-canonical-folder checks.
- [x] 1.6 Remove or rename folder-first new-workspace helpers and test hooks such as `handle_workspace_folder_selection()` and `select_workspace_folder_for_test()` so test-only API names match the new concept.
- [x] 1.7 Update visible copy, tooltips, status messages, comments, and test names that describe `New Workspace` as opening or selecting a folder.

## 2. Real Folder Row Presentation

- [x] 2.1 Remove the synthetic one-folder `Files` presentation helper from `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs` so top-level workspace folder rows use their actual folder-derived labels.
- [x] 2.2 Remove the synthetic folder-landmark icon path from `workspace_section/icon_presentation.rs` when it is no longer used, while preserving normal directory, file, and placeholder icons.
- [x] 2.3 Ensure one-folder and multi-folder workspace sections share the same row presentation, context-menu targeting, tooltip/full-path exposure, and folder-note/remove/reorder action behavior.
- [x] 2.4 Ensure zero-folder workspaces keep the explicit empty folder-set label and never render a selectable, expandable, draggable, or context-menu-capable fake folder row.
- [x] 2.5 Preserve the no-horizontal-scrollbar contract with long folder labels, duplicate basenames, the new drag handle, row controls, and tooltips or equivalent full-path affordances.
- [x] 2.6 Audit code, resources, tests, comments, README, `AGENTS.md`, and OpenSpec text for synthetic workspace-membership labels including `Files`, `Files & Folders`, `FOLDER_LANDMARK_ICON_NAME`, and `workspace_folder_row_display_name`; keep only intentional non-sidebar uses such as command-palette `Files` mode or ordinary file-tree terminology.

## 3. Reorder-Only Drag And Drop UX

- [x] 3.1 Add a compact drag handle or equivalent explicit reorder affordance to top-level workspace folder rows without placing it where deep `GtkTreeExpander` indentation can push it out of view.
- [x] 3.2 Limit drag initiation to the explicit reorder affordance or another top-level-folder-only reorder surface; descendant file rows, descendant directory rows, placeholder rows, empty states, headers, and drill-down rows must not initiate workspace-folder reorder drags.
- [x] 3.3 Keep DnD payloads based on stable `WorkspaceId` and `WorkspaceFolderId`, and reject drops whose payload workspace does not match the target section.
- [x] 3.4 Add above/below insertion indicator state for valid top-level folder targets using drag motion position; never show a centered drop-into-folder state.
- [x] 3.5 Clear insertion indicator state on drag leave, failed drop, successful drop, cancellation, model rebuild, and `GtkListItem` unbind so recycled rows cannot retain stale drop feedback.
- [x] 3.6 Reject descendant-row, placeholder-row, empty-state, section-header, drill-down, and cross-workspace drops without changing workspace order or showing a valid insertion indicator.
- [x] 3.7 Route successful drops through the existing absolute-index folder reorder callback, then persist through the latest-state-wins workspace persistence path and notify workspace-aware consumers.
- [x] 3.8 Preserve Move Up and Move Down folder context actions as the non-pointer reorder path, with accessible labels/tooltips and the same persisted reorder behavior as DnD.
- [x] 3.9 Ensure reorder code does not call filesystem mutation helpers or path-writing helpers; it may only mutate workspace metadata order and UI presentation state.
- [x] 3.10 Suppress folder-row expansion, collapse, child-store materialization, focus/drill-down, selection, and workspace-watch restart side effects while an active workspace-folder reorder drag is hovering over sidebar rows; drag motion may update only the transient insertion indicator.
- [x] 3.11 Replace or refine the insertion indicator presentation so valid before/after targets render exactly one smooth rounded horizontal line, with no filled rectangular accent area, duplicate overlapping line, row drop highlight, or centered drop-into-folder cue.

## 4. Workspace Section Collapse

- [x] 4.1 Add an explicit chevron, disclosure button, or equivalent collapse/expand affordance near the workspace header label in `resources/ui/workspace-section.ui`.
- [x] 4.2 Track workspace-section collapsed state separately from `GtkTreeListRow::expanded` folder-row state so collapsing the section body does not expand or collapse individual folder rows.
- [x] 4.3 Make collapse hide the section body, including the folder tree, empty-folder-set label, and drill-down body/header presentation, while keeping the workspace header, label, Add Folder, Refresh, and header context menu reachable.
- [x] 4.4 Make expand restore the previous folder-body presentation where possible, including expanded folder rows and active drill-down state that still refer to existing rows.
- [x] 4.5 Preserve collapsed state across ordinary in-window section rebuilds for the same `WorkspaceId`, including add, remove, reorder, manual refresh, and scope-filter rebuild paths.
- [x] 4.6 Keep collapsed state out of the workspace-state JSON payload and avoid introducing a workspace persistence migration for this UI-only state.
- [x] 4.7 Replace or demote the existing header double-click folder-toggle behavior so the explicit disclosure affordance is the primary path and any remaining double-click gesture performs the same section-body collapse/expand behavior.
- [x] 4.8 Ensure section collapse does not change current workspace scope, workspace-aware consumer coverage, folder membership, folder order, filesystem content, or persisted workspace metadata.
- [x] 4.9 Preserve constrained-width behavior with the new disclosure control, long workspace names, Add Folder, Refresh, and header context-menu targeting.

## 5. Tests

- [x] 5.1 Add unit/service coverage for creating an empty named workspace, trimming names, rejecting empty names, assigning a stable workspace ID, selecting the new scope, and saving/reloading the empty folder set.
- [x] 5.2 Add sidebar widget tests for opening the new-workspace name modal, confirming a valid name, canceling without mutation, and attempting an empty/whitespace name without creating a workspace.
- [x] 5.3 Add sidebar widget tests proving a newly created empty workspace is selected immediately, renders its header and empty folder-set state, and keeps `Add Folder` reachable.
- [x] 5.4 Add sidebar/widget tests proving `Add Folder` after empty workspace creation appends the selected folder, preserves existing duplicate-folder feedback, and does not recreate the workspace.
- [x] 5.5 Add workspace-section widget tests proving a one-folder workspace displays the actual folder label/path instead of `Files`, and a multi-folder workspace displays each configured folder in stored order without a fake grouping row.
- [x] 5.6 Add widget tests for zero-folder, one-folder, many-folder, long-label, duplicate-basename, overlapping-folder, and constrained-width sidebar states, including no horizontal scrollbar and no control/label overlap.
- [x] 5.7 Add workspace-section collapse widget tests proving the header disclosure toggles the body, keeps header controls reachable, hides/restores the zero-folder empty state, reduces many-folder vertical footprint, and preserves previous folder-row expansion or drill-down state when expanded again.
- [x] 5.8 Add collapse regression tests proving section collapse does not change current workspace scope, command-palette/search/notes/browser coverage, folder order, folder membership, or workspace persistence payload shape.
- [x] 5.9 Add DnD tests for visible top-level reorder handles, absence of handles on descendant rows, above and below insertion indicator states, and indicator cleanup on invalid drop or row recycling.
- [x] 5.10 Add DnD/action reorder tests for before-row and after-row drops, same-workspace success, descendant-row rejection, drill-down rejection, cross-workspace rejection, and unchanged order on invalid targets.
- [x] 5.11 Add a filesystem-boundary regression test with fixture folders and sentinel files proving drag-and-drop reorder and Move Up/Move Down do not create, delete, move, copy, rename, or rewrite filesystem content.
- [x] 5.12 Add accessibility/keyboard coverage proving workspace-section collapse, non-pointer reorder, and the new drag affordance remain reachable and have meaningful accessible labels or tooltips.
- [x] 5.13 Run existing command-palette and notes/browser focused tests to prove this UX refinement did not change `Files` mode, workspace result de-duplication, folder-note targeting, or notes-browser document-row de-duplication.
- [x] 5.14 Add a DnD hover regression test proving active reorder hover over top-level folders, descendant folders, and expander regions does not change any folder-row expanded state, does not materialize child stores solely because of hover, and does not restart workspace watches solely because of hover.
- [x] 5.15 Add focused indicator coverage proving a valid reorder target exposes a single fixed-height rounded insertion line and no filled rectangular accent area, duplicate line, GTK row drop highlight, or drop-into-folder cue; prefer semantic widget/CSS allocation assertions, adding screenshot or pixel checks only if the harness can make them reliable.

## 6. Validation

- [x] 6.1 Run `rg -n "workspace root|WorkspaceRoot|workspace_root|root_paths|Open Workspace Note|workspace-note|WorkspaceNote|workspace_note" crates resources README.md AGENTS.md openspec` and document or remove any domain/user-facing root-language regressions introduced by this change.
- [x] 6.2 Run `rg -n "Files & Folders|FOLDER_LANDMARK_ICON_NAME|workspace_folder_row_display_name|Open Folder|select_workspace_folder_for_test|handle_workspace_folder_selection" crates resources README.md AGENTS.md openspec` and confirm remaining hits are intentional or removed.
- [x] 6.3 Run `cargo fmt --all -- --check`.
- [x] 6.4 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 6.5 Run focused non-widget tests for workspace model and manager changes, including `cargo test -p lushtext-core workspace`.
- [x] 6.6 Run focused widget tests with `scripts/run-widget-tests.sh --headless -- sidebar` and `scripts/run-widget-tests.sh --headless -- workspace_section`.
- [x] 6.7 Run focused command-palette and notes/browser regression tests with the existing harness or exact test filters touched by this change.
- [x] 6.8 Run `make check-policy` after filesystem-boundary and terminology changes.
- [x] 6.9 Run `openspec validate refine-workspace-folder-set-ux --type change --strict`.
- [x] 6.10 Run `openspec validate --changes --strict`, `openspec validate --specs --strict`, and `git diff --check`.
- [x] 6.11 After the DnD hover and insertion-indicator fixes land, rerun focused workspace-section widget tests, relevant sidebar tests, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, OpenSpec strict validation, and `git diff --check`.
