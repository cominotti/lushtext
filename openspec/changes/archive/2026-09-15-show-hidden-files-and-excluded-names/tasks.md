## 1. Settings and domain rule

- [x] 1.1 Add `workspace-show-hidden-files` (`b`, default `false`) and `workspace-excluded-names` (`as`, default `['.git']`, the schema's first array-typed key) to `data/dev.cominotti.lushtext.gschema.xml` with summaries and descriptions; add the `config::keys` constants
- [x] 1.2 Add GTK-free `model/workspace_visibility.rs` with `WorkspaceEntryVisibility { show_hidden, excluded_names: Arc<HashSet<OsString>> }`, `fn admits(&self, name: &OsStr) -> bool` implementing excluded-then-dot-then-visible, `Default` reproducing today's behavior, `app_data()` (hidden off, empty set) for non-workspace scans, and a normalizing constructor (trim, reject empty and `/`, dedupe); register the module in `model/mod.rs`
- [x] 1.3 Unit-test the rule: default hides dotfiles, excluded wins over show-hidden, non-dot excluded names, exact case-sensitive basename matching, normalization rejects, and that `app_data()` equals today's behavior

## 2. Filesystem boundary and workspace scanners

- [x] 2.1 In `services/filesystem/tree.rs`, add a borrowed `visibility: &WorkspaceEntryVisibility` parameter to the three traversal entry points and replace the three `include_hidden` checks with `visibility.admits(file_name)` against the raw `OsStr`; keep `DirectoryScanPolicy` `Copy`, keep `visible_workspace()` `const`, and retire the `include_hidden` field
- [x] 2.2 Update every caller mechanically: all 18 `visible_workspace()` sites and all 24 struct-literal sites (drafts, local history, search backup, format-upgrade inventory, bookmark and folder-note sidecars, workspace quarantine, style schemes, `palette/notes.rs`) pass `WorkspaceEntryVisibility::app_data()`; the filesystem fixture helper passes a value that includes hidden entries
- [x] 2.3 Update boundary unit tests, including one proving an excluded non-dot name is skipped and one proving `app_data()` still hides dotfiles
- [x] 2.4 Thread the visibility through `services/file_tree.rs` (`scan_directory*`, `is_dir_empty`) and update its hidden-file tests to cover both modes and an excluded name
- [x] 2.5 Add a `visibility` field to `services/palette/index.rs::FileIndexBuildRequest` and thread it to the per-directory scan; keep `IGNORED_INDEX_DIRS` as an additional unconditional directory predicate; extend palette service tests for revealed dotfiles, excluded names, root folders always indexed, and the internal skip list staying active with hidden on
- [x] 2.6 Add `#[serde(default)] hidden: bool` to `model/content_search.rs::ContentSearchOptions`, include it in `toggle_summary()`, and add a `visibility: &WorkspaceEntryVisibility` parameter to `search_with_plan` (the excluded set never enters the options); in `services/content_search/search.rs` set `builder.hidden(!options.hidden)` and merge basename rejection into the **single** `filter_entry` closure (a second call would replace the overlapping-root exclusion)
- [x] 2.7 Service tests: hidden on/off, excluded names, overlapping roots still deduplicated with excluded names present, and a fixture round-trip proving `search-history.json` and `saved-searches.json` written without `hidden` load with hidden off and are not quarantined

## 3. Sidebar workflow

- [x] 3.1 In `ui/sidebar/policy.rs`, add the pure construction of `WorkspaceEntryVisibility` from the two key values; build it once per refresh pass on the GTK thread and carry it in the pending child-scan request so `workspace_section/scan_execution.rs` no longer constructs `gio::Settings` per `populate_child_store`; pass the same value to the in-scan lookahead and to the top-level folder-row `is_dir_empty` probe in `workspace_section/folder_execution.rs`
- [x] 3.2 Subscribe once at the `LushtextSidebar` level to `changed::workspace-show-hidden-files` and `changed::workspace-excluded-names`, own both handlers in a `gtk_lush_signals::SignalBag` cleared on dispose, and fan out to live sections via the automatic full-refresh path (`schedule_refresh(true, Vec::new(), false)`), not the manual path that emits a status message and announcement
- [x] 3.3 Expose any new observable facts through the existing workspace-tree evidence surface, not new `*_for_test` functions (the ceiling is at 165/165)
- [x] 3.4 Widget tests, each resetting both keys on exit: toggling on reveals dot-children in an expanded directory without remounting the `TreeListModel`; a dot-only nested folder and a dot-only top-level folder lose `(Empty)`; no status message or announcement on toggle; a removed section receives no refresh; a dot-name created in place stays visible until the next refresh; readiness is asserted with the blocker armed on the write path; the header row still holds only the selector and New Workspace at the `Small` preset

## 4. Command palette workflow

- [x] 4.1 In `ui/window/palette_shell.rs` (a called presentation surface that carries the value and decides nothing), build the visibility once per rebuild from the two keys into `FileIndexBuildRequest`, and connect both key changes to the existing debounced rebuild
- [x] 4.2 Widget tests: palette finds `.github/workflows/ci.yml` after turning hidden on; adding `vendor` to the excluded list removes those files; `node_modules` stays absent from the palette with hidden on while visible in the sidebar

## 5. App action, menu, and shortcut

- [x] 5.1 In `app.rs`, register boolean-stateful `app.show-hidden-files` whose state mirrors the GSettings key, and register `<Control><Shift>h` with `set_accels_for_action` beside `app.quit` (do not touch `ui/window/actions.rs`, which triggers the sidebar animation visual-proof matrix)
- [x] 5.2 Add a "Show Hidden Files" check item to `primary_menu` in `resources/ui/window.blp`, list the shortcut in `resources/ui/shortcuts.blp`, run `make blueprint-generate`, and regenerate `resources/ui/template-contract.json` with `python3 scripts/check-ui-template-contract.py --write-baseline`
- [x] 5.3 Add the `ACTION_CATALOG` entry (`ActionScope::App`, no parameter, `Bool` state, anchor `action-app-show-hidden-files`, at least one coverage lane)
- [x] 5.4 Add the action to `BASELINE_APP_ACTIONS` and to `STATIC_VISIBLE_ACTION_IDS` for the primary-menu surface
- [x] 5.5 Add the `docs/automation-reference.md` row modelled on `action-win-toggle-sidebar`, with all ten fields in catalog declaration order
- [x] 5.6 Add `show_hidden_files` and `excluded_names` (max 64 entries, 255 bytes each, truncation flagged) to the workspace automation snapshot with `snapshot-field-*` anchors, projection-map rows if projected from the sidebar evidence, and correct the "all ten of its fields" prose
- [x] 5.7 Widget tests: menu activation flips key and check state; `Ctrl+Shift+H` toggles the mode and does not open Find and Replace; automation activation is reflected in the snapshot including both new fields

## 6. Preferences editor

- [x] 6.1 Add the "Hidden and Excluded Items" group to the Workspace page in `resources/ui/preferences.blp` with the "Show Hidden Files" `AdwSwitchRow` (two-way bind) and the "Always Excluded Names" `AdwExpanderRow` shell; `ensure_type()` both `AdwExpanderRow` and `AdwEntryRow` in `class_init` before `bind_template()`; regenerate the `.ui` and the template contract
- [x] 6.2 In `ui/preferences/`, project one `AdwActionRow` per name from the key using the dynamic-row pattern of `data_page.rs`, wire the add `AdwEntryRow` (trim, reject empty, `/`, duplicates by focusing the existing row, accessible error description), the remove buttons, Reset to Defaults, and an explicit "No excluded names" row; write the full array on every mutation and re-project on the key's `changed` signal without duplicating rows
- [x] 6.3 Apply `accessibility::apply_row_accessibility` to every dynamic row and add metadata for the remove buttons, the add entry, and the error state per `.agents/rules/ui.md`; add the new Libadwaita-only types to widget-harness template validation
- [x] 6.4 Widget tests, each resetting both keys: add trims and appends; invalid and duplicate submissions leave the key unchanged; remove updates the key and shows the empty state; Reset restores `[".git"]`; external key change re-projects; switch row and menu check item agree

## 7. Search panel

- [x] 7.1 Add a "Hidden files" `GtkToggleButton` beside the gitignore toggle in the search panel template (regenerate the `.ui` and the template contract, noting positional path keys shift) and `imp.rs`; add `LushtextSearchPanel::seed_hidden_toggle()` and call it from both window reveal sites (startup restore and the toggle action in `ui/window/search.rs`), and re-seed on global key change while the panel is open and idle
- [x] 7.2 Include the toggle in the request the panel builds, in `evidence.rs`, in `accessibility.rs`, and in history and saved-search capture and restore under the `restoring_history` guard; land all wiring in `imp.rs`/`execution.rs`/`evidence.rs` so `ui/search_panel/mod.rs` (369 of 370 lines) gains zero lines
- [x] 7.3 Widget tests: toggle seeded on open at both reveal sites; per-search override does not write the key; excluded names are never searched with the toggle on; saved search restores the toggle and its row summary shows the axis; global change re-seeds an idle panel

## 8. Hidden-folder notes, parity, and performance proof

- [x] 8.1 Widget test: bookmark a line in a file under `.github/` with hidden off, verify Browse Bookmarks lists and activates it, then turn hidden on and verify the sidebar row and gutter mark
- [x] 8.2 Cross-surface parity widget test over one fixture (hidden, excluded, ordinary entries, and a dot-named root): for mode off and on, the sidebar tree, palette index, and content search report the same visible file set, equal to the rule applied directly to the fixture
- [x] 8.3 Assert app-data scanners still pass `WorkspaceEntryVisibility::app_data()` (drafts, local history, search backup, format upgrade, sidecars, quarantine, style schemes) and rerun their existing tests
- [x] 8.4 Add a hidden-on fixture to the performance smoke or Criterion scan benchmarks so a large dotfile tree stays measured, and update `docs/end-user-coverage.md` for the changed lane expectations

## 9. Documentation and gates

- [x] 9.1 Update `README.md` features, `AGENTS.md` (Key Design Decisions entry for workspace entry visibility and the `model/workspace_visibility.rs` module-layout line), and `.agents/rules/ui.md` for the expander list-editor pattern and its justified divergence
- [x] 9.2 Update `docs/automation.md` and `docs/automation-reference.md` for the action, snapshot fields, and readiness notes; run `make check-automation-docs` and `make automation-client-self-test`
- [x] 9.3 Update `docs/accessibility.md` and `docs/accessibility-matrix.md` for the menu item, shortcut, preferences rows, and search toggle
- [x] 9.4 Update `docs/workflow-readability-matrix.md`: re-derive the measured cells of WFR-WORKSPACE-TREE, WFR-COMMAND-PALETTE, and WFR-SEARCH-REPLACE (file counts, production lines, per-kind seam counts, pure-policy consumers), correct the already-stale `ui/sidebar/policy.rs` owned-policy cell, add `model/workspace_visibility.rs` to the cross-cutting mechanism table with its owning-workflow count (reporting mutation coverage as a gain from zero), and bump the coverage-proof attribution counts for `model/**` and `ui/preferences/**`; run `make check-workflow-boundaries`
- [x] 9.5 Run `make check-blueprint` and `make check-ui-template-contract`; then, after the last source edit, `make visual-geometry-smoke`, `make visual-smoke`, and `make accessibility-smoke` (their fingerprints must match the final tree); then `make check`, `make test`, `make test-widget`; fix any pre-existing blocker surfaced in the same stream
