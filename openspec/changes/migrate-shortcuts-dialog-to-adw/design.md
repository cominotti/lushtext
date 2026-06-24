## Context

LushText currently ships `resources/ui/shortcuts.blp` as a GTK
`ShortcutsWindow` resource and presents it through the window action
`win.show-help-overlay`. GTK has deprecated `GtkShortcutsWindow`,
`GtkShortcutsSection`, `GtkShortcutsGroup`, and `GtkShortcutsShortcut`; the
project carries a narrow Blueprint warning exception for that file until the
shortcut-help surface is modernized.

The automated builder diagnostics work found one remaining
`future_gate_candidate` in the `shortcuts-no-context` runtime probe:
`AdwDialogHost` was allocated while a resize was still queued. The most direct
fix is to stop constructing the deprecated top-level GTK shortcuts window and
move to Libadwaita's maintained `AdwShortcutsDialog` surface, which is available
in the current Libadwaita 1.9 binding.

## Goals / Non-Goals

**Goals:**

- Present Keyboard Shortcuts through `AdwShortcutsDialog` while preserving the
  user-visible command, labels, groups, accelerators, and no-context behavior.
- Update widget tests and builder diagnostics so they observe the dialog as an
  `AdwDialog` owned by the active `LushtextWindow`.
- Remove the `GtkShortcuts*` Blueprint warning allowlist once the deprecated
  widgets disappear from the template.
- Prove the builder-diagnostics follow-up is resolved with local and CI
  artifacts, or record a precise remaining toolkit/probe classification if a
  warning survives the migration.

**Non-Goals:**

- Do not change actual application accelerators or add new shortcut commands.
- Do not promote builder diagnostics to default pull-request blocking CI in this
  change.
- Do not add a new dependency or upgrade GTK/Libadwaita for this migration.
- Do not redesign the primary menu, command palette, or action catalog command
  identity.

## Decisions

### Keep `win.show-help-overlay` as the public command

`AdwApplication` can auto-load a `shortcuts-dialog.ui` resource and expose an
`app.shortcuts` action. This change keeps the existing window action instead and
manually loads the shipped dialog resource. That preserves the current action
catalog, menu item, command-palette entry, automation reference, and tests with
minimal behavioral churn.

Alternative considered: replace `win.show-help-overlay` with `app.shortcuts`.
That aligns more directly with Libadwaita's automatic shortcut-dialog path, but
it would retarget a visible command and force broader action-catalog,
automation, documentation, and accelerator decisions. It can remain a later
cleanup after the widget migration is proven stable.

### Convert the template to Libadwaita shortcut widgets

`resources/ui/shortcuts.blp` should switch from `using Gtk 4.0` shortcut
widgets to the Libadwaita shortcuts dialog family, keeping the generated file at
`resources/ui/shortcuts.ui`. The root object should be an
`AdwShortcutsDialog`, with a stable object ID such as `shortcuts_dialog`, and
the existing General/Search/Notes groups should map to Libadwaita shortcuts
sections/items without fake rows or test-only content.

Alternative considered: keep `GtkShortcutsWindow` and classify the debug warning
as benign. That would leave the deprecated template and warning allowlist in
place, so it does not solve the maintenance problem that the diagnostics lane
surfaced.

### Reuse the active dialog through Libadwaita window APIs

The current code finds an existing `GtkShortcutsWindow` by scanning application
windows and matching the transient parent. After migration, the helper should
look at the active `LushtextWindow`'s visible/dialog list, downcast or type-check
the existing `AdwShortcutsDialog`, and present it instead of constructing a
second copy.

Tests should likewise stop enumerating application windows and should assert the
active window has one visible shortcuts dialog. The no-context, reuse,
document-state, dense, and constrained-geometry coverage should remain.

### Treat builder diagnostics as proof, not suppression

The implementation should re-run the local builder diagnostics target after the
template and tests migrate. The CI diagnostics artifact should then be checked
for `future_gate_candidate == 0`. If the `shortcuts-no-context` `AdwDialogHost`
warning persists after the deprecated window is gone, the next step is fresh
triage of probe timing and Libadwaita-private geometry behavior before changing
classifier policy. The resulting classifier must stay scoped to the exact
shortcut probe and diagnostic text so other `AdwDialogHost` geometry warnings
still surface as future-gate candidates.

## Risks / Trade-offs

- `AdwShortcutsDialog` semantics differ from `GtkShortcutsWindow` -> Preserve the
  current command contract in specs and tests, and verify no-context, dense, and
  constrained states rather than only checking object construction.
- Rust binding or Blueprint syntax details may differ from docs examples -> Keep
  the first implementation small, regenerate `.ui`, and let `make
  check-blueprint` plus widget tests catch source/generated mismatches.
- The builder warning may be caused by probe timing or an internal
  `AdwDialogHost` path rather than the deprecated shortcut window -> Require
  diagnostics proof after migration and avoid a classifier exemption until the
  new evidence is specific.
- Keeping `win.show-help-overlay` means the app does not yet use Libadwaita's
  automatic `app.shortcuts` action -> This intentionally avoids command-surface
  churn; a later proposal can bridge or retarget if the product wants the
  automatic action.

## Migration Plan

1. Convert the shortcut template to `AdwShortcutsDialog`, regenerate
   `resources/ui/shortcuts.ui`, and refresh template-contract output.
2. Update `show_help_overlay()` to load and present the Adwaita dialog from the
   existing resource path and to reuse an already-visible shortcuts dialog.
3. Update widget tests, builder-diagnostics coverage expectations, and any docs
   that still call the surface a `GtkShortcutsWindow`.
4. Remove the `GtkShortcuts*` known-warning policy from Blueprint validation
   scripts and guidance.
5. Run Blueprint, widget, action/doc, local builder diagnostics, and CI
   diagnostics checks before marking tasks complete.

Rollback is straightforward: revert the change if the Adwaita dialog proves
unusable. Do not keep a mixed state where code expects `AdwShortcutsDialog` while
Blueprint validation still accepts deprecated `GtkShortcuts*` warnings.

## Open Questions

- None known for the proposal. During implementation, the only expected
  conditional branch is whether builder diagnostics become clean immediately or
  reveal a second probe/toolkit timing issue after the deprecated window is
  removed.
