## 1. Regroup Inline Alert Controls

- [x] 1.1 Move `dismiss_button` into the same horizontal `actions_box` as retry, discard, and save controls.
- [x] 1.2 Keep dismiss ordered last after all workflow actions for warning and error alerts.
- [x] 1.3 Update `render_notification` so the action group stays visible for every visible alert because dismiss is always present.
- [x] 1.4 Preserve existing retry, discard, save, and dismiss callback routing without changing `InlineActionNotification`.

## 2. Refine Button Styling

- [x] 2.1 Add a scoped CSS class to inline-alert buttons so contrast changes do not affect unrelated buttons.
- [x] 2.2 Add subtle resting surface/border styling that distinguishes alert buttons from warning and error backgrounds.
- [x] 2.3 Add slightly stronger hover and active styling while preserving Adwaita-compatible warning/error alert surfaces.
- [x] 2.4 Confirm the icon-only dismiss button keeps its accessible label and remains visually balanced with text buttons.

## 3. Update Tests

- [x] 3.1 Add or update widget tests proving warning alerts group `Discard...`, `Save...`, and dismiss in one horizontal action group.
- [x] 3.2 Add or update widget tests proving error alerts group retry and dismiss in one horizontal action group.
- [x] 3.3 Add coverage for informational warnings where dismiss is the only visible control in the action group.
- [x] 3.4 Preserve narrow-width tests proving each visible alert control receives a positive allocation.
- [x] 3.5 Add CSS or widget-structure coverage proving contrast styling is scoped to inline-alert buttons.

## 4. Documentation and Verification

- [x] 4.1 Update `AGENTS.md` and `.agents/rules/ui.md` if their inline-alert guidance needs the grouped-control layout.
- [x] 4.2 Run `openspec validate refine-inline-alert-actions --strict`.
- [x] 4.3 Run focused inline-alert widget tests.
- [x] 4.4 Run `gtk4-builder-tool validate resources/ui/info-bar.ui`.
- [x] 4.5 Run `make check`.
- [x] 4.6 Run `make test-widget-headless` or `make test` before final acceptance.
- [x] 4.7 Exercise an alert workflow with `make run` and confirm stderr is free of GTK, GLib, and pixman warnings.
