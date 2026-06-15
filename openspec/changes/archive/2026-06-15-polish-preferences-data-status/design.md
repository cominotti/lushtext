## Context

`Preferences > Data` is the maintenance surface for the existing format-upgrade workflow. Today the page scans app-owned metadata on dialog construction and on manual refresh, renders a `Data Format` status row, conditionally shows the `Update Data` row, and lists bounded details. The current implementation already disables the refresh and convert buttons during scan/apply work, but fast current-state scans can complete before the user perceives that anything happened. The template also always shows the `Actions` group, so the normal current state can display an empty section.

This change is UI-only. The `services::format_upgrade` scan, plan, and apply contracts stay unchanged and continue to return plain Rust values without GTK state.

## Goals / Non-Goals

**Goals:**

- Make the current/no-op Data page look intentional by hiding the `Actions` group when no action rows are visible.
- Show a persistent verified-current affordance after a completed current scan.
- Keep manual refresh visibly in progress for a short minimum dwell interval, even when the background scan finishes immediately.
- Preserve the existing Convert/Retry and future-version behavior.
- Extend widget coverage for empty/current, in-flight refresh, current-success, non-current, dense details, and failure states.

**Non-Goals:**

- Change metadata scan, planning, conversion, backup, or recovery semantics.
- Add new actions beyond the existing Convert/Retry path.
- Replace the existing Details list or change startup compatibility dialogs.
- Add a global notification for routine current-state rescans.

## Decisions

1. **Hide the Actions group, not just the Convert row, when no actions are available.**

   The template should expose the Actions `AdwPreferencesGroup` as a template child, and `render_data_plan()` should set the group visible only when at least one real action row is visible. This keeps the normal current state visually quiet and avoids a fake "No actions available" row that would duplicate the Details section.

   Alternatives considered: keep the group with a disabled explanatory row, or move Convert into the Format row. The explanatory row still reads like an actionable section, and inline Convert would crowd the status row with mixed controls.

2. **Represent verified-current as a suffix affordance beside Refresh.**

   The Data Format row should include a small success indicator before the refresh button, visible only when the last completed scan has no action and no failure. The subtitle remains the accessible text signal (`Data format is current`), so the icon reinforces rather than replaces the message.

   Alternatives considered: changing the refresh icon to a check mark, or adding a separate Details-only success row. Replacing refresh would hide the available command, while Details-only feedback would not answer the user's click near the control they activated.

3. **Hold completed scan rendering until a short minimum dwell has elapsed.**

   `run_data_scan()` should enter a verifying presentation immediately, record the start time, run the existing blocking scan off the GTK thread, and render the completed plan only after both the worker result and the minimum dwell interval are satisfied. The refresh and convert controls remain disabled until the final plan is rendered. Because only one data operation may be in flight, the implementation can keep the pending plan local to that operation and use a weak dialog reference for delayed completion.

   The dwell should be short enough not to make slow scans feel slower; a one-second interval is enough for users to notice the state change while keeping the page responsive.

   Alternatives considered: no dwell and only a status-bar message, or a toast/global notification. No dwell keeps the current invisible-click problem; a global notification is too loud for a routine Preferences self-check.

4. **Keep non-current and failure states free of success styling.**

   The verified-current affordance must hide for upgradeable, future-version, unsupported/recovery, conversion failure, conversion in-flight, and verification in-flight states. Convert/Retry remains the only suggested action when applicable.

## Risks / Trade-offs

- **Risk: Artificial delay makes refresh feel sluggish.** -> Mitigation: apply the dwell only to fast scans; scans that exceed the dwell complete as soon as the worker returns.
- **Risk: Success icon becomes the only accessible signal.** -> Mitigation: keep the subtitle as the primary textual status and update the indicator's accessible label or tooltip.
- **Risk: Actions group visibility drifts from row visibility.** -> Mitigation: centralize the predicate in `render_data_plan()` and cover it with widget tests for current, convert, and failure states.
- **Risk: Delayed callbacks outlive the dialog.** -> Mitigation: use weak dialog references and avoid retaining GTK widgets from background or timeout closures.
