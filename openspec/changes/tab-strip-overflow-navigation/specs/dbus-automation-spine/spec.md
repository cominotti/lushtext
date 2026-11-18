## ADDED Requirements

### Requirement: Tab overflow navigation has deterministic readiness
The readiness blocker `tab-overflow` SHALL be present while a tab-strip scroll step or hold is in progress, while the strip's overflow controls have a reconcile pending, and while the All Tabs popover is still publishing or retiring rows. The aggregate predicates `visual-geometry-settled`, `accessibility-settled` and `idle` SHALL include it, a timeout SHALL name it as the bounded blocker, and a `tab-overflow` workflow event SHALL be derived from it.

#### Scenario: Waiting for a scroll step
- **WHEN** automation activates `win.scroll-tabs-forward` and then waits for `visual-geometry-settled`
- **THEN** the wait returns only after the step has landed and the scroll actions' enabled states are current

#### Scenario: Timeout names the blocker
- **WHEN** a wait times out while a hold-to-repeat is still scrolling the strip
- **THEN** the timeout reports the blocker `tab-overflow`

### Requirement: The snapshot exposes bounded tab overflow state
The snapshot object `window.tab_strip` SHALL expose `overflowing`, `scroll_back_enabled`, `scroll_forward_enabled` and `tab_count`. `surfaces.all_tabs_popover_visible` SHALL report the popover, and `active_transient_surface` SHALL report `all-tabs-popover` while it is the topmost open transient surface after the command palette, workspace search and Open popover. No field may expose document text, tab titles or paths.

#### Scenario: Snapshot of an overflowing strip
- **WHEN** a client reads the snapshot of a window whose strip overflows and is scrolled to its start
- **THEN** it reports `overflowing` true, `scroll_back_enabled` false, `scroll_forward_enabled` true, and the popover hidden

#### Scenario: Popover state
- **WHEN** a client opens the All Tabs popover with `win.set-all-tabs-visible`, sets a query with `win.set-all-tabs-query`, and closes it
- **THEN** the snapshot reports the popover visible with `active_transient_surface` `all-tabs-popover`, then hidden

### Requirement: Visual geometry exposes the tab overflow surfaces
The visual-geometry snapshot SHALL expose the named surfaces `tab-strip-overflow-controls`, the bounds of the trailing control group, and `all-tabs-popover`, the popover's bounds. A surface that is not shown SHALL be reported absent with reason `controls-hidden` or `popover-closed`.

#### Scenario: Controls absent while tabs fit
- **WHEN** a client reads visual geometry while all tabs fit and fewer than two tabs are open
- **THEN** `tab-strip-overflow-controls` is reported absent with reason `controls-hidden`

#### Scenario: Controls present on overflow
- **WHEN** the strip overflows
- **THEN** `tab-strip-overflow-controls` reports window-relative bounds at the strip's trailing end
