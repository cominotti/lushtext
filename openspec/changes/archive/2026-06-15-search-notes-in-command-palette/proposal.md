## Why

The command palette's `Notes` mode currently behaves like a second command search filtered to note-related actions, which duplicates `Commands` mode and conflicts with the user's expectation that `Notes` means stored note content. LushText already has a rich `Browse Notes...` model for bookmarks, folder notes, document notes, and eligible open-tab rows, so the palette should make those records directly searchable without hiding note actions from the complete command search.

## What Changes

- Make command palette `Notes` mode list actual note and bookmark records instead of note workflow commands.
- Reuse the existing Notes browser vocabulary for note result sections: `Bookmarks`, `Folder Notes`, `Document Notes`, and `Open Tabs`.
- Search note rows by visible title, path/location metadata, workspace/folder metadata, bookmark label/line metadata, and note body text for document and folder notes.
- Keep bookmark source excerpts lazy and out of palette matching so palette search does not read closed source files just to decide whether a bookmark matches.
- Keep note-related actions available in `Commands` mode with `Notes` command subtitles, but stop duplicating them under the top-level `Notes` result group in `All` mode.
- Preserve command palette source priority and keyboard/mouse mode selection while adding note record activation for bookmarks, document notes, and folder notes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `command-palette-source-groups`: Redefine `Notes` mode and the `All`-mode Notes group from note command launchers to searchable note/bookmark records, with sectioned note categories and note-row activation.

## Impact

- Affected specs: `openspec/specs/command-palette-source-groups/spec.md`.
- Affected Rust areas: palette model/search result types, palette service note-search source, command palette row transport/item activation, window-owned note snapshot/listing integration, and widget/service tests.
- Existing note storage, note browser persistence, and command action IDs remain compatible.
