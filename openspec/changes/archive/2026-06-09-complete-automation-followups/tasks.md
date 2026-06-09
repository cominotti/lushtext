## 1. Baseline And Scope Confirmation

- [x] 1.1 Confirm `add-dbus-automation-spine` is complete or otherwise available in the worktree before implementing this follow-up.
- [x] 1.2 Inspect `.github/workflows/end-user-smoke.yml`, `Makefile`, `scripts/run-automation-smoke.sh`, and `scripts/automation-smoke-driver.py` to record the current automation smoke command and artifact path.
- [x] 1.3 Inspect `resources/ui/window.blp`, `resources/ui/window.ui`, `resources/ui/shortcuts.blp`, and `resources/ui/shortcuts.ui` to confirm the Keyboard Shortcuts menu action and shortcut-window resource.
- [x] 1.4 Inspect `crates/lushtext-core/src/ui/window/actions.rs`, command-palette command definitions, and the action catalog row for `win.show-help-overlay`.
- [x] 1.5 Decide the stable client entry point name and path, preferring a script path plus optional Makefile alias over a new Rust crate unless implementation proves otherwise.
- [x] 1.6 Record any dependency decision in comments/docs if the client needs more than standard Python, `gdbus`, and existing host tools.

## 2. Scheduled Automation Smoke Lane

- [x] 2.1 Add an `automation` entry to the scheduled/manual end-user smoke workflow matrix.
- [x] 2.2 Configure the automation matrix command as `make automation-smoke SMOKE_ARTIFACT_DIR=build/smoke`.
- [x] 2.3 Configure the automation matrix artifact path as `build/smoke/automation`.
- [x] 2.4 Ensure artifact upload still runs with `if: always()` for the new automation lane.
- [x] 2.5 Confirm the workflow's Fedora dependency install covers every command used by `make automation-smoke`.
- [x] 2.6 Keep the automation lane out of required pull-request CI unless a separate explicit decision is made.
- [x] 2.7 Add a workflow or script check proving the scheduled matrix contains the automation lane with the expected command and artifact path.
- [x] 2.8 Update end-user coverage documentation so scheduled/manual automation smoke coverage is described alongside other host-sensitive lanes.

## 3. Keyboard Shortcuts Action Registration

- [x] 3.1 Register `win.show-help-overlay` as a window action.
- [x] 3.2 Implement the action by loading or presenting the existing `GtkShortcutsWindow` resource from `resources/ui/shortcuts.ui`.
- [x] 3.3 Set the active `LushtextWindow` as the shortcut window's transient parent when the toolkit surface supports it.
- [x] 3.4 Ensure repeated activations present or focus the existing shortcut window instead of leaking duplicate stale windows.
- [x] 3.5 Keep the action available with no active document, no workspace, no notes, no bookmarks, and no visible search context.
- [x] 3.6 Ensure activating the shortcut action does not change document contents, tab selection, modified state, workspace persistence, search state, or settings.
- [x] 3.7 Ensure the primary menu and command palette still point to `win.show-help-overlay` after the action is registered.
- [x] 3.8 Preserve normal GTK dismissal behavior for the shortcut window, including close button and Escape where supported by the toolkit.

## 4. Action Catalog And Menu Drift

- [x] 4.1 Change the `win.show-help-overlay` catalog row from `VisibleUnregisteredGap` to `Exported`.
- [x] 4.2 Change the `win.show-help-overlay` external activation safety from `UnsupportedGap` to the correct supported user-command classification.
- [x] 4.3 Update the row owner from the placeholder `services/palette` to the concrete window owner that registers the action.
- [x] 4.4 Update the row enablement text to describe the no-document/no-context availability.
- [x] 4.5 Update the row surfaces to include primary menu, command palette, and D-Bus action activation.
- [x] 4.6 Update the row coverage lanes to include unit and widget coverage, plus automation smoke or manual diagnostic coverage if applicable.
- [x] 4.7 Add or update action catalog tests proving `win.show-help-overlay` is no longer an unsupported visible gap.
- [x] 4.8 Add or update visible-static-action and command-palette action audits proving the menu and palette references resolve to the cataloged exported action.
- [x] 4.9 Update generated or checked automation reference artifacts consumed by documentation tooling.

