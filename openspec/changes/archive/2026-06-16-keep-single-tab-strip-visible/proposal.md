## Why

LushText currently lets Libadwaita hide the tab strip when exactly one unpinned tab is open, which removes the only visible pointer target for tab-context actions such as Pin or Unpin. This now conflicts with LushText's own tab model, where pinned state persists across sessions and the tab strip remains the primary active-document surface.

## What Changes

- Keep the normal-mode tab strip visible whenever at least one tab is open, including the single unpinned-tab state.
- Keep the tab strip hidden when no tabs are open so the empty document state remains clean.
- Preserve Focus Mode behavior: Focus Mode continues to suppress the ordinary header bar, tab bar, status bar, workspace sidebar, and document-properties surface.
- Ensure pin/unpin, tab context menu reachability, pinned indicators, and tab-state visual contracts stay consistent across 0, 1, and many-tab states.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `tab-context-actions`: Require the tab strip and its context menu target to remain visible in normal mode for any open tab count greater than zero.

## Impact

- `resources/ui/window.blp` and regenerated `resources/ui/window.ui` for tab-strip visibility/autohide behavior.
- `crates/lushtext-core/src/ui/window/` tab, focus-mode, and content-stack shell logic if explicit visibility synchronization is needed.
- Widget/visual coverage for empty state, single unpinned tab, single pinned tab, multiple tabs, constrained geometry, and Focus Mode.
