## Context

`LushtextInfoBar` now renders editor-scoped notifications with GTK5-supported widgets: a `GtkRevealer`, a styled alert box, labels, action buttons, and a dismiss button. The replacement preserves the workflows, but the current layout separates the dismiss button from the recovery actions. For alerts such as "Draft Changes Restored", users naturally read `Discard...`, `Save...`, and dismiss as one set of choices.

The current CSS also relies mostly on Adwaita semantic warning/error colors. That keeps the alert subtle, but the buttons can blend into the warning surface and their hover state is quieter than desired.

## Goals / Non-Goals

**Goals:**

- Put every visible inline-alert control in one horizontal action group.
- Keep dismiss at the end of that action group, after any retry, discard, save, normalize, or other workflow buttons.
- Preserve the safer two-band layout: message text can occupy the first row, while controls remain grouped on the trailing row.
- Increase button/background and hover contrast slightly without making alerts look heavy or custom-themed.
- Keep existing notification payloads, callbacks, accessibility role/announcement behavior, and GTK5-safe widget choices.
- Add tests that assert grouping, order, allocation, and contrast-related CSS hooks.

**Non-Goals:**

- Do not change the notification bus, `InlineActionNotification`, or editor/window callback routing.
- Do not change alert wording or workflow semantics.
- Do not force message text and all controls into a single horizontal row.
- Do not introduce a new dependency, custom drawing, or per-widget CSS provider.
- Do not redesign status-bar notifications, search-panel notifications, dialogs, or toasts.

## Decisions

### Keep the message and controls as separate rows

The alert should keep a message area and a trailing action area. The message row contains title/body text, and the action row contains all controls horizontally:

```text
Draft Changes Restored
Unsaved changes from a previous session have been restored.

                                      [ Discard... ] [ Save... ] [ X ]
```

This preserves narrow-width behavior because the text can wrap above the buttons instead of competing with them in the same horizontal allocation. The rejected alternative is a single row containing message text and all controls; the previous replacement already showed that a tightly constrained horizontal row can starve buttons or create brittle allocation behavior.

### Move dismiss into the action group

The dismiss button should be a child of the same horizontal `actions_box` as retry/discard/save. `render_notification` should treat the action group as visible whenever an alert is visible because dismiss is always available. Recovery buttons remain conditionally visible based on the payload, but dismiss remains the final control.

This keeps `Dismiss` visually tied to the same decision cluster while preserving the existing `connect_dismissed` callback path.

### Use local alert-button CSS classes instead of broad button overrides

Inline alert controls should receive a specific CSS class, such as `inline-alert-button`, so contrast tweaks apply only to alert buttons. Styling should stay small and semantic:

- Resting state: faint neutral surface and border over warning/error backgrounds.
- Hover state: slightly stronger surface/border.
- Active state: subtle pressed shade.
- Focus state: keep GTK/Adwaita focus indication visible.

The rejected alternative is changing all `.editor-inline-alert button` rules broadly with strong colors. That risks making the close button too loud and drifting away from Adwaita.

### Keep accessibility behavior intact

The alert surface should keep its accessible alert role, label/description updates, and announcement call. Moving dismiss in the template should not change the semantic title/body announcement or the explicit accessible label on the icon-only dismiss control.

## Risks / Trade-offs

- [Risk] A horizontal action group can still become tight at very narrow editor widths. -> Mitigation: keep controls below the message, preserve action-label wrapping where useful, and assert positive allocations in narrow-layout tests.
- [Risk] Stronger contrast can make the alert look too heavy against Adwaita warning/error colors. -> Mitigation: use subtle alpha-based surfaces and verify both warning and error variants.
- [Risk] Moving dismiss could accidentally make no-action alerts hide the action row. -> Mitigation: tests should cover informational alerts where dismiss is the only visible control.
- [Risk] CSS selectors could affect unrelated buttons. -> Mitigation: use a dedicated inline-alert button class and keep selectors scoped under `.editor-inline-alert`.

## Migration Plan

1. Update the inline-alert template so `dismiss_button` lives inside `actions_box` after the workflow buttons.
2. Ensure `actions_box` remains visible for every visible alert because dismiss is always present.
3. Add scoped CSS classes/rules for inline alert button resting, hover, active, and focus-visible states.
4. Update widget tests for button order, same-parent grouping, no-action dismiss visibility, positive allocation, and supported-widget structure.
5. Run focused inline-alert tests, `gtk4-builder-tool validate`, `make check`, headless widget tests, and a live `make run` warning capture.

Rollback is local to the inline alert template, CSS, and widget tests. If the grouped controls regress narrow layouts, restore the separate dismiss placement while keeping the contrast CSS isolated for reassessment.

## Open Questions

None.
