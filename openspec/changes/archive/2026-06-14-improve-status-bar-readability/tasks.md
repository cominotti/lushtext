## 1. Status Bar Layout Tuning

- [x] 1.1 Inspect the current status-bar allocations, margins, CSS classes, and relevant widget/window tests before editing.
- [x] 1.2 Tune `resources/ui/status-bar.blp` and `resources/style/style.css` so the bar gains modest readable vertical comfort while remaining a one-row, lower-prominence status strip.
- [x] 1.3 Preserve the existing workspace toggle, non-flashing left gap, message area, and metadata control boundaries while adjusting spacing.
- [x] 1.4 Regenerate `resources/ui/status-bar.ui` from Blueprint and confirm generated template drift is intentional.

## 2. Behavioral And Visual Coverage

- [x] 2.1 Add or update status-bar widget tests for readable centered message/metadata layout, hidden-metadata empty state, long-message ellipsizing, and unchanged flash boundaries.
- [x] 2.2 Add or update window geometry coverage for short/tiny windows so the status bar remains visible without consuming unusable editor space.
- [x] 2.3 Verify light, dark, and high-contrast visual states for no-document, populated metadata, long message, and severity-flash cases.
- [x] 2.4 Confirm document metadata controls remain reachable and no unintended horizontal scrollbar or wrapped second row appears at narrow widths.

## 3. Validation

- [x] 3.1 Run Blueprint validation/regeneration checks relevant to `status-bar.blp` and generated UI.
- [x] 3.2 Run targeted widget tests for `status_bar` and affected window status-bar geometry/notification cases.
- [x] 3.3 Run the broad widget gate (`make test-widget-headless`) or the repo-accepted equivalent for UI chrome changes.
- [x] 3.4 Run `openspec validate improve-status-bar-readability --strict` and fix any proposal/spec/task issues.
