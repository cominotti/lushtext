## 1. Tooltip Binding

- [x] 1.1 Locate the Open popover recent-row factory bind path and derive a full path tooltip string from the currently bound `OpenPopoverItem::path()`.
- [x] 1.2 Assign the full path tooltip to the row's non-action hover surface during every bind without changing row layout, sizing, ellipsizing, or scroller behavior.
- [x] 1.3 Preserve the trailing remove button's existing `Remove` tooltip, accessibility label, and click behavior.
- [x] 1.4 Keep the bind path free of filesystem probes, canonicalization, persistence reads, or synchronous path existence checks.

## 2. Regression Tests

- [x] 2.1 Add a widget test proving a representative recent row exposes a tooltip equal to its full absolute activation path.
- [x] 2.2 Add a widget test proving a long/deep path with spaces, symbols, or mixed-width text remains complete in the tooltip even while visible row text ellipsizes.
- [x] 2.3 Add a widget test proving the remove button tooltip remains `Remove` and row removal still does not activate the document or close the popover.
- [x] 2.4 Add a dense-list or filtering widget test proving recycled row widgets refresh to the newly bound row's path tooltip and do not leak stale tooltip text.
- [x] 2.5 Add an empty/no-match state assertion proving no fake recent rows or path tooltips appear while search and file chooser controls remain reachable.

## 3. Verification

- [x] 3.1 Run `openspec validate show-open-popover-path-tooltips --strict`.
- [x] 3.2 Run the focused Open popover widget tests for `crates/lushtext/tests/widget/open_popover.rs`.
- [x] 3.3 Run the appropriate broader GTK widget gate if focused tests expose lifecycle, geometry, or row-recycling instability.
