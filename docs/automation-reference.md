# Automation Reference

This is the developer-facing contract for LushText automation. Keep it in sync
with `crates/lushtext-core/src/services/action_catalog/mod.rs`,
`crates/lushtext-core/src/ui/automation.rs`, and
`crates/lushtext-core/src/model/automation.rs`, plus the reusable client in
`scripts/lushtext-automation.py`.

Run:

```sh
make check-automation-docs
```

The check verifies action anchors, D-Bus method/property anchors, snapshot
field anchors, workflow event field anchors, readiness predicate and blocker
anchors, action table fields, stable D-Bus error names, user-guide baseline
terms, scenario-helper flag entries, and scenario manifest field anchors in
this file, every reusable automation-client command/status/result/artifact
anchor, plus every stable AT-SPI anchor used by the accessibility smoke helper.

<!-- automation-helper-flags: run-automation-smoke --artifact-dir --binary run-crash-recovery-smoke --artifact-dir --binary run-accessibility-smoke --artifact-dir --binary run-visual-smoke --artifact-dir --binary capture-lushtext-mutter --file --output --search --expected-search-matches --enable-minimap --enable-atspi --window-action --window-string-action --wait-predicate --wait-window-action --wait-atspi-text --color-scheme --capture-artifact-dir --atspi-tree-output --atspi-focus-output --binary --width --height --keep-artifacts run-portal-sandbox-smoke --artifact-dir check-flatpak-permissions --manifest --self-test lushtext-automation introspect catalog snapshot predicates events wait action artifact-summary self-test --bus-name --object-path --interface --window-path --timeout-ms --json --field --string --bool --uint32 --variant-json -->

## Stability Policy

- `InterfaceVersion` starts at `1`. Increment it when a method, property,
  snapshot field, field meaning, or action-catalog row changes incompatibly for
  automation consumers.
- Additive fields are allowed, but they must be documented here and covered by
  `make check-automation-docs`.
- Mutating operations stay on normal GTK/GIO actions. The
  `dev.cominotti.lushtext.Automation1` object is read-only except for waiting.
- LushText keeps full filesystem permission. Do not describe this automation
  layer as a portals-only migration. `make check-flatpak-permissions` fails if
  the Flatpak manifest loses `--filesystem=host`.
- Snapshot JSON must remain bounded and content-safe. Do not expose document
  text, note bodies, draft bodies, local-history contents, complete search
  result text, or private persistence identifiers.

## D-Bus Interface

Bus name: `dev.cominotti.lushtext`

Object path: `/dev/cominotti/lushtext/Automation`

Interface: `dev.cominotti.lushtext.Automation1`

| Anchor | Member | Kind | Signature | Meaning |
| --- | --- | --- | --- | --- |
| <span id="dbus-property-interface-version"></span>`dbus-property-interface-version` | `InterfaceVersion` | property | `u` | Stable contract version for the app-owned automation object. |
| <span id="dbus-property-enabled"></span>`dbus-property-enabled` | `Enabled` | property | `b` | Always `true` while the object is registered. |
| <span id="dbus-property-build-profile"></span>`dbus-property-build-profile` | `BuildProfile` | property | `s` | Diagnostic build profile, currently `debug` or `release`. |
| <span id="dbus-method-get-action-catalog"></span>`dbus-method-get-action-catalog` | `GetActionCatalog` | method | `() -> (s json)` | Returns pretty JSON for the action catalog rows documented below. |
| <span id="dbus-method-get-snapshot"></span>`dbus-method-get-snapshot` | `GetSnapshot` | method | `() -> (s json)` | Returns the bounded app/window snapshot documented below. |
| <span id="dbus-method-get-readiness-predicates"></span>`dbus-method-get-readiness-predicates` | `GetReadinessPredicates` | method | `() -> (s json)` | Returns pretty JSON for the supported readiness predicate rows documented below. |
| <span id="dbus-method-get-workflow-events"></span>`dbus-method-get-workflow-events` | `GetWorkflowEvents` | method | `() -> (s json)` | Returns the bounded workflow event snapshot documented below. |
| <span id="dbus-method-wait-for-ready"></span>`dbus-method-wait-for-ready` | `WaitForReady` | method | `(s predicate, u timeout_msec) -> (b ok, s status, s detail)` | Waits for a named readiness predicate. `status` is one of the readiness statuses documented below. |
| <span id="dbus-method-wait-for-idle"></span>`dbus-method-wait-for-idle` | `WaitForIdle` | method | `(u timeout_msec) -> (b ok, s detail)` | Waits for tracked workflows to settle. On timeout, `detail` names the first blocker. |

## D-Bus Errors

| Anchor | Error | Meaning |
| --- | --- | --- |
| <span id="dbus-error-unavailable"></span>`dbus-error-unavailable` | `dev.cominotti.lushtext.Automation1.Error.Unavailable` | The app weak reference is gone while handling the request. |
| <span id="dbus-error-internal"></span>`dbus-error-internal` | `dev.cominotti.lushtext.Automation1.Error.Internal` | Catalog construction, snapshot serialization, or another internal read-only projection failed. |
| <span id="dbus-error-unknown-method"></span>`dbus-error-unknown-method` | `dev.cominotti.lushtext.Automation1.Error.UnknownMethod` | The caller requested a method outside the documented Automation1 interface. |

## Readiness Statuses

`WaitForReady` returns `ok=false` with a stable status when readiness cannot be
reported. Smoke helpers also use the same vocabulary for host-side failures
that happen before LushText can answer on D-Bus.

| Status | Meaning |
| --- | --- |
| `ready` | The requested predicate settled before the timeout. |
| `predicate-timeout` | The predicate stayed blocked until the caller's timeout expired. |
| `workflow-failure` | The workflow reached a failed app state instead of a ready state, such as a failed file load. |
| `automation-unavailable` | The automation object, app process, or expected action group was unavailable. |
| `unsupported-host-tooling` | A required smoke host tool such as Mutter, `gdbus`, or PyGObject was unavailable. |
| `unknown-predicate` | The caller requested a predicate not supported by this interface version. |

## Automation CLI Client

`scripts/lushtext-automation.py` is the supported same-user helper for agents
and developers that want a stable command-line wrapper around Automation1 and
`org.gtk.Actions`. It does not add a private mutation channel: the `action`
command reads `GetActionCatalog`, rejects non-exported or unsupported-gap rows,
and activates only normal app/window GTK actions with typed GVariant
parameters. Use `make automation-client-self-test` to validate the client
parser, status envelope, parameter rendering, and artifact-summary reader
without launching LushText.

Common flags:

- `--bus-name` defaults to `dev.cominotti.lushtext`.
- `--object-path` defaults to `/dev/cominotti/lushtext/Automation`.
- `--interface` defaults to `dev.cominotti.lushtext.Automation1`.
- `--window-path` defaults to `/dev/cominotti/lushtext/window/1`.
- `--timeout-ms` sets D-Bus call and readiness wait timeouts.
- `--json` prints the stable result envelope instead of human output.
- `--field` selects a dotted field from JSON-like command data.
- `--string`, `--bool`, `--uint32`, and `--variant-json` provide typed action parameters.

| Anchor | Command | Meaning |
| --- | --- | --- |
| <span id="automation-client-command-introspect"></span>`automation-client-command-introspect` | `introspect` | Reads `org.freedesktop.DBus.Introspectable.Introspect` for the Automation1 object. |
| <span id="automation-client-command-catalog"></span>`automation-client-command-catalog` | `catalog` | Reads and parses `GetActionCatalog` JSON. |
| <span id="automation-client-command-snapshot"></span>`automation-client-command-snapshot` | `snapshot` | Reads and parses the bounded `GetSnapshot` JSON. |
| <span id="automation-client-command-predicates"></span>`automation-client-command-predicates` | `predicates` | Reads and parses `GetReadinessPredicates` JSON. |
| <span id="automation-client-command-events"></span>`automation-client-command-events` | `events` | Reads and parses `GetWorkflowEvents` JSON. |
| <span id="automation-client-command-wait"></span>`automation-client-command-wait` | `wait [predicate]` | Calls `WaitForReady`; `legacy-idle` calls `WaitForIdle` for compatibility. |
| <span id="automation-client-command-action"></span>`automation-client-command-action` | `action ACTION` | Activates a cataloged exported `app.` or `win.` action through `org.gtk.Actions.Activate`. |
| <span id="automation-client-command-artifact-summary"></span>`automation-client-command-artifact-summary` | `artifact-summary DIR` | Summarizes a smoke `scenario-manifest.json`, summary JSON, warning scan, waits, actions, and D-Bus artifacts. |
| <span id="automation-client-command-self-test"></span>`automation-client-command-self-test` | `self-test` | Runs local parser, parameter, result, and artifact-summary checks without a live app. |

### Client Result Envelope

With `--json`, every client command returns the fields below. Without `--json`,
data commands print their payload directly and failure commands print the same
status vocabulary.

| Anchor | Field | Meaning |
| --- | --- | --- |
| <span id="automation-client-result-field-ok"></span>`automation-client-result-field-ok` | `ok` | Boolean success indicator; true when `status` is `ok`, `ready`, or `artifact-skipped`. |
| <span id="automation-client-result-field-status"></span>`automation-client-result-field-status` | `status` | Stable client status listed below. |
| <span id="automation-client-result-field-command"></span>`automation-client-result-field-command` | `command` | Client subcommand that produced the result. |
| <span id="automation-client-result-field-detail"></span>`automation-client-result-field-detail` | `detail` | Human-readable bounded summary for terminal output and logs. |
| <span id="automation-client-result-field-data"></span>`automation-client-result-field-data` | `data` | Command payload, selected field value, action activation detail, artifact summary, or failure context. |

### Client Statuses And Exits

