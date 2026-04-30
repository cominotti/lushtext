## Why

The minimap viewport indicator currently uses the system accent color, which can produce a bright blue block on the right edge of dark editor tabs. The indicator is useful and should remain visible, but it should feel like editor navigation chrome rather than an attention-grabbing selection state.

## What Changes

- Replace the accent-colored minimap viewport styling with a subtler neutral treatment that still clearly marks the visible document region.
- Keep the existing `GtkSourceMap` ownership of minimap navigation, scrolling, and viewport geometry.
- Preserve semantic marker colors for bookmarks, search matches, modified lines, and long-line warnings so those signals remain distinguishable from the viewport indicator.
- Add regression coverage that prevents the viewport indicator from returning to accent-blue styling.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `editor-minimap`: The viewport overlay requirement now includes visual calmness: it must remain visible without using the system accent color or competing with semantic markers.

## Impact

- Affects minimap CSS in `resources/style/style.css`.
- May affect the bundled GtkSourceView style-scheme resources if the neutral color should align with `map-overlay`.
- Adds or updates widget/CSS regression coverage for minimap viewport styling.
- No changes to persisted settings, file formats, keyboard shortcuts, minimap navigation behavior, or dependencies.
