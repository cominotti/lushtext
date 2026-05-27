## Context

LushText currently exposes three note-like concepts beside bookmarks:

- Range Notes, implemented internally as annotations on saved-file line ranges.
- Document Notes, one rich note per saved file.
- Workspace Notes, one rich note per workspace root.

Range Notes are the only note type that depends on live text-range anchors, editor highlight tags, GtkSourceView annotation providers, annotation sidecars, a dedicated export workflow, and range-tracking persistence on every relevant buffer edit. Document Notes and Workspace Notes already cover the durable markdown-capable note use case without binding notes to moving line ranges. The proposal removes Range Notes as an active shipped capability while preserving the remaining bookmark, document-note, workspace-note, and unified browse workflows.

## Goals / Non-Goals

**Goals:**

- Remove all active Range Note implementation, tests, UI, command, preference, documentation, and OpenSpec surface.
- Retire the `sidecar-annotations` active capability and update related active specs so they no longer require range-note behavior.
- Keep bookmarks, document notes, workspace notes, rich note editor surfaces, and the workspace-scoped `Browse Notes...` workflow.
- Ensure the application no longer reads, writes, exports, highlights, renames, or restores annotation sidecars.
- Leave historical archived OpenSpec changes as immutable project history.

**Non-Goals:**

- Do not provide an import or conversion path from Range Notes into Document Notes.
- Do not rename Document Notes or Workspace Notes.
- Do not redesign the remaining Notes browser beyond removing Range Note rows and preserving expected behavior for the remaining entries.
- Do not remove GtkSourceView itself or unrelated source-view features.

## Decisions

### 1. Remove the feature family instead of hiding UI entry points

The implementation should delete the annotation model, annotation service, editor projection, actions, preferences, export path, and tests rather than merely disabling menu items. This matches the user's request to remove all implementation, tests, and leftovers of the concept.

Alternative considered: leave the persistence and editor projection in place but remove public UI. That would keep hidden code paths and future maintenance burden, which is exactly what this change is meant to eliminate.

### 2. Remove all annotation sidecar access paths

After this change, LushText should not touch `$XDG_DATA_HOME/lushtext/annotations/` in normal workflows. There is no read, write, export, rename, import, conversion, or cleanup path for Range Note sidecars.

Alternative considered: add startup cleanup that deletes the annotations directory. That would add new Range Note-specific behavior during a removal, so this change instead removes the application-level contract entirely.

### 3. Keep the shared rich note editor primitives

`RichNoteBody`, `NoteViewMode`, and the shared edit/render note surface remain because document notes and workspace notes depend on them. The removal boundary is the saved-file range anchoring and annotation-specific UI/persistence, not markdown-capable notes in general.

Alternative considered: remove shared note primitives and rebuild document/workspace note editing separately. That would couple an unrelated refactor to this feature deletion and raise regression risk.

### 4. Collapse the Notes browser to remaining entry types

The unified `Browse Notes...` surface should continue to list bookmarks, workspace notes, and document notes for the current workspace scope. Range Note entries, range-note search matching, pending annotation focus, and range-note Open routing should be removed.

Alternative considered: remove the Notes browser entirely because it previously included Range Notes. That would regress document/workspace notes and bookmarks and would exceed the scope of this cleanup.

### 5. Remove the highlight preference and schema key

`annotation-highlights-visible` and the `Show Annotation Highlights` preference should be removed with the editor projection. No hidden setting remains because no code should consume that key after the feature is gone.

Alternative considered: leave the key in the schema as an unused setting. That would be a leftover active contract for a removed feature and would keep documentation and preference cleanup incomplete.

## Risks / Trade-offs

- [Old sidecar files may remain on disk] -> The app has no Range Note sidecar code path after this change; filesystem cleanup is outside the active application contract.
- [Range-note wording can survive in mixed docs/specs/tests] -> Add explicit grep-based verification over active surfaces, excluding archived OpenSpec history if archive preservation is desired.
- [Removing annotation code can accidentally remove shared note helpers] -> Keep the boundary around `model::note`, document-note services, workspace-note services, and `build_note_editor_surface`.
- [The Notes browser may become empty in states previously populated only by Range Notes] -> Existing empty-browser feedback remains acceptable because no supported note entries exist in that scope.
- [GSettings schema removal may leave old user settings in dconf] -> Old keys are unused; the application does not read them.

## Implementation Plan

1. Remove annotation model/service modules and all callers.
2. Remove editor-page annotation projection state, methods, GSettings handlers, end-user-action reconciliation, and tests.
3. Remove window actions, shortcuts, command-palette commands, context/menu items, dialogs, export workflow, browser Range entries, and pending annotation focus.
4. Remove annotation highlight preference rows and GSettings schema/config constants.
5. Update docs, metainfo, AGENTS/nested AGENTS, and active OpenSpec specs to describe the remaining note surface.
6. Delete or rewrite tests that asserted Range Note behavior; keep or add tests proving remaining Notes menu/browser behavior still works without Range Notes.
7. Verify with formatting, Rust tests, widget tests, OpenSpec validation, and a final grep sweep for active Range Note leftovers.

Rollback is source-based: restore the removed modules, schema key, actions, docs, and specs from the parent commit.

## Open Questions

None.
