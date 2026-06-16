## 1. Source Parity Baseline

- [x] 1.1 Record the GNOME Text Editor 50.1 source baseline used for parity, including `src/style.css`, `src/editor-open-popover.ui`, `src/editor-sidebar-row.ui`, and the Open popover controller behavior.
- [x] 1.2 Map the current LushText Open popover divergences: `GtkSingleSelection`, row height request, `GtkLabel` text cells, missing marker/spacer column, close-button placement, and scroller max-height approximation.
- [x] 1.3 Confirm the existing Open popover CSS constants against GNOME Text Editor 50.1 and list any remaining CSS differences before editing.

## 2. GNOME-Shaped Row Implementation

- [x] 2.1 Replace the recent-list `GtkSingleSelection` wiring with a `GtkNoSelection`-equivalent model and update activation to resolve rows by visible position.
- [x] 2.2 Introduce a source-compatible recent row widget or builder layout with a grid, leading homogeneous marker/spacer stack, title/subtitle/age text cells, and trailing remove button spanning both text rows.
- [x] 2.3 Use source-compatible text overflow behavior for row title, subtitle, and optional age, preferring `GtkInscription` where the GTK Rust bindings support it cleanly.
- [x] 2.4 Update remove-button wiring so the trailing `window-close-symbolic` `flat`/`circular` button removes only that recent entry, never activates the row, and keeps the popover open.
- [x] 2.5 Match GNOME Text Editor 50.1 scroller sizing and popover constants, including 250px list content width, 600px max content height, natural-height propagation, and no horizontal scrollbar.
- [x] 2.6 Preserve existing LushText recent-history behavior, search ranking, file-chooser routing, open-tab exclusion, duplicate-safe document activation, and empty/no-results stack switching.
- [x] 2.7 Regenerate any UI resources produced from `.blp` files and keep generated resources in sync with source templates.

## 3. Model And Lifecycle Regression Tests

- [x] 3.1 Add or extend pure recent-document service tests for ordering, deduplication, missing-path pruning, unsupported URI rejection, persistence load/save, corrupt-file recovery, duplicate path spelling, and reopen-after-close behavior.
- [x] 3.2 Add lifecycle tests proving only live file-backed tabs are excluded from the Open popover and closed same-session documents reappear without restarting.
- [x] 3.3 Add tests for recent rows opened through file chooser, sidebar, command palette, desktop/CLI activation where covered by current harnesses, and recent-row activation.
- [x] 3.4 Add tests for Save As, failed load, cancelled load, rename/delete workflows, session restore, and close-tab-for-path interactions where those workflows can affect recent visibility.
- [x] 3.5 Add tests for removing recents while the popover is visible, repeated removals, and transition from populated rows to the empty state.

## 4. GTK Widget, Keyboard, And Accessibility Regression Tests

- [x] 4.1 Add widget structure tests proving the recent list uses no-selection interaction and each populated row exposes the GNOME-shaped child layout.
- [x] 4.2 Add widget tests for the row marker/spacer column, title/subtitle/age text widgets, text overflow properties, caption/dim-label classes, row CSS classes, and remove-button column/row-span placement.
- [x] 4.3 Add geometry tests for row margins, first-row top offset, grid spacing, close-button min size, close-button padding, 250px list content width, 600px max content height, and absence of horizontal scrolling.
- [x] 4.4 Add keyboard tests for `Ctrl+K`, search focus, stale search clearing, top scroll reset, `Down` from search, `Up` from the first row, `Enter` first-match activation, activation after filtering, and `Escape` dismissal.
- [x] 4.5 Add pointer/event tests proving single-click row activation works and remove-button clicks do not activate the row or close the popover.
- [x] 4.6 Add accessibility tests for the header Open control, search entry, chooser button, list region, row labels, remove controls, empty state, focus order, and dismissibility.

## 5. Visual Proof And State-Matrix Smoke Coverage

- [x] 5.1 Add visual proof scenarios for no recents, one row, representative rows, exactly ten rows, more than ten rows, awkward labels, all recents open, and all recents closed.
- [x] 5.2 Add visual proof for constrained header width, constrained popover geometry, 720p height, item-region-only scrolling, and no unintended horizontal scrollbar.
- [x] 5.3 Capture light and dark style contexts and assert no selected/accent row state is visible in populated Open popover scenarios.
- [x] 5.4 Assert close-button alignment and row text readability in visual proof artifacts so row parity regressions are visible in screenshots and geometry reports.

## 6. Verification

- [x] 6.1 Run `openspec validate match-gnome-text-editor-open-popover-rows --strict`.
- [x] 6.2 Run `openspec validate --specs --strict` and `git diff --check`.
- [x] 6.3 Run focused recent-document model/service tests.
- [x] 6.4 Run focused Open popover GTK widget tests headlessly.
- [x] 6.5 Run `make test-widget-headless`.
- [x] 6.6 Run the focused visual proof or smoke command that covers the Open popover row scenarios.
- [x] 6.7 Run `make check-automation-docs` if action catalog, automation snapshots, accessible anchors, readiness fields, or automation docs change.
- [x] 6.8 Run `make pre-commit` before considering the implementation complete.
