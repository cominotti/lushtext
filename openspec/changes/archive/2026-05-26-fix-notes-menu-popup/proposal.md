## Why

The header-bar `Notes` menu can become visible but fail to open when clicked after the menu simplification work. This blocks the primary discovery path for bookmarks, range notes, document notes, workspace notes, browsing, and export from the header bar.

## What Changes

- Make the `Notes` menu popup open reliably when the visible header-bar button is clicked.
- Preserve dynamic bookmark toggle labeling without replacing the menu model during the popup activation path.
- Keep existing action sensitivity and workflow guards for saved-file and workspace-scope context.
- Add regression coverage that exercises the menu as a popup, not only as an inspectable `GMenuModel`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-notes-menu`: The dedicated `Notes` menu must open a visible popup when activated and must not rebuild or replace its menu model in a way that cancels the open.

## Impact

- Affects `crates/lushtext-core/src/ui/window/imp.rs` and `crates/lushtext-core/src/ui/window/notes.rs` menu refresh/open wiring.
- Affects widget tests in `crates/lushtext/tests/widget/window.rs`.
- No dependency, storage, sidecar, or user-data migration impact.
