## 1. Template And State Wiring

- [x] 1.1 Expose the Data page Actions group as a template child and add a compact verified-current indicator beside the existing Data Format refresh button.
- [x] 1.2 Regenerate the GtkBuilder UI and template contract after the Blueprint edit.
- [x] 1.3 Update `LushtextPreferences` template children, default state, accessibility metadata, and status helpers for the Actions group and verified-current indicator.

## 2. Scan Presentation Behavior

- [x] 2.1 Centralize Data page scan presentation states so verification-in-flight disables refresh/convert controls, hides the verified-current indicator, and shows the verifying subtitle immediately.
- [x] 2.2 Defer completed scan rendering until both the background plan result and the short minimum dwell interval are satisfied.
- [x] 2.3 Update `render_data_plan()` so the Actions group is visible only when a real action row is visible and the verified-current indicator is visible only for current/no-failure scans.
- [x] 2.4 Preserve existing Convert, Retry, future-version, dense-detail, and failure-state behavior while routing successful conversion back through the refreshed scan presentation.

## 3. Test Coverage

- [x] 3.1 Extend Preferences widget tests to assert current data hides the Actions group and shows the verified-current affordance.
- [x] 3.2 Add or extend a manual refresh widget test that observes verification-in-flight state, disabled refresh behavior, and delayed return to current status for fast no-op scans.
- [x] 3.3 Extend existing future-version and failed-convert tests to assert the verified-current affordance is hidden outside current-state scans.
- [x] 3.4 Keep dense details coverage proving long or many items stay inside the bounded details scroller without exposing empty actions.

## 4. Validation

- [x] 4.1 Run `make blueprint-generate`.
- [x] 4.2 Run `make check-blueprint`.
- [x] 4.3 Run the focused Preferences widget tests through the widget harness.
- [x] 4.4 Run `make test-widget-headless`.
- [x] 4.5 Run `make visual-geometry-smoke` if the final template change materially shifts the Preferences dialog geometry.
- [x] 4.6 Run `git diff --check`.
