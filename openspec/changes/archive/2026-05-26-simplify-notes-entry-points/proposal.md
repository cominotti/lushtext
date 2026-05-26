## Why

LushText's note-related workflows are now powerful enough that the `Notes` menu reads like an implementation index: bookmarks, range notes, document notes, workspace notes, browsers, and export all sit side by side. This makes the surface harder to scan than it needs to be and creates a conceptual mismatch where `Browse Notes...` excludes bookmarks even though bookmarks live in the same menu.

## What Changes

- Simplify the header-bar `Notes` menu into a short set of high-level, GNOME-HIG-compatible actions.
- Keep `Browse Notes...` as the primary discovery surface for note-related saved metadata.
- Include bookmarks in the unified `Browse Notes...` browser as their own section alongside workspace notes, document notes, and range notes.
- Replace the separate `Browse Bookmarks...` menu entry with the unified `Browse Notes...` entry.
- Make the bookmark toggle menu label reflect the active cursor context, such as `Add Bookmark` or `Remove Bookmark`.
- Move cursor-specific edit actions (`Edit Bookmark Label...`, `Edit Range Note...`) out of the header menu while preserving shortcuts, command-palette commands, and workflow guards.
- Add contextual entry points for note workflows where they naturally belong, such as document notes from file context and workspace notes from workspace context.
- Fill the missing keyboard-shortcuts documentation for the existing `Edit Range Note` shortcut.
- Preserve note storage, sidecar identity, export format, and existing editor/browser behavior unless explicitly called out above.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-notes-menu`: Simplify the `Notes` menu contract, remove low-context edit-only items, and keep the menu focused on high-level note entry points.
- `line-bookmarks`: Add bookmarks to the unified `Browse Notes...` surface and make bookmark menu labeling reflect add/remove context.
- `workspace-notes`: Expand the workspace-scoped `Browse Notes...` contract so it aggregates bookmarks together with workspace, document, and range notes.

## Impact

- Affected UI resource files: `resources/ui/window.ui`, `resources/ui/shortcuts.ui`, and any relevant context-menu resources or builders.
- Affected shell and workflow code: `crates/lushtext-core/src/ui/window/actions.rs`, `crates/lushtext-core/src/ui/window/notes.rs`, sidebar workspace-section context-menu wiring, editor context-menu wiring, and command-palette metadata where labels or grouping need to stay coherent.
- Affected services: likely reuse `bookmark_service` listing in the unified Notes browser without changing bookmark storage.
- Affected tests: widget coverage for the `Notes` menu structure/state, unified browser sections/search/open behavior, context menu entry points, and shortcuts overlay parity.
- No new external dependencies, storage migrations, or data-format changes.
