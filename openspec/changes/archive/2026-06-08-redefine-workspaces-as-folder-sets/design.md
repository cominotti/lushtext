## Context

The current LushText workspace model is intentionally single-folder: `WorkspaceConfig` stores one directory path, the sidebar builds one `WorkspaceSection` per persisted workspace, search and palette resolve one path list per scope, and workspace notes are keyed by the canonical path of that one directory. That contract is clear, but it does not match the desired product model: a workspace should be a named, ordered set of zero or more unique folders.

This change is cross-cutting because "workspace root" is embedded in persistence, model helpers, sidebar tree setup, search/palette indexing, notes/bookmark browsing, Markdown preview context, tests, UI strings, and developer documentation. The implementation must distinguish the new domain language from internal GTK tree vocabulary:

- Domain/user model: workspace, workspace folder, folder set, folder note.
- Internal tree model: a displayed tree may still have root rows, root stores, and root models when those terms describe GTK tree structure rather than the workspace domain.

The old pre-public bare `entries` workspace JSON shape remains unsupported recovery metadata. The new folder-set shape is a deliberate public migration from the current v1 single-folder envelope payload, not a revival of that pre-public format.

## Goals / Non-Goals

**Goals:**

- Redefine a workspace as a named, ordered set of zero or more unique folders.
- Forbid duplicate canonical folders inside one workspace while allowing the same canonical folder in different workspaces.
- Allow overlapping folders inside one workspace, including parent/descendant folder pairs.
- Preserve the sidebar as a literal view of the curated folder list, so overlapping folders can show the same file in multiple folder trees.
- De-duplicate search results and command-palette workspace file rows by canonical file identity when overlapping folders cover the same file.
- Preserve `Open Tabs` as the highest-priority command-palette file source.
- Convert workspace-root notes into folder notes, preserving canonical-folder identity and note sidecars while renaming UI/domain code.
- Make zero-folder, one-folder, and multi-folder states explicit in sidebar, notes menu, folder-note commands, and Browse Notes.
- Migrate current v1 single-folder workspaces to folder-set payloads safely and continue to preserve unsupported or malformed workspace metadata before replacement.
- Include targeted tests for model invariants, persistence migration, sidebar behavior, search/palette de-duplication, notes/browser semantics, DnD/reorder behavior, and naming cleanup.

**Non-Goals:**

- Do not add workspace-group drag-and-drop reordering. This change is about reordering folders inside a workspace.
- Do not introduce a separate group-level workspace note. Notes attached to folders remain folder notes; document notes and bookmarks remain document-level state.
- Do not delete folder-note sidecars when a folder is removed from a workspace. Removing a folder is a sidebar/workspace membership change, not note deletion.
- Do not de-duplicate the sidebar file tree. Literal overlapping folder display is intentional.
- Do not add external dependencies for drag-and-drop, persistence, search, or notes.

## Decisions

### 1. Persist `WorkspaceFolder` entries instead of a single path

`WorkspaceConfig` should become a folder-set aggregate:

```text
WorkspaceConfig
├── id: WorkspaceId
├── name: String
└── folders: Vec<WorkspaceFolder>

WorkspaceFolder
├── id: WorkspaceFolderId
└── path: PathBuf
```

Each folder gets a stable folder ID for UI operations such as remove, reorder, context menus, DnD payloads, and test assertions. Folder-note identity remains based on canonical folder path, not the folder ID, so removing and re-adding the same folder can recover the same note.

Alternative considered: persist only `Vec<PathBuf>`. That is simpler, but DnD and context menus become position/path driven, which is fragile while rows are recycled and paths can change after in-app directory renames.

### 2. Canonical folder uniqueness is scoped to one workspace

Adding a folder canonicalizes the selected path and compares it only against folders already in the target workspace. The same canonical folder may belong to another workspace. Missing or temporarily uncanonicalizable folders should use the same degraded-but-usable fallback pattern already used by notes and workspace listing: preserve the configured path, surface recoverable feedback where appropriate, and avoid silently duplicating a folder once canonical identity can be resolved.

Alternative considered: global uniqueness across every workspace. That would reduce duplicate indexing complexity, but it would make separate workspace groupings less useful and would surprise users who intentionally reuse a shared folder in multiple contexts.

### 3. Folder order is authoritative for display and primary context

Folder order drives:

- Sidebar top-level folder tree order inside a workspace section.
- Search/palette traversal priority and primary folder context when overlapping folders cover the same document.
- Notes/browser workspace metadata when one document is covered by multiple folders.

Reordering folders must persist and notify workspace-scope consumers. It may update relative subtitles or context labels for overlapping documents, but it must not create duplicate search or palette rows.

Alternative considered: choose the most specific folder as primary context. That is attractive for relative paths, but it makes drag-and-drop reordering feel cosmetic. User-controlled order is more honest because the feature explicitly lets users curate and reorder folders.

### 4. Sidebar remains literal; search and palette de-duplicate documents

The sidebar should render each configured folder tree independently. If `/repo` and `/repo/src` both belong to one workspace, files under `/repo/src` may appear in both displayed trees.

Search and command palette are navigation/result surfaces, so each canonical file identity should appear once per result set. For command palette grouping, `Open Tabs` keeps priority over workspace-indexed rows; workspace-indexed rows are then de-duplicated across all folders in the current scope.

Alternative considered: de-duplicate the sidebar tree too. That makes backend behavior uniform, but it hides the user's curated folder entries and makes overlapping folders feel broken.

### 5. Folder notes replace workspace-root notes in the domain language

