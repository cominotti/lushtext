## 1. Shared Note Search Model

- [x] 1.1 Add a GTK-free palette note result model with note category, display title, subtitle/detail text, searchable fields, and activation target variants for bookmarks, folder notes, and document notes.
- [x] 1.2 Extract or centralize the current `Browse Notes...` listing policy so palette note rows and the Notes browser share workspace-scope filtering, open-tab note handling, live bookmark overlay, de-duplication, and recovery diagnostics.
- [x] 1.3 Implement note-result matching for title, visible metadata, workspace/folder metadata, bookmark label and line metadata, and document/folder note body text without reading bookmark source excerpts for matching.
- [x] 1.4 Add service/model tests for note row construction, category ordering, metadata/body matching, empty sets, overlapping workspace folders, open-tab note rows, and the bookmark-excerpt non-match rule.

## 2. Command Palette Source Integration

- [x] 2.1 Extend command palette result transport and `PaletteItem` data to carry activatable note rows without exposing note bodies through automation snapshots.
- [x] 2.2 Add a cached note source to `LushtextCommandPalette`, refreshed through window-owned source updates rather than per-keystroke filesystem reads.
- [x] 2.3 Update grouped result assembly so `Notes` mode shows only note record categories ordered as `Bookmarks`, `Folder Notes`, `Document Notes`, and `Open Tabs`.
- [x] 2.4 Update `All` mode so note record categories appear after file groups and before `Commands`, using `Open Tab Notes` for saved open-tab note rows to avoid colliding with the file `Open Tabs` group.
- [x] 2.5 Keep `Commands` mode as the complete command registry search, including note-related actions with `Notes` subtitles, and stop using note command sections as the `Notes` mode result source.
- [x] 2.6 Update command-palette placeholder/accessibility copy so `Notes` mode describes note record search rather than note actions.

## 3. Window Refresh And Activation

- [x] 3.1 Refresh palette note sources when the palette opens, when workspace scope or restored folders change, and after note/bookmark mutations that can affect palette rows.
- [x] 3.2 Use generation tokens or equivalent freshness checks so stale background note-source loads cannot replace newer palette note rows.
- [x] 3.3 Route activated bookmark rows through the existing open-at-line workflow, closing the palette only after dispatch.
- [x] 3.4 Route activated folder-note rows through the existing folder-note target workflow without requiring an active document.
- [x] 3.5 Route activated document-note rows through the existing open-document plus document-note workflow with the correct workspace folder context.
- [x] 3.6 Preserve Escape, click-away dismissal, keyboard result navigation, header skipping, focus restoration, and command-palette target actions after note-row activation support is added.

## 4. UI State Coverage

- [x] 4.1 Replace current widget tests that expect `Notes` mode command sections with tests for `Bookmarks`, `Folder Notes`, `Document Notes`, and `Open Tabs` note categories.
- [x] 4.2 Add widget coverage proving `Notes` mode matches document-note body text, folder-note body text, bookmark labels/line metadata, workspace names, folder paths, and file paths.
- [x] 4.3 Add widget coverage proving `Notes` mode excludes files and commands while `Commands` mode still includes note commands with `Notes` subtitles.
- [x] 4.4 Add widget coverage for `All` mode source order, including file `Open Tabs`, workspace files, note record categories, `Open Tab Notes`, and complete `Commands` results.
- [x] 4.5 Add activation coverage for bookmark, folder-note, and document-note palette rows.
- [x] 4.6 Add empty/no-match and dense/awkward-row coverage proving no fake rows, result-list-only scrolling, stable controls, and no unintended horizontal scrolling.
- [x] 4.7 Update visual smoke setup/assertions for `command-palette-notes` so the scenario seeds real note records instead of relying on note command rows.

## 5. Validation

- [x] 5.1 Run `openspec validate search-notes-in-command-palette --strict`.
- [x] 5.2 Run focused palette and note service tests, including `cargo test -p lushtext-core services::palette` and any new note-search service test target.
- [x] 5.3 Run focused widget tests with `scripts/run-widget-tests.sh --headless -- command_palette`.
- [x] 5.4 Run `make visual-smoke` or the focused visual smoke lane covering `command-palette-notes` if available.
- [x] 5.5 Run `make check-automation-docs` if automation docs or visible command-palette snapshot fields change.
- [x] 5.6 Run the final repo gate selected for the implementation scope, at minimum `make pre-commit` after Rust/UI changes.
