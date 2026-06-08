## 1. Domain Model and Naming

- [x] 1.1 Replace the single-folder workspace domain model with `WorkspaceFolderId`, `WorkspaceFolder`, and `WorkspaceConfig { id, name, folders }` while preserving stable workspace IDs.
- [x] 1.2 Rename domain-facing model/service helpers from workspace-root language to workspace-folder or folder-set language, including scope path helpers and primary-context helpers.
- [x] 1.3 Rename workspace-note domain types, services, UI modules, commands, comments, fixtures, and tests to folder-note terminology, leaving old `workspace-note` names only in explicit sidecar compatibility or migration code.
- [x] 1.4 Update user-facing labels, tooltips, status messages, dialog copy, action names, command-palette labels, context-menu labels, and resource files so folder membership and folder-note workflows never mention a singular workspace root.
- [x] 1.5 Update README, root and nested `AGENTS.md` guidance, OpenSpec canonical specs, and developer comments so the documented concept is a workspace as an ordered folder set.
- [x] 1.6 Run a naming audit over code, UI resources, tests, fixtures, specs, and docs for `workspace root`, `WorkspaceRoot`, `workspace_root`, `root_paths`, `Open Workspace Note`, `workspace-note`, `WorkspaceNote`, and `workspace_note`, then rename every domain/user-facing occurrence and document any remaining compatibility-only or tree-internal use.
- [x] 1.7 Keep internal `root` vocabulary only where it clearly refers to GTK tree root rows, tree stores, traversal roots, or legacy migration diagnostics, with nearby names or comments making that distinction clear.

## 2. Persistence and Migration

- [x] 2.1 Teach workspace loading to accept the current supported v1 `workspaces[].root` payload and migrate it in memory to one `WorkspaceFolder` entry.
- [x] 2.2 Teach workspace loading to accept the new `workspaces[].folders[]` payload, including zero-folder workspaces and stable folder IDs.
- [x] 2.3 Ensure all workspace saves write only the new folder-set payload and never write the old singular `root` payload field.
- [x] 2.4 Preserve unsupported pre-public `entries` documents, wrong-kind envelopes, future versions, malformed JSON, and unsupported file kinds through existing recovery metadata before replacement.
- [x] 2.5 Implement per-workspace canonical folder uniqueness so duplicate canonical folders are rejected only inside the target workspace while the same folder remains allowed in other workspaces.
- [x] 2.6 Preserve overlapping folder membership, including parent/descendant pairs, without treating them as duplicates.
- [x] 2.7 Keep missing or temporarily uncanonicalizable folders recoverable without silently creating duplicates after canonical identity becomes available.
- [x] 2.8 Extend latest-state-wins persistence so add, remove, reorder, rename, scope selection, and workspace removal cannot be overwritten by stale debounced snapshots.
- [x] 2.9 Add service tests for v1 `root` migration, new `folders` restore/save, zero-folder persistence, unsupported metadata preservation, scoped duplicate rejection, cross-workspace reuse, overlaps, canonicalization fallback, and rapid mutation ordering.

## 3. Sidebar Folder Sets

- [x] 3.1 Render one workspace section per workspace and one top-level folder tree per workspace folder in persisted folder order.
- [x] 3.2 Render zero-folder workspaces as real workspace sections with header controls and an explicit empty folder-set state.
- [x] 3.3 Add an add-folder workflow inside existing workspace sections that appends valid folders, rejects duplicates inside that workspace, persists state, and refreshes workspace-aware consumers.
- [x] 3.4 Add a remove-folder workflow that removes only workspace membership, preserves files on disk, preserves folder-note sidecars, and leaves the workspace present even when it becomes empty.
- [x] 3.5 Add folder reorder through drag-and-drop using stable model identifiers rather than recycled row widgets.
- [x] 3.6 Add a non-pointer reorder path such as Move Up and Move Down actions that uses the same persisted reorder logic as drag-and-drop.
- [x] 3.7 Keep file/directory row operations unchanged below each folder tree, including new file, new folder, rename, delete, document note, local history, file peek, and Focus Folder behavior.
- [x] 3.8 Add top-level folder row context actions for `Open Folder Note...`, remove-from-workspace, and reorder controls where appropriate.
- [x] 3.9 Preserve literal sidebar behavior for overlaps so the same file may appear under both a parent folder tree and a descendant folder tree.
- [x] 3.10 Keep the fixed workspace selector row, no-horizontal-scroll contract, constrained-width behavior, long-path handling, and dense-list scrolling stable for zero, one, many, long, and overlapping folder sets.
- [x] 3.11 Add widget tests for multi-folder ordering, zero-folder sections, duplicate add feedback, remove-without-delete behavior, drag-and-drop reorder, keyboard/action reorder, overlapping folder visibility, context-menu labels, and constrained geometry.

## 4. Workspace Scope, Search, and Palette

