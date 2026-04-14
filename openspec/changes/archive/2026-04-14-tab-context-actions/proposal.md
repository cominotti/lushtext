## Why

LushText's tab strip already supports opening, selecting, and safely closing tabs, but it still lacks the tab-management affordances people expect once they are working across many files. Adding pinning, bulk-close helpers, and explicit left or right movement will make the editor feel more complete while reducing repetitive tab-bar drag and close work.

## What Changes

- Add a tab context menu on the `AdwTabBar` so users can manage the clicked tab directly from the strip.
- Add a `Pin` or `Unpin` action that keeps pinned tabs grouped at the leading side of the tab bar and preserves that state across session restore.
- Add `Close All Tabs to the Right` and `Close Other Tabs` actions that reuse the existing unsaved-change confirmation flow instead of bypassing it.
- Add `Move Left` and `Move Right` actions so users can reorder the current tab without drag-and-drop.
- Keep tab-order and pin-state persistence aligned with the restored session so restart returns the same layout the user arranged.

## Capabilities

### New Capabilities
- `tab-context-actions`: Manage tabs from the tab strip with context actions for pinning, safe bulk closing, and directional reordering.

### Modified Capabilities

None.

## Impact

- Affected code:
  - `resources/ui/window.ui`
  - `crates/lushtext-core/src/ui/window/imp.rs`
  - `crates/lushtext-core/src/ui/window/documents.rs`
  - `crates/lushtext-core/src/ui/window/session_persistence.rs`
  - `crates/lushtext-core/src/model/session.rs`
  - widget and session restore tests under `crates/lushtext/tests/` and related unit-test modules
- Affected systems:
  - `AdwTabBar` and `AdwTabView` menu wiring
  - bulk tab-close workflows and unsaved-change dialogs
  - session serialization and restore ordering
- Dependencies:
  - No new external dependencies expected; the work can use the existing libadwaita tab APIs already in the workspace.
