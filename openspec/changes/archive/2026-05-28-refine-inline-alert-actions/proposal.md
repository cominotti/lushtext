## Why

The GTK5-safe inline alert replacement keeps the recovery workflows working, but the action layout now feels more separated than the old infobar pattern. Inline alert controls should read as one action cluster, and their button surfaces should remain easy to distinguish from the warning or error background without making the alert visually heavy.

## What Changes

- Move every visible inline-alert control into one horizontal trailing action group.
- Place the dismiss control at the end of the same group as retry, discard, save, normalize, or other alert actions.
- Keep the message text and action group responsive: message text may wrap above the controls, but visible controls must remain adjacent horizontally.
- Slightly increase resting and hover contrast for inline-alert buttons against warning and error surfaces.
- Preserve the current editor-scoped notification model, action routing, accessibility semantics, and GTK5-supported widget set.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `editor-inline-alerts`: refine action placement and button contrast for the GTK5-safe editor inline alert surface introduced by `replace-gtk-infobar`.

## Impact

- Affected code: `resources/ui/info-bar.ui`, `resources/style/style.css`, `crates/lushtext-core/src/ui/info_bar`, and inline-alert widget tests.
- User-visible impact: warning and error alerts keep the same messages and workflows, but controls appear as one grouped row such as `Discard...`, `Save...`, and dismiss.
- Dependency impact: no new crate or runtime dependency is expected.
- OpenSpec sequencing: this follow-up assumes the completed `replace-gtk-infobar` change remains the base for `editor-inline-alerts`.
