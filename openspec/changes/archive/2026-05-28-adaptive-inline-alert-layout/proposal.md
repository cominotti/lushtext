# Proposal: adaptive-inline-alert-layout

## Why

The editor inline alert (`LushtextInfoBar`) always renders as two stacked bands — message on top, action cluster on its own trailing row — even when the editor column is wide enough to show everything on one line. That always-stacked layout is the safe choice for narrow columns, but it spends a row of vertical height above the editor unconditionally and reads as heavier than GNOME's inline-banner idiom when there is room to spare.

## What Changes

- Introduce a width-adaptive layout: the alert message and its trailing action cluster share one horizontal line when the editor column is wide enough, and the action cluster wraps onto its own row beneath the message only when the column is too narrow.
- Wrap the existing `message_box` and `actions_box` in a libadwaita `AdwWrapBox` so the toolkit drives the horizontal-until-cramped behavior instead of a fixed vertical stack.
- Keep the action cluster atomic: `actions_box` (retry / discard / save / dismiss) wraps as a single unit and its buttons never split across rows.
- Preserve everything else: the `GtkRevealer` host, warning/error styling, the discard/save `GtkSizeGroup`, the `.inline-alert-button` contrast classes, dismiss ordered last, accessibility role and announcements, and the `render_notification` / `connect_*` API.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `editor-inline-alerts`: add a width-adaptive presentation of the message and action group, and strengthen the guarantee that the action group wraps as one atomic unit. Follow-up to `replace-gtk-infobar` and `refine-inline-alert-actions` (both archived 2026-05-28).

## Impact

- Affected code: `resources/ui/info-bar.ui`, `crates/lushtext-core/src/ui/info_bar` (type registration for `AdwWrapBox`), possibly `resources/style/style.css` (wrap spacing), and inline-alert widget tests.
- User-visible impact: on wide editors the restored-draft / load-error banners read as a single compact line; on narrow editors they keep today's stacked behavior. Wording and workflows are unchanged.
- Dependency impact: none. `AdwWrapBox` (libadwaita 1.7) is already available under the workspace `v1_9` feature gate; no crate, feature, or `cargo hakari` change.
- OpenSpec sequencing: assumes the archived `replace-gtk-infobar` and `refine-inline-alert-actions` changes remain the base for `editor-inline-alerts`.
