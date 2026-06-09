## Context

`add-dbus-automation-spine` established the app-owned Automation1 D-Bus object, the action catalog, readiness predicates, workflow-event snapshots, `make automation-smoke`, and documentation drift checks. The remaining follow-ups are practical completion work:

- `.github/workflows/end-user-smoke.yml` schedules host-sensitive smoke lanes, but it does not yet include the new automation smoke lane.
- `win.show-help-overlay` appears in `resources/ui/window.ui`, `resources/ui/window.blp`, and the command palette command list, but the action catalog currently marks it as a visible unregistered unsupported gap.
- The automation docs give raw `gdbus` examples, while agents and developers need a safer, supported wrapper for the common sequence: inspect, activate a documented action, wait, snapshot, and summarize artifacts.

The existing automation boundary remains the key constraint. Mutation stays on documented GTK/GIO actions; Automation1 remains read-only except for readiness waits; snapshots stay bounded and content-safe; Flatpak keeps full filesystem access; portal diagnostics remain diagnostic only.

## Goals / Non-Goals

**Goals:**

- Schedule the D-Bus automation smoke lane in the same manual/scheduled workflow that already preserves visual, crash, portal/sandbox, accessibility, and performance artifacts.
- Close the Keyboard Shortcuts action gap by registering `win.show-help-overlay` and proving it opens the shipped `GtkShortcutsWindow` through primary menu, command palette, and D-Bus action activation.
- Add a supported `lushtext-automation` client wrapper for same-user developers and agents that can inspect Automation1, activate cataloged actions, wait on readiness predicates, and summarize smoke artifacts.
- Keep the wrapper's outputs bounded, machine-readable, and documented so downstream agent tooling can rely on stable fields.
- Extend drift checks so docs fail when client commands, flags, output fields, action status, or scheduled smoke expectations drift.

**Non-Goals:**

- No new private mutation API.
- No portals-only migration or Flatpak permission narrowing.
- No `DBusActivatable=true`, desktop-file action metadata, or launch-behavior change.
- No replacement of `scripts/run-automation-smoke.sh`, visual smoke, AT-SPI smoke, or widget tests.
- No pixel-perfect screenshot comparison for the Keyboard Shortcuts window.
- No dependency on third-party Python packages or a new Rust CLI crate unless implementation proves the standard toolchain cannot satisfy the contract.

## Decisions

### 1. Add `automation` to the scheduled/manual smoke matrix

The scheduled workflow should gain a matrix entry:

```text
lane: automation
command: make automation-smoke SMOKE_ARTIFACT_DIR=build/smoke
artifact_path: build/smoke/automation
```

This keeps the automation proof visible outside local runs without making it PR-required. The workflow already installs the host tooling used by `make automation-smoke`: GTK, Libadwaita, GtkSourceView, `dbus-daemon`, `glib2`, Mutter, PipeWire, Python, and PyGObject. If the lane discovers a missing host dependency, it should skip or fail consistently with the smoke helper's documented unsupported-host behavior and preserve whatever artifacts exist.

Alternatives considered:

- Add automation smoke to required PR CI. Rejected because it uses a real compositor and D-Bus session, matching the host-sensitive policy of the existing end-user smoke workflow.
- Fold automation checks into visual smoke. Rejected because automation smoke has no screenshot dependency and should remain a faster, D-Bus-specific proof lane.

### 2. Register `win.show-help-overlay` as a normal window action

The action already exists in visible surfaces. Implementation should add a window action that opens the existing `resources/ui/shortcuts.ui` / `GtkShortcutsWindow` resource, sets the active LushText window as transient parent, presents it, and keeps normal GTK dismissal behavior. The action should be enabled without requiring an active document because keyboard-shortcut help is useful from empty startup, restored sessions, and failed-placeholder states.

The action catalog should change only for this row:

- `exposure`: `exported`
- `safety`: `stable-user-command` or `contextual-user-command` if GTK requires an active window
- `owner`: `window/actions` or another concrete window module, not `services/palette`
- `surfaces`: primary menu, command palette, D-Bus action
- `coverage`: unit, widget, and automation smoke or manual diagnostic if the smoke lane cannot inspect the shortcut window reliably

Visible state extremes to cover:

```text
No document         -> action opens the shortcut window
One active document -> action opens the shortcut window without changing document state
Many shortcuts      -> shortcut content scrolls inside the shortcut window, not the app shell
Constrained window  -> title/header/close remain reachable; content is not clipped into uselessness
```

Alternatives considered:

- Remove the visible command from menu/palette. Rejected because the shortcut window resource exists and the command is useful.
- Keep it documented as an unsupported gap. Rejected because the gap is small, user-visible, and now the action catalog makes such drift very obvious.

### 3. Implement the client wrapper as a thin process tool over existing contracts

The supported client should be a small script such as `scripts/lushtext-automation.py`, optionally exposed through a `make automation-client-smoke` or `make automation-client-self-test` target. It should shell out to standard desktop tools (`gdbus`, optionally `gio`) and parse Automation1 JSON with Python's standard library. This avoids new runtime dependencies and keeps the tool usable inside the same Fedora container/host environments as smoke tests.

Candidate command shape:

```text
scripts/lushtext-automation.py introspect
scripts/lushtext-automation.py catalog [--json]
scripts/lushtext-automation.py snapshot [--json] [--field PATH]
scripts/lushtext-automation.py predicates [--json]
scripts/lushtext-automation.py events [--json]
scripts/lushtext-automation.py wait PREDICATE [--timeout-ms N] [--json]
scripts/lushtext-automation.py action ACTION [--string TEXT|--bool true|--uint32 N|--variant-json JSON] [--window-path PATH]
scripts/lushtext-automation.py artifact-summary DIR [--json]
scripts/lushtext-automation.py self-test
```

