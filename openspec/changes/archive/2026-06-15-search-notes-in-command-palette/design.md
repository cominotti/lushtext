## Context

The command palette already exposes `All`, `Files`, `Notes`, and `Commands`, but `Notes` currently searches note-related command launchers instead of saved note records. `Commands` still includes those actions, so the current `Notes` mode duplicates command search while the actual note content search remains hidden behind `Browse Notes...`.

`Browse Notes...` already has the right product model: it lists bookmarks, folder notes, document notes, and eligible open-tab rows; it searches row titles, row metadata, and document/folder note bodies; and it activates entries through the correct note workflows. The palette should reuse that model while staying lightweight enough for repeated command-palette filtering.

## Goals / Non-Goals

**Goals:**

- Make command palette `Notes` mode search actual note and bookmark records.
- Preserve the mode order `All`, `Files`, `Notes`, `Commands`.
- Preserve `Commands` mode as the complete command/action search, including note actions with `Notes` subtitles.
- Reuse the Notes browser category vocabulary: `Bookmarks`, `Folder Notes`, `Document Notes`, and `Open Tabs`.
- Avoid per-keystroke filesystem reads while users type in the palette.
- Keep note result activation routed through the same workflows as `Browse Notes...`.
- Cover empty, populated, many-result, awkward-name/body, and constrained-geometry states.

**Non-Goals:**

- Do not replace or remove `Browse Notes...`; it remains the full preview/edit browser.
- Do not search bookmark source excerpts from the palette.
- Do not add a new note storage format or migrate existing note sidecars.
- Do not remove note-related commands from `Commands` mode.
- Do not expose note bodies through automation snapshots beyond already visible palette row metadata.

## Decisions

### 1. Treat `Notes` as a record source, not a command subset

`Notes` mode will list note records only. The note-related actions (`Browse Notes`, `Open Document Note`, `Toggle Bookmark`, and friends) remain searchable in `Commands` mode and appear in the `Commands` group in `All` mode when they match the query.

Alternative considered: keep note actions in `Notes` and add note records below them. That preserves the current implementation shape, but it keeps the ambiguity that triggered this change and makes the mode harder to scan.

### 2. Use note category separators for note rows

Dedicated `Notes` mode will group matching rows as:

```text
Bookmarks
Folder Notes
Document Notes
Open Tabs
```

`All` mode already has an `Open Tabs` group for file-backed tabs, so supplemental note rows from saved open tabs should use a distinct `Open Tab Notes` header there. This keeps one-level palette headers readable without introducing nested sections.

Alternative considered: show a single `Notes` group in `All` mode and rely on row subtitles for note type. That is simpler visually, but it loses the category separators the user expects from the Notes workflow.

### 3. Introduce a GTK-free palette note row shape

Add a GTK-free value object for palette note results, with display title, subtitle, optional preview/detail text, category, searchable text, and an activation target:

```text
Bookmark(path, line)
FolderNote(workspace_name, folder)
DocumentNote(path, workspace_folders)
```

The window adapter can convert those rows to palette `PaletteItem`s and activate them through existing note workflows. The model/service layer should not depend on GTK widgets or private `NotesBrowserEntry` UI state.

Alternative considered: reuse the private `NotesBrowserEntry` type directly. That would reduce initial code, but it lives in `ui/window/notes.rs` and carries preview/UI concerns that should not become the palette's data contract.

### 4. Refresh note rows as a palette source, not during every query

Listing note sidecars and open-editor snapshots should happen when palette sources are refreshed: opening the palette, workspace scope changes, and note-relevant mutations. Search filtering should run against an in-memory note vector, just as files are searched against an in-memory index.

Stale background note loads must be ignored if the workspace scope, open-tab state, or note source generation changes before completion.

Alternative considered: list sidecars inside every palette search task. That would make the implementation direct, but it risks disk work on every keystroke and would make large note sets feel uneven.

### 5. Reuse Notes browser matching semantics where they matter

Palette note search should match row title, subtitle/location metadata, workspace/folder metadata, bookmark label and line metadata, and document/folder note body text. Body matching should be full-text/case-insensitive rather than command fuzzy matching only, so exact note content is discoverable.

Bookmark source excerpts remain preview-only in `Browse Notes...`; the palette should not read closed source files merely to make bookmark rows match.

## Risks / Trade-offs

- Note source refresh can race with palette typing -> use source generations and ignore stale completions.
- Large note bodies can make filtering expensive -> prepare query once, cap rendered rows per note category, and reuse the existing bounded search/debounce pattern.
- `All` mode can become visually dense with several note categories -> preserve source order, omit empty groups, and keep headers presentation-only.
- Shared note-row extraction can accidentally change `Browse Notes...` behavior -> keep service tests for listing/matching and widget tests for both palette and browser entry behavior.
- Open-tab note rows need careful wording in `All` mode -> use `Open Tab Notes` there to avoid colliding with the file `Open Tabs` group.

## Migration Plan

No persisted data migration is required. Existing note sidecars, bookmarks, command action IDs, shortcuts, and the `SearchMode::Notes` stable name remain in place. The behavior behind that mode changes from command subset search to note record search.

Rollback is straightforward: restore the previous `Notes` mode search path and command grouping behavior. Because no data format changes are involved, rollback does not require user-data migration.
