## Why

LushText's minimap is visually present, but its end-of-document interaction still diverges from GNOME Text Editor. The remaining mismatch is most noticeable near the last third of the minimap, where the viewport indicator runs out of travel too early because the editor does not provide the same overscroll tail room GNOME Text Editor uses.

## What Changes

- Add GNOME-style dynamic end-of-document overscroll to the editor view so the main document and minimap both keep extra blank tail space after the last line.
- Use that shared overscroll tail to improve end-of-file minimap dragging and clicking before introducing any custom minimap interaction math.
- Add focused regression coverage for the overscroll behavior and for the minimap/editor geometry relationship near the end of the document.
- Leave any fully custom click or drag remapping as a later fallback only if overscroll alignment still proves insufficient.

## Capabilities

### New Capabilities
- `minimap-navigation-parity`: Ensure minimap clicks and drags near end-of-file match the expected document-region targeting and viewport-indicator anchoring behavior by preserving GNOME-style overscroll tail room.

### Modified Capabilities

## Impact

- Affected code: editor-page source view allocation or sizing logic, minimap wiring in `crates/lushtext-core/src/ui/editor_page/minimap.rs`, and any nearby editor-page workflow needed to update bottom-margin overscroll dynamically.
- Affected tests: minimap widget coverage in `crates/lushtext/tests/widget/` plus any focused editor-page tests for dynamic overscroll updates.
- Dependencies and systems: `GtkSourceMap` margin binding semantics, GTK widget tests, and the existing minimap OpenSpec follow-up flow.
