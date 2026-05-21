## Context

The command palette currently renders a flat result list backed by `PaletteItem` objects. `SearchMode` is stored in the palette widget and cycled with Tab; the UI exposes it only as a passive label. File lookup uses a `FileIndex` rebuilt from the window's current workspace scope, while commands are merged into `All` mode by score.

The workspace selector already establishes a native pattern for mouse-selectable scope controls through `GtkDropDown`. The palette should follow that pattern for mode selection, but its result sources need a slightly different model: open file-backed tabs are active document state, while indexed workspace files are workspace-scope state.

## Goals / Non-Goals

**Goals:**

- Make command palette mode selection available by mouse while preserving Tab cycling.
- Present file and mixed results in stable, labeled source groups.
- Keep the workspace file group aligned with the sidebar's current workspace scope:
  - `Selected Workspace` for a concrete workspace.
  - `All Workspaces` for the aggregate sidebar scope.
- Prioritize open file-backed tabs before workspace-indexed files.
- Preserve existing file activation, command activation, fuzzy matching, and background indexing behavior.

**Non-Goals:**

- Do not add a permanent extra `All Workspaces` group when a concrete workspace is selected.
- Do not index untitled tabs as file results.
- Do not change workspace search panel behavior.
- Do not add a new command registry or fuzzy matching dependency.

## Decisions

### Use a `GtkDropDown` for palette mode selection

The palette mode control will become a dropdown with `All`, `Files`, and `Commands`, mirroring the sidebar selector's mouse-friendly pattern. Tab remains a keyboard shortcut by updating the same selected mode rather than maintaining a separate label-only path.

Alternative considered: use segmented buttons. That would make all modes visible at once, but it would be a larger visual change and would not match the explicit "like the Workspace selector" direction.

### Treat open tabs as a separate source, not part of workspace indexing

Open file-backed tabs should be collected from the window's tab state and passed to the palette as a lightweight snapshot. These results can include open files outside the current workspace scope, because they represent active documents rather than workspace traversal. Workspace-indexed results still come from the existing current-scope file index.

Alternative considered: fold open tabs into the `FileIndex`. That would blur active documents with workspace files, make de-duplication harder, and risk showing out-of-scope open documents under the wrong source label.

### Build grouped palette rows in the UI adapter layer

The palette should convert raw file and command hits into a grouped presentation model containing non-activatable group headers plus activatable file and command rows. The service layer can continue to provide fuzzy matching over plain file and command data; GTK-specific row/header objects belong in `ui/command_palette`.

Alternative considered: introduce a service-level grouped result type. That would move presentation concerns into the service layer and make source labels harder to keep aligned with GTK/UI state.

### Search each source independently, then concatenate groups

Grouped results should not be globally score-merged across open tabs, workspace files, and commands. Instead, each source is searched and ranked internally, then groups are concatenated in the required order:

- `Files` mode: `Open Tabs`, then the workspace file group.
- `All` mode: `Open Tabs`, then the workspace file group, then `Commands`.
- `Commands` mode: command rows only.

This gives users predictable navigation: active documents first, current workspace next, commands last in mixed mode.

Alternative considered: keep the current global merge and insert headers around the merged rows. That would make section order unstable and could put commands above open tabs in `All` mode, which contradicts the requested priority.

### Deduplicate by absolute path across file sources

When an open tab path also appears in the workspace index, it should appear only in `Open Tabs`. A simple absolute-path set can filter workspace hits after open-tab hits have been collected.

Alternative considered: show duplicates with different labels. That would expose implementation details and make Enter/click behavior feel arbitrary.

## Risks / Trade-offs

- [Risk] Group headers become accidentally activatable or selectable → Mitigation: model headers as a distinct palette item kind and skip them during activation and keyboard movement.
- [Risk] `All` mode becomes visually longer than before → Mitigation: keep the existing overall palette limits and apply per-source caps during implementation if needed.
- [Risk] Open tabs outside the current workspace surprise users → Mitigation: keep them under the explicit `Open Tabs` label and keep workspace-indexed files under the scope-aware workspace label.
- [Risk] Mode dropdown changes focus behavior inside the overlay → Mitigation: keep the search entry focused when opening the palette and ensure mouse mode changes rebuild results without stealing keyboard search flow.

## Migration Plan

No data migration is required. The change is local to command palette presentation, palette/window integration, and tests. Rolling back would restore the label-based mode display and flat result list without touching persisted workspace state.

## Open Questions

None. Section-specific result caps can be tuned during implementation if widget tests or manual review show that one source crowds out the others.
