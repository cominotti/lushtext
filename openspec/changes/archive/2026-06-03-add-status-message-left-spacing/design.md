## Context

`flash-status-message-area` introduced a full-width `message_area_box` around `message_label` so notification flashes cover the whole feedback lane. That wrapper begins immediately after the workspace-sidebar toggle, which makes the pulse background feel visually attached to the left icon even though the icon itself is not flashed.

The status bar has three horizontal regions: workspace toggle, message area, and document metadata. The follow-up should preserve that structure and only add a small left-side breathing room between the first two regions.

## Goals / Non-Goals

**Goals:**

- Add a small visual gap between the workspace-sidebar toggle/icon and the status-bar message area.
- Keep the flash background scoped to `message_area_box` and the full remaining message lane.
- Keep the gap outside the flashing background so the workspace toggle remains visually separate.
- Preserve bottom-bar height, metadata placement, message text size, and rapid-repeat pulse behavior.

**Non-Goals:**

- Changing notification text, colors, pulse timing, or lifecycle.
- Reworking status-bar layout beyond the left message-area inset.
- Adding a new notification surface or changing editor inline alerts.

## Decisions

### Put spacing on the message-area wrapper

Apply a small start margin to `message_area_box` instead of adding padding inside the wrapper. A wrapper margin creates a stable non-flashing gap between the workspace toggle and the pulse background; internal padding would still color the gap during a flash and would not solve the visual crowding as cleanly.

Alternative considered: increase the label's existing left margin. That would move the text, but the flash background would still begin immediately beside the workspace icon.

Alternative considered: add global spacing to the status-bar root box. That could also affect the metadata controls or other children and would make the layout contract less explicit.

### Keep the inset small and testable

Use a small fixed value in the "few pixels" range, expected to land around 4-8 px. This is enough to visually separate the workspace toggle from the message lane without making the bottom bar look sparse or stealing meaningful space from status text.

Alternative considered: use theme spacing variables only. The status-bar template already uses fixed compact margins for adjacent controls, so a small fixed start margin is more predictable and easier to assert in widget tests.

## Risks / Trade-offs

- Gap accidentally flashes if implemented as internal padding -> Use `message_area_box.margin-start` or an equivalent external spacing mechanism.
- Message text loses too much horizontal room -> Keep the inset small and leave metadata layout unchanged.
- Tests become too pixel-perfect -> Assert a bounded small start margin and the continued parentage/class contract rather than screenshot pixels.
