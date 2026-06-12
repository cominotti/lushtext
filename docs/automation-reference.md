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

<!-- automation-helper-flags: run-automation-smoke --artifact-dir --binary run-crash-recovery-smoke --artifact-dir --binary run-accessibility-smoke --artifact-dir --binary run-visual-smoke --artifact-dir --binary cargo-gtk-proof-run --artifact-dir --binary --scenario-dir --case-filter --oracle capture-lushtext-mutter --file --output --search --expected-search-matches --enable-minimap --enable-atspi --window-action --window-string-action --window-bool-action --wait-predicate --wait-window-action --wait-atspi-text --color-scheme --capture-artifact-dir --atspi-tree-output --atspi-focus-output --binary --width --height --keep-artifacts run-portal-sandbox-smoke --artifact-dir check-flatpak-permissions --manifest --self-test lushtext-automation introspect catalog snapshot predicates events wait action artifact-summary visual-geometry-capture self-test --bus-name --object-path --interface --window-path --timeout-ms --json --field --string --bool --uint32 --variant-json --scenario-id --size-id --direction --color-scheme --word-wrap --fixture-kind --viewport-position -->

## Stability Policy

- `InterfaceVersion` starts at `1`. Increment it when a method, property,
  snapshot field, field meaning, or action-catalog row changes incompatibly for
  automation consumers.
- Additive fields are allowed, but they must be documented here and covered by
  `make check-automation-docs`.
- Mutating operations stay on normal GTK/GIO actions. The
  `dev.cominotti.lushtext.Automation1` object is read-only except for waiting.
- Automation1 may use `gtk-lush-proof-spine` internally for generic proof
  projections, but that crate does not define the D-Bus interface. The
  Automation1 names and signatures in this reference remain authoritative.
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
- `visual-geometry-capture` also accepts `--scenario-id`, `--size-id`,
  `--direction`, `--color-scheme`, `--word-wrap`, `--fixture-kind`, and
  `--viewport-position` so ambiguous live state is overridden explicitly instead
  of guessed.

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
| <span id="automation-client-command-visual-geometry-capture"></span>`automation-client-command-visual-geometry-capture` | `visual-geometry-capture DIR` | Captures the current live Automation1 visual-geometry snapshot, writes bounded capture artifacts, and emits a runnable visual-geometry scenario for headless replay. |
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
| <span id="automation-client-status-visual-comparison-failed"></span><span id="automation-client-exit-visual-comparison-failed"></span>`visual-comparison-failed` | `visual-comparison-failed` | `1` | `artifact-summary` found a protected-region pixel difference in a visual geometry scenario. |
| <span id="automation-client-status-pixel-anchor-failed"></span><span id="automation-client-exit-pixel-anchor-failed"></span>`pixel-anchor-failed` | `pixel-anchor-failed` | `1` | `artifact-summary` found a declared pixel anchor or relative pixel-anchor delta failure in a visual geometry scenario. |
| <span id="automation-client-status-state-mismatch"></span><span id="automation-client-exit-state-mismatch"></span>`state-mismatch` | `state-mismatch` | `1` | `artifact-summary` found a geometry-anchor relationship mismatch in a visual geometry scenario. |
| <span id="automation-client-status-warning-scan-failed"></span><span id="automation-client-exit-warning-scan-failed"></span>`warning-scan-failed` | `warning-scan-failed` | `1` | `artifact-summary` found unexpected GTK, GDK, Adwaita, AT-SPI, or assertion warnings. |
| <span id="automation-client-status-missing-field"></span><span id="automation-client-exit-missing-field"></span>`missing-field` | `missing-field` | `2` | `visual-geometry-capture` could not infer a required replay field and needs an explicit override. |
| <span id="automation-client-status-workflow-failure"></span><span id="automation-client-exit-workflow-failure"></span>`workflow-failure` | `workflow-failure` | `1` | Automation1 or the client self-test reported a failed workflow/invariant. |
| <span id="automation-client-status-artifact-error"></span><span id="automation-client-exit-artifact-error"></span>`artifact-error` | `artifact-error` | `1` | `artifact-summary` found missing, malformed, failed, or unrecognized artifact evidence. |
| <span id="automation-client-status-artifact-skipped"></span><span id="automation-client-exit-artifact-skipped"></span>`artifact-skipped` | `artifact-skipped` | `0` | `artifact-summary` found a skipped lane and reports it distinctly without claiming coverage passed. |

