## Context

`LushtextInfoBar` is an editor-page driving adapter: it translates `InlineActionNotification` values from the notification bus into GTK widgets above the editor. The current implementation uses two `GtkInfoBar` template children, one for error/access recovery and one for warning/draft or disk-change recovery. GTK 4.10 deprecated `GtkInfoBar`, and GTK's GTK5 migration guide says it is going away.

GTK documents no direct replacement for `GtkInfoBar`; its suggested replacement shape is a `GtkRevealer` containing ordinary labels and buttons. Libadwaita provides `AdwBanner`, but it exposes a title plus one optional action button, while LushText needs a title, body text, close affordance, and up to two actions.

Architecturally, this change should stay inside `ui/info_bar` and adjacent CSS/tests. Hexagonal Architecture means framework-facing GTK code remains in `ui/`, services remain GTK-free, and domain/model types remain pure. Command-Query Separation means the alert widget's connector methods remain command-shaped signal registration, and `render_notification` remains a UI update command rather than returning presentation state for callers to inspect.

## Goals / Non-Goals

**Goals:**
- Remove all `GtkInfoBar` usage from `LushtextInfoBar` and its template.
- Preserve the current editor inline alert behavior: persistent warning/error messages, wrapping title/body text, optional primary and secondary actions, and explicit dismissal.
- Keep `InlineActionNotification`, `NotificationBus`, and window/editor wiring stable.
- Keep the replacement GTK5-safe by using supported GTK/Libadwaita primitives.
- Preserve narrow-window usability for action buttons and message text.
- Replace the current `#[expect(deprecated)]` allowances with normal GTK4 code.

**Non-Goals:**
- Redesign the notification bus or introduce a new service abstraction.
- Replace editor inline alerts with transient toasts or modal dialogs.
- Convert unrelated status-bar, search-panel, or dialog notifications.
- Change user-facing alert wording or recovery workflows.
- Add a new dependency.

## Decisions

### Use a custom `GtkRevealer` alert row, not `AdwBanner`

The replacement will keep `LushtextInfoBar` as a custom widget backed by one `GtkRevealer`. The revealer child will be a `GtkBox` styled as an inline alert with title, body, action area, and close button.

`AdwBanner` was considered because it is the obvious libadwaita contextual bar. It is not sufficient here because it supports one title string and one optional button, while LushText warning alerts can expose both `_Discard...` and `_Save...` actions and also need body text. Using `AdwBanner` would either drop functionality or require stacking extra widgets around it, which would be less direct than GTK's documented `GtkRevealer` approach.

### Keep the public widget API stable

`LushtextInfoBar` will keep its existing methods:
- `render_notification`
- `connect_retry`
- `connect_save`
- `connect_discard`
- `connect_dismissed`

Callers in `window/documents.rs`, `editor_page`, and notification rendering should not need to learn a new port. The widget remains the driving adapter boundary, and signal closures inside `imp.rs` should stay thin dispatchers to stored callbacks.

### Collapse the template to one alert row with three semantic action buttons

The template should no longer maintain separate error and warning bars. A single row can render both styles by changing CSS classes and label content. It should keep three template buttons:
- `retry_button` for error primary actions
- `discard_button` for warning primary actions
- `save_button` for warning secondary actions

Keeping semantic buttons avoids a hidden state machine where one generic primary button changes meaning depending on the last rendered notification. `render_notification` can show exactly the buttons relevant to the current notification style and hide the rest.

### Style with Adwaita CSS variables and global app CSS

The alert row should use app CSS classes such as `.editor-inline-alert`, `.warning`, and `.error`. Warning and error variants should use libadwaita semantic variables like `--warning-bg-color`, `--warning-fg-color`, `--error-bg-color`, and `--error-fg-color`, with a border/shade that remains readable in light and dark styles.

This follows GTK5 migration guidance away from deprecated widget-specific styling and local CSS providers. LushText already loads global app CSS, so the replacement should extend `resources/style/style.css` rather than attach a widget-local provider.

### Treat hiding as both reveal state and widget visibility

`GtkRevealer` provides the slide transition, but hidden revealer children can still exist in the widget tree. The implementation should set the alert visible before revealing it, and when dismissing or clearing, set `reveal-child=false` and then make the alert surface not visible after the reveal transition completes. If animation-end handling is too heavy for the first implementation, the safe fallback is to hide immediately after clearing because the current `GtkInfoBar` behavior is persistent, not animation-critical.

This prevents hidden alerts from leaving stale keyboard/a11y targets or unintended layout residue.

### Keep notification semantics in services/model unchanged

The existing `InlineActionNotification` value object already contains the alert style, title, body, and optional action labels. No domain or service type needs to know whether the UI uses `GtkInfoBar`, `GtkRevealer`, or boxes. The only implementation-specific mapping belongs in `ui/info_bar`.

## Risks / Trade-offs

- [Risk] The custom row may not exactly match Adwaita's old `GtkInfoBar` spacing or colors. -> Mitigation: use libadwaita semantic variables, keep the row full-width above the editor, and verify light/dark appearance with widget screenshots or manual GTK runs.
- [Risk] The single-row template could accidentally route a warning primary action to retry or an error primary action to discard. -> Mitigation: keep separate semantic buttons and tests that click retry, discard/normalize, and save/save-as workflows.
- [Risk] Narrow layouts can hide or compress actions. -> Mitigation: preserve wrapping action labels, the warning button size group, and tests that assert visible action buttons in narrow editor layouts.
- [Risk] Hidden revealer content can remain focusable or visible to accessibility tooling. -> Mitigation: pair reveal state with widget visibility or transition-completion cleanup.
- [Risk] Tests currently assert `GtkInfoBar`-specific properties such as `revealed`. -> Mitigation: update tests to assert public behavior and supported widget structure instead of deprecated widget internals.

## Migration Plan

1. Replace `resources/ui/info-bar.ui` internals with a `GtkRevealer` containing one styled alert row.
2. Update `crates/lushtext-core/src/ui/info_bar/imp.rs` template children, signal wiring, button wrapping, and dismiss handling.
3. Update `crates/lushtext-core/src/ui/info_bar/mod.rs` rendering logic to drive one row and style class set.
4. Add CSS for the warning/error row using Adwaita semantic variables.
5. Update tests to assert no `GtkInfoBar` remains and all existing alert workflows still render and dispatch.
6. Run focused widget tests and the normal project verification gate.

Rollback is straightforward: the change is local to UI resources, CSS, and widget code. If a visual or behavioral regression appears before release, restore the previous `LushtextInfoBar` implementation while keeping the OpenSpec change open for a corrected replacement.

## Open Questions

None.
