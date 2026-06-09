## 1. Tighten Inline Alert Rhythm

- [x] 1.1 Update `.editor-inline-alert` in `resources/style/style.css` to use balanced compact vertical padding, with equal top and bottom padding no greater than 6 CSS pixels.
- [x] 1.2 Preserve the existing horizontal inset, bottom border, warning/error color classes, and scoped `.inline-alert-button` contrast selectors.
- [x] 1.3 Confirm `resources/ui/info-bar.ui` still keeps the message and action group inside the existing `AdwWrapBox` without changing action order, grouping, or dismiss placement.

## 2. Update Focused Coverage

- [x] 2.1 Add or update inline-alert widget/CSS coverage proving the alert surface uses balanced compact top and bottom padding.
- [x] 2.2 Preserve coverage for restored-draft warnings with discard, save, and dismiss controls in the wide one-line layout.
- [x] 2.3 Preserve coverage for retryable error alerts and informational warnings where dismiss is the only visible action.
- [x] 2.4 Preserve constrained-width coverage proving wrapped alert text remains readable and every visible action control receives positive allocation.

## 3. Verification

- [x] 3.1 Run `gtk4-builder-tool validate resources/ui/info-bar.ui` and record the expected standalone `AdwWrapBox` limitation; template validation is covered by the focused widget harness that initializes Libadwaita.
- [x] 3.2 Run the focused inline-alert widget tests through `scripts/run-widget-tests.sh --headless -- --exact <test-name>` or the nearest available focused widget-test filter.
- [x] 3.3 Run `openspec validate tighten-inline-alert-height --strict`.
- [x] 3.4 Run `make check`.
- [x] 3.5 Inspect a restored-document warning with workspace sidebar and document properties visible, using `make run` or an existing visual capture path, and confirm the alert is shorter, balanced, readable, and free of GTK/GLib warnings.
