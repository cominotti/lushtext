## 1. Styling

- [x] 1.1 Update the minimap `GtkSourceMap` slider CSS so the viewport indicator uses neutral editor-chrome colors instead of `@accent_bg_color` or `@accent_color`.
- [x] 1.2 Preserve the existing minimap slider geometry, including border radius, side margins, padding, and width behavior.
- [x] 1.3 Verify semantic marker strip drawing remains unchanged for bookmark, search, modified-line, and long-line markers.

## 2. Regression Coverage

- [x] 2.1 Add a focused regression test that fails if minimap slider styling references accent color tokens again.
- [x] 2.2 Keep existing minimap widget tests passing for native `GtkSourceMap` geometry, controller ownership, and EOF tail behavior.

## 3. Verification

- [x] 3.1 Run the focused test target that covers minimap styling and editor-page minimap behavior.
- [x] 3.2 Launch LushText in dark mode with the minimap enabled and visually confirm that the viewport indicator remains visible but no longer appears as a bright accent-colored block.
- [x] 3.3 Run `openspec validate soften-minimap-viewport-indicator`.
