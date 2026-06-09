## Why

Restored-document inline alerts are visually solid, but the warning surface is a little taller than the surrounding editor/sidebar/properties chrome expects. Tightening the alert height slightly will make the alert bottom align more cleanly with adjacent panel boundaries while keeping the content balanced and readable.

## What Changes

- Reduce the editor inline alert's vertical rhythm slightly while preserving balanced top and bottom padding.
- Keep warning and error alert content, action grouping, dismiss placement, contrast, accessibility, and GTK5-supported widget structure unchanged.
- Preserve constrained-width behavior where the message and action group remain readable and the action group wraps as one unit when needed.
- Add focused verification for the restored-draft warning, error/retry alerts, no-workflow-action alerts, and narrow editor widths.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `editor-inline-alerts`: refine the inline alert visual rhythm so warning and error alerts remain balanced while occupying slightly less vertical space in normal one-line layouts.

## Impact

- Affected code: `resources/style/style.css`, `resources/ui/info-bar.ui`, `resources/ui/properties-panel.ui`, plus focused inline-alert widget or visual tests if existing coverage needs a height/rhythm assertion.
- User-visible impact: restored-document and other editor-scoped inline alerts sit a little tighter above the editor content, with the adjacent document-properties content nudged by a few pixels so the restored-warning layout aligns across the workspace separator and Location card.
- Dependency impact: no new crate, runtime dependency, GSettings key, or persisted data migration is expected.
