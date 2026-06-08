## Why

LushText's current workspace model treats a workspace as exactly one folder, which makes curated project views awkward when a user wants one named workspace to contain several related directories. Redefining a workspace as an ordered set of unique folders also gives the sidebar, search, command palette, and notes browser a clearer long-term contract: the sidebar reflects the user's curated folder set literally, while search-like consumers can de-duplicate document results by identity.

## What Changes

- **BREAKING** Redefine a workspace from one named directory root into a named, ordered set of zero or more unique folders.
- **BREAKING** Rename the user-facing and domain-level concept away from "workspace root" toward "workspace folder" or "folder"; "root" wording may remain only in internal tree/traversal code where it truly means a displayed tree root row.
- Allow the same canonical folder to belong to different workspaces, but reject adding the same canonical folder more than once inside one workspace.
- Allow overlapping folders inside a workspace, including parent/descendant combinations such as `/repo` and `/repo/src`.
- Preserve sidebar literalness: overlapping workspace folders appear as separate top-level folder trees, so the same file may be visible in more than one sidebar folder tree.
- De-duplicate workspace search results by canonical file identity so a matching file appears only once even when reachable through overlapping workspace folders.
- De-duplicate command-palette workspace file results by canonical file identity while preserving the existing `Open Tabs` priority over workspace-indexed files.
- Preserve folder order as the sidebar display order and as the primary context tie-breaker for search/palette/notes metadata when multiple folders cover the same document.
- Convert workspace notes into folder notes: note persistence continues to follow the canonical folder path, but menu labels, browser sections, commands, services, models, comments, tests, fixtures, and documentation must no longer present these as notes for a singular workspace root.
- Make single-folder note entry points deterministic: zero-folder workspaces disable folder-note actions, one-folder workspaces may open that folder note directly, and multi-folder workspaces must ask for or present a clear folder target instead of guessing.
- Update persistence, recovery, and tests so existing v1 single-root workspace state migrates safely into the new folder-set payload, while old pre-public bare `entries` shapes remain unsupported recovery metadata.
- Add comprehensive tests across model/service behavior, persistence/recovery, sidebar widget behavior, search/palette de-duplication, notes browser/folder-note behavior, context menus, command labels, and documentation/string cleanup.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `workspace-state-persistence`: Replace single-root workspace payload semantics with ordered folder sets, canonical per-workspace folder uniqueness, v1 migration, recovery behavior, latest-state persistence, and naming cleanup expectations.
- `workspace-sidebar-shell`: Replace one-folder workspace sections with workspace sections that own ordered folder trees, folder add/remove/reorder flows, literal overlapping folder display, empty/large/constrained states, and folder-level context actions.
- `workspace-scope`: Update shared scope consumers so concrete workspaces resolve to ordered folder sets, search de-duplicates overlapping results, and palette/search metadata uses folder-order tie-breakers.
- `command-palette-source-groups`: Require workspace-indexed file de-duplication across overlapping workspace folders while preserving `Open Tabs` priority and source-group presentation.
- `workspace-notes`: Rename workspace-root notes to folder notes, preserve canonical folder-note identity, update Browse Notes behavior for overlapping folder coverage, and make zero/one/many folder note targets explicit.
- `document-notes`: Update Browse Notes document-note scope semantics from workspace roots to workspace folder coverage, with de-duplicated document rows and clear primary folder context.
- `line-bookmarks`: Update Browse Notes bookmark scope semantics from workspace roots to workspace folder coverage, with de-duplicated document rows and clear primary folder context.
- `document-notes-menu`: Rename note entry points from workspace notes to folder notes and define action sensitivity/targeting for zero, one, and many folders in the current workspace.
- `workspace-tree-refresh`: Update automatic and manual refresh contracts to cover each workspace folder's displayed tree, including overlapping folder trees and reordering without disruptive rebuilds.
- `markdown-preview-local-images`: Update workspace-relative local image resolution to use the current workspace's folder set rather than a singular workspace root list.

## Impact

- Affected model/service code: `crates/lushtext-core/src/model/workspace.rs`, `services/workspace_manager.rs`, `services/workspace_watch.rs`, `services/palette/index.rs`, `services/content_search`, note/bookmark/document-note listing helpers, workspace/folder-note services, migration ledger flows, and persistent JSON fixtures/tests.
- Affected UI code: `ui/sidebar/**`, especially workspace orchestration and `workspace_section/`; `ui/window/workspace_scope.rs`; `ui/window/notes.rs`; `ui/window/focus_indexing.rs`; `ui/command_palette/**`; search panel workspace root handling; Markdown preview context wiring; menus and context-menu labels.
- Affected resources/docs/specs: `resources/ui/sidebar.ui`, `resources/ui/workspace-section.ui`, user-facing labels/tooltips/actions, root `AGENTS.md`, nested sidebar/window/editor guidance, README persistence and notes sections, OpenSpec canonical specs, test fixtures, and any comments or identifiers that still describe the old domain concept as a singular workspace root.
- No new external dependencies are expected. GTK drag-and-drop reordering should use existing GTK4/gtk4-rs primitives and model-state updates.
- Implementation must include a cleanup audit proving that old "workspace root" wording and identifiers are fully renamed where they describe the domain or user-facing behavior; remaining "root" terminology must be justified as internal tree/traversal vocabulary.
