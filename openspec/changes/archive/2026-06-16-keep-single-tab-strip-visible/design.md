## Context

LushText uses `AdwTabView` for document pages and `AdwTabBar` as the visible tab-management surface. The current template lets `AdwTabBar` use its default autohide behavior, so a single unpinned tab has no visible tab strip while a single pinned tab can reveal the strip again. That creates an inconsistent normal editing state because the tab strip owns context-menu actions such as Pin or Unpin, close-other operations, move operations, and the pinned-state indicator.

The window already has separate shell state for no-tab empty content and Focus Mode chrome suppression. Those states should remain explicit: empty windows should not gain an inert tab strip, and Focus Mode should continue to hide ordinary chrome.

## Goals / Non-Goals

**Goals:**

- Keep the tab strip visible in normal mode whenever at least one tab is open.
- Preserve no-tab empty state cleanliness by hiding the tab strip when there are no tabs.
- Preserve Focus Mode's existing chrome suppression.
- Keep tab-context actions targeted through the existing Adwaita tab context menu.
- Cover state extremes: no tabs, one unpinned tab, one pinned tab, multiple tabs, constrained geometry, and Focus Mode.

**Non-Goals:**

- Add a new header-bar Pin or Unpin button.
- Add command-palette or shortcut access to tab pinning.
- Change pinned-tab grouping, session persistence, duplicate-tab detection, or save/close safety.
- Redesign the tab strip styling.

## Decisions

### Disable one-tab autohide, then gate visibility through shell state

Set the tab bar so Libadwaita no longer hides it just because the view has one unpinned tab. Then keep LushText responsible for hiding or showing the tab strip according to shell state:

- visible when normal mode has `tab_view.n_pages() > 0`
- hidden when there are no tabs
- hidden while Focus Mode is active

This keeps the desired product rule independent from Libadwaita's default "one unpinned tab means no tab bar" heuristic.

Alternative considered: leave autohide enabled and add another Pin entry point. That would solve only the pinning affordance, while still leaving single unpinned tabs visually different from single pinned tabs and from the canonical "tab strip is the primary active-document surface" model.

### Keep tab-context actions target-based

The existing tab context menu uses `AdwTabView::setup-menu` to target the clicked tab, including background tabs. The change should preserve that model instead of retargeting tab actions to the active page globally.

Alternative considered: make `win.toggle-tab-pinned` operate on the active tab when no context target exists. That would make automation easier but blur the current separation between generic active-tab commands and context-target tab commands.

### Treat Focus Mode as a stronger chrome state

Focus Mode already hides the header bar, tab bar, and status bar. The single-tab visibility rule should not weaken that contract. If a helper is introduced to synchronize tab-strip visibility, Focus Mode should be one of its inputs rather than a later override that can drift.

Alternative considered: rely on the current Focus Mode `set_visible(false)` calls after every tab-count update. That is fragile if future tab-count changes happen while Focus Mode is active.

## Risks / Trade-offs

- More persistent chrome in one-tab normal mode -> Mitigate by preserving existing restrained tab-strip styling and verifying constrained-height geometry.
- Empty state accidentally shows a blank tab strip -> Mitigate with explicit no-tabs widget coverage.
- Focus Mode tab strip reappears after tab open/close while focused -> Mitigate by centralizing or consistently applying visibility from both tab count and Focus Mode state.
- Visual smoke baselines may change because one-tab normal mode gains a tab strip -> Mitigate by refreshing the relevant visual proof artifacts as part of implementation.