## 5. Automation Client Core

- [x] 5.1 Create the supported automation client script or helper at the chosen stable path.
- [x] 5.2 Add command-line parsing for `introspect`, `catalog`, `snapshot`, `predicates`, `events`, `wait`, `action`, `artifact-summary`, and `self-test`.
- [x] 5.3 Add global flags for D-Bus destination, Automation1 object path, interface, window action object path, timeout, JSON output, and quiet or human-readable output where appropriate.
- [x] 5.4 Implement host-tool detection for `gdbus` and any other required command.
- [x] 5.5 Implement Automation1 method calls for `GetActionCatalog`, `GetSnapshot`, `GetReadinessPredicates`, `GetWorkflowEvents`, `WaitForReady`, and introspection.
- [x] 5.6 Parse Automation1 JSON payloads using the standard library and report malformed payloads as stable client errors.
- [x] 5.7 Implement field extraction for snapshot/catalog/event payloads without printing unrelated unbounded data.
- [x] 5.8 Add bounded text handling for every client detail string and artifact-summary excerpt.
- [x] 5.9 Ensure the client never prints document contents, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers.

## 6. Automation Client Action Activation

- [x] 6.1 Implement catalog lookup for fully qualified actions such as `win.set-search-query`.
- [x] 6.2 Reject unknown actions with stable `unknown-action` status.
- [x] 6.3 Reject `unsupported-gap`, `visible-unregistered-gap`, and non-exported widget-scoped actions with stable `unsupported-action` status.
- [x] 6.4 Infer the expected parameter type from the catalog before building any GVariant parameter.
- [x] 6.5 Support no-parameter actions.
- [x] 6.6 Support string parameters.
- [x] 6.7 Support boolean parameters.
- [x] 6.8 Support unsigned integer parameters.
- [x] 6.9 Support variant-map parameters only if an existing or future cataloged action needs them; otherwise reject them clearly.
- [x] 6.10 Reject parameter mismatches before calling D-Bus.
- [x] 6.11 Activate app-scoped and window-scoped actions through `org.gtk.Actions.Activate` on the documented object path.
- [x] 6.12 Preserve app-owned context and enablement behavior rather than synthesizing private widget-local context.
- [x] 6.13 Add an action activation smoke or integration proof using a safe action such as `win.set-search-query` followed by `WaitForReady` and `GetSnapshot`.

## 7. Automation Client Artifact Summaries

- [x] 7.1 Implement `artifact-summary` for automation smoke directories with `scenario-manifest.json`.
- [x] 7.2 Summarize scenario status, failure reason, skip reason, summary path, manifest path, warning scan status, D-Bus summaries, waits, state assertions, and key artifact paths.
- [x] 7.3 Summarize `workflow-events.json` presence and capped/sequence state without embedding the whole event log unless bounded JSON output explicitly requests it.
- [x] 7.4 Summarize snapshot artifact presence without embedding full snapshot payloads in human output.
- [x] 7.5 Recognize failed automation artifacts and exit nonzero while preserving useful evidence paths in the output.
- [x] 7.6 Recognize skipped automation artifacts and report a distinct stable status without claiming coverage passed.
- [x] 7.7 Report unknown or malformed artifact directories with stable `artifact-error` status.
- [x] 7.8 Add optional generic summary support for visual, crash-recovery, accessibility, and portal/sandbox artifacts when their existing manifests or summary files are present.

## 8. Client Output, Exit Codes, And Self-Test

- [x] 8.1 Define the stable result envelope with `ok`, `status`, `command`, `detail`, and `data`.
- [x] 8.2 Define the stable status vocabulary: `ready`, `ok`, `usage-error`, `unsupported-host-tooling`, `automation-unavailable`, `dbus-error`, `unknown-action`, `unsupported-action`, `parameter-mismatch`, `predicate-timeout`, `workflow-failure`, and `artifact-error`.
- [x] 8.3 Define and implement documented exit codes for success, app/predicate/artifact failure, usage or parameter mismatch, automation unavailable, and unsupported host tooling.
- [x] 8.4 Ensure every failing command can emit JSON error output with the same envelope shape as successful commands.
- [x] 8.5 Add `self-test` coverage for argument parsing, result-envelope serialization, status-to-exit-code mapping, representative D-Bus payload parsing, and malformed artifact summaries.
- [x] 8.6 Add fixtures for representative Automation1 JSON, action catalog JSON, wait responses, and scenario manifests.
- [x] 8.7 Keep fixture payloads bounded and free of real user paths or contents.

