## 1. Tab Strip Visibility

- [x] 1.1 Update the window template so `AdwTabBar` no longer auto-hides solely because there is one unpinned tab.
- [x] 1.2 Add or update window-shell visibility synchronization so the tab strip is visible in normal mode when `tab_view.n_pages() > 0`.
- [x] 1.3 Ensure the same visibility path hides the tab strip when there are no tabs and while Focus Mode is active.
- [x] 1.4 Regenerate committed UI from Blueprint and keep the template contract in sync.

## 2. Behavior Coverage

- [x] 2.1 Add widget coverage proving the no-tab empty state does not render an inert or blank tab strip.
- [x] 2.2 Add widget coverage proving a single unpinned tab renders a visible tab-strip target and exposes the Pin context action.
- [x] 2.3 Add widget coverage proving a single pinned tab renders consistently and exposes the Unpin context action.
- [x] 2.4 Add widget coverage proving multiple-tab context actions still target the clicked tab and preserve pinned/unpinned grouping.
- [x] 2.5 Add widget coverage proving Focus Mode suppresses the tab strip and restores it on exit when tabs remain open.

## 3. Geometry And Validation

- [x] 3.1 Add or update constrained-geometry coverage proving the tab strip, editor viewport, and status bar remain usable with one open tab.
- [x] 3.2 Run `make check-blueprint`.
- [x] 3.3 Run focused widget tests for the tab-strip scenarios.
- [x] 3.4 Run `make test-widget-headless`.
- [x] 3.5 Run `make visual-geometry-smoke` and refresh visual proof artifacts if the one-tab chrome change requires it.
- [x] 3.6 Run `openspec validate keep-single-tab-strip-visible --strict`.
