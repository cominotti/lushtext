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
The snapshot object `window.tab_strip` SHALL expose `overflowing`, `scroll_back_enabled`, `scroll_forward_enabled`, `selected_tab_in_view` and `tab_count`. `surfaces.all_tabs_popover_visible` SHALL report the popover, and `active_transient_surface` SHALL report `all-tabs-popover` while it is the topmost open transient surface after the command palette, workspace search and Open popover. No field may expose document text, tab titles or paths.

#### Scenario: Snapshot of an overflowing strip
- **WHEN** a client reads the snapshot of a window whose strip overflows and is scrolled to its start
- **THEN** it reports `overflowing` true, `scroll_back_enabled` false, `scroll_forward_enabled` true, and the popover hidden

#### Scenario: Popover state
- **WHEN** a client opens the All Tabs popover with `win.set-all-tabs-visible`, sets a query with `win.set-all-tabs-query`, and closes it
- **THEN** the snapshot reports the popover visible with `active_transient_surface` `all-tabs-popover`, then hidden

### Requirement: The snapshot reports whether the selected tab is in view
`window.tab_strip.selected_tab_in_view` SHALL report whether the strip shows the whole selected tab. It SHALL be null while the strip is hidden, with no selected tab or a pinned one, and while that tab has no bounds.

#### Scenario: Selected tab after a restore
- **WHEN** a client waits for `visual-geometry-settled` after a session restore whose tabs overflow the strip, with the first, a middle or the last tab selected, and reads the snapshot
- **THEN** `selected_tab_in_view` is true

#### Scenario: Strip scrolled away from the selected tab
- **WHEN** the strip is scrolled until the selected tab is out of view
- **THEN** `selected_tab_in_view` is false

#### Scenario: Strip hidden
- **WHEN** a client reads the snapshot while Focus Mode hides the strip
- **THEN** `selected_tab_in_view` is null

### Requirement: Visual geometry exposes the tab strip's control surfaces
Beside the existing `tab-strip` surface (the whole tab bar), the visual-geometry snapshot SHALL expose `tab-strip-overflow-controls` (the trailing control group), `tab-strip-scroll-back`, `tab-strip-scroll-forward` and `tab-strip-all-tabs`, each button with a pixel anchor named after it with the suffix `-ink`. A control that is not shown SHALL be reported absent with reason `controls-hidden`.

#### Scenario: Controls absent while tabs fit
- **WHEN** a client reads visual geometry while all tabs fit and fewer than two tabs are open
- **THEN** `tab-strip-overflow-controls`, both scroll buttons and `tab-strip-all-tabs` are reported absent with reason `controls-hidden`

#### Scenario: Controls present on overflow
- **WHEN** the strip overflows
- **THEN** `tab-strip-overflow-controls`, `tab-strip-scroll-back`, `tab-strip-scroll-forward` and `tab-strip-all-tabs` report window-relative bounds at the strip's trailing end, inside `tab-strip`
- **AND** the pixel anchors `tab-strip-scroll-back-ink`, `tab-strip-scroll-forward-ink` and `tab-strip-all-tabs-ink` find drawn pixels

### Requirement: Visual geometry exposes the All Tabs popover surfaces
The visual-geometry snapshot SHALL expose `all-tabs-popover`, `all-tabs-popover-search` and `all-tabs-popover-list` (the popover, its search entry and its row list, with the pixel anchor `all-tabs-popover-list-ink`), reported absent with reason `popover-closed` while the popover is closed. No surface may carry a file name.

#### Scenario: Popover open
- **WHEN** the All Tabs popover is open
- **THEN** `all-tabs-popover`, `all-tabs-popover-search` and `all-tabs-popover-list` report bounds inside the window

#### Scenario: Popover closed
- **WHEN** the All Tabs popover closes
- **THEN** all three popover surfaces are absent with reason `popover-closed`
