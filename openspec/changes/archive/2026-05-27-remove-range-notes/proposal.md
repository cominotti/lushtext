## Why

Range Notes add a second, overlapping saved-file note model beside document notes while carrying a large amount of UI, persistence, search, export, and test surface. Removing the concept simplifies LushText's note system around bookmarks, document notes, and workspace notes, and avoids keeping an underused range-anchored workflow alive through future editor and sidebar work.

## What Changes

- **BREAKING** Remove the Range Notes feature entirely, including creation, editing, browsing, highlighting, export, rename sidecar handling, and restore behavior.
- **BREAKING** Remove the annotation sidecar contract under `$XDG_DATA_HOME/lushtext/annotations/`; no import, conversion, or read path remains.
- Remove range-note actions, shortcuts, command-palette entries, context-menu items, Notes menu items, preferences, GSettings keys, docs, tests, and implementation modules.
- Keep bookmarks, document notes, workspace notes, the shared rich note editor surface, and the workspace-scoped `Browse Notes...` flow for the remaining note types.
- Leave historical archived OpenSpec changes intact, but remove active specs and live documentation references for Range Notes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `sidecar-annotations`: retire the saved-file range-note capability and remove its active requirements.
- `document-notes-menu`: remove Range Note entry points from the Notes menu, command/context surfaces, and availability rules.
- `workspace-notes`: remove Range Notes from the workspace-scoped Notes browser contract.
- `workspace-scope`: remove annotation and range-note export behavior from workspace-aware note/export scoping.

## Impact

- Rust model/service removal: `model::annotation`, `services::annotation_service`, and their tests.
- Editor/UI removal: live annotation projection, range-note dialogs, highlight preference, note-browser range entries, menu/actions/shortcuts, and command-palette commands.
- Persistence behavior: LushText no longer reads, writes, exports, renames, or deletes annotation sidecars as part of normal workflows.
- Documentation/spec updates: README, metainfo, AGENTS/nested AGENTS, next docs, active OpenSpec specs, and verification sweeps must no longer describe Range Notes as a shipped feature.