### Artifact Summary Fields

| Anchor | Field | Meaning |
| --- | --- | --- |
| <span id="automation-client-artifact-field-artifact-dir"></span>`automation-client-artifact-field-artifact-dir` | `artifact_dir` | Absolute artifact directory that was summarized. |
| <span id="automation-client-artifact-field-status"></span>`automation-client-artifact-field-status` | `status` | Final manifest status, usually `passed`, `failed`, or `skipped`. |
| <span id="automation-client-artifact-field-schema-version"></span>`automation-client-artifact-field-schema-version` | `schema_version` | Version of the Rust visual proof summary schema when present. |
| <span id="automation-client-artifact-field-engine"></span>`automation-client-artifact-field-engine` | `engine` | Proof engine metadata, distinguishing Rust default proof from Python oracle or diagnostic output. |
| <span id="automation-client-artifact-field-scenario-source"></span>`automation-client-artifact-field-scenario-source` | `scenario_source` | Rust proof scenario source metadata such as scenario root, manifest count, and expanded case count. |
| <span id="automation-client-artifact-field-parity"></span>`automation-client-artifact-field-parity` | `parity` | Inline Rust/Python parity metadata when a parity run produced the summary. |
| <span id="automation-client-artifact-field-parity-report"></span>`automation-client-artifact-field-parity-report` | `parity_report` | Parsed parity report metadata from the summary or sibling `parity-report.json`. |
| <span id="automation-client-artifact-field-environment-report"></span>`automation-client-artifact-field-environment-report` | `environment_report` | Parsed Rust proof host/runtime environment report when present. |
| <span id="automation-client-artifact-field-missing-capabilities"></span>`automation-client-artifact-field-missing-capabilities` | `missing_capabilities` | Bounded host capability diagnostics for skipped or unsupported visual proof runs. |
| <span id="automation-client-artifact-field-scenario-id"></span>`automation-client-artifact-field-scenario-id` | `scenario_id` | Stable scenario id from the manifest. |
| <span id="automation-client-artifact-field-scenario-type"></span>`automation-client-artifact-field-scenario-type` | `scenario_type` | Visual geometry scenario family, when present. |
| <span id="automation-client-artifact-field-failure-status"></span>`automation-client-artifact-field-failure-status` | `failure_status` | Stable machine-readable failure status for visual geometry artifacts. |
| <span id="automation-client-artifact-field-failure-reason"></span>`automation-client-artifact-field-failure-reason` | `failure_reason` | Bounded failure reason when the scenario failed. |
| <span id="automation-client-artifact-field-skip-reason"></span>`automation-client-artifact-field-skip-reason` | `skip_reason` | Bounded host or tooling reason when the scenario skipped. |
| <span id="automation-client-artifact-field-invariant-id"></span>`automation-client-artifact-field-invariant-id` | `invariant_id` | Named visual invariant verified by the case, when the scenario declares one. |
| <span id="automation-client-artifact-field-manifest"></span>`automation-client-artifact-field-manifest` | `manifest` | Absolute path to `scenario-manifest.json`. |
| <span id="automation-client-artifact-field-source-manifest"></span>`automation-client-artifact-field-source-manifest` | `source_manifest` | Source scenario manifest that generated a visual geometry case. |
| <span id="automation-client-artifact-field-summary"></span>`automation-client-artifact-field-summary` | `summary` | Parsed `summary.json` payload when present. |
| <span id="automation-client-artifact-field-runtime-warning-scan"></span>`automation-client-artifact-field-runtime-warning-scan` | `runtime_warning_scan` | Text from `assertions/runtime-warning-scan.txt` when present. |
| <span id="automation-client-artifact-field-warnings"></span>`automation-client-artifact-field-warnings` | `warnings` | Parsed warning-scan status for visual geometry artifacts. |
| <span id="automation-client-artifact-field-workflow-events"></span>`automation-client-artifact-field-workflow-events` | `workflow_events` | Bounded workflow-event artifact summary: relative path, last sequence, capped flag, and event count. |
| <span id="automation-client-artifact-field-snapshots"></span>`automation-client-artifact-field-snapshots` | `snapshots` | Relative paths for snapshot JSON artifacts without embedding their payloads. |
| <span id="automation-client-artifact-field-geometry-snapshots"></span>`automation-client-artifact-field-geometry-snapshots` | `geometry_snapshots` | Visual geometry snapshot artifact rows by capture step, without embedding document contents. |
| <span id="automation-client-artifact-field-screenshots"></span>`automation-client-artifact-field-screenshots` | `screenshots` | Screenshot artifact rows by capture step. |
| <span id="automation-client-artifact-field-protected-regions"></span>`automation-client-artifact-field-protected-regions` | `protected_regions` | Manifest regions that must remain pixel-identical except for declared masks. |
| <span id="automation-client-artifact-field-pixel-anchors"></span>`automation-client-artifact-field-pixel-anchors` | `pixel_anchors` | Manifest pixel anchors that must be detected in before/after screenshots. |
| <span id="automation-client-artifact-field-relative-pixel-anchors"></span>`automation-client-artifact-field-relative-pixel-anchors` | `relative_pixel_anchors` | Manifest relationships between detected pixel anchors, such as bounded vertical deltas. |
| <span id="automation-client-artifact-field-pixel-anchor-assertion-count"></span>`automation-client-artifact-field-pixel-anchor-assertion-count` | `pixel_anchor_assertion_count` | Number of pixel anchors declared for the case or root summary. |
| <span id="automation-client-artifact-field-pixel-anchor-evidence"></span>`automation-client-artifact-field-pixel-anchor-evidence` | `pixel_anchor_evidence` | Bounded per-anchor row positions, row deltas, crop artifact paths, and app-geometry diagnostics. |
| <span id="automation-client-artifact-field-final-geometry"></span>`automation-client-artifact-field-final-geometry` | `final_geometry` | Selected before/after sidebar, editor, and minimap geometry rows captured after final allocation settling. |
| <span id="automation-client-artifact-field-app-vs-rendered-disagreements"></span>`automation-client-artifact-field-app-vs-rendered-disagreements` | `app_vs_rendered_disagreements` | Diagnostic rows where Automation1 anchors stayed stable but screenshot-derived rendered rows moved. |
| <span id="automation-client-artifact-field-rendered-anchor-stability"></span>`automation-client-artifact-field-rendered-anchor-stability` | `rendered_anchor_stability` | Per-step warmup-vs-final screenshot row stability for declared rendered pixel anchors. |
| <span id="automation-client-artifact-field-allowed-changing-regions"></span>`automation-client-artifact-field-allowed-changing-regions` | `allowed_changing_regions` | Manifest regions that may change only under explicit geometry-anchor assertions. |
| <span id="automation-client-artifact-field-comparison-report"></span>`automation-client-artifact-field-comparison-report` | `comparison_report` | Parsed protected-region comparison result when present. |
| <span id="automation-client-artifact-field-visual-geometry-cases"></span>`automation-client-artifact-field-visual-geometry-cases` | `visual_geometry_cases` | Root-lane case rows with pass, fail, skip, protected-region, and manifest summaries. |
| <span id="automation-client-artifact-field-verified-invariant-ids"></span>`automation-client-artifact-field-verified-invariant-ids` | `verified_invariant_ids` | Root visual-geometry summary invariant ids verified by passing unfiltered cases. |
| <span id="automation-client-artifact-field-pixel-verified-invariant-ids"></span>`automation-client-artifact-field-pixel-verified-invariant-ids` | `pixel_verified_invariant_ids` | Root visual-geometry summary invariant ids verified by passing unfiltered cases with declared pixel anchors. |
| <span id="automation-client-artifact-field-animation-sampling"></span>`automation-client-artifact-field-animation-sampling` | `animation_sampling` | Scenario animation sampling contract, including invariant id, frame count, skew, and required anchors. |
| <span id="automation-client-artifact-field-animation-frame-evidence"></span>`automation-client-artifact-field-animation-frame-evidence` | `animation_frame_evidence` | Bounded animation-frame report with sampled frames, mapped geometry samples, phase evidence, anchor rows, and failure details. |
| <span id="automation-client-artifact-field-animation-frame-sample-count"></span>`automation-client-artifact-field-animation-frame-sample-count` | `animation_frame_sample_count` | Count of screenshot frames evaluated for animation-frame proof. |
| <span id="automation-client-artifact-field-animation-verified-invariant-ids"></span>`automation-client-artifact-field-animation-verified-invariant-ids` | `animation_verified_invariant_ids` | Root visual-geometry invariant ids verified by passing timestamp-correlated animation-frame evidence. |
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
| `cargo gtk-proof run` | `--artifact-dir DIR` | Writes same-session before/after screenshots, bounded geometry snapshots, protected-region and pixel-anchor comparison reports, animation-stream evidence, warning scans, per-case manifests, and root summary artifacts to `DIR`. |
| `cargo gtk-proof run` | `--binary PATH` | Runs the given LushText binary instead of `target/debug/lushtext`. |
| `cargo gtk-proof run` | `--scenario-dir DIR` | Loads visual invariant scenario manifests from `DIR`. |
| `cargo gtk-proof run` | `--case-filter TEXT` | Runs only visual geometry cases whose generated id contains `TEXT`. |
| `cargo gtk-proof run` | `--oracle python` | Runs the legacy Python visual runner under Rust supervision as explicit diagnostic/oracle output. The resulting `python-visual-oracle` engine metadata is non-authoritative and skipped summaries do not count as proof. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--file PATH` | Opens the fixture file in the isolated LushText process before capture. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--output PATH` | Writes the captured headless Mutter monitor PNG to this path. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--search TEXT` | Sets in-document search through `win.set-search-query` and waits for `search-complete`. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--expected-search-matches N` | When `--search` is set, waits until Automation1 reports this editor match count before screenshot capture. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--enable-minimap` | Enables the minimap GSettings key before launching LushText. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--enable-atspi` | Starts a private AT-SPI registry even when the scenario does not set text through AT-SPI. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--window-action ACTION` | Activates a window-scoped `org.gtk.Actions` action before capture; may be repeated. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--window-string-action ACTION=TEXT` | Activates a window-scoped `org.gtk.Actions` action with one string parameter before capture; may be repeated. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--window-bool-action ACTION=true\|false` | Activates a window-scoped `org.gtk.Actions` action with one boolean parameter before capture; may be repeated. |
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
| `scripts/lushtext-automation.py` | `visual-geometry-capture` | Writes `live-snapshot.json`, `capture-manifest.json`, and a generated visual-geometry scenario from the current live window state. |
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
| `scripts/lushtext-automation.py` | `--scenario-id ID` | Overrides the generated live visual-geometry scenario id. |
| `scripts/lushtext-automation.py` | `--size-id ID` | Overrides the generated live visual-geometry matrix size id. |
| `scripts/lushtext-automation.py` | `--direction hide\|show` | Overrides the sidebar action direction for live visual-geometry capture. |
| `scripts/lushtext-automation.py` | `--color-scheme MODE` | Supplies `default`, `force-light`, or `force-dark` when live theme cannot be inferred safely. |
| `scripts/lushtext-automation.py` | `--word-wrap BOOL` | Supplies the live word-wrap state when it cannot be inferred safely. |
| `scripts/lushtext-automation.py` | `--fixture-kind KIND` | Supplies `plain-lines` or `markdown-dense` when the active fixture kind is ambiguous. |
| `scripts/lushtext-automation.py` | `--viewport-position POS` | Supplies `top` or `mid` when the source-view scroll anchor is ambiguous. |

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
`assertions/<capture>-manifest.json` per visual scenario. The visual geometry
runner writes one case-level `scenario-manifest.json` per generated matrix case
and a root `summary.json`; those artifacts identify authoritative Rust engine
metadata, schema version, scenario source, parity status, and unsupported-host
reasons when host tooling is missing. Case manifests extend the same bounded review
index idea with `scenario_type`, `protected_regions`,
`allowed_changing_regions`, `geometry_snapshots`, `pixel_anchors`,
`relative_pixel_anchors`, `invariant_id`, and `comparison_report` fields
summarized by `scripts/lushtext-automation.py artifact-summary`. These
manifests are the review indexes for their scenarios: rich payloads stay in
sibling files, while each manifest records paths, compact status rows, selected
environment details, and failure or skip reasons. The canonical shared field
list is checked against `scripts/automation-smoke-driver.py`; adding, removing,
or renaming a shared field requires updating this table and rerunning
`make check-automation-docs`.

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
| <span id="scenario-manifest-field-invariant-id"></span>`scenario-manifest-field-invariant-id` | `invariant_id` | Named visual invariant verified by this visual geometry case, when declared. |
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
| <span id="scenario-manifest-field-pixel-anchors"></span>`scenario-manifest-field-pixel-anchors` | `pixel_anchors` | Visual geometry pixel anchors that must be detected from before/after screenshot pixels. |
| <span id="scenario-manifest-field-relative-pixel-anchors"></span>`scenario-manifest-field-relative-pixel-anchors` | `relative_pixel_anchors` | Bounded relationships between detected pixel anchors, such as minimap edge-to-content-row deltas. |
| <span id="scenario-manifest-field-pixel-anchor-assertion-count"></span>`scenario-manifest-field-pixel-anchor-assertion-count` | `pixel_anchor_assertion_count` | Number of pixel anchors declared for the visual geometry case. |
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
and `minimap-refresh`, while recovery restore remains a readiness predicate
until a dedicated recovery-specific event source exists.

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
| <span id="snapshot-field-visual-geometry"></span>`snapshot-field-visual-geometry` | `window.visual_geometry` | `object` | Bounded geometry anchors for screenshot invariant tooling, without document text. |
| <span id="snapshot-field-scale-factor"></span>`snapshot-field-scale-factor` | `visual_geometry.scale_factor` | `i32` | Window scale factor used when logical rectangles map to screenshot pixels. |
| <span id="snapshot-field-coordinate-space"></span>`snapshot-field-coordinate-space` | `visual_geometry.coordinate_space` | `string` | Coordinate space shared by all rectangles, currently window logical pixels. |
| <span id="snapshot-field-ready"></span>`snapshot-field-ready` | `visual_geometry.ready` | `bool` | Whether `visual-geometry-settled` is currently satisfied. |
| <span id="snapshot-field-blocker"></span>`snapshot-field-blocker` | `visual_geometry.blocker` | `string?` | First visual readiness blocker, if any. |
| <span id="snapshot-field-name"></span>`snapshot-field-name` | `visual_geometry.surfaces[].name`, `visual_geometry.pixel_anchors[].name`, `visual_geometry.scroll_anchors[].name` | `string` | Stable visual surface, pixel-anchor, or scroll-anchor name. |
| <span id="snapshot-field-rect"></span>`snapshot-field-rect` | `visual_geometry.surfaces[].rect`, `visual_geometry.pixel_anchors[].rect` | `object?` | Rectangle for a visible surface or pixel anchor in the snapshot coordinate space. |
| <span id="snapshot-field-allocation"></span>`snapshot-field-allocation` | `visual_geometry.surfaces[].allocation` | `object?` | Allocated surface size when a widget or computed region exists. |
| <span id="snapshot-field-absence-reason"></span>`snapshot-field-absence-reason` | `visual_geometry.surfaces[].absence_reason`, `visual_geometry.pixel_anchors[].absence_reason` | `string?` | Stable reason a surface or pixel anchor is hidden, unavailable, zero-sized, or absent. |
| <span id="snapshot-field-x"></span>`snapshot-field-x` | `visual_geometry.surfaces[].rect.x`, `visual_geometry.pixel_anchors[].rect.x` | `i32` | Left coordinate in the snapshot coordinate space. |
| <span id="snapshot-field-y"></span>`snapshot-field-y` | `visual_geometry.surfaces[].rect.y`, `visual_geometry.pixel_anchors[].rect.y` | `i32` | Top coordinate in the snapshot coordinate space. |
| <span id="snapshot-field-width"></span>`snapshot-field-width` | `visual_geometry.surfaces[].rect.width`, `visual_geometry.surfaces[].allocation.width`, `visual_geometry.pixel_anchors[].rect.width` | `i32` | Surface or pixel-anchor width in logical GTK pixels. |
| <span id="snapshot-field-height"></span>`snapshot-field-height` | `visual_geometry.surfaces[].rect.height`, `visual_geometry.surfaces[].allocation.height`, `visual_geometry.pixel_anchors[].rect.height` | `i32` | Surface or pixel-anchor height in logical GTK pixels. |
| <span id="snapshot-field-pixel-anchors"></span>`snapshot-field-pixel-anchors` | `visual_geometry.pixel_anchors` | `array` | App-computed crop and diagnostic anchors for screenshot assertions, such as minimap viewport top edge, viewport fill, bottom edge, and first content row. These are not the rendered-effect pass/fail oracle by themselves. |
| <span id="snapshot-field-surface"></span>`snapshot-field-surface` | `visual_geometry.pixel_anchors[].surface` | `string` | Visual surface that owns the pixel anchor. |
| <span id="snapshot-field-native-minimap"></span>`snapshot-field-native-minimap` | `visual_geometry.native_minimap` | `object` | Bounded native `GtkSourceMap` slider diagnostics for rendered-effect proof; diagnostic only, not the pass/fail oracle. |
| <span id="snapshot-field-projection-source"></span>`snapshot-field-projection-source` | `visual_geometry.native_minimap.projection_source` | `string?` | Stable source label for the native slider estimate, such as `upstream-visible-rect-estimate`. |
| <span id="snapshot-field-source-map-allocation"></span>`snapshot-field-source-map-allocation` | `visual_geometry.native_minimap.source_map_allocation` | `object?` | Source-map widget allocation used by native minimap diagnostics. |
| <span id="snapshot-field-source-map-rect"></span>`snapshot-field-source-map-rect` | `visual_geometry.native_minimap.source_map_rect` | `object?` | Source-map widget rectangle in the visual snapshot coordinate space. |
| <span id="snapshot-field-editor-visible-rect"></span>`snapshot-field-editor-visible-rect` | `visual_geometry.native_minimap.editor_visible_rect` | `object?` | Editor visible rect in editor buffer coordinates, without document text. |
| <span id="snapshot-field-source-map-visible-rect"></span>`snapshot-field-source-map-visible-rect` | `visual_geometry.native_minimap.source_map_visible_rect` | `object?` | Source-map visible rect in source-map buffer coordinates, without rendered text. |
| <span id="snapshot-field-source-view-vadjustment"></span>`snapshot-field-source-view-vadjustment` | `visual_geometry.native_minimap.source_view_vadjustment` | `object?` | Source-view vertical adjustment summary used to explain top anchoring. |
| <span id="snapshot-field-source-map-vadjustment"></span>`snapshot-field-source-map-vadjustment` | `visual_geometry.native_minimap.source_map_vadjustment` | `object?` | Source-map vertical adjustment summary used to detect stale map scroll. |
| <span id="snapshot-field-editor-document-height"></span>`snapshot-field-editor-document-height` | `visual_geometry.native_minimap.editor_document_height` | `i32?` | Bounded editor document height used by the native slider estimate. |
| <span id="snapshot-field-source-map-document-height"></span>`snapshot-field-source-map-document-height` | `visual_geometry.native_minimap.source_map_document_height` | `i32?` | Bounded source-map document height used by the native slider estimate. |
| <span id="snapshot-field-border-left"></span>`snapshot-field-border-left` | `visual_geometry.native_minimap.border_left` | `i32?` | Source-map left CSS border input used by the native slider width estimate. |
| <span id="snapshot-field-border-right"></span>`snapshot-field-border-right` | `visual_geometry.native_minimap.border_right` | `i32?` | Source-map right CSS border input used by the native slider width estimate. |
| <span id="snapshot-field-native-slider-estimate"></span>`snapshot-field-native-slider-estimate` | `visual_geometry.native_minimap.native_slider_estimate` | `object?` | Upstream-informed estimate of the native slider rectangle in snapshot coordinates. |
| <span id="snapshot-field-native-slider-visible-bounds"></span>`snapshot-field-native-slider-visible-bounds` | `visual_geometry.native_minimap.native_slider_visible_bounds` | `object?` | Native slider rectangle vertically fitted to the visible source-map allocation for crop-safe diagnostics while preserving the native horizontal CSS outset. |
| <span id="snapshot-field-line-projection-rect"></span>`snapshot-field-line-projection-rect` | `visual_geometry.native_minimap.line_projection_rect` | `object?` | Older line-projection viewport estimate retained as diagnostic contrast. |
| <span id="snapshot-field-first-content-row-rect"></span>`snapshot-field-first-content-row-rect` | `visual_geometry.native_minimap.first_content_row_rect` | `object?` | First rendered minimap content-row estimate in snapshot coordinates. |
| <span id="snapshot-field-at-lower"></span>`snapshot-field-at-lower` | `visual_geometry.native_minimap.*_vadjustment.at_lower` | `bool?` | Whether the adjustment is at its lower bound. |
| <span id="snapshot-field-value-milli"></span>`snapshot-field-value-milli` | `visual_geometry.native_minimap.*_vadjustment.value_milli` | `i64?` | Vertical adjustment value multiplied by 1000 for bounded diagnostics. |
| <span id="snapshot-field-lower-milli"></span>`snapshot-field-lower-milli` | `visual_geometry.native_minimap.*_vadjustment.lower_milli` | `i64?` | Vertical adjustment lower bound multiplied by 1000 for bounded diagnostics. |
| <span id="snapshot-field-upper-milli"></span>`snapshot-field-upper-milli` | `visual_geometry.native_minimap.*_vadjustment.upper_milli` | `i64?` | Vertical adjustment upper bound multiplied by 1000 for bounded diagnostics. |
| <span id="snapshot-field-page-size-milli"></span>`snapshot-field-page-size-milli` | `visual_geometry.native_minimap.*_vadjustment.page_size_milli` | `i64?` | Vertical adjustment page size multiplied by 1000 for bounded diagnostics. |
| <span id="snapshot-field-scroll-anchors"></span>`snapshot-field-scroll-anchors` | `visual_geometry.scroll_anchors` | `array` | Scroll anchors for editor-like surfaces that explain top and left stability. |
| <span id="snapshot-field-at-left"></span>`snapshot-field-at-left` | `visual_geometry.scroll_anchors[].at_left` | `bool?` | Whether the horizontal adjustment is at the lower content edge, allowing the widget's own margin tolerance. |
| <span id="snapshot-field-at-top"></span>`snapshot-field-at-top` | `visual_geometry.scroll_anchors[].at_top` | `bool?` | Whether the vertical adjustment is at the lower content edge, allowing the widget's own margin tolerance. |
| <span id="snapshot-field-x-value-milli"></span>`snapshot-field-x-value-milli` | `visual_geometry.scroll_anchors[].x_value_milli` | `i64?` | Horizontal adjustment value multiplied by 1000. |
| <span id="snapshot-field-x-lower-milli"></span>`snapshot-field-x-lower-milli` | `visual_geometry.scroll_anchors[].x_lower_milli` | `i64?` | Horizontal lower bound multiplied by 1000. |
| <span id="snapshot-field-y-value-milli"></span>`snapshot-field-y-value-milli` | `visual_geometry.scroll_anchors[].y_value_milli` | `i64?` | Vertical adjustment value multiplied by 1000. |
| <span id="snapshot-field-y-lower-milli"></span>`snapshot-field-y-lower-milli` | `visual_geometry.scroll_anchors[].y_lower_milli` | `i64?` | Vertical lower bound multiplied by 1000. |
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
| <span id="readiness-predicate-visual-geometry-settled"></span>`readiness-predicate-visual-geometry-settled` | `visual-geometry-settled` | stable | `app-startup`, `session-restore`, `file-load`, `draft-autosave`, `preview-animation`, `workspace-persist`, `workspace-filter-animation`, `command-palette-index`, `workspace-search`, `editor-search`, `replace-preview`, `minimap-refresh` | GTK layout, adaptive shell state, minimap projection, and visual scenario blockers are settled. Sidebar animation scenarios must still prove final workspace-sidebar/editor allocation relationships before screenshot proof is accepted. |
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
| <span id="readiness-minimap-refresh"></span>`readiness-minimap-refresh` | `minimap-refresh` | Minimap projection, marker, or viewport geometry is waiting for the post-layout refresh debounce. |
| <span id="readiness-preview-animation"></span>`readiness-preview-animation` | `preview-animation` | Preview layout switching or embedded Markdown widget repair is still settling. |
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
| <span id="action-win-toggle-preview-pane"></span>`action-win-toggle-preview-pane` | `win.toggle-preview-pane` | Preview Pane | `none` | `bool` | `exported` | `diagnostic-only` | `window/preview` | dbus-action | Requires an active tab; exported for preview-state setup. | unit, widget, visual-smoke |
| <span id="action-win-set-preview-pane-visible"></span>`action-win-set-preview-pane-visible` | `win.set-preview-pane-visible` | Set Preview Pane Visibility | `bool` | `none` | `exported` | `diagnostic-only` | `window/preview` | dbus-action | Requires an active tab; exits preview-only mode before showing the side-by-side pane. | unit, widget, visual-smoke |
| <span id="action-win-toggle-preview-mode"></span>`action-win-toggle-preview-mode` | `win.toggle-preview-mode` | Markdown Preview | `none` | `bool` | `exported` | `stable-user-command` | `window/preview` | primary-menu, keyboard-shortcut, dbus-action | Requires an active tab and no visible side-by-side preview pane. | unit, widget, visual-smoke |
| <span id="action-win-set-preview-mode"></span>`action-win-set-preview-mode` | `win.set-preview-mode` | Set Markdown Preview Mode | `bool` | `none` | `exported` | `stable-user-command` | `window/preview` | dbus-action | Requires an active tab; hides the side-by-side preview pane before entering preview-only mode. | unit, widget, visual-smoke |
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
