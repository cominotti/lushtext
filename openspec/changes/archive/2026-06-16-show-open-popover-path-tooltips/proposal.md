## Why

The Open popover uses compact, GNOME-style recent rows where long file names and parent paths intentionally ellipsize. Users need a reliable way to confirm the exact local file before opening it, especially when recent entries have similar names or deep paths.

## What Changes

- Show the full absolute activation path as a hover tooltip for each recent-document row in the Open popover.
- Keep row layout, width, ellipsizing, scrolling, activation, and GNOME-style visual parity unchanged.
- Preserve the remove control's action tooltip so hovering the close button still describes removal rather than the document path.
- Refresh row tooltips whenever virtualized rows are rebound so dense lists, filtering, and row recycling cannot show stale paths.
- Add focused widget regression coverage for representative rows, awkward/deep paths, dense or filtered lists, and the remove-button exception.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `recent-open-popover`: Recent rows must reveal their full absolute activation path on hover without disturbing GNOME-style row presentation or remove-button behavior.

## Impact

- Affected code: `crates/lushtext-core/src/ui/open_popover/*`.
- Affected tests: Open popover widget tests under `crates/lushtext/tests/widget/open_popover.rs`.
- No persistence format, public API, dependency, or filesystem behavior changes are expected.
