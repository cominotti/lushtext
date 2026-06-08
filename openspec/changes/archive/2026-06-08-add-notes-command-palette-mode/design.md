## Context

The command palette currently models three search modes: `All`, `Files`, and `Commands`. Static note and bookmark commands already exist in the command registry, but they are categorized as `Edit` or `View`, so keyboard-first discovery does not line up with the dedicated header-bar `Notes` menu or the `Browse Notes...` browser.

Existing notes specs already define the deeper search surface for bookmarks, folder notes, document notes, and saved open-tab note rows. This change should improve command-palette discovery and categorization without creating a second note index or reading sidecar note bodies during palette search.

## Goals / Non-Goals

**Goals:**

- Add a first-class `Notes` command palette mode in the same selector and Tab cycle as the existing modes.
- Reclassify note and bookmark workflow commands under a `Notes` command category.
- Present note commands in clear sections for `All` and `Notes` modes.
- Keep command activation routed through the existing action ids and workflow handlers.
- Add focused unit/widget coverage for mode order, command categorization, grouping, and exclusion rules.

**Non-Goals:**

- Do not index, rank, or display individual bookmark rows, document-note rows, folder-note rows, or note-body matches inside the command palette.
- Do not change note sidecar formats, persistence, migration, or the `Browse Notes...` browser behavior.
- Do not add new keyboard shortcuts or dependencies.

## Decisions

### Model Notes in the palette domain

Add `SearchMode::Notes` and `CommandCategory::Notes` in `model::palette`. This keeps selector labels, Tab order, dropdown positions, command subtitles, and service filtering backed by the same pure Rust model as the existing modes and categories.

Alternative considered: infer note commands by string matching labels or action ids only in the UI. That would avoid a model enum change, but it would make categorization harder to test and easier to drift as commands are renamed.

### Keep the palette as a workflow launcher

`Notes` mode filters the static command registry to note/bookmark workflow commands. It does not enumerate individual note records, bookmark records, or note-body hits. Users who want stored-note search continue through `Browse Notes...`, which already has the right workspace scoping, open-tab handling, preview behavior, lazy bookmark excerpts, and corruption/recovery rules.

Alternative considered: add note sidecar search directly to the command palette. That would duplicate `Browse Notes...` behavior, add file I/O to a latency-sensitive palette path, and create subtle conflicts around workspace scoping and open-tab supplemental rows.

### Use intent sections for Notes mode

In `Notes` mode, group commands by user intent rather than by implementation type:

- `Browse`: `Browse Notes`, `Browse Bookmarks`
- `Current Document`: `Toggle Bookmark`, `Edit Bookmark`, `Open Document Note`
- `Bookmark Navigation`: `Next Bookmark`, `Previous Bookmark`
- `Workspace`: `Open Folder Note`

These section names match what the user is trying to do and avoid mixing document-local actions with workspace actions. Empty sections should be omitted for narrow queries.

Alternative considered: a single flat `Notes` list. That would be cheaper to implement, but it would not solve the main discoverability problem once the palette is open with an empty or broad query.

### Split note commands out of All mode

`All` mode should keep the existing file-source priority, then show matching note commands under `Notes`, then matching non-note commands under `Commands`. Note commands must not appear in both sections. This makes note workflows visible without changing `Files` mode or hiding the full command registry from `Commands` mode.

Alternative considered: leave all commands under `Commands` in `All` mode and rely on subtitles. That preserves the current structure, but it keeps note workflows buried exactly where users are already missing them.

## Risks / Trade-offs

- Mode-position drift could break dropdown synchronization or Tab cycling. Mitigation: update `SearchMode::ALL`, `position`, `from_position`, labels, placeholders, and widget tests together.
- Section filtering could accidentally hide a note command from one mode. Mitigation: centralize the note command predicate or section mapping and test all expected note action ids.
- `Commands` mode could surprise users if note commands disappear. Mitigation: keep `Commands` mode as the complete command registry and only split note commands specially in `All` and `Notes` modes.
- Empty section headers could clutter narrow searches. Mitigation: keep the existing append-only-when-nonempty group behavior for note sections.

## Migration Plan

No data migration is required. Implement the enum/category additions, reclassify static commands, add grouping/filter helpers, and update tests. Rollback is the reverse code change because no persisted state or external API changes are introduced.

## Open Questions

- None.
