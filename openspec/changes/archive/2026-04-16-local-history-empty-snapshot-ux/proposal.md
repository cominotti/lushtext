## Why

LushText's local-history browser currently has two connected UX problems. First,
valid empty snapshots render as a blank preview plus raw `0 B` metadata, which
looks like a failure instead of a real historical document state. Second,
file-backed draft restore currently allows the browser to surface fresh
baseline entries for the pre-restore disk file even when that state is not
meaningful from the user's point of view.

This should be fixed now because the current timeline mixes technically correct
history with entries that do not match how users experience a restored
document. The browser should both explain legitimate empty states clearly and
avoid creating noisy baseline entries that exist only because a draft was
reapplied on top of older on-disk content.

## What Changes

- Clarify legitimate empty local-history snapshots in the browser so they
  render as an explicit empty-snapshot state instead of an ambiguous blank
  preview.
- Update snapshot metadata presentation so remaining empty snapshots are
  described semantically, not only as `0 B`.
- Treat file-backed draft restore as continuity of the user's unsaved work
  rather than as a new edit cycle that deserves a fresh baseline entry for the
  stale on-disk file.
- Keep empty snapshots restorable when they are still valid historical states,
  while adjusting secondary actions to match the lack of text content.
- Update the local-history product note and living spec so both the
  empty-snapshot explanation and the draft-restored timeline behavior become
  part of the permanent contract.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `local-history`: The browse experience now requires explicit UX for empty
  snapshots, and draft-restored files now require timeline behavior that avoids
  surfacing noisy baseline entries from pre-restore disk state.

## Impact

- Affected code: `crates/lushtext-core/src/ui/window/local_history.rs`,
  `crates/lushtext-core/src/ui/editor_page/local_history.rs`,
  `crates/lushtext-core/src/ui/window/drafts.rs`, and the widget tests covering
  local-history browsing states.
- Affected systems: local-history preview UX, snapshot metadata copy,
  action-state clarity for empty snapshots, and automatic baseline capture
  semantics after draft restore.
- Affected docs: `docs/next/session-time-travel.md` and
  `openspec/specs/local-history/spec.md` should describe the empty-snapshot
  and draft-restored timeline behavior explicitly.
- Dependencies and APIs: no new dependency is expected; the change stays within
  the existing `AdwDialog` + `AdwNavigationSplitView` browser workflow.
