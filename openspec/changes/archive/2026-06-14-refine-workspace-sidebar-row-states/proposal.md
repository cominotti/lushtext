## Why

Workspace file-tree rows currently keep a permanent selection highlight after ordinary clicks, which makes a clicked folder or file look more important than it is. The sidebar should instead reserve persistent row emphasis for files that are actually open in tabs, while keeping transient hover, press, keyboard focus, and peek affordances readable.

## What Changes

- Remove the misleading persistent mouse-click selection highlight from workspace folder and file rows.
- Preserve transient hover/press feedback for pointer use and a clear focus-visible state for keyboard navigation and Space-to-peek.
- Add a subtle persistent indicator for file rows whose path is open in any tab.
- Add a slightly stronger but still restrained indicator for the file row matching the currently active tab.
- Keep open-file indicators synchronized when tabs open, close, switch, fail to load, rename, Save As, delete, restore from session, or when file-tree rows are rebound through GTK row recycling.
- Preserve the existing file-tree interactions: double-click activation, directory disclosure, context menus, inline rename, file peek, focus-folder, folder reorder DnD, and no-horizontal-scrollbar sidebar geometry.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-sidebar-shell`: Clarify workspace file-tree row visual states so sticky selection no longer implies app state, while open and active tab file rows gain explicit sidebar indicators.

## Impact

- Affected code: `crates/lushtext-core/src/ui/sidebar/`, especially `workspace_section/`, plus window-to-sidebar tab-state synchronization from `crates/lushtext-core/src/ui/window/`.
- Affected resources: `resources/style/style.css` and possibly `resources/ui/workspace-section.ui` / `.blp` if a row indicator widget is added.
- Affected tests: widget coverage for sidebar row states, recycled rows, tab open/close/switch/rename/Save As/delete flows, hover/focus/peek behavior, dense trees, long labels, and constrained sidebar geometry.
- No new persistence, command actions, external APIs, or third-party dependencies are expected.
