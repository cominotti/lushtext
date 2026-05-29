# Tasks: adaptive-inline-alert-layout

## 1. Template and type registration

- [x] 1.1 Register `AdwWrapBox` (ensure its type) in the `LushtextInfoBar` class init so the template can instantiate it.
- [x] 1.2 Restructure `resources/ui/info-bar.ui` so an `AdwWrapBox` hosts `message_box` and `actions_box` as its two wrappable children, replacing the vertical `alert_box` stack.
- [x] 1.3 Keep `actions_box` a single horizontal group (retry, discard, save, dismiss last) and keep the discard/save `GtkSizeGroup`.
- [x] 1.4 Validate the template at runtime via the widget harness (`gtk4-builder-tool` links only GTK and cannot resolve `AdwWrapBox`; the harness loads the template with libadwaita initialized, which is the real validation).

## 2. Rendering and styling

- [x] 2.1 Confirm `render_notification` and button-visibility logic are unchanged (dismiss always visible; workflow buttons per payload).
- [x] 2.2 Set wrap spacing via `AdwWrapBox` `child-spacing`/`line-spacing` properties (layout, not CSS); `.editor-inline-alert` and `.inline-alert-button` CSS rules kept unchanged.
- [x] 2.3 `justify=spread` + `justify-last-line=true` with default `wrap-policy=natural` keeps actions beside the message until the message's one-line natural width no longer fits; validated by the wide/narrow widget tests.

## 3. Tests

- [x] 3.1 Wide editor: assert the message and action group occupy one horizontal row (same band).
- [x] 3.2 Narrow editor: assert the action group wraps onto its own row beneath the message.
- [x] 3.3 Both widths: assert the action buttons stay in one horizontal group (never split) and each visible button has a positive allocation.
- [x] 3.4 Assert the container is `AdwWrapBox` and the template contains no `GtkInfoBar`.

## 4. Verification and docs

- [x] 4.1 Run focused inline-alert widget tests, `make check`, and headless widget tests. (inline-alert subset green; `make check` clippy+fmt clean; full headless suite 218/218 pass with the runner's zero-unexpected-warning gate.)
- [x] 4.2 Live `make run`: confirmed by the maintainer — restored-draft and load-error alerts behave correctly at wide and narrow editor widths with no `Trying to measure GtkBox ...` / pixman warnings. (Could not be run in the headless implementation session; verified on the maintainer's desktop.)
- [x] 4.3 Updated `.agents/rules/ui.md` (Inline Alerts) and `AGENTS.md` (info_bar description) with the AdwWrapBox adaptive layout. README has no alert-layout detail to update.
