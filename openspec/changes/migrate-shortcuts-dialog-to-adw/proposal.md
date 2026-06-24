## Why

The builder diagnostics lane found one future-gate candidate in the Keyboard
Shortcuts probe: a debug-runtime `AdwDialogHost` allocation warning while
presenting the current shortcut help surface. That surface is still built from
deprecated `GtkShortcutsWindow` widgets, and LushText now targets Libadwaita 1.9
where `AdwShortcutsDialog` is the maintained replacement.

## What Changes

- Replace the shipped shortcut-help template with a Libadwaita
  `AdwShortcutsDialog`-based surface while preserving the existing shortcut
  groups, labels, and accelerators.
- Keep the public `win.show-help-overlay` command, primary-menu entry,
  command-palette entry, action-catalog status, and no-context availability
  stable.
- Update widget tests and builder-diagnostics coverage so they observe a
  window-owned Adwaita dialog rather than a separate `GtkShortcutsWindow`.
- Remove the narrow Blueprint compiler warning allowance for deprecated
  `GtkShortcuts*` widgets once the template no longer uses them.
- Re-run local and CI builder diagnostics and treat the shortcut probe's
  previous warning as fixed only when the artifact no longer reports it as a
  future-gate candidate; if it persists after migration, classify it with fresh
  evidence rather than hiding it by default.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `menu-workflow-coverage`: the Keyboard Shortcuts command presents a
  maintained Libadwaita shortcuts dialog/surface instead of a deprecated
  `GtkShortcutsWindow`, while keeping the command contract stable.
- `blueprint-validation-hardening`: Blueprint validation no longer accepts
  deprecated `GtkShortcuts*` compiler warnings as known-good output.

## Impact

- Code and templates: `resources/ui/shortcuts.blp`,
  `resources/ui/shortcuts.ui`, `resources/dev.cominotti.lushtext.gresource.xml`
  if the resource name or object ID changes, and
  `crates/lushtext-core/src/ui/window/actions.rs`.
- Tests and diagnostics:
  `crates/lushtext/tests/widget/window.rs`,
  `scripts/builder-diagnostics-coverage.json`, and the builder diagnostics
  smoke/runtime lane.
- Documentation and rules: Blueprint validation guidance, agent/rule warning
  policy text, and command/action documentation only where wording still says
  "shortcut window" or refers to deprecated `GtkShortcuts*` allowance.
- Dependencies: no new dependency is expected; the existing Libadwaita 1.9 Rust
  binding already exposes `ShortcutsDialog`, `ShortcutsSection`, and
  `ShortcutsItem`.
