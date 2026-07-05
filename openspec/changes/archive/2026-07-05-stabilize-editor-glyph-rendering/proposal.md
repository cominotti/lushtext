## Why

Typing `[` in a blank LushText document through `make run` clips the glyph's top stroke on the still-active editor line. Pressing Enter makes the previous line redraw correctly, which proves the old change did not fix the live typing path and that settled geometry proof alone is not enough.

The previous broad plan for visual-geometry, minimap, and Automation1 changes has been discarded. The verified fix is narrower: give the main editor a safe font/line-box contract and add a live key-event smoke that fails before Enter or focus changes can repair the pixels.

## What Changes

- Disable `GtkTextView`'s built-in `monospace` mode on the main editor surface, while still applying the selected monospace font through LushText's editor surface class.
- Add an editor-only line-height guard that leaves enough top ink room for Adwaita Mono on GTK 4.22 / GtkSourceView 5.20 without loosening minimap or sidebar metrics.
- Add a focused `editor-glyph-live-smoke` lane that opens an isolated GUI session, types real `[` key events, captures the active line before Enter, and verifies top horizontal ink in the screenshot crop.
- Keep the GResource prefix/rerun fixes so bundled resource edits reach `make run` instead of being silently stale.

## Capabilities

### New Capabilities

- `editor-glyph-rendering`: Defines the active-line glyph top-ink contract and the live smoke proof required for future editor font/metric changes.

## Impact

- Affected UI code: editor template and display-wide font CSS in `crates/lushtext-core/src/ui/theme.rs`.
- Affected build/runtime resources: GResource prefix and build script rerun inputs.
- Affected smoke coverage: Makefile, end-user smoke workflow, workflow checker, and `scripts/editor-glyph-live-smoke.py`.
- No public file format, app-data migration, Automation1, or minimap behavior change is expected.