- [x] 4.1 Replace workspace-scope path resolution with ordered folder-set helpers for concrete scopes and aggregate `All workspaces` scopes.
- [x] 4.2 Keep selecting an empty workspace as a valid shared scope that produces empty folder coverage rather than rebasing to another workspace.
- [x] 4.3 Notify search, command palette, notes, bookmarks, Markdown preview, and refresh consumers when folders are added, removed, or reordered without changing the selected workspace.
- [x] 4.4 Update workspace search to traverse all folders in scope order and de-duplicate result files by canonical file identity when folders overlap.
- [x] 4.5 Use workspace order and then folder order as the primary context tie-breaker for de-duplicated search results.
- [x] 4.6 Update command-palette file indexing so `Open Tabs` keeps priority, workspace-indexed rows are de-duplicated by canonical file identity, and aggregate scope handles duplicate files across workspaces.
- [x] 4.7 Preserve command-palette group labels `Open Tabs`, `Selected Workspace`, `All Workspaces`, and `Commands` while updating their folder-set semantics.
- [x] 4.8 Add tests for selected workspace scope, aggregate scope, empty workspace scope, overlapping folder search de-duplication, palette `Open Tabs` suppression, workspace-index duplicate suppression, aggregate duplicate suppression, and folder-order primary context.

## 5. Folder Notes and Notes Browser

- [x] 5.1 Implement folder-note open, edit, render, clear, save, and preview flows using canonical folder identity and folder-note UI labels.
- [x] 5.2 Decide and implement the sidecar compatibility strategy: either migrate old workspace-note sidecars to a folder-note kind or keep a compatibility loader isolated to legacy sidecar code.
- [x] 5.3 Preserve folder-note content across workspace renames, folder reorders, folder removal and re-add, and in-app folder renames.
- [x] 5.4 Add retryable folder-note migration ledger handling for folder renames and compatibility sidecar migration failures.
- [x] 5.5 Add conservative folder-note reconciliation for duplicate old/new sidecars and preserve ambiguous conflicts with diagnostics.
- [x] 5.6 Implement zero-folder, one-folder, multi-folder, folder-row, and aggregate-scope behavior for `Open Folder Note...` without guessing targets.
- [x] 5.7 Update the `Notes` header menu, command palette commands, and context menus from workspace-note actions to folder-note actions.
- [x] 5.8 Update `Browse Notes...` to list folder notes per folder identity and document notes/bookmarks per canonical saved-file identity.
- [x] 5.9 De-duplicate document-note and bookmark rows when overlapping workspace folders cover the same saved file, while preserving one folder-note row per folder-note identity.
- [x] 5.10 Keep eligible saved open-tab bookmark and document-note rows in the dedicated `Open Tabs` section when they fall outside the current workspace folder coverage.
- [x] 5.11 Preserve lazy, bounded, off-main-thread bookmark excerpt loading and update excerpt Markdown context to use workspace folder coverage.
- [x] 5.12 Add service and widget tests for folder-note sidecars, legacy workspace-note compatibility, corruption diagnostics, retryable migrations, duplicate reconciliation, zero/one/many note targeting, menu sensitivity, context-menu labels, notes-browser folder-note rows, document/bookmark de-duplication, open-tab rows, lazy previews, and terminology cleanup.

## 6. Refresh, Watchers, and Markdown Preview

- [x] 6.1 Update workspace watching to track currently materialized top-level workspace folder rows and expanded directories rather than one workspace root.
- [x] 6.2 Ensure zero-folder workspaces start no fake watchers and remain refreshable without filesystem work.
- [x] 6.3 Keep manual refresh scoped to all configured folders in a workspace section and preserve folder order, expansion, selection, and drill-down state when paths still exist.
- [x] 6.4 Keep automatic refresh visually stable for unchanged rows, access-only watcher noise, overlapping folder trees, and broad folders with unreadable descendants.
- [x] 6.5 Surface recoverable refresh feedback per workspace section or folder when watcher startup, runtime watching, or manual refresh fails.
- [x] 6.6 Update Markdown preview workspace-relative local image resolution to use ordered workspace folder coverage for concrete and aggregate scopes.
- [x] 6.7 Use folder order to resolve ambiguous workspace-relative local images and show explicit fallback for zero-folder or unresolved scopes.
- [x] 6.8 Add tests for watcher setup, zero-folder no-op refresh, multi-folder manual refresh, reorder-preserving refresh, overlap refresh, watcher failure feedback, Markdown image folder-order resolution, and zero-folder unresolved image fallback.

## 7. Verification

- [x] 7.1 Run formatting checks for all touched Rust, UI resource, and documentation files.
- [x] 7.2 Run focused unit and integration tests for workspace model, workspace manager, folder identity, search, palette indexing, note services, bookmark/document-note listing, Markdown preview resolution, and refresh/watch services.
- [x] 7.3 Run the relevant GTK widget-test harness for sidebar folder sets, drag-and-drop or action reorder, notes menu, notes browser, and geometry/state-extreme coverage.
- [x] 7.4 Run Clippy and the project-required build/test smoke checks for the touched crates.
- [x] 7.5 Run `openspec validate --change redefine-workspaces-as-folder-sets --strict` and any repo-required OpenSpec validation ladder after specs are synced or archived.
- [x] 7.6 Run `git diff --check`.
- [x] 7.7 Re-run the naming audit from task 1.6 after all implementation and documentation edits, and include the remaining allowed compatibility/tree-internal terms in the implementation summary.
