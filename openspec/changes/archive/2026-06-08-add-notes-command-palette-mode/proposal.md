## Why

Notes and bookmark workflows are now important enough to have their own menu and browser, but the command palette still makes them feel incidental by scattering them across `Edit` and `View` commands. A dedicated Notes palette mode and category gives keyboard-first users a fast, predictable gateway to note workflows without duplicating the deeper `Browse Notes...` search surface.

## What Changes

- Add `Notes` as a first-class command palette mode alongside `All`, `Files`, and `Commands`.
- Add a `Notes` command category for bookmark, document-note, folder-note, and notes-browser commands.
- Show note-related commands in a dedicated `Notes` section in `All` mode, before the generic `Commands` section.
- In `Notes` mode, show only note/bookmark workflow commands, grouped by user intent:
  - `Browse`
  - `Current Document`
  - `Bookmark Navigation`
  - `Workspace`
- Keep full note, bookmark, and note-body result search inside `Browse Notes...`; the command palette remains a workflow launcher.
- Keep `Commands` mode as the full command search surface, including commands in the `Notes` category.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `command-palette-source-groups`: Add the `Notes` mode, the `Notes` command category, and the sectioning rules for notes-related commands in `All`, `Notes`, and `Commands` modes.

## Impact

- Affected code:
  - `crates/lushtext-core/src/model/palette.rs`
  - `crates/lushtext-core/src/services/palette/commands.rs`
  - `crates/lushtext-core/src/ui/command_palette/`
  - `crates/lushtext-core/src/services/palette/tests.rs`
  - `crates/lushtext/tests/widget/command_palette.rs`
- No new dependencies or persistence formats.
- No changes to document-note, folder-note, bookmark sidecar data, or the `Browse Notes...` browser search behavior.
