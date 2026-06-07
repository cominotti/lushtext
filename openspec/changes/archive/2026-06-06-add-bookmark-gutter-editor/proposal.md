## Why

Bookmarks already appear as first-class gutter marks, but editing one still depends on moving the cursor to the bookmarked line and invoking a separate command. Clicking the visible bookmark mark should expose the same edit affordance directly, and the edit surface should let users correct a bookmark's line without relying on fragile drag gestures.

## What Changes

- Add a bookmark edit dialog that can be opened by activating an existing bookmark gutter mark.
- Expand bookmark editing from label-only updates to label and line updates.
- Keep line changes non-destructive: moving a bookmark updates its live mark and sidecar record without modifying the source file bytes.
- Preserve the existing bookmark save, browse, navigation, minimap, and sidecar identity behavior.
- Do not add draggable gutter marks or a new Favorites/file-pin concept in this change.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `line-bookmarks`: Add direct gutter-mark editing and bookmark line reassignment through a small modal edit surface.

## Impact

- Affected code: `crates/lushtext-core/src/ui/editor_page/bookmarks.rs`, `crates/lushtext-core/src/ui/editor_page/mod.rs`, `crates/lushtext-core/src/ui/editor_page/imp.rs`, `crates/lushtext-core/src/ui/window/notes.rs`, bookmark-related widget tests, and README/manual test documentation.
- Affected behavior: clicking an existing bookmark gutter mark opens editing UI; changing the line moves that bookmark in the active buffer and persists through the existing debounced bookmark save path.
- Dependencies: no new crate or runtime dependencies expected.
