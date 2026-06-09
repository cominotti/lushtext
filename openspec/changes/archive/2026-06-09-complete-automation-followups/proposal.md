## Why

The D-Bus automation spine made LushText observable and scriptable, but three practical follow-ups remain: the new automation smoke lane is not yet scheduled, one visible command still documents a real action gap, and humans or agents still need to stitch together raw `gdbus`/`gio` calls by hand. This change turns the spine into a more complete daily tool by scheduling its proof lane, closing the Keyboard Shortcuts command gap, and adding a supported automation client wrapper.

## What Changes

- Add `automation-smoke` to the scheduled/manual end-user smoke workflow with preserved artifacts, skip/failure reporting, and the same host-sensitive policy as the existing visual, crash-recovery, portal/sandbox, accessibility, and performance smoke lanes.
- Register the existing `win.show-help-overlay` command as a real exported window action that opens the shipped `GtkShortcutsWindow` from the primary menu, command palette, and D-Bus action activation.
- Update the action catalog so `win.show-help-overlay` moves from `visible-unregistered-gap` / `unsupported-gap` to an exported, user-facing command with unit/widget/automation coverage and fresh documentation.
- Add a supported `lushtext-automation` developer/agent client wrapper for common Automation1 operations: introspection, snapshot reads, readiness waits, workflow-event reads, action catalog reads, GTK action activation, scenario artifact summaries, and bounded JSON extraction.
- Ensure the wrapper never becomes a private mutation API: state-changing behavior still routes through documented GTK/GIO actions and read-only observations still route through Automation1.
- Extend docs and drift checks so the new client commands, flags, output fields, examples, and scheduled smoke lane stay current alongside the existing action/D-Bus/snapshot/scenario reference.
- No portals-only migration, no Flatpak permission narrowing, no new activation metadata, and no breaking change to existing helper scripts or smoke artifact locations.

## Capabilities

### New Capabilities

- `automation-client-tools`: Supported command-line tooling for same-user agents and developers to inspect Automation1, activate documented GTK/GIO actions, wait for readiness predicates, summarize smoke artifacts, and consume bounded machine-readable output.

### Modified Capabilities

- `desktop-visual-smoke-coverage`: The scheduled/manual end-user smoke workflow must include the D-Bus automation smoke lane and preserve its artifacts.
- `menu-workflow-coverage`: The visible Keyboard Shortcuts command must resolve to a registered action and be represented as supported in the action catalog.

## Impact

- Affected Rust areas: `crates/lushtext-core/src/ui/window/actions.rs`, action/catalog registration under `crates/lushtext-core/src/services/action_catalog/`, command-palette/menu action audits, and widget tests around live actions and shortcut overlay behavior.
- Affected scripts/tooling: a new or extended automation client under `scripts/` or an equivalent supported helper path, `Makefile` targets if needed, `scripts/check-automation-docs.py`, and smoke artifact summary parsing.
- Affected CI: `.github/workflows/end-user-smoke.yml` matrix gains an `automation` lane that runs `make automation-smoke SMOKE_ARTIFACT_DIR=build/smoke` and uploads `build/smoke/automation`.
- Affected documentation: `docs/automation.md`, `docs/automation-reference.md`, `docs/end-user-coverage.md`, `README.md`, `AGENTS.md`, and relevant `.agents/rules`/skill references for the supported automation client and scheduled smoke lane.
- Dependency posture: prefer standard Python plus `gdbus`/`gio`/`jq`-optional parsing already used by the smoke lanes; adding a new Rust or Python package dependency requires explicit justification and packaging validation.
