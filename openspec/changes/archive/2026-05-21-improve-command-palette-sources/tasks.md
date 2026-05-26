## 1. Mode Selector Interaction

- [x] 1.1 Replace the command palette's passive mode label with a `GtkDropDown` listing `All`, `Files`, and `Commands`.
- [x] 1.2 Wire dropdown selection changes to the existing `SearchMode` state, placeholder text, and result rebuild flow.
- [x] 1.3 Preserve Tab and Shift+Tab mode cycling by updating the same dropdown-backed state and keeping the search entry keyboard flow intact.

## 2. Grouped Result Model

- [x] 2.1 Extend the palette item/presentation model to represent non-activatable source headers in addition to file and command rows.
- [x] 2.2 Render group headers as labeled separators and ensure keyboard movement and activation skip header rows.
- [x] 2.3 Add grouped result assembly for `Files` mode: `Open Tabs`, then the current workspace-scope file group.
- [x] 2.4 Add grouped result assembly for `All` mode: `Open Tabs`, then the current workspace-scope file group, then `Commands`.
- [x] 2.5 Deduplicate file paths so open file-backed tabs suppress duplicate workspace-indexed file rows.

## 3. Window And Scope Integration

- [x] 3.1 Provide the palette with a current snapshot of open file-backed tabs, including display name, subtitle/path, and absolute path.
- [x] 3.2 Provide the palette with the current workspace-scope label, using `Selected Workspace` for concrete scopes and `All Workspaces` for the aggregate sidebar scope.
- [x] 3.3 Refresh grouped palette results when open tabs, Save As path changes, sidebar file operations, or workspace scope changes alter the available sources.
- [x] 3.4 Keep existing file and command activation behavior unchanged after grouped rows are introduced.

## 4. Tests And Validation

- [x] 4.1 Add or update command palette widget tests for mouse mode selection, Tab synchronization, placeholder updates, and focus behavior.
- [x] 4.2 Add grouped result tests for `Files` mode ordering, `All` mode ordering, workspace-scope labels, de-duplication, and non-activatable headers.
- [x] 4.3 Add or update window/workspace-scope integration coverage for concrete workspace and aggregate `All workspaces` palette behavior.
- [x] 4.4 Update affected UI strings, CSS, or local documentation if the implementation changes visible text or layout conventions.
- [x] 4.5 Run focused widget tests for the command palette and workspace-scope behavior.
- [x] 4.6 Run `cargo clippy --workspace --all-targets -- -D warnings` and `./scripts/run-widget-tests.sh --auto`.
- [x] 4.7 Run `openspec validate improve-command-palette-sources --strict`.