Defaults should match the documented Automation1 interface:

- bus name `dev.cominotti.lushtext`
- object path `/dev/cominotti/lushtext/Automation`
- interface `dev.cominotti.lushtext.Automation1`
- first-window action path `/dev/cominotti/lushtext/window/1`, overridable for future multi-window work

Before activating an action, the wrapper should read the action catalog, find the requested fully qualified action ID, reject `unsupported-gap` or `visible-unregistered-gap` rows, verify parameter type compatibility, and then call `org.gtk.Actions.Activate`. It should not synthesize widget-local context or mutate private widget state.

Structured output should be stable and compact:

```json
{
  "ok": true,
  "status": "ready",
  "command": "wait",
  "detail": "search-complete is ready",
  "data": {}
}
```

Errors should also be stable enough for agents:

- `usage-error`
- `unsupported-host-tooling`
- `automation-unavailable`
- `dbus-error`
- `unknown-action`
- `unsupported-action`
- `parameter-mismatch`
- `predicate-timeout`
- `workflow-failure`
- `artifact-error`

Suggested exit-code contract:

- `0`: command succeeded or readiness reached
- `1`: app-reported failure, failed predicate, failed artifact validation, or failed self-test
- `2`: invalid CLI usage or parameter mismatch
- `3`: automation object, action group, or D-Bus session unavailable
- `4`: required host tool unavailable

Alternatives considered:

- A Rust CLI binary. Rejected for the first pass because it would add workspace build surface and packaging decisions for a helper that can be implemented safely with standard Python and `gdbus`.
- A large scenario runner rewrite. Rejected because `run-automation-smoke.sh` and `automation-smoke-driver.py` already own full smoke scenarios; the wrapper should improve everyday ergonomics, not become another scenario engine.
- Raw documentation-only examples. Rejected because agents need reliable parsing, exits, and error categories.

### 4. Keep artifact inspection bounded and lane-aware

`artifact-summary DIR` should inspect known smoke artifact layouts without embedding unbounded payloads. For `build/smoke/automation`, it should summarize `scenario-manifest.json`, `summary.txt`, `assertions/runtime-warning-scan.txt`, snapshots, readiness artifacts, workflow events, action lists, and failure/skip reasons. For visual/crash/accessibility/portal directories, it may summarize common manifest/status/warning files when present, but it should not claim deep validation outside documented fields.

The output should help humans and agents answer:

- Did the lane pass, fail, or skip?
- Which scenario or capture failed?
- Which artifact path contains the useful evidence?
- Were warning scans clean?
- Which D-Bus/action/snapshot assertions ran?

It must not dump screenshots, document text, note bodies, search result bodies, or private sidecar/draft identifiers.

### 5. Make docs drift checks cover the new public surface

`make check-automation-docs` should learn the wrapper's public commands, flags, output fields, and status names from source or a checked manifest, then verify `docs/automation-reference.md` and `docs/automation.md` document them. Its self-test should remove at least one representative wrapper command/flag/status doc and confirm the check fails.

The action catalog change for `win.show-help-overlay` should continue to be checked by existing catalog/doc drift gates, but tests should also prove the row is no longer an unsupported gap.

## Risks / Trade-offs

- **[Risk: Wrapper becomes a second automation API]** -> Keep every mutation routed through cataloged `org.gtk.Actions`; document that the wrapper is a client, not a new app interface.
- **[Risk: Raw `gdbus` output parsing is brittle]** -> Keep parsing concentrated in one helper, test representative outputs, and fail with stable `dbus-error` or `automation-unavailable` statuses when parsing fails.
- **[Risk: Scheduled automation smoke is flaky on hosted runners]** -> Keep it in the existing scheduled/manual host-sensitive workflow, upload partial artifacts on failure, and use existing unsupported-host categories instead of false passes.
- **[Risk: Shortcut window tests become geometry-fragile]** -> Use widget assertions for action registration, dialog/window presentation, title/role/close reachability, and constrained sizing rather than exact pixels.
- **[Risk: Artifact summaries accidentally expose content]** -> Cap all embedded text and summarize paths/counts/statuses rather than payload files.
- **[Risk: Multi-window support arrives later]** -> Accept `--window-path` overrides now and default to `/window/1`; future multi-window discovery can be additive.

## Migration Plan

1. Update the scheduled/manual smoke workflow to include the automation lane and prove artifacts upload under the expected path.
2. Register and test `win.show-help-overlay`, then update the catalog row and docs to remove the unsupported gap.
3. Add the automation client wrapper with command parser, D-Bus calls, action activation, wait handling, artifact summary, self-test, and stable output/error statuses.
4. Extend docs and `make check-automation-docs` so the wrapper and scheduled lane are part of the automation contract.
5. Run focused unit/widget/script tests, `make automation-smoke`, documentation drift checks, OpenSpec validation, and full formatting/lint gates.

Rollback is straightforward: remove the workflow matrix row, remove or disable the wrapper docs/target, and leave `win.show-help-overlay` registered if it works as a legitimate user command. If shortcut presentation proves unsafe, revert the catalog row to an explicit unsupported gap with a documented blocker rather than leaving a silent visible command.

## Open Questions

- Should the wrapper be named `scripts/lushtext-automation.py`, `scripts/automation-client.py`, or exposed through a Makefile alias as the stable entry point?
- Should `artifact-summary` support all smoke lanes immediately or start with automation smoke and only provide generic summaries for the others?
- Should the wrapper provide a `launch` command later, or should launching remain owned by smoke helpers so this change stays focused on a running app/session?
