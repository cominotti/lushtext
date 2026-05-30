## Why

Search markers in the editor minimap can appear far below the rendered document, especially when the editor has GNOME-style end-of-file overscroll. The minimap's semantic marker strip is currently projected by raw line count, so it can drift away from the `GtkSourceMap` geometry it is drawn beside.

## What Changes

- Align semantic minimap markers with the same rendered geometry used by the bound `GtkSourceMap`.
- Prevent search, bookmark, modified-line, and long-line markers from painting inside blank EOF overscroll tail space when the corresponding document lines end above it.
- Preserve existing minimap visibility, native click/drag navigation, viewport indicator styling, marker colors, and marker-lane semantics.
- Add extensive regression coverage across unit-style projection behavior, widget-level marker geometry, EOF overscroll, search marker lifecycle, all marker categories, and live GTK screenshot or geometry verification where practical.
- Add idempotent local development tooling so future live GTK verification has the required Flatpak runtime dependencies plus input and screenshot helpers.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `editor-minimap`: Semantic navigation markers must track the `GtkSourceMap` layout geometry, including EOF overscroll, instead of being spread across the full marker-strip height by raw document line count.

## Impact

- Affected code: `crates/lushtext-core/src/ui/editor_page/minimap.rs`, focused editor-page minimap tests, any GTK widget-test helpers needed to inspect marker geometry, and local development/debugging setup docs or scripts.
- Affected behavior: visual placement of minimap semantic markers only.
- No application API, data format, runtime dependency, preference, or migration changes are expected.