The persistence sidecar now uses a folder-note document kind for newly written sidecars while retaining an explicit compatibility reader for the old workspace-note sidecar kind. Saving over a compatible legacy sidecar rewrites it with the folder-note kind. User-facing strings should say "Folder Note..." or "Open Folder Note..." for a concrete folder target.

Single-target behavior:

- Current workspace has zero folders: folder-note action is insensitive or reports that a folder must be added first.
- Current workspace has one folder: the main menu/command can open that folder note directly.
- Current workspace has multiple folders: the main menu/command must present a clear folder choice or open Browse Notes focused to folder notes; it must not guess.
- Folder row context menu: always has a clear folder target and may open that folder note directly.

Alternative considered: add one note for the workspace group. That is a distinct product feature and would need its own identity, migration, UI, and browse semantics. Keeping notes folder-level preserves existing durable identity behavior.

### 6. Replace scope path helpers with folder-set helpers

APIs named around `root_paths` should be renamed to folder-set terminology wherever they expose domain behavior, for example:

- `all_workspace_folder_paths()`
- `folder_paths_for_scope()`
- `current_workspace_folder_paths()`
- `primary_folder_for_path()`
- `deduplicated_files_for_scope()` or service-local equivalents

Internal `workspace_section::roots` and `TreeListModel` helpers may retain "root" names only where they mean GTK/root-row mechanics. The implementation must include an audit that classifies every remaining "workspace root" occurrence as either removed/renamed or intentionally internal tree vocabulary.

### 7. Persistence migration is v1-envelope compatible but payload-breaking

The current public workspace state envelope is version 1. To avoid forcing all app-owned JSON documents to v2 at once, the workspace service can load both payload shapes for the workspace-state kind:

- Current payload: `workspaces[].root`
- New payload: `workspaces[].folders[]`

On save, write only the new folder-set payload. A current single-folder workspace becomes a workspace with one `WorkspaceFolder`. Empty workspaces persist as `folders: []`. Unsupported pre-public bare JSON and wrong-kind/future-version envelopes continue through existing recovery/quarantine handling.

Alternative considered: bump the global JSON envelope version. That is cleaner in abstract, but the helper currently centralizes one supported version for all document kinds. A payload migration inside the same workspace-state kind keeps the change scoped.

### 8. Use model-state DnD, not row-widget identity

GTK list/tree rows are recycled, so drag-and-drop should carry stable workspace/folder identifiers or canonical paths and mutate `WorkspacesFile` state. The sidebar then rebuilds or reconciles the relevant section from state and persists through the existing debounced latest-state-wins pipeline.

The DnD implementation should also provide non-pointer reorder actions, such as Move Up/Move Down in a folder context menu or accessible action path, so keyboard and assistive users are not locked out of ordering.

## Risks / Trade-offs

- [Risk] Search/palette duplicate suppression can hide a legitimate second route to the same file. -> Mitigation: only suppress duplicate file rows in result surfaces, keep sidebar duplicate visibility literal, and use folder order for the displayed context.
- [Risk] Canonicalization failures on missing/unavailable folders can make uniqueness checks inconsistent. -> Mitigation: keep best-effort canonical identity plus path fallback, report recoverable issues, and re-normalize when folder metadata becomes available.
- [Risk] Renaming every domain occurrence of "workspace root" can accidentally touch internal tree-root code. -> Mitigation: require a naming audit and allow "root" only for GTK/tree traversal vocabulary with comments/tests updated accordingly.
- [Risk] Multi-folder folder-note actions can become ambiguous. -> Mitigation: require explicit zero/one/many behavior and direct folder-row note actions.
- [Risk] Existing tests and fixtures assume one root per workspace. -> Mitigation: update golden fixtures, add migration fixtures, and keep old pre-public unsupported-shape fixtures to prove no accidental compatibility regression.
- [Risk] Overlapping broad folders can increase search/index work. -> Mitigation: canonical visited-file de-duplication in search/palette, existing indexing caps, and folder-order traversal that can skip already-seen canonical files.
- [Risk] Sidebar DnD over a `GtkTreeListModel` can interfere with tree row activation, file context menus, and drill-down. -> Mitigation: limit DnD to top-level workspace folder rows or explicit drag handles, keep file tree row activation unchanged, and cover mouse/keyboard reorder paths with widget tests.

## Migration Plan

1. Introduce `WorkspaceFolder` and folder-set helpers while preserving a compatibility loader for current single-folder workspace payloads.
2. Update the sidebar to render one workspace section per workspace and one top-level folder tree per configured folder, including zero-folder sections.
3. Rename domain/user-facing root terminology throughout models, services, UI labels, comments, docs, specs, tests, and fixtures.
4. Update workspace-aware consumers to accept ordered folder sets and apply the correct literal-vs-deduplicated behavior.
5. Convert workspace-note workflows and labels into folder-note workflows while preserving sidecar identity and migration/recovery behavior.
6. Add DnD and non-pointer reorder actions, then wire persistence and workspace-structure notifications.
7. Run the full targeted verification ladder and naming audit.

Rollback is data-sensitive because saving after migration writes the new folder-set payload. Recovery should preserve unsupported future payloads, but practical rollback to an older release would require restoring a backed-up `workspaces.json` or adding a one-time downgrade tool, which is out of scope.

## Open Questions

- Should the new folder-set workspace payload stay under envelope version 1 with a compatibility loader, or should the workspace-state document kind get a workspace-local payload version field?
- Resolved: newly written folder-note sidecars use the folder-note document kind; the old workspace-note kind is accepted only through compatibility constants, fixtures, and migration aliases.
- In a multi-folder workspace, should the header/menu `Open Folder Note...` action open a compact chooser dialog or open Browse Notes filtered to folder notes?
