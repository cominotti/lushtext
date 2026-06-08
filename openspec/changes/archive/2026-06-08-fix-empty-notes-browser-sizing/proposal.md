## Why

Opening `Browse Notes...` from an empty window now exposes the existing empty Notes browser, but that empty dialog can collapse into a very narrow, hard-to-read column. This makes the new window-scoped Notes entry point feel broken at the exact moment it should be explaining the empty state.

## What Changes

- Keep the empty Notes browser at a stable, readable compact-browser size instead of allowing the status-page content to drive an undersized dialog or scrollable empty state.
- Preserve the existing `No notes yet` empty-state copy, close behavior, Escape behavior, and rule that empty browsing must not create fake note rows or persistence data.
- Add widget coverage that verifies the empty Notes browser has usable rendered dimensions, not only the expected `content_width` property value.
- Inspect the nearby Local History empty-dialog pattern for the same sizing trap and either align it or document why it is unaffected.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `document-notes-menu`: The empty `Browse Notes...` browser state must remain legible and stable when opened from an empty or no-workspace window.

## Impact

- Affected UI code: `crates/lushtext-core/src/ui/window/notes.rs`
- Potentially related UI code to inspect: `crates/lushtext-core/src/ui/window/local_history.rs`
- Affected widget coverage: `crates/lushtext/tests/widget/window.rs`
- No persistence, note indexing, sidecar schema, command-palette, shortcut, or menu-model changes are expected.
