## Why

The `Notes` header-bar menu currently disappears in no-tab states even though `Browse Notes…` can be useful without an active document. This makes a workspace/global notes surface feel attached to whichever editor tab happens to exist, and it leaves users without a visible way to browse notes after closing the last tab.

## What Changes

- Make the header-bar `Notes` menu a window-scoped entry point instead of a tab-scoped surface.
- Keep `Browse Notes…` visible and actionable from the header menu even when there are no open tabs, including the empty no-workspace state that already has a clear Notes browser empty state.
- Continue using item sensitivity for scope-specific actions:
  - saved-document actions stay insensitive without a saved active file
  - `Open Folder Note…` stays insensitive in `All workspaces` or when no concrete workspace folder target is selected
- Preserve the existing menu placement, menu sections, primary-menu exclusion, command-palette actions, shortcuts, and Notes browser behavior.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `document-notes-menu`: The Notes menu availability requirement changes so the header menu remains visible as a stable window-level Browse Notes entry point, while individual menu actions continue to express document and workspace applicability through sensitivity.

## Impact

- Affected specs: `openspec/specs/document-notes-menu/spec.md`
- Affected UI code: `crates/lushtext-core/src/ui/window/notes.rs`
- Affected widget coverage: `crates/lushtext/tests/widget/window.rs`
- No persistence, sidecar format, command-palette, shortcut, or Notes browser indexing changes are expected.
