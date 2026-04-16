## Why

The current local-history browser satisfies the MVP contract, but it still reads
more like a conventional modal than a true history viewer. On larger desktop
windows, the preview does not get enough visual dominance to feel like a place
for reading and comparing earlier document states with confidence.

This should be tightened now because local history is already implemented and
usable, which makes the next UX gap very concrete: the browser needs to open as
an intentionally large, viewer-first surface while still respecting GNOME HIG's
guidance for simple, parent-bounded secondary windows.

## What Changes

- Change the local-history browser requirement so it opens as a much larger,
  viewer-first dialog rather than a modest modal with a preview attached.
- Give the preview pane clear visual priority on wide windows, with the
  snapshot list acting as a narrower browse rail instead of competing equally
  for space.
- Define sizing rules that feel generous on desktop displays but remain bounded
  by the parent window and degrade cleanly on smaller widths.
- Preserve the existing adaptive narrow-window navigation flow instead of
  forcing a wide split layout everywhere.
- Update the product note and living OpenSpec requirement so the intended
  viewer feel, size, and layout balance become part of the permanent contract.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `local-history`: The browse experience now requires a larger, viewer-first
  adaptive dialog with explicit preview-dominant sizing and layout behavior.

## Impact

- Affected code: `crates/lushtext-core/src/ui/window/local_history.rs` and the
  widget tests that validate local-history browser presentation.
- Affected systems: local-history browsing UX, adaptive dialog sizing, preview
  layout balance, and narrow-window navigation behavior.
- Affected docs: `docs/next/session-time-travel.md` and
  `openspec/specs/local-history/spec.md` need to reflect the larger
  GNOME-HIG-friendly viewer contract.
- Dependencies and APIs: no new external dependency is expected; the change
  builds on the existing `AdwDialog` and `AdwNavigationSplitView` composition.
