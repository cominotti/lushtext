## 1. Baseline And Binding Check

- [x] 1.1 Confirm the current builder diagnostics artifact still identifies `shortcuts-no-context` as the only remaining `future_gate_candidate`.
- [x] 1.2 Confirm the workspace Libadwaita Rust binding exposes `ShortcutsDialog`, `ShortcutsSection`, `ShortcutsItem`, and window dialog inspection APIs needed for reuse tests.

## 2. Shortcut Dialog Migration

- [x] 2.1 Convert `resources/ui/shortcuts.blp` from deprecated `GtkShortcuts*` widgets to the Libadwaita shortcuts dialog widget family while preserving existing shortcut groups, labels, and accelerators.
- [x] 2.2 Regenerate `resources/ui/shortcuts.ui` and template-contract artifacts, keeping the resource path stable unless the implementation proves a rename is required.
- [x] 2.3 Update `show_help_overlay()` to load the Adwaita shortcuts dialog from the shipped resource, present it for the active window, and reuse an already-visible shortcuts dialog instead of creating duplicates.
- [x] 2.4 Preserve the public `win.show-help-overlay` action, menu entry, command-palette entry, action-catalog status, and no-context enablement semantics.

## 3. Coverage Updates

- [x] 3.1 Update widget helpers and tests to observe an `AdwShortcutsDialog` owned by the active `LushtextWindow` rather than a separate `GtkShortcutsWindow`.
- [x] 3.2 Preserve tests for no-context presentation, duplicate activation reuse, unchanged document/tab/workspace/settings state, dense shortcut content, and constrained geometry.
- [x] 3.3 Update builder-diagnostics coverage metadata so the `shortcuts-no-context` probe still maps to the shortcut dialog template and runtime surface.
- [x] 3.4 Add or update a narrow assertion that generated shortcut UI no longer contains deprecated `GtkShortcutsWindow`, `GtkShortcutsSection`, `GtkShortcutsGroup`, or `GtkShortcutsShortcut` classes.

## 4. Warning Policy And Guidance

- [x] 4.1 Remove the deprecated `GtkShortcuts*` known-warning allowance from `scripts/blueprint-templates.sh`.
- [x] 4.2 Update `docs/blueprint-validation.md`, `.agents/rules/build.md`, `.agents/rules/ui.md`, and any nearby guidance that still says `resources/ui/shortcuts.blp` is allowed to emit `GtkShortcuts*` deprecation warnings.
- [x] 4.3 Update command or automation documentation only where the text still describes a top-level shortcut window instead of the supported shortcut dialog/surface.

## 5. Diagnostics Proof

- [x] 5.1 Run the local builder diagnostics target with a debug-enabled runtime and preserve the summary, findings, coverage, and raw logs.
- [x] 5.2 Verify the local diagnostics result reports zero actionable findings, zero unclassified findings, and no `shortcuts-no-context` future-gate candidate.
- [ ] 5.3 Run or dispatch the CI builder diagnostics lane for the pushed commit and inspect the uploaded artifact for the same clean shortcut probe result.
- [x] 5.4 If the `AdwDialogHost` allocation warning persists after the migration, classify it with fresh probe/toolkit evidence before changing any classifier policy.

## 6. Validation

- [x] 6.1 Run `make check-blueprint`.
- [x] 6.2 Run targeted widget tests covering the Keyboard Shortcuts command.
- [x] 6.3 Run `make check-agent-docs` if any agent rules or guidance changed.
- [x] 6.4 Run the relevant end-user smoke or builder-diagnostics smoke command that owns the shortcut probe.
- [x] 6.5 Run `openspec validate migrate-shortcuts-dialog-to-adw --strict`, `openspec validate --changes --strict`, and `git diff --check`.
