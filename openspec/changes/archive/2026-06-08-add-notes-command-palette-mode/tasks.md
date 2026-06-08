## 1. Palette Model and Registry

- [x] 1.1 Add `Notes` to `SearchMode`, including selector order, dropdown positions, labels, placeholders, forward cycling, and reverse cycling.
- [x] 1.2 Add `Notes` to `CommandCategory` and ensure command subtitles render `Notes` with existing shortcut hints.
- [x] 1.3 Reclassify note and bookmark commands in `services::palette::commands::all_commands()` to `CommandCategory::Notes` while preserving action ids, labels, and shortcuts.
- [x] 1.4 Add a central note-command predicate or section mapping that covers `Browse Notes`, `Browse Bookmarks`, `Toggle Bookmark`, `Edit Bookmark`, `Next Bookmark`, `Previous Bookmark`, `Open Document Note`, and `Open Folder Note`.

## 2. Palette Search and Grouping

- [x] 2.1 Update command-palette result assembly so `Notes` mode searches only note-category commands.
- [x] 2.2 Group `Notes` mode results under nonempty `Browse`, `Current Document`, `Bookmark Navigation`, and `Workspace` headers in that order.
- [x] 2.3 Update `All` mode grouping so matching note commands appear under `Notes` after file groups and before non-note `Commands`.
- [x] 2.4 Ensure `All` mode does not duplicate note commands in the generic `Commands` section.
- [x] 2.5 Keep `Commands` mode as the complete command search surface, including note-category commands.
- [x] 2.6 Keep command activation routed through existing palette item action ids and existing window actions.

## 3. Automated Coverage

- [x] 3.1 Update palette model and service tests for the new `Notes` mode, selector order, category label, and note command categorization.
- [x] 3.2 Update command-palette widget tests for dropdown contents, Tab/Shift+Tab cycling, and `Notes` placeholder text.
- [x] 3.3 Add widget coverage proving `Notes` mode uses the required intent sections and excludes files and non-note commands.
- [x] 3.4 Add widget coverage proving `All` mode orders `Open Tabs`, workspace files, `Notes`, and `Commands`, with no duplicated note commands.
- [x] 3.5 Add coverage proving `Commands` mode still finds note commands and displays the `Notes` category subtitle.

## 4. Validation

- [x] 4.1 Run `cargo fmt --all`.
- [x] 4.2 Run the focused palette service tests.
- [x] 4.3 Run the focused command palette widget tests with the existing widget-test harness.
- [x] 4.4 Run `openspec validate add-notes-command-palette-mode --strict`.
- [x] 4.5 Run `git diff --check`.