| Anchor | Status | Exit | Meaning |
| --- | --- | --- | --- |
| <span id="automation-client-status-ok"></span><span id="automation-client-exit-ok"></span>`ok` | `ok` | `0` | Generic read, action, self-test, or artifact-summary command succeeded. |
| <span id="automation-client-status-ready"></span><span id="automation-client-exit-ready"></span>`ready` | `ready` | `0` | Requested readiness predicate settled successfully. |
| <span id="automation-client-status-usage-error"></span><span id="automation-client-exit-usage-error"></span>`usage-error` | `usage-error` | `2` | CLI arguments, timeout, or selected field path are malformed. |
| <span id="automation-client-status-unsupported-host-tooling"></span><span id="automation-client-exit-unsupported-host-tooling"></span>`unsupported-host-tooling` | `unsupported-host-tooling` | `4` | Required host command such as `gdbus` is unavailable. |
| <span id="automation-client-status-automation-unavailable"></span><span id="automation-client-exit-automation-unavailable"></span>`automation-unavailable` | `automation-unavailable` | `3` | The app, D-Bus name, object path, or method did not answer. |
| <span id="automation-client-status-dbus-error"></span><span id="automation-client-exit-dbus-error"></span>`dbus-error` | `dbus-error` | `1` | D-Bus output could not be parsed, or a cataloged activation failed through D-Bus. |
| <span id="automation-client-status-unknown-predicate"></span><span id="automation-client-exit-unknown-predicate"></span>`unknown-predicate` | `unknown-predicate` | `2` | Automation1 reported that the requested readiness predicate is not supported. |
| <span id="automation-client-status-unknown-action"></span><span id="automation-client-exit-unknown-action"></span>`unknown-action` | `unknown-action` | `2` | The requested action is absent from `GetActionCatalog`. |
| <span id="automation-client-status-unsupported-action"></span><span id="automation-client-exit-unsupported-action"></span>`unsupported-action` | `unsupported-action` | `2` | The action is cataloged but not exported, widget-scoped, or marked unsupported. |
| <span id="automation-client-status-parameter-mismatch"></span><span id="automation-client-exit-parameter-mismatch"></span>`parameter-mismatch` | `parameter-mismatch` | `2` | Supplied action parameter type does not match the cataloged parameter type. |
| <span id="automation-client-status-predicate-timeout"></span><span id="automation-client-exit-predicate-timeout"></span>`predicate-timeout` | `predicate-timeout` | `1` | A readiness wait returned `ok=false` before the requested predicate settled. |
| <span id="automation-client-status-workflow-failure"></span><span id="automation-client-exit-workflow-failure"></span>`workflow-failure` | `workflow-failure` | `1` | Automation1 or the client self-test reported a failed workflow/invariant. |
| <span id="automation-client-status-artifact-error"></span><span id="automation-client-exit-artifact-error"></span>`artifact-error` | `artifact-error` | `1` | `artifact-summary` found missing, malformed, failed, or unrecognized artifact evidence. |
| <span id="automation-client-status-artifact-skipped"></span><span id="automation-client-exit-artifact-skipped"></span>`artifact-skipped` | `artifact-skipped` | `0` | `artifact-summary` found a skipped lane and reports it distinctly without claiming coverage passed. |

### Artifact Summary Fields

| Anchor | Field | Meaning |
| --- | --- | --- |
| <span id="automation-client-artifact-field-artifact-dir"></span>`automation-client-artifact-field-artifact-dir` | `artifact_dir` | Absolute artifact directory that was summarized. |
| <span id="automation-client-artifact-field-status"></span>`automation-client-artifact-field-status` | `status` | Final manifest status, usually `passed`, `failed`, or `skipped`. |
| <span id="automation-client-artifact-field-scenario-id"></span>`automation-client-artifact-field-scenario-id` | `scenario_id` | Stable scenario id from the manifest. |
| <span id="automation-client-artifact-field-failure-reason"></span>`automation-client-artifact-field-failure-reason` | `failure_reason` | Bounded failure reason when the scenario failed. |
| <span id="automation-client-artifact-field-skip-reason"></span>`automation-client-artifact-field-skip-reason` | `skip_reason` | Bounded host or tooling reason when the scenario skipped. |
| <span id="automation-client-artifact-field-manifest"></span>`automation-client-artifact-field-manifest` | `manifest` | Absolute path to `scenario-manifest.json`. |
| <span id="automation-client-artifact-field-summary"></span>`automation-client-artifact-field-summary` | `summary` | Parsed `summary.json` payload when present. |
| <span id="automation-client-artifact-field-runtime-warning-scan"></span>`automation-client-artifact-field-runtime-warning-scan` | `runtime_warning_scan` | Text from `assertions/runtime-warning-scan.txt` when present. |
| <span id="automation-client-artifact-field-workflow-events"></span>`automation-client-artifact-field-workflow-events` | `workflow_events` | Bounded workflow-event artifact summary: relative path, last sequence, capped flag, and event count. |
| <span id="automation-client-artifact-field-snapshots"></span>`automation-client-artifact-field-snapshots` | `snapshots` | Relative paths for snapshot JSON artifacts without embedding their payloads. |
| <span id="automation-client-artifact-field-dbus-artifacts"></span>`automation-client-artifact-field-dbus-artifacts` | `dbus_artifacts` | Relative paths for D-Bus, catalog, snapshot, workflow, and introspection artifacts. |
| <span id="automation-client-artifact-field-state-assertions"></span>`automation-client-artifact-field-state-assertions` | `state_assertions` | Manifest state-proof rows. |
| <span id="automation-client-artifact-field-waits"></span>`automation-client-artifact-field-waits` | `waits` | Manifest readiness waits. |
| <span id="automation-client-artifact-field-actions"></span>`automation-client-artifact-field-actions` | `actions` | Manifest action activations. |

## Scenario Helper Flags

