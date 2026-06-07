## Why

`Browse Notes...` currently behaves correctly as a workspace-scoped browser, but it can feel incomplete when the user has a saved file open outside the active workspace scope and has bookmarks or a document note attached to that file. The most coherent path is to keep workspace scope explicit while also treating saved open tabs as first-class, temporary sources in the browser.

## What Changes

- Add an explicit `Open Tabs` section to `Browse Notes...` for saved open files that are outside the current workspace scope.
- Include live bookmarks from those open tabs, using the current editor bookmark projection so unsaved debounced sidecar state still appears immediately.
- Include document notes attached to those saved open tabs, even when the file does not belong to a restored workspace root.
- Let `Browse Notes...` open when there are no restored workspaces but there are eligible saved open-tab notes or bookmarks.
- Keep workspace rows scoped and labeled as before; open-tab rows MUST NOT be represented as belonging to a fake workspace.
- Update browser search, preview, and Open behavior so open-tab rows are searchable, clearly sourced, and navigable.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `workspace-notes`: Extend the unified `Browse Notes...` surface from strictly workspace-only listing to workspace results plus a clearly separated open-tab supplemental section.
- `line-bookmarks`: Add requirements for bookmarks from saved open tabs outside the current workspace scope to appear in `Browse Notes...`.
- `document-notes`: Add requirements for document notes attached to saved open tabs outside the current workspace scope to appear in `Browse Notes...`.

## Impact

- `crates/lushtext-core/src/ui/window/notes.rs`: notes-browser loading, entry modeling, section grouping, preview copy, search matching, and Open behavior.
- `crates/lushtext-core/src/services/bookmark_service.rs`: may need a helper for listing or resolving bookmark state for explicit open-tab paths without walking workspace roots.
- `crates/lushtext-core/src/services/document_note_service.rs`: may need a helper for loading document notes for explicit open-tab paths without walking workspace roots.
- `crates/lushtext/tests/widget/window.rs`: widget coverage for open-tab bookmarks/document notes outside workspace scope and no-workspace browse behavior.