## 9. Tests For Keyboard Shortcuts UI

- [x] 9.1 Add a widget test that the live window action list includes `show-help-overlay`.
- [x] 9.2 Add a widget test that the primary menu action string and command-palette command both resolve to the registered action.
- [x] 9.3 Add a widget test that activating `win.show-help-overlay` presents a `GtkShortcutsWindow` or the toolkit-specific shortcut help surface.
- [x] 9.4 Add a widget test that the action works with no active file-backed document and no workspace context.
- [x] 9.5 Add a widget test that activating the action preserves active document text, modified state, selected tab, workspace scope, and search state.
- [x] 9.6 Add a constrained-geometry widget or visual smoke assertion that shortcut help remains bounded and closeable.
- [x] 9.7 Add a dense-shortcuts assertion that content scrolls inside the shortcut help surface while header and close controls remain reachable.
- [x] 9.8 Add or update automation smoke assertions if the shortcut action can be verified safely in the real-process lane.

## 10. Documentation And Drift Checks

- [x] 10.1 Update `docs/automation.md` with automation client usage, examples, safety rules, and troubleshooting.
- [x] 10.2 Update `docs/automation-reference.md` with client commands, flags, result envelope fields, statuses, exit codes, artifact-summary fields, and examples.
- [x] 10.3 Update the action table entry for `win.show-help-overlay` to reflect supported exported status.
- [x] 10.4 Update `docs/end-user-coverage.md` to document scheduled automation smoke.
- [x] 10.5 Update `README.md` with the supported client entry point and scheduled automation smoke command where user-facing.
- [x] 10.6 Update `AGENTS.md`, relevant `.agents/rules/*.md`, and debugging/testing skill references for the client wrapper and scheduled automation lane.
- [x] 10.7 Extend `scripts/check-automation-docs.py` so client commands, flags, statuses, output fields, exit codes, and artifact-summary fields are checked against documentation.
- [x] 10.8 Extend the documentation drift self-test to prove a missing client command or status fails the check.
- [x] 10.9 Ensure documentation continues to state that the client does not create a private mutation API and does not imply a portals-only migration.
- [x] 10.10 Run `make check-automation-docs` after all documentation and drift-check updates.

## 11. Scheduled Workflow And Client Validation

- [x] 11.1 Run the workflow matrix validation or equivalent local check for `.github/workflows/end-user-smoke.yml`.
- [x] 11.2 Run the automation client `self-test`.
- [x] 11.3 Run the automation client against a live isolated LushText process for at least `catalog`, `snapshot`, `predicates`, `wait idle`, `events`, and one safe `action` command.
- [x] 11.4 Run `make automation-smoke`.
- [x] 11.5 Run `make visual-smoke` or a targeted shortcut-window capture when host support is available and UI geometry changed.
- [x] 11.6 Run any new script-level unit tests for artifact summary and output parsing.

## 12. Rust And Widget Validation

- [x] 12.1 Run targeted action catalog unit tests.
- [x] 12.2 Run targeted command-palette and menu workflow tests.
- [x] 12.3 Run targeted window widget tests for `win.show-help-overlay`.
- [x] 12.4 Run `cargo fmt --all -- --check`.
- [x] 12.5 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 12.6 Run `make test-unit`.
- [x] 12.7 Run `make test-int`.
- [x] 12.8 Run `make test-widget-headless`.

## 13. Final OpenSpec And Repository Checks

- [x] 13.1 Run `openspec validate complete-automation-followups --strict`.
- [x] 13.2 Run `openspec validate --changes --strict`.
- [x] 13.3 Run `openspec validate --specs --strict`.
- [x] 13.4 Run `openspec validate --all --strict`.
- [x] 13.5 Run `git diff --check`.
- [x] 13.6 Confirm `openspec instructions apply --change complete-automation-followups --json` reports the change ready for implementation.