| Helper | Flag | Meaning |
| --- | --- | --- |
| `scripts/run-automation-smoke.sh` | `--artifact-dir DIR` | Writes D-Bus introspection, app/window action-list and Describe, catalog, readiness predicates, workflow events, snapshots, state/snapshot sync, predicate waits, legacy idle wait, `scenario-manifest.json`, `workflow-events.json`, runtime-warning-scan, log, fixture, and summary artifacts to `DIR`. |
| `scripts/run-automation-smoke.sh` | `--binary PATH` | Runs the given LushText binary instead of `target/debug/lushtext`. |
| `scripts/run-crash-recovery-smoke.sh` | `--artifact-dir DIR` | Writes SIGKILL/relaunch recovery metadata, Automation1 wait and snapshot assertions, AT-SPI recovery diagnostics, warning scan, screenshot, `scenario-manifest.json`, and summary artifacts to `DIR`. |
| `scripts/run-crash-recovery-smoke.sh` | `--binary PATH` | Runs the given LushText binary instead of `target/debug/lushtext`. |
| `scripts/run-accessibility-smoke.sh` | `--artifact-dir DIR` | Writes accessibility screenshots, AT-SPI tree/focus artifacts, stable-anchor assertions, focus-path assertions, warning scan, environment report, and summary artifacts to `DIR`. |
| `scripts/run-accessibility-smoke.sh` | `--binary PATH` | Runs the given LushText binary instead of `target/debug/lushtext`. |
| `scripts/run-visual-smoke.sh` | `--artifact-dir DIR` | Writes screenshot, Automation1 snapshot, surface/search/workspace/notes/bookmarks/command-palette assertions, per-capture `*-manifest.json`, warning-scan, capture-session, environment, and summary artifacts to `DIR`. |
| `scripts/run-visual-smoke.sh` | `--binary PATH` | Runs the given LushText binary instead of `target/debug/lushtext`. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--file PATH` | Opens the fixture file in the isolated LushText process before capture. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--output PATH` | Writes the captured headless Mutter monitor PNG to this path. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--search TEXT` | Sets in-document search through `win.set-search-query` and waits for `search-complete`. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--expected-search-matches N` | When `--search` is set, waits until Automation1 reports this editor match count before screenshot capture. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--enable-minimap` | Enables the minimap GSettings key before launching LushText. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--enable-atspi` | Starts a private AT-SPI registry even when the scenario does not set text through AT-SPI. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--window-action ACTION` | Activates a window-scoped `org.gtk.Actions` action before capture; may be repeated. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--window-string-action ACTION=TEXT` | Activates a window-scoped `org.gtk.Actions` action with one string parameter before capture; may be repeated. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--wait-predicate PREDICATE` | Waits for an Automation1 readiness predicate before the final snapshot; may be repeated for scenario-specific gates. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--wait-window-action ACTION` | Waits until a window-scoped `org.gtk.Actions` action is enabled, useful for dialog-mounted action groups such as Browse Notes. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--wait-atspi-text TEXT` | Waits until a bounded AT-SPI tree for LushText contains text such as an empty-state title or dialog row label. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--color-scheme MODE` | Sets `default`, `force-light`, or `force-dark` color-scheme GSettings before launch. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--capture-artifact-dir DIR` | Keeps the helper's isolated data/config/cache/runtime logs and Automation1 snapshot in `DIR`. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--atspi-tree-output PATH` | Writes a bounded AT-SPI tree excerpt for scenarios that intentionally verify accessible state. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--atspi-focus-output PATH` | Writes the focused AT-SPI node path for accessibility-sensitive captures. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--binary PATH` | Runs the given LushText binary instead of `target/debug/lushtext`. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--width PX` | Uses this virtual-monitor width for the isolated headless Mutter session. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--height PX` | Uses this virtual-monitor height for the isolated headless Mutter session. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--keep-artifacts` | Preserves the helper's artifact directory after a successful capture. |
| `scripts/run-portal-sandbox-smoke.sh` | `--artifact-dir DIR` | Writes runtime, portal, Flatpak/Snap, denial-scan, permission-posture, and summary artifacts to `DIR`. |
| `scripts/check-flatpak-permissions.py` | `--manifest PATH` | Checks the given Flatpak manifest for the intentional `--filesystem=host` permission. |
| `scripts/check-flatpak-permissions.py` | `--self-test` | Also proves the checker fails when a representative manifest loses full filesystem access. |
| `scripts/lushtext-automation.py` | `introspect` | Reads the Automation1 D-Bus introspection XML. |
| `scripts/lushtext-automation.py` | `catalog` | Reads the action catalog. |
| `scripts/lushtext-automation.py` | `snapshot` | Reads the bounded automation snapshot. |
| `scripts/lushtext-automation.py` | `predicates` | Reads readiness predicate metadata. |
| `scripts/lushtext-automation.py` | `events` | Reads workflow event metadata. |
| `scripts/lushtext-automation.py` | `wait` | Waits for a readiness predicate or legacy idle. |
| `scripts/lushtext-automation.py` | `action` | Activates a cataloged exported app/window action with an optional typed parameter. |
| `scripts/lushtext-automation.py` | `artifact-summary` | Summarizes a smoke scenario manifest and sibling artifacts. |
| `scripts/lushtext-automation.py` | `self-test` | Runs client parser and artifact-summary self-tests. |
| `scripts/lushtext-automation.py` | `--bus-name NAME` | Overrides the session-bus destination, defaulting to `dev.cominotti.lushtext`. |
| `scripts/lushtext-automation.py` | `--object-path PATH` | Overrides the Automation1 object path. |
| `scripts/lushtext-automation.py` | `--interface NAME` | Overrides the Automation1 interface name. |
| `scripts/lushtext-automation.py` | `--window-path PATH` | Overrides the window action-group object path. |
| `scripts/lushtext-automation.py` | `--timeout-ms MSEC` | Sets readiness and D-Bus call timeout budget. |
| `scripts/lushtext-automation.py` | `--json` | Emits the stable result envelope. |
| `scripts/lushtext-automation.py` | `--field FIELD` | Selects a dotted field from JSON-like command data. |
| `scripts/lushtext-automation.py` | `--string TEXT` | Supplies a string parameter for `action`. |
| `scripts/lushtext-automation.py` | `--bool BOOL` | Supplies a boolean parameter for `action`. |
| `scripts/lushtext-automation.py` | `--uint32 N` | Supplies an unsigned 32-bit integer parameter for `action`. |
| `scripts/lushtext-automation.py` | `--variant-json JSON` | Supplies a future variant-map parameter with string, bool, and u32 values. |

## Stable AT-SPI Smoke Anchors

`scripts/run-accessibility-smoke.sh` treats the anchors below as stable
user-facing accessibility metadata. They are public automation anchors only
because they are also meaningful names and roles for assistive technology
users. The documentation drift check derives this list from the smoke helper's
`assert_anchor` and `record_focus_anchor` calls.

The command palette mode control has a GTK accessible label of `Command
palette mode`, but AT-SPI currently exposes the combo box by selected value, so
the stable smoke anchor is `Files`. In headless sessions AT-SPI may also omit a
focused node; the helper records `focused_name=<unreported>` and passes only
when the expected focus target remains visible in the same AT-SPI tree.

| Anchor | Surface | Role | Expected Name | Owning Workflow | Stability |
| --- | --- | --- | --- | --- | --- |
| <span id="atspi-anchor-window-shell-page-tab-list-open-document-tabs"></span>`atspi-anchor-window-shell-page-tab-list-open-document-tabs` | Window shell | `page tab list` | `Open document tabs` | Tab strip and active document navigation | Stable public accessibility anchor |
| <span id="atspi-anchor-window-shell-toggle-button-toggle-workspace-sidebar"></span>`atspi-anchor-window-shell-toggle-button-toggle-workspace-sidebar` | Window shell | `toggle button` | `Toggle workspace sidebar` | Workspace sidebar visibility | Stable public accessibility anchor |
| <span id="atspi-anchor-window-shell-grouping-document-metadata"></span>`atspi-anchor-window-shell-grouping-document-metadata` | Window shell | `grouping` | `Document metadata` | Status metadata and document properties entry points | Stable public accessibility anchor |
| <span id="atspi-anchor-window-shell-button-new-file"></span>`atspi-anchor-window-shell-button-new-file` | Window shell | `button` | `New file` | New document command | Stable public accessibility anchor |
| <span id="atspi-anchor-window-shell-button-open-file"></span>`atspi-anchor-window-shell-button-open-file` | Window shell | `button` | `Open file` | Open document command | Stable public accessibility anchor |
| <span id="atspi-anchor-window-shell-button-notes-menu"></span>`atspi-anchor-window-shell-button-notes-menu` | Window shell | `button` | `Notes menu` | Notes and bookmarks entry point | Stable public accessibility anchor |
| <span id="atspi-anchor-window-shell-button-main-menu"></span>`atspi-anchor-window-shell-button-main-menu` | Window shell | `button` | `Main menu` | Primary app menu | Stable public accessibility anchor |
| <span id="atspi-anchor-window-shell-toggle-button-toggle-document-properties"></span>`atspi-anchor-window-shell-toggle-button-toggle-document-properties` | Window shell | `toggle button` | `Toggle document properties` | Document properties visibility | Stable public accessibility anchor |
| <span id="atspi-anchor-workspace-sidebar-button-new-workspace"></span>`atspi-anchor-workspace-sidebar-button-new-workspace` | Workspace sidebar | `button` | `New Workspace` | Workspace creation | Stable public accessibility anchor |
| <span id="atspi-anchor-command-palette-entry-command-palette-query"></span>`atspi-anchor-command-palette-entry-command-palette-query` | Command palette | `entry` | `Command palette query` | Command palette search text | Stable public accessibility anchor |
| <span id="atspi-anchor-command-palette-list-command-palette-results"></span>`atspi-anchor-command-palette-list-command-palette-results` | Command palette | `list` | `Command palette results` | Command/file result navigation | Stable public accessibility anchor |
| <span id="atspi-anchor-command-palette-combo-box-files"></span>`atspi-anchor-command-palette-combo-box-files` | Command palette | `combo box` | `Files` | Command palette mode selector selected value | Stable public accessibility anchor with GTK/AT-SPI naming caveat |
| <span id="atspi-anchor-notes-browser-dialog-notes"></span>`atspi-anchor-notes-browser-dialog-notes` | Notes browser | `dialog` | `Notes` | Notes browser shell | Stable public accessibility anchor |
| <span id="atspi-anchor-notes-browser-grouping-no-notes-yet"></span>`atspi-anchor-notes-browser-grouping-no-notes-yet` | Notes browser | `grouping` | `No notes yet` | Notes empty state | Stable public accessibility anchor |
| <span id="atspi-anchor-notes-browser-button-close"></span>`atspi-anchor-notes-browser-button-close` | Notes browser | `button` | `Close` | Notes browser dismissal | Stable public accessibility anchor |
| <span id="atspi-focus-command-palette-command-palette-query"></span>`atspi-focus-command-palette-command-palette-query` | Command palette | focus path | `Command palette query` | Command palette initial focus | Stable focus target; helper accepts visible fallback when headless AT-SPI does not report focus |

## Scenario Manifest Schema

`scripts/run-automation-smoke.sh` and
`scripts/run-crash-recovery-smoke.sh` write `scenario-manifest.json` beside the
assertion artifacts, while `scripts/run-visual-smoke.sh` writes one bounded
`assertions/<capture>-manifest.json` per visual scenario. These manifests are
the review indexes for their scenarios: rich payloads stay in sibling files,
while each manifest records paths, compact status rows, selected environment
details, and failure or skip reasons. The canonical field list is checked
against `scripts/automation-smoke-driver.py`; adding, removing, or renaming a
field requires updating this table and rerunning `make check-automation-docs`.

| Anchor | Field | Meaning |
| --- | --- | --- |
| <span id="scenario-manifest-field-schema-version"></span>`scenario-manifest-field-schema-version` | `schema_version` | Manifest schema version. Increment only when the manifest shape changes incompatibly for artifact consumers. |
| <span id="scenario-manifest-field-scenario-id"></span>`scenario-manifest-field-scenario-id` | `scenario_id` | Stable scenario id, such as `automation-dbus-smoke`, `crash-recovery-smoke`, or `visual-smoke/<capture>`. |
| <span id="scenario-manifest-field-description"></span>`scenario-manifest-field-description` | `description` | Short human-readable scenario purpose. |
| <span id="scenario-manifest-field-status"></span>`scenario-manifest-field-status` | `status` | Current or final scenario status: `running`, `passed`, `failed`, or `skipped`. |
| <span id="scenario-manifest-field-started-at"></span>`scenario-manifest-field-started-at` | `started_at` | UTC timestamp when the manifest was initialized. |
| <span id="scenario-manifest-field-updated-at"></span>`scenario-manifest-field-updated-at` | `updated_at` | UTC timestamp for the latest manifest write. |
| <span id="scenario-manifest-field-finished-at"></span>`scenario-manifest-field-finished-at` | `finished_at` | UTC timestamp when the scenario reached a terminal status, or `null` while running. |
| <span id="scenario-manifest-field-failure-reason"></span>`scenario-manifest-field-failure-reason` | `failure_reason` | Bounded failure detail when the scenario fails. |
| <span id="scenario-manifest-field-skip-reason"></span>`scenario-manifest-field-skip-reason` | `skip_reason` | Bounded host/tooling skip detail when the scenario is skipped. |
| <span id="scenario-manifest-field-launch-mode"></span>`scenario-manifest-field-launch-mode` | `launch_mode` | Scenario launch topology, such as `dbus-run-session+headless-mutter`. |
| <span id="scenario-manifest-field-helper-arguments"></span>`scenario-manifest-field-helper-arguments` | `helper_arguments` | Structured helper arguments, currently artifact directory and binary path. |
| <span id="scenario-manifest-field-fixture-setup"></span>`scenario-manifest-field-fixture-setup` | `fixture_setup` | Fixture rows with name, kind, relative artifact path, and bounded detail. |
| <span id="scenario-manifest-field-actions"></span>`scenario-manifest-field-actions` | `actions` | GTK/GIO action activations with object path, bounded parameters, status, detail, and optional artifact. |
| <span id="scenario-manifest-field-waits"></span>`scenario-manifest-field-waits` | `waits` | Readiness waits with predicate, timeout, ok flag, status, bounded detail, and optional artifact. |
| <span id="scenario-manifest-field-state-assertions"></span>`scenario-manifest-field-state-assertions` | `state_assertions` | State proof rows for snapshot checks, catalog/action agreement, and summary assertions. |
| <span id="scenario-manifest-field-screenshots"></span>`scenario-manifest-field-screenshots` | `screenshots` | Screenshot artifact rows. The D-Bus-only smoke currently leaves this empty. |
| <span id="scenario-manifest-field-at-spi-assertions"></span>`scenario-manifest-field-at-spi-assertions` | `at_spi_assertions` | AT-SPI assertion rows or explicit `not-run` diagnostics when a lane disables AT-SPI. |
| <span id="scenario-manifest-field-dbus-summaries"></span>`scenario-manifest-field-dbus-summaries` | `dbus_summaries` | D-Bus method/property summary rows with member name, kind, status, bounded detail, and artifact. |
| <span id="scenario-manifest-field-warnings"></span>`scenario-manifest-field-warnings` | `warnings` | Runtime warning scan status, unexpected count, bounded detail, and scan artifact path. |
| <span id="scenario-manifest-field-environment"></span>`scenario-manifest-field-environment` | `environment` | Selected non-secret runtime context such as app id, object path, binary, virtual monitor, GSettings, renderer, portal flag, and isolated XDG paths. |
| <span id="scenario-manifest-field-bounded-artifact-policy"></span>`scenario-manifest-field-bounded-artifact-policy` | `bounded_artifact_policy` | Embedded text cap and rule that large payloads stay in bounded sibling artifacts. |
| <span id="scenario-manifest-field-steps"></span>`scenario-manifest-field-steps` | `steps` | Ordered command, wait, D-Bus, launch, warning-scan, and state-assertion step rows with timing, status, bounded detail, and artifacts. |

## Workflow Event Schema

`GetWorkflowEvents` returns serialized `AutomationWorkflowEventsSnapshot` JSON.
The event log is bounded to the most recent transitions. Workflow events are
diagnostic state-change records derived from the same readiness blockers used
by `WaitForReady`; they are not a command channel and do not include document
contents. Current stable workflow IDs include `file-load`, `save`, `search`,
`workspace-refresh`, `content-search`, `replace-preview`, `session-restore`,
while recovery restore remains a readiness predicate until a dedicated
recovery-specific event source exists.

| Anchor | Field | Type | Meaning |
| --- | --- | --- | --- |
| <span id="workflow-event-field-last-sequence"></span>`workflow-event-field-last-sequence` | `last_sequence` | `u64` | Highest event sequence emitted by this process; `0` means no event has been emitted yet. |
| <span id="workflow-event-field-capped"></span>`workflow-event-field-capped` | `capped` | `bool` | Whether older events were ever dropped from the bounded list; gaps before the first retained sequence are expected after this becomes true. |
| <span id="workflow-event-field-events"></span>`workflow-event-field-events` | `events` | `array` | Recent workflow events in sequence order. |
| <span id="workflow-event-field-sequence"></span>`workflow-event-field-sequence` | `events[].sequence` | `u64` | Monotonic per-process event sequence. |
| <span id="workflow-event-field-workflow-id"></span>`workflow-event-field-workflow-id` | `events[].workflow_id` | `string` | Stable workflow ID. |
| <span id="workflow-event-field-phase"></span>`workflow-event-field-phase` | `events[].phase` | `string` | `started` or `finished`. |
| <span id="workflow-event-field-status"></span>`workflow-event-field-status` | `events[].status` | `string` | `running` for start events or `settled` for finish events. |
| <span id="workflow-event-field-summary"></span>`workflow-event-field-summary` | `events[].summary` | `string` | Bounded human-readable summary for smoke artifacts. |
| <span id="workflow-event-field-blocker"></span>`workflow-event-field-blocker` | `events[].blocker` | `string?` | Readiness blocker associated with the transition, if known. |

## Snapshot Schema

`GetSnapshot` returns serialized `AutomationSnapshot` JSON. All fields are
read-only observations. Paths may appear for file-backed tabs because they are
already visible in the application UI. Buffer text and private persistence
tokens must not appear. `workspace.scope_workspace_id` is an allowed stable
automation identity for the visible workspace selector; draft IDs, note IDs,
bookmark IDs, local-history snapshot IDs, and sidecar identity keys remain
private. Free-form text fields are capped to 4 KiB of UTF-8 and
receive a ` [truncated]` suffix when shortened.

| Anchor | Field | Type | Meaning |
| --- | --- | --- | --- |
| <span id="snapshot-field-interface-version"></span>`snapshot-field-interface-version` | `interface_version` | `u32` | Version of the automation interface that produced the snapshot. |
| <span id="snapshot-field-enabled"></span>`snapshot-field-enabled` | `enabled` | `bool` | Whether this process has the automation object active. |
| <span id="snapshot-field-app-id"></span>`snapshot-field-app-id` | `app_id` | `string` | Application ID that owns the D-Bus name. |
| <span id="snapshot-field-app-version"></span>`snapshot-field-app-version` | `app_version` | `string` | LushText build version. |
| <span id="snapshot-field-build-profile"></span>`snapshot-field-build-profile` | `build_profile` | `string` | Build profile used for diagnostics. |
| <span id="snapshot-field-idle"></span>`snapshot-field-idle` | `idle` | `bool` | `true` when no tracked app-owned workflow blocker is active. |
| <span id="snapshot-field-idle-blocker"></span>`snapshot-field-idle-blocker` | `idle_blocker` | `string?` | First tracked blocker while `idle` is `false`. |
| <span id="snapshot-field-window"></span>`snapshot-field-window` | `window` | `object?` | Active LushText window snapshot, if one exists. |
| <span id="snapshot-field-tab-count"></span>`snapshot-field-tab-count` | `window.tab_count` | `u32` | Number of open editor tabs. |
| <span id="snapshot-field-active-tab-index"></span>`snapshot-field-active-tab-index` | `window.active_tab_index` | `u32?` | Selected tab index. |
| <span id="snapshot-field-tabs"></span>`snapshot-field-tabs` | `window.tabs` | `array` | Non-content metadata for every tab. |
| <span id="snapshot-field-surfaces"></span>`snapshot-field-surfaces` | `window.surfaces` | `object` | Shell and secondary-surface state. |
| <span id="snapshot-field-search"></span>`snapshot-field-search` | `window.search` | `object` | In-document and workspace-search state. |
| <span id="snapshot-field-index"></span>`snapshot-field-index` | `tabs[].index` | `u32` | Zero-based tab index. |
| <span id="snapshot-field-active"></span>`snapshot-field-active` | `tabs[].active` | `bool` | Whether this tab is selected. |
| <span id="snapshot-field-title"></span>`snapshot-field-title` | `tabs[].title` | `string` | Display title shown in the tab strip. |
| <span id="snapshot-field-document-kind"></span>`snapshot-field-document-kind` | `tabs[].document_kind` | `string` | `file` or `untitled`. |
| <span id="snapshot-field-path"></span>`snapshot-field-path` | `tabs[].path` | `string?` | File-backed tab path, if present. |
| <span id="snapshot-field-modified"></span>`snapshot-field-modified` | `tabs[].modified` | `bool` | Whether the buffer has unsaved edits. |
| <span id="snapshot-field-saving"></span>`snapshot-field-saving` | `tabs[].saving` | `bool` | Whether a save is currently in flight. |
| <span id="snapshot-field-load-state"></span>`snapshot-field-load-state` | `tabs[].load_state` | `string` | `untitled`, `loading`, `loaded`, `failed`, or `unknown`. |
| <span id="snapshot-field-file-size"></span>`snapshot-field-file-size` | `tabs[].file_size` | `u64?` | On-disk file size when known. |
| <span id="snapshot-field-draft-present"></span>`snapshot-field-draft-present` | `tabs[].draft_present` | `bool` | Whether the tab has draft identity, without exposing the draft ID. |
| <span id="snapshot-field-evicted"></span>`snapshot-field-evicted` | `tabs[].evicted` | `bool` | Whether the tab buffer has been evicted for memory pressure. |
| <span id="snapshot-field-pinned"></span>`snapshot-field-pinned` | `tabs[].pinned` | `bool` | Whether the tab is pinned. |
| <span id="snapshot-field-workspace-sidebar-visible"></span>`snapshot-field-workspace-sidebar-visible` | `surfaces.workspace_sidebar_visible` | `bool` | Rendered workspace sidebar visibility. |
| <span id="snapshot-field-workspace-sidebar-requested"></span>`snapshot-field-workspace-sidebar-requested` | `surfaces.workspace_sidebar_requested` | `bool` | User-requested workspace sidebar visibility. |
| <span id="snapshot-field-document-properties-visible"></span>`snapshot-field-document-properties-visible` | `surfaces.document_properties_visible` | `bool` | Rendered document-properties visibility. |
| <span id="snapshot-field-document-properties-requested"></span>`snapshot-field-document-properties-requested` | `surfaces.document_properties_requested` | `bool` | User-requested document-properties visibility. |
| <span id="snapshot-field-compact-surface"></span>`snapshot-field-compact-surface` | `surfaces.compact_surface` | `string?` | Compact layout slot owner, if any. |
| <span id="snapshot-field-command-palette-visible"></span>`snapshot-field-command-palette-visible` | `surfaces.command_palette_visible` | `bool` | Command palette revealer state. |
| <span id="snapshot-field-search-panel-visible"></span>`snapshot-field-search-panel-visible` | `surfaces.search_panel_visible` | `bool` | Workspace search panel revealer state. |
| <span id="snapshot-field-preview-pane-visible"></span>`snapshot-field-preview-pane-visible` | `surfaces.preview_pane_visible` | `bool` | Side-by-side Markdown preview pane state. |
| <span id="snapshot-field-preview-mode"></span>`snapshot-field-preview-mode` | `surfaces.preview_mode` | `bool` | Preview-only Markdown mode state. |
| <span id="snapshot-field-focus-mode"></span>`snapshot-field-focus-mode` | `surfaces.focus_mode` | `bool` | Focus Mode state. |
| <span id="snapshot-field-minimap-requested"></span>`snapshot-field-minimap-requested` | `surfaces.minimap_requested` | `bool` | Minimap preference state; document policy may suppress rendering. |
| <span id="snapshot-field-status-bar-visible"></span>`snapshot-field-status-bar-visible` | `surfaces.status_bar_visible` | `bool` | Status bar widget visibility. |
| <span id="snapshot-field-active-transient-surface"></span>`snapshot-field-active-transient-surface` | `surfaces.active_transient_surface` | `string?` | Topmost shell-owned transient surface known to automation. |
| <span id="snapshot-field-editor-search-visible"></span>`snapshot-field-editor-search-visible` | `search.editor_search_visible` | `bool` | Selected editor search bar visibility. |
| <span id="snapshot-field-editor-query"></span>`snapshot-field-editor-query` | `search.editor_query` | `string?` | Selected editor query when its search UI is visible. |
| <span id="snapshot-field-editor-match-count"></span>`snapshot-field-editor-match-count` | `search.editor_match_count` | `i32?` | Selected editor occurrence count, when available. |
| <span id="snapshot-field-workspace-search-visible"></span>`snapshot-field-workspace-search-visible` | `search.workspace_search_visible` | `bool` | Workspace search panel visibility. |
| <span id="snapshot-field-workspace-query"></span>`snapshot-field-workspace-query` | `search.workspace_query` | `string` | Current workspace search query. |
| <span id="snapshot-field-workspace-searching"></span>`snapshot-field-workspace-searching` | `search.workspace_searching` | `bool` | Whether workspace search is currently running. |
| <span id="snapshot-field-workspace-match-count"></span>`snapshot-field-workspace-match-count` | `search.workspace_match_count` | `u32` | Total workspace matches accumulated for the current query. |
| <span id="snapshot-field-workspace-file-count"></span>`snapshot-field-workspace-file-count` | `search.workspace_file_count` | `u32` | Number of files with workspace matches. |
| <span id="snapshot-field-workspace-result-capped"></span>`snapshot-field-workspace-result-capped` | `search.workspace_result_capped` | `bool` | Whether the workspace search result cap was reached. |
| <span id="snapshot-field-workspace"></span>`snapshot-field-workspace` | `window.workspace` | `object` | Workspace configuration and current scope state, without scanning the filesystem. |
| <span id="snapshot-field-command-palette"></span>`snapshot-field-command-palette` | `window.command_palette` | `object` | Command palette visibility, mode, query, and index counters without result row text. |
| <span id="snapshot-field-notes"></span>`snapshot-field-notes` | `window.notes` | `object` | Notes and bookmark state already live in the window, without sidecar reads or note bodies. |
| <span id="snapshot-field-local-history"></span>`snapshot-field-local-history` | `window.local_history` | `object` | Local-history availability state that can be answered from active editor policy. |
| <span id="snapshot-field-content-search"></span>`snapshot-field-content-search` | `window.content_search` | `object` | Workspace content-search and Replace All state summaries, without match bodies or file content. |
| <span id="snapshot-field-notifications"></span>`snapshot-field-notifications` | `window.notifications` | `object` | Status/progress notification summary for assertions. |
| <span id="snapshot-field-scope-kind"></span>`snapshot-field-scope-kind` | `workspace.scope_kind` | `string` | Current workspace scope kind: `all` or `workspace`. |
| <span id="snapshot-field-scope-workspace-id"></span>`snapshot-field-scope-workspace-id` | `workspace.scope_workspace_id` | `string?` | Stable automation identity for the selected visible workspace scope. |
| <span id="snapshot-field-scope-workspace-name"></span>`snapshot-field-scope-workspace-name` | `workspace.scope_workspace_name` | `string?` | User-visible selected workspace name, if any. |
| <span id="snapshot-field-workspace-count"></span>`snapshot-field-workspace-count` | `workspace.workspace_count` | `u32` | Total persisted workspace count. |
| <span id="snapshot-field-folder-count"></span>`snapshot-field-folder-count` | `workspace.folder_count` | `u32` | Total configured folder memberships across all workspaces. |
| <span id="snapshot-field-scoped-folder-count"></span>`snapshot-field-scoped-folder-count` | `workspace.scoped_folder_count` | `u32` | Folder memberships covered by the current scope. |
| <span id="snapshot-field-no-workspaces"></span>`snapshot-field-no-workspaces` | `workspace.no_workspaces` | `bool` | Whether no persisted workspaces exist. |
| <span id="snapshot-field-persistence-inflight"></span>`snapshot-field-persistence-inflight` | `workspace.persistence_inflight` | `bool` | Whether the sidebar is writing workspace state in the background. |
| <span id="snapshot-field-persistence-dirty"></span>`snapshot-field-persistence-dirty` | `workspace.persistence_dirty` | `bool` | Whether another workspace save is pending after the in-flight write. |
| <span id="snapshot-field-filter-animation-active"></span>`snapshot-field-filter-animation-active` | `workspace.filter_animation_active` | `bool` | Whether workspace filter animation is active. |
| <span id="snapshot-field-visible"></span>`snapshot-field-visible` | `command_palette.visible`, `content_search.visible` | `bool` | Whether the palette or workspace-search panel is currently revealed. |
| <span id="snapshot-field-query"></span>`snapshot-field-query` | `command_palette.query`, `content_search.query` | `string` | Current query text for the palette or workspace-search panel. |
| <span id="snapshot-field-mode"></span>`snapshot-field-mode` | `command_palette.mode` | `string` | Current palette mode: `all`, `files`, `notes`, or `commands`. |
| <span id="snapshot-field-result-count"></span>`snapshot-field-result-count` | `command_palette.result_count` | `u32` | Rendered palette row count, including section headers. |
| <span id="snapshot-field-file-index-count"></span>`snapshot-field-file-index-count` | `command_palette.file_index_count` | `u32` | Number of indexed workspace files known to the palette. |
| <span id="snapshot-field-open-tab-source-count"></span>`snapshot-field-open-tab-source-count` | `command_palette.open_tab_source_count` | `u32` | Number of open file-backed tabs supplied as palette sources. |
| <span id="snapshot-field-pending-index-update-count"></span>`snapshot-field-pending-index-update-count` | `command_palette.pending_index_update_count` | `u32` | Queued file-index mutations waiting for debounce flush. |
| <span id="snapshot-field-notes-menu-open"></span>`snapshot-field-notes-menu-open` | `notes.notes_menu_open` | `bool` | Whether the notes menu popover is currently open. |
| <span id="snapshot-field-active-document-file-backed"></span>`snapshot-field-active-document-file-backed` | `notes.active_document_file_backed`, `local_history.active_document_file_backed` | `bool` | Whether the active document is file-backed for notes, bookmarks, or local history. |
| <span id="snapshot-field-active-document-bookmark-count"></span>`snapshot-field-active-document-bookmark-count` | `notes.active_document_bookmark_count` | `u32` | Live bookmark count for the active editor tab. |
| <span id="snapshot-field-active-line-has-bookmark"></span>`snapshot-field-active-line-has-bookmark` | `notes.active_line_has_bookmark` | `bool` | Whether the active cursor line has a bookmark. |
| <span id="snapshot-field-document-note-available"></span>`snapshot-field-document-note-available` | `notes.document_note_available` | `bool` | Whether the active document can open the document-note workflow. |
| <span id="snapshot-field-folder-note-available"></span>`snapshot-field-folder-note-available` | `notes.folder_note_available` | `bool` | Whether a folder-note action is meaningful for the current workspace scope. |
| <span id="snapshot-field-browse-available"></span>`snapshot-field-browse-available` | `local_history.browse_available` | `bool` | Whether the active document can browse local history. |
| <span id="snapshot-field-automatic-capture-available"></span>`snapshot-field-automatic-capture-available` | `local_history.automatic_capture_available` | `bool` | Whether the active document can capture automatic local-history snapshots. |
| <span id="snapshot-field-availability"></span>`snapshot-field-availability` | `local_history.availability` | `string` | Size-policy classification for the active document: `full`, `save-only`, or `unavailable`. |
| <span id="snapshot-field-regex-enabled"></span>`snapshot-field-regex-enabled` | `content_search.regex_enabled` | `bool` | Whether workspace search regex mode is enabled. |
| <span id="snapshot-field-case-sensitive"></span>`snapshot-field-case-sensitive` | `content_search.case_sensitive` | `bool` | Whether workspace search case-sensitive mode is enabled. |
| <span id="snapshot-field-whole-word-enabled"></span>`snapshot-field-whole-word-enabled` | `content_search.whole_word_enabled` | `bool` | Whether workspace search whole-word mode is enabled. |
| <span id="snapshot-field-gitignore-enabled"></span>`snapshot-field-gitignore-enabled` | `content_search.gitignore_enabled` | `bool` | Whether `.gitignore` filtering is enabled. |
| <span id="snapshot-field-glob-filter"></span>`snapshot-field-glob-filter` | `content_search.glob_filter` | `string?` | Current glob filter text when present. |
| <span id="snapshot-field-searching"></span>`snapshot-field-searching` | `content_search.searching` | `bool` | Whether a workspace search worker is currently running. |
| <span id="snapshot-field-file-count"></span>`snapshot-field-file-count` | `content_search.file_count` | `u32` | Number of files with matches in the current workspace search summary. |
| <span id="snapshot-field-match-count"></span>`snapshot-field-match-count` | `content_search.match_count` | `u32` | Total match count in the current workspace search summary. |
| <span id="snapshot-field-result-capped"></span>`snapshot-field-result-capped` | `content_search.result_capped` | `bool` | Whether the workspace search result cap was reached. |
| <span id="snapshot-field-replace-query"></span>`snapshot-field-replace-query` | `content_search.replace_query` | `string` | Current replacement text. |
| <span id="snapshot-field-replace-preview-mode"></span>`snapshot-field-replace-preview-mode` | `content_search.replace_preview_mode` | `bool` | Whether Replace All preview rows are visible. |
| <span id="snapshot-field-replace-preview-pending"></span>`snapshot-field-replace-preview-pending` | `content_search.replace_preview_pending` | `bool` | Whether Replace All preview generation is pending. |
| <span id="snapshot-field-replace-preview-count"></span>`snapshot-field-replace-preview-count` | `content_search.replace_preview_count` | `u32` | Number of generated replacement preview rows. |
| <span id="snapshot-field-checked-replacement-count"></span>`snapshot-field-checked-replacement-count` | `content_search.checked_replacement_count` | `u32` | Number of checked replacement preview rows. |
| <span id="snapshot-field-has-undo-backup"></span>`snapshot-field-has-undo-backup` | `content_search.has_undo_backup` | `bool` | Whether a Replace All undo backup is available. |
| <span id="snapshot-field-history-count"></span>`snapshot-field-history-count` | `content_search.history_count` | `u32` | Number of recent history rows loaded into the workspace search panel. |
| <span id="snapshot-field-saved-search-count"></span>`snapshot-field-saved-search-count` | `content_search.saved_search_count` | `u32` | Number of named saved searches loaded into the workspace search panel. |
| <span id="snapshot-field-navigation-match-count"></span>`snapshot-field-navigation-match-count` | `content_search.navigation_match_count` | `u32` | Number of flat match navigation targets. |
| <span id="snapshot-field-current-navigation-match-index"></span>`snapshot-field-current-navigation-match-index` | `content_search.current_navigation_match_index` | `u32?` | Current flat match navigation index, if any. |
| <span id="snapshot-field-status-text"></span>`snapshot-field-status-text` | `notifications.status_text` | `string?` | Current visible status-bar message text, if any. |
| <span id="snapshot-field-status-severity"></span>`snapshot-field-status-severity` | `notifications.status_severity` | `string?` | Current visible status-bar severity: `info`, `warning`, or `error`. |
| <span id="snapshot-field-generation"></span>`snapshot-field-generation` | `notifications.generation` | `u64` | Notification-bus generation for detecting visible-view changes. |
| <span id="snapshot-field-search-progress-visible"></span>`snapshot-field-search-progress-visible` | `notifications.search_progress_visible` | `bool` | Whether delayed workspace-search progress is allowed to render. |

## Readiness Predicates

`GetReadinessPredicates` returns these rows as JSON. `WaitForReady` accepts the
`Predicate` value and waits until every listed blocker is absent. Use the
narrowest predicate that matches the workflow under test; use `idle` only when
a scenario truly needs all tracked app-owned work to settle.

| Anchor | Predicate | Stability | Blockers | Meaning |
| --- | --- | --- | --- | --- |
| <span id="readiness-predicate-app-startup"></span>`readiness-predicate-app-startup` | `app-startup` | stable | `app-startup`, `session-restore`, `file-load`, `draft-autosave`, `command-palette-index`, `workspace-persist`, `workspace-filter-animation` | Application startup has produced an active window and settled startup-owned restore work. |
| <span id="readiness-predicate-window-actions-exported"></span>`readiness-predicate-window-actions-exported` | `window-actions-exported` | stable | `app-startup` | The active window exists; smoke helpers still probe its `org.gtk.Actions` object externally before treating bus export as proven. |
| <span id="readiness-predicate-file-open-complete"></span>`readiness-predicate-file-open-complete` | `file-open-complete` | stable | `app-startup`, `file-load` | File-backed editor tabs are no longer loading. A failed load reports `workflow-failure` instead of readiness. |
| <span id="readiness-predicate-search-complete"></span>`readiness-predicate-search-complete` | `search-complete` | stable | `app-startup`, `editor-search`, `workspace-search`, `replace-preview` | Editor search, workspace search, and Replace All preview work are no longer pending. |
| <span id="readiness-predicate-save-complete"></span>`readiness-predicate-save-complete` | `save-complete` | stable | `app-startup`, `save`, `close-safety`, `draft-autosave` | Editor saves, close-safety checks, and draft autosaves are no longer pending. |
| <span id="readiness-predicate-workspace-refresh-complete"></span>`readiness-predicate-workspace-refresh-complete` | `workspace-refresh-complete` | stable | `app-startup`, `workspace-persist`, `workspace-filter-animation`, `command-palette-index` | Workspace persistence, scope filter animation, and command-palette index debounce are settled. |
| <span id="readiness-predicate-session-restore-complete"></span>`readiness-predicate-session-restore-complete` | `session-restore-complete` | stable | `app-startup`, `session-restore`, `file-load`, `draft-autosave` | Session restore and immediate file/draft follow-up work are settled. |
| <span id="readiness-predicate-recovery-restore-complete"></span>`readiness-predicate-recovery-restore-complete` | `recovery-restore-complete` | stable | `app-startup`, `session-restore`, `file-load`, `draft-autosave`, `workspace-persist`, `command-palette-index` | Startup recovery restore and immediate post-restore indexing or persistence work are settled. |
| <span id="readiness-predicate-idle"></span>`readiness-predicate-idle` | `idle` | stable | all readiness blockers | Every tracked app-owned readiness blocker is settled. |

## Readiness Blockers

`WaitForIdle` is the compatibility alias for `WaitForReady("idle", timeout)`.
`GetSnapshot.idle` and `GetSnapshot.idle_blocker` use the same blocker set.

| Anchor | Blocker | Meaning |
| --- | --- | --- |
| <span id="readiness-app-startup"></span>`readiness-app-startup` | `app-startup` | The application has not produced an active LushText window yet. |
| <span id="readiness-close-safety"></span>`readiness-close-safety` | `close-safety` | A close/quit safety flow is still resolving modified or saving documents. |
| <span id="readiness-command-palette-index"></span>`readiness-command-palette-index` | `command-palette-index` | Command palette file-index mutations are still waiting for debounce flush. |
| <span id="readiness-draft-autosave"></span>`readiness-draft-autosave` | `draft-autosave` | A draft autosave is in flight. |
| <span id="readiness-editor-search"></span>`readiness-editor-search` | `editor-search` | The selected editor search context has not finished counting occurrences. |
| <span id="readiness-file-load"></span>`readiness-file-load` | `file-load` | At least one editor tab is still loading file contents. |
| <span id="readiness-preview-animation"></span>`readiness-preview-animation` | `preview-animation` | Preview pane animation is active. |
| <span id="readiness-replace-preview"></span>`readiness-replace-preview` | `replace-preview` | Replace All preview generation is still running. |
| <span id="readiness-save"></span>`readiness-save` | `save` | At least one editor tab has a save in flight. |
| <span id="readiness-session-restore"></span>`readiness-session-restore` | `session-restore` | Startup session/draft restoration is still active. |
| <span id="readiness-workspace-filter-animation"></span>`readiness-workspace-filter-animation` | `workspace-filter-animation` | Workspace scope/filter animation is still reconciling visible sections. |
| <span id="readiness-workspace-persist"></span>`readiness-workspace-persist` | `workspace-persist` | Workspace state persistence is in flight or queued. |
| <span id="readiness-workspace-search"></span>`readiness-workspace-search` | `workspace-search` | Workspace search is still running. |

## Exposure Vocabulary

`exported` means the action is registered on the app or window and can be
activated through `org.gtk.Actions`. `widget-scoped` means GTK resolves the
action from a local action group, usually a context menu or search-options menu.
`visible-unregistered-gap` documents a visible command that still lacks a
registered action and should not be used for automation.

`stable-user-command` actions are appropriate for same-user automation when the
documented enablement rule is satisfied. `contextual-user-command` actions are
normal user operations but depend on active document, selected row, dialog, or
menu context. `diagnostic-only` actions exist to prepare or inspect states that
are useful for tests but are not primary user commands. `unsupported-gap` rows
are explicit TODOs, not supported automation.

## Action Catalog

The action table is the user and developer reference for what LushText exposes
or intentionally tracks as a gap. The `Action` column uses the GTK action id
spelling. `Param` and `State` use catalog value kinds; the equivalent GVariant
signatures are `bool -> b`, `string -> s`, `u32 -> u`, and
`variant-map -> a{sv}`.

| Anchor | Action | Label | Param | State | Exposure | Safety | Owner | Surfaces | Enablement | Coverage |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| <span id="action-app-preferences"></span>`action-app-preferences` | `app.preferences` | Preferences | `none` | `none` | `exported` | `contextual-user-command` | `app` | primary-menu, command-palette, dbus-action | Requires an active window. | unit |
| <span id="action-app-quit"></span>`action-app-quit` | `app.quit` | Quit | `none` | `none` | `exported` | `contextual-user-command` | `app` | command-palette, dbus-action | Always registered; close flows still own save/modified safety. | unit |
| <span id="action-app-about"></span>`action-app-about` | `app.about` | About LushText | `none` | `none` | `exported` | `contextual-user-command` | `app` | primary-menu, command-palette, dbus-action | Requires an active window. | unit |
| <span id="action-win-new-tab"></span>`action-win-new-tab` | `win.new-tab` | New File | `none` | `none` | `exported` | `stable-user-command` | `window/actions` | header-button, primary-menu, keyboard-shortcut, command-palette, dbus-action | Always enabled. | unit, widget |
| <span id="action-win-open-file"></span>`action-win-open-file` | `win.open-file` | Open File | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | header-button, command-palette, dbus-action | Always enabled; opens the normal file dialog. | unit, widget |
| <span id="action-win-open-folder"></span>`action-win-open-folder` | `win.open-folder` | Open Folder | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | command-palette, dbus-action | Always enabled; opens the normal workspace folder flow. | unit |
| <span id="action-win-save"></span>`action-win-save` | `win.save` | Save | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | primary-menu, keyboard-shortcut, command-palette, dbus-action | Requires an active tab. | unit, widget |
| <span id="action-win-save-as"></span>`action-win-save-as` | `win.save-as` | Save As | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | primary-menu, keyboard-shortcut, command-palette, dbus-action | Requires an active tab. | unit, widget |
| <span id="action-win-show-local-history"></span>`action-win-show-local-history` | `win.show-local-history` | Local History | `none` | `none` | `exported` | `contextual-user-command` | `window/local_history` | primary-menu, editor-context-menu, keyboard-shortcut, command-palette, dbus-action | Requires a saved active document with local-history browsing available. | unit, widget |
| <span id="action-win-show-encoding-controls"></span>`action-win-show-encoding-controls` | `win.show-encoding-controls` | Text Encoding | `none` | `none` | `exported` | `contextual-user-command` | `window/encoding` | status-bar, dbus-action | Requires an active document when the status-bar control is useful. | unit, widget |
| <span id="action-win-show-line-ending-controls"></span>`action-win-show-line-ending-controls` | `win.show-line-ending-controls` | Line Endings | `none` | `none` | `exported` | `contextual-user-command` | `window/encoding` | status-bar, dbus-action | Requires an active document when the status-bar control is useful. | unit, widget |
| <span id="action-win-show-file-health"></span>`action-win-show-file-health` | `win.show-file-health` | File Health | `none` | `none` | `exported` | `contextual-user-command` | `window/encoding` | properties-panel, dbus-action | Requires an active document with inspectable file-health state. | unit, widget |
| <span id="action-win-cycle-invisible-characters"></span>`action-win-cycle-invisible-characters` | `win.cycle-invisible-characters` | Cycle Invisible Characters | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | keyboard-shortcut, dbus-action | Requires an active tab. | unit, widget |
| <span id="action-win-begin-search"></span>`action-win-begin-search` | `win.begin-search` | Find and Replace | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | primary-menu, keyboard-shortcut, command-palette, dbus-action | Requires an active tab. | unit, widget |
| <span id="action-win-set-search-query"></span>`action-win-set-search-query` | `win.set-search-query` | Set In-Document Search Query | `string` | `none` | `exported` | `contextual-user-command` | `window/actions` | dbus-action | Requires an active tab. | unit, widget |
| <span id="action-win-begin-replace"></span>`action-win-begin-replace` | `win.begin-replace` | Begin Replace | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | keyboard-shortcut, dbus-action | Requires an active tab. | unit |
| <span id="action-win-next-match"></span>`action-win-next-match` | `win.next-match` | Next In-Document Match | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | keyboard-shortcut, dbus-action | Requires the in-document search UI to be visible. | unit |
| <span id="action-win-prev-match"></span>`action-win-prev-match` | `win.prev-match` | Previous In-Document Match | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | keyboard-shortcut, dbus-action | Requires the in-document search UI to be visible. | unit |
| <span id="action-win-close-tab"></span>`action-win-close-tab` | `win.close-tab` | Close Tab | `none` | `none` | `exported` | `contextual-user-command` | `window/actions` | keyboard-shortcut, command-palette, dbus-action | Requires an active tab; normal close safety still applies. | unit, widget |
| <span id="action-win-select-tab"></span>`action-win-select-tab` | `win.select-tab` | Select Tab | `u32` | `none` | `exported` | `contextual-user-command` | `window/actions` | dbus-action | Requires an active tab; out-of-range indices leave the active tab unchanged. | unit, widget |
| <span id="action-win-toggle-command-palette"></span>`action-win-toggle-command-palette` | `win.toggle-command-palette` | Command Palette | `none` | `none` | `exported` | `stable-user-command` | `window/focus_indexing` | keyboard-shortcut, dbus-action | Always enabled. | unit, widget |
| <span id="action-win-set-command-palette-query"></span>`action-win-set-command-palette-query` | `win.set-command-palette-query` | Set Command Palette Query | `string` | `none` | `exported` | `contextual-user-command` | `window/focus_indexing` | dbus-action | Requires a visible command palette. | unit, widget |
| <span id="action-win-set-command-palette-mode"></span>`action-win-set-command-palette-mode` | `win.set-command-palette-mode` | Set Command Palette Mode | `string` | `none` | `exported` | `contextual-user-command` | `window/focus_indexing` | dbus-action | Requires a visible command palette; accepts all, files, notes, or commands. | unit, widget |
| <span id="action-win-toggle-search-panel"></span>`action-win-toggle-search-panel` | `win.toggle-search-panel` | Workspace Search | `none` | `none` | `exported` | `stable-user-command` | `window/search` | keyboard-shortcut, dbus-action | Always enabled; the panel owns empty/no-workspace states. | unit, widget |
| <span id="action-win-set-search-panel-visible"></span>`action-win-set-search-panel-visible` | `win.set-search-panel-visible` | Set Workspace Search Visibility | `bool` | `none` | `exported` | `stable-user-command` | `window/actions` | dbus-action | Always enabled; follows the same focus and transition path as the workspace search toggle. | unit, widget |
| <span id="action-win-search-next-match"></span>`action-win-search-next-match` | `win.search-next-match` | Next Workspace Search Match | `none` | `none` | `exported` | `contextual-user-command` | `window/search` | keyboard-shortcut, dbus-action | Requires visible workspace search results. | unit, widget |
| <span id="action-win-search-prev-match"></span>`action-win-search-prev-match` | `win.search-prev-match` | Previous Workspace Search Match | `none` | `none` | `exported` | `contextual-user-command` | `window/search` | keyboard-shortcut, dbus-action | Requires visible workspace search results. | unit, widget |
| <span id="action-win-toggle-bookmark"></span>`action-win-toggle-bookmark` | `win.toggle-bookmark` | Toggle Bookmark | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | editor-context-menu, keyboard-shortcut, command-palette, dbus-action | Requires a saved active document. | unit, widget |
| <span id="action-win-notes-toggle-bookmark"></span>`action-win-notes-toggle-bookmark` | `win.notes-toggle-bookmark` | Add or Remove Bookmark | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | notes-menu, dbus-action | Requires a saved active document. | unit, widget |
| <span id="action-win-edit-bookmark-label"></span>`action-win-edit-bookmark-label` | `win.edit-bookmark-label` | Edit Bookmark | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | editor-context-menu, keyboard-shortcut, command-palette, dbus-action | Requires a saved active document with a bookmark at the cursor. | unit, widget |
| <span id="action-win-next-bookmark"></span>`action-win-next-bookmark` | `win.next-bookmark` | Next Bookmark | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | keyboard-shortcut, command-palette, dbus-action | Requires a saved active document with navigable bookmarks. | unit, widget |
| <span id="action-win-prev-bookmark"></span>`action-win-prev-bookmark` | `win.prev-bookmark` | Previous Bookmark | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | keyboard-shortcut, command-palette, dbus-action | Requires a saved active document with navigable bookmarks. | unit, widget |
| <span id="action-win-show-bookmarks"></span>`action-win-show-bookmarks` | `win.show-bookmarks` | Browse Bookmarks | `none` | `none` | `exported` | `stable-user-command` | `window/notes` | command-palette, dbus-action | Always enabled; dialog owns empty states. | unit, widget |
| <span id="action-win-open-document-note"></span>`action-win-open-document-note` | `win.open-document-note` | Open Document Note | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | editor-context-menu, command-palette, dbus-action | Requires a saved active document. | unit, widget |
| <span id="action-win-notes-open-document-note"></span>`action-win-notes-open-document-note` | `win.notes-open-document-note` | Open Document Note | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | notes-menu, dbus-action | Requires a saved active document. | unit, widget |
| <span id="action-win-open-folder-note"></span>`action-win-open-folder-note` | `win.open-folder-note` | Open Folder Note | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | command-palette, dbus-action | Requires a current workspace folder context. | unit, widget |
| <span id="action-win-notes-open-folder-note"></span>`action-win-notes-open-folder-note` | `win.notes-open-folder-note` | Open Folder Note | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | notes-menu, dbus-action | Requires a current workspace folder context. | unit, widget |
| <span id="action-win-show-notes"></span>`action-win-show-notes` | `win.show-notes` | Browse Notes | `none` | `none` | `exported` | `stable-user-command` | `window/notes` | command-palette, dbus-action | Always enabled; browser owns empty states. | unit, widget |
| <span id="action-win-notes-show-notes"></span>`action-win-notes-show-notes` | `win.notes-show-notes` | Browse Notes | `none` | `none` | `exported` | `stable-user-command` | `window/notes` | notes-menu, dbus-action | Always enabled; browser owns empty states. | unit, widget |
| <span id="action-win-set-notes-browser-query"></span>`action-win-set-notes-browser-query` | `win.set-notes-browser-query` | Set Notes Browser Query | `string` | `none` | `exported` | `contextual-user-command` | `window/notes` | dbus-action | Requires a visible Browse Notes dialog. | unit, widget |
| <span id="action-win-select-notes-browser-row"></span>`action-win-select-notes-browser-row` | `win.select-notes-browser-row` | Select Notes Browser Row | `u32` | `none` | `exported` | `contextual-user-command` | `window/notes` | dbus-action | Requires a visible Browse Notes dialog and a visible zero-based row index. | unit, widget |
| <span id="action-win-open-notes-browser-selection"></span>`action-win-open-notes-browser-selection` | `win.open-notes-browser-selection` | Open Selected Notes Browser Row | `none` | `none` | `exported` | `contextual-user-command` | `window/notes` | dbus-action | Requires a visible Browse Notes dialog with a selected row. | unit, widget |
| <span id="action-win-discard-changes"></span>`action-win-discard-changes` | `win.discard-changes` | Discard Changes | `none` | `none` | `exported` | `contextual-user-command` | `window/documents` | primary-menu, dbus-action | Requires a modified file-backed active document; confirmation still applies. | unit, widget |
| <span id="action-win-toggle-sidebar"></span>`action-win-toggle-sidebar` | `win.toggle-sidebar` | Toggle Sidebar | `none` | `bool` | `exported` | `stable-user-command` | `window/actions` | status-bar, command-palette, dbus-action | Always enabled; compact layout may render the requested state differently. | unit, widget |
| <span id="action-win-set-sidebar-visible"></span>`action-win-set-sidebar-visible` | `win.set-sidebar-visible` | Set Sidebar Visibility | `bool` | `none` | `exported` | `stable-user-command` | `window/actions` | dbus-action | Always enabled; compact layout may render the requested state differently. | unit, widget |
| <span id="action-win-toggle-properties"></span>`action-win-toggle-properties` | `win.toggle-properties` | Document Properties | `none` | `bool` | `exported` | `stable-user-command` | `window/actions` | header-button, keyboard-shortcut, command-palette, dbus-action | Always enabled; compact layout may render the requested state differently. | unit, widget |
| <span id="action-win-set-properties-visible"></span>`action-win-set-properties-visible` | `win.set-properties-visible` | Set Document Properties Visibility | `bool` | `none` | `exported` | `stable-user-command` | `window/actions` | dbus-action | Always enabled; compact layout may render the requested state differently. | unit, widget |
| <span id="action-win-toggle-minimap"></span>`action-win-toggle-minimap` | `win.toggle-minimap` | Toggle Minimap | `none` | `bool` | `exported` | `stable-user-command` | `window/actions` | primary-menu, keyboard-shortcut, dbus-action | Always enabled; document and Focus Mode policy may suppress rendering. | unit, widget |
| <span id="action-win-set-minimap-visible"></span>`action-win-set-minimap-visible` | `win.set-minimap-visible` | Set Minimap Visibility | `bool` | `none` | `exported` | `stable-user-command` | `window/actions` | dbus-action | Always enabled; document and Focus Mode policy may suppress rendering. | unit, widget |
| <span id="action-win-fullscreen"></span>`action-win-fullscreen` | `win.fullscreen` | Fullscreen | `none` | `none` | `exported` | `stable-user-command` | `window/actions` | primary-menu, dbus-action | Enabled while the window is not fullscreen. | unit, widget |
| <span id="action-win-unfullscreen"></span>`action-win-unfullscreen` | `win.unfullscreen` | Leave Fullscreen | `none` | `none` | `exported` | `stable-user-command` | `window/actions` | primary-menu, dbus-action | Enabled while the window is fullscreen. | unit, widget |
| <span id="action-win-toggle-fullscreen"></span>`action-win-toggle-fullscreen` | `win.toggle-fullscreen` | Fullscreen | `none` | `none` | `exported` | `stable-user-command` | `window/actions` | keyboard-shortcut, command-palette, dbus-action | Always enabled. | unit, widget |
| <span id="action-win-toggle-focus-mode"></span>`action-win-toggle-focus-mode` | `win.toggle-focus-mode` | Focus Mode | `none` | `bool` | `exported` | `stable-user-command` | `window/focus_mode` | header-button, keyboard-shortcut, command-palette, dbus-action | Always enabled. | unit, widget |
| <span id="action-win-set-focus-mode"></span>`action-win-set-focus-mode` | `win.set-focus-mode` | Set Focus Mode | `bool` | `none` | `exported` | `stable-user-command` | `window/focus_mode` | dbus-action | Always enabled; changes state through the normal Focus Mode transition. | unit, widget |
| <span id="action-win-toggle-preview-pane"></span>`action-win-toggle-preview-pane` | `win.toggle-preview-pane` | Preview Pane | `none` | `bool` | `exported` | `diagnostic-only` | `window/preview` | dbus-action | Requires an active tab; exported for preview-state setup. | unit |
| <span id="action-win-set-preview-pane-visible"></span>`action-win-set-preview-pane-visible` | `win.set-preview-pane-visible` | Set Preview Pane Visibility | `bool` | `none` | `exported` | `diagnostic-only` | `window/preview` | dbus-action | Requires an active tab; exits preview-only mode before showing the side-by-side pane. | unit, widget |
| <span id="action-win-toggle-preview-mode"></span>`action-win-toggle-preview-mode` | `win.toggle-preview-mode` | Markdown Preview | `none` | `bool` | `exported` | `stable-user-command` | `window/preview` | primary-menu, keyboard-shortcut, dbus-action | Requires an active tab and no visible side-by-side preview pane. | unit, widget |
| <span id="action-win-set-preview-mode"></span>`action-win-set-preview-mode` | `win.set-preview-mode` | Set Markdown Preview Mode | `bool` | `none` | `exported` | `stable-user-command` | `window/preview` | dbus-action | Requires an active tab; hides the side-by-side preview pane before entering preview-only mode. | unit, widget |
| <span id="action-win-print"></span>`action-win-print` | `win.print` | Print | `none` | `none` | `exported` | `contextual-user-command` | `window/print` | primary-menu, keyboard-shortcut, command-palette, dbus-action | Requires an active tab. | unit, widget |
| <span id="action-win-zoom-in"></span>`action-win-zoom-in` | `win.zoom-in` | Zoom In | `none` | `none` | `exported` | `stable-user-command` | `window/zoom` | custom-menu-widget, keyboard-shortcut, command-palette, dbus-action | Enabled while zoom is below the maximum. | unit, widget |
| <span id="action-win-zoom-out"></span>`action-win-zoom-out` | `win.zoom-out` | Zoom Out | `none` | `none` | `exported` | `stable-user-command` | `window/zoom` | custom-menu-widget, keyboard-shortcut, command-palette, dbus-action | Enabled while zoom is above the minimum. | unit, widget |
| <span id="action-win-zoom-reset"></span>`action-win-zoom-reset` | `win.zoom-reset` | Reset Zoom | `none` | `none` | `exported` | `stable-user-command` | `window/zoom` | custom-menu-widget, keyboard-shortcut, command-palette, dbus-action | Always enabled. | unit, widget |
| <span id="action-win-toggle-tab-pinned"></span>`action-win-toggle-tab-pinned` | `win.toggle-tab-pinned` | Pin or Unpin Tab | `none` | `none` | `exported` | `contextual-user-command` | `window/tabs` | tab-context-menu, dbus-action | Requires a tab context-menu target. | unit, widget |
| <span id="action-win-close-tabs-right"></span>`action-win-close-tabs-right` | `win.close-tabs-right` | Close All Tabs to the Right | `none` | `none` | `exported` | `contextual-user-command` | `window/tabs` | tab-context-menu, dbus-action | Requires a tab context-menu target; normal close safety still applies. | unit, widget |
| <span id="action-win-close-other-tabs"></span>`action-win-close-other-tabs` | `win.close-other-tabs` | Close Other Tabs | `none` | `none` | `exported` | `contextual-user-command` | `window/tabs` | tab-context-menu, dbus-action | Requires a tab context-menu target; normal close safety still applies. | unit, widget |
| <span id="action-win-move-tab-left"></span>`action-win-move-tab-left` | `win.move-tab-left` | Move Tab Left | `none` | `none` | `exported` | `contextual-user-command` | `window/tabs` | tab-context-menu, dbus-action | Requires a movable tab context-menu target. | unit, widget |
| <span id="action-win-move-tab-right"></span>`action-win-move-tab-right` | `win.move-tab-right` | Move Tab Right | `none` | `none` | `exported` | `contextual-user-command` | `window/tabs` | tab-context-menu, dbus-action | Requires a movable tab context-menu target. | unit, widget |
| <span id="action-win-show-help-overlay"></span>`action-win-show-help-overlay` | `win.show-help-overlay` | Keyboard Shortcuts | `none` | `none` | `exported` | `stable-user-command` | `window/actions` | primary-menu, command-palette, dbus-action | Always enabled; opens the shipped keyboard-shortcuts help window. | unit, widget |
| <span id="action-search-options-regex"></span>`action-search-options-regex` | `search-options.regex` | Regular Expressions | `none` | `bool` | `widget-scoped` | `contextual-user-command` | `search_bar` | search-options-menu | Requires a visible in-document search bar. | unit, widget |
| <span id="action-search-options-case-sensitive"></span>`action-search-options-case-sensitive` | `search-options.case-sensitive` | Case Sensitive | `none` | `bool` | `widget-scoped` | `contextual-user-command` | `search_bar` | search-options-menu | Requires a visible in-document search bar. | unit, widget |
| <span id="action-search-options-whole-word"></span>`action-search-options-whole-word` | `search-options.whole-word` | Match Whole Word Only | `none` | `bool` | `widget-scoped` | `contextual-user-command` | `search_bar` | search-options-menu | Requires a visible in-document search bar. | unit, widget |
| <span id="action-section-focus-folder"></span>`action-section-focus-folder` | `section.focus-folder` | Focus Folder | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-file-context-menu | Requires a directory row context. | unit, widget |
| <span id="action-section-local-history"></span>`action-section-local-history` | `section.local-history` | Local History | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-file-context-menu | Requires a file row context. | unit, widget |
| <span id="action-section-document-note"></span>`action-section-document-note` | `section.document-note` | Open Document Note | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-file-context-menu | Requires a file row context. | unit, widget |
| <span id="action-section-folder-note"></span>`action-section-folder-note` | `section.folder-note` | Open Folder Note | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-folder-context-menu | Requires a workspace folder row context. | unit, widget |
| <span id="action-section-move-folder-up"></span>`action-section-move-folder-up` | `section.move-folder-up` | Move Up | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-folder-context-menu | Requires a movable workspace folder row context. | unit, widget |
| <span id="action-section-move-folder-down"></span>`action-section-move-folder-down` | `section.move-folder-down` | Move Down | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-folder-context-menu | Requires a movable workspace folder row context. | unit, widget |
| <span id="action-section-remove-folder"></span>`action-section-remove-folder` | `section.remove-folder` | Remove from Workspace | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-folder-context-menu | Requires a workspace folder row context and confirmation. | unit, widget |
| <span id="action-section-new-file"></span>`action-section-new-file` | `section.new-file` | New File | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-file-context-menu, sidebar-folder-context-menu | Requires a file or folder context that can create children. | unit, widget |
| <span id="action-section-new-dir"></span>`action-section-new-dir` | `section.new-dir` | New Folder | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-file-context-menu, sidebar-folder-context-menu | Requires a file or folder context that can create children. | unit, widget |
| <span id="action-section-rename"></span>`action-section-rename` | `section.rename` | Rename | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-file-context-menu | Requires a renameable file or folder context. | unit, widget |
| <span id="action-section-delete"></span>`action-section-delete` | `section.delete` | Delete | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | sidebar-file-context-menu | Requires a file or folder context and confirmation. | unit, widget |
| <span id="action-ws-header-add-folder"></span>`action-ws-header-add-folder` | `ws-header.add-folder` | Add Folder | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | workspace-header-context-menu | Requires a workspace header context. | unit, widget |
| <span id="action-ws-header-open-folder-note"></span>`action-ws-header-open-folder-note` | `ws-header.open-folder-note` | Open Folder Note | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | workspace-header-context-menu | Requires a workspace header context. | unit, widget |
| <span id="action-ws-header-rename"></span>`action-ws-header-rename` | `ws-header.rename` | Rename Workspace | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | workspace-header-context-menu | Requires a workspace header context. | unit, widget |
| <span id="action-ws-header-unlist"></span>`action-ws-header-unlist` | `ws-header.unlist` | Remove Workspace | `none` | `none` | `widget-scoped` | `contextual-user-command` | `sidebar/workspace_section` | workspace-header-context-menu | Requires a workspace header context and confirmation. | unit, widget |
