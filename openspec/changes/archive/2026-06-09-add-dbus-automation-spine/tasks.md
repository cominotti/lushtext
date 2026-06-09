## 1. Baseline Inventory And Architecture

- [x] 1.1 Record a live baseline of app/window `org.gtk.Actions` using a private `dbus-run-session` and headless Mutter, preserving the action list as an implementation note or test fixture.
- [x] 1.2 Inventory app, window, search, sidebar, tab, notes, bookmark, local-history, preview, print, zoom, and context-menu action registration points.
- [x] 1.3 Inventory command palette command definitions and map them to registered action IDs.
- [x] 1.4 Inventory visible menu resources, shortcuts, toolbar buttons, status-bar controls, and context-menu commands that should appear in the action catalog.
- [x] 1.5 Decide whether the custom read-only automation interface uses GIO D-Bus registration only or adds `zbus`, documenting the decision and any dependency impact.
- [x] 1.6 If a dependency is added, update Cargo manifests, regenerate Flatpak cargo sources, and validate dependency policy.

## 2. Action Catalog

- [x] 2.1 Create the action catalog data model with action scope, ID, label, parameter type, state type, enablement rule, owning workflow, user-visible surfaces, external activation safety, docs anchor, and coverage lane fields.
- [x] 2.2 Implement catalog construction from the existing command/action definitions or a single declarative registry that action registration consumes.
- [x] 2.3 Add an audit that compares registered app/window actions against the action catalog.
- [x] 2.4 Add an audit that compares command palette commands against the action catalog.
- [x] 2.5 Add an audit that compares menu/shortcut declarations against the action catalog where they are statically discoverable.
- [x] 2.6 Add tests that fail on missing catalog entries, stale action IDs, duplicate action IDs, and parameter/state type mismatches.
- [x] 2.7 Add a generated or checked artifact for the developer reference to consume without hand-copying action details.

## 3. Parameterized Actions And Command Parity

- [x] 3.1 Add a parameterized in-tab search action that opens or updates search text through the normal search workflow.
- [x] 3.2 Add tests that the parameterized search action updates query text, match highlighting, result counts, minimap markers, focus behavior, and close behavior consistently with visible typing.
- [x] 3.3 Add parameterized or stateful actions for preview mode and preview pane state if current toggles cannot express deterministic target state.
- [x] 3.4 Add parameterized or target-state actions for workspace sidebar, document properties, search panel, focus mode, and minimap where deterministic scenario setup needs target state rather than toggles.
- [x] 3.5 Add action support for selecting tabs or identifying active tabs in scenario-safe ways without relying on tab strip coordinates.
- [x] 3.6 Add action support for notes/bookmark browser setup and navigation only where it maps to real user-visible behavior.
- [x] 3.7 Add invalid-parameter tests for every new parameterized action.
- [x] 3.8 Add no-context tests for every new parameterized action that requires an active document, workspace, note, bookmark, or visible surface.

## 4. Read-Only Automation D-Bus Interface

- [x] 4.1 Add an automation adapter module at the appropriate UI/window or application boundary without violating `ui -> services -> model` dependency direction.
- [x] 4.2 Register a versioned automation D-Bus object on the session bus under a stable object path while LushText owns its application bus name.
- [x] 4.3 Expose introspectable read-only methods/properties for interface version, build/profile gate, enabled state, action catalog, and app snapshot.
- [x] 4.4 Implement bounded snapshot collection for active document identity, tab metadata, modified state, failed-load state, saving/loading state, and pinned state when available.
- [x] 4.5 Implement bounded snapshot collection for visible/requested shell surfaces, compact secondary-surface state, active transient surface, search UI, search panel, preview mode, minimap, status bar, and focus mode.
- [x] 4.6 Implement bounded snapshot collection for workspace, command palette, notes, bookmarks, local-history, content search, and notification summaries.
- [x] 4.7 Ensure snapshot collection runs on the GTK main context and never performs blocking filesystem or expensive search/index work on the GTK thread.
- [x] 4.8 Add workflow started/finished or equivalent state-change events for file load/save, search, workspace refresh, content search, replace preview, session restore, and recovery restore where practical.
- [x] 4.9 Add unit/widget/integration tests for snapshot serialization, redaction/bounding, version fields, disabled states, and event emission.
- [x] 4.10 Add a private-session D-Bus smoke test that introspects the automation object and reads a snapshot from a real LushText process.

## 5. Readiness And Wait Predicates

- [x] 5.1 Define readiness predicates for app startup, window action export, file-open completion, search completion, save completion, workspace refresh completion, session restore completion, and recovery restore completion.
- [x] 5.2 Implement `WaitForIdle` or equivalent readiness method that observes GTK idle work, background callback delivery, layout synchronization, and workflow state.
- [x] 5.3 Implement timeout/error reporting that distinguishes predicate timeout, workflow failure, unavailable automation surface, and unsupported host tooling.
- [x] 5.4 Add tests that readiness waits complete for successful workflows and time out clearly for impossible predicates.
- [x] 5.5 Update smoke helpers to use readiness predicates instead of fixed sleeps wherever the app-owned predicate exists.

## 6. Scenario Runner And Smoke Helpers

- [x] 6.1 Add a scenario definition format or structured helper arguments for fixture setup, launch mode, actions, waits, AT-SPI assertions, screenshots, warning scans, and artifact manifests.
- [x] 6.2 Extend the headless Mutter helper to activate parameterized actions and read automation snapshots.
- [x] 6.3 Add a search/minimap scenario that opens a file, sets query text through action/D-Bus, verifies state, captures a screenshot, and scans warnings.
- [x] 6.4 Add preview/properties scenarios for normal, compact, and constrained geometry.
- [x] 6.5 Add workspace scenarios covering zero folders, representative folders, dense/awkward folder names, refresh, and constrained geometry.
- [x] 6.6 Add notes/bookmarks scenarios covering no notes, one/few notes, dense note/bookmark sets, and constrained geometry.
- [x] 6.7 Add command palette scenarios covering files, commands, notes mode if present, no-context state, dense results, and dismissal.
- [x] 6.8 Add crash/recovery scenario integration that uses automation snapshots to verify restored tabs, draft state, and recovery diagnostics before screenshot capture.
- [x] 6.9 Add portal/sandbox diagnostic scenario support that records portal state without changing filesystem permissions.
- [x] 6.10 Ensure every scenario writes a bounded artifact manifest with steps, state assertions, screenshots, AT-SPI excerpts, D-Bus summaries, warnings, environment, and skip/failure reason.

## 7. Accessibility And AT-SPI Anchors

- [x] 7.1 Audit search, replace, command palette, search panel, notes browser, bookmarks dialog, local history, encoding/file-health dialogs, save-changes dialog, sidebar rows, tab strip, and context menus for stable accessible names and roles.
- [x] 7.2 Add or adjust accessible labels/descriptions for icon-only or role-ambiguous controls used by users and smoke helpers.
- [x] 7.3 Add widget tests for stable accessible roles/names on updated surfaces.
- [x] 7.4 Update AT-SPI helper usage to target stable accessible names before falling back to broad role scans.
- [x] 7.5 Extend accessibility smoke to verify focus path, stable anchors, visible editables, and no-context/dense/constrained states covered by the scenario matrix.
- [x] 7.6 Ensure accessibility smoke skips clearly when host accessibility runtime is unavailable and does not claim accessibility coverage from action/D-Bus checks alone.

## 8. Desktop Activation And Packaging Guardrails

- [x] 8.1 Add D-Bus action introspection checks for app actions and window actions in a private headless session.
- [x] 8.2 Add checks that exported stateful action state agrees with automation snapshot state after toggles and target-state actions settle.
- [x] 8.3 Evaluate `DBusActivatable=true` and desktop actions against native, staged development, Flatpak, Snap, CLI, MIME, and file-manager activation behavior.
- [x] 8.4 Not applicable: desktop D-Bus activation metadata was not proven safe, so no metadata update was made; see 8.5 for the documented blocked branch.
- [x] 8.5 If desktop D-Bus activation metadata is not proven safe, leave it disabled and document the blocker in automation/packaging docs.
- [x] 8.6 Add a guard that Flatpak full filesystem permission remains present for this change.
- [x] 8.7 Update portal/sandbox smoke artifacts to record full permission posture, portal bus names, runtime identity, and diagnostic-only portal state.
- [x] 8.8 Confirm portal/sandbox tasks do not implement or imply portals-only migration.

## 9. Documentation And Drift Gates

- [x] 9.1 Add `docs/automation.md` with supported use cases, examples, safety boundaries, scenario usage, portal/screenshot caveats, troubleshooting, and release/development differences.
- [x] 9.2 Add or generate a developer automation reference documenting every public action, D-Bus method, property, signal, snapshot field, readiness predicate, scenario helper flag, environment gate, and stability level.
- [x] 9.3 Document every stable AT-SPI anchor used by smoke helpers with surface, role, name, owning workflow, and stability classification.
- [x] 9.4 Update `docs/end-user-coverage.md` with the automation-backed scenario lanes and boundaries.
- [x] 9.5 Update `README.md` for user-facing automation/debugging commands and any new validation commands.
- [x] 9.6 Update `AGENTS.md`, relevant nested `AGENTS.md`, `.agents/rules/build.md`, `.agents/rules/documentation.md`, and `.agents/rules/widget-wiring.md` for the new automation contract.
- [x] 9.7 Update gtk-testing and gtk-agentic-debugging skill references with the new action/snapshot/scenario workflow.
- [x] 9.8 Add `make check-automation-docs` or an equivalent policy check that compares exposed actions, D-Bus members, snapshot fields, helper flags, and docs anchors.
- [x] 9.9 Add CI/pre-commit wiring for the documentation drift check if it is deterministic enough for routine validation.
- [x] 9.10 Add tests proving the drift check fails on a missing action doc, missing D-Bus member doc, and missing scenario/helper flag doc.

## 10. Test Coverage

- [x] 10.1 Add unit tests for action catalog construction, serialization, coverage metadata, duplicate detection, and docs anchor presence.
- [x] 10.2 Add widget tests for parameterized actions, stateful target actions, no-context behavior, invalid parameters, and state synchronization.
- [x] 10.3 Add integration or real-process smoke tests for D-Bus action introspection and automation snapshot reads.
- [x] 10.4 Add visual smoke assertions for search/minimap, preview/properties, workspace, notes/bookmarks, command palette, recovery, dense/awkward, empty/no-context, and constrained geometry scenarios.
- [x] 10.5 Add accessibility smoke assertions for stable AT-SPI anchors and focus paths in the scenario matrix.
- [x] 10.6 Add portal/sandbox smoke assertions that preserve full filesystem permission and record portal diagnostics.
- [x] 10.7 Add regression tests that save/close/replace/destructive actions cannot bypass existing modified-document, durable-write, confirmation, or recovery safety behavior through automation.
- [x] 10.8 Add warning scans for unexpected GTK, GDK, Libadwaita, GIO, D-Bus, portal, AT-SPI, and filesystem warnings in new smoke paths.

## 11. Validation

- [x] 11.1 Run `cargo fmt --all -- --check`.
- [x] 11.2 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 11.3 Run `make test-unit`.
- [x] 11.4 Run `make test-int`.
- [x] 11.5 Run `make test-widget-headless`.
- [x] 11.6 Run targeted new D-Bus/automation smoke checks in an isolated session.
- [x] 11.7 Run `make visual-smoke` when required host support is available.
- [x] 11.8 Run `make accessibility-smoke` when required host support is available.
- [x] 11.9 Run `make portal-sandbox-smoke` and verify full filesystem permission remains documented and present.
- [x] 11.10 Run `make crash-recovery-smoke` when required host support is available.
- [x] 11.11 Run `make check-blueprint` if UI resources or generated templates change.
- [x] 11.12 Run `make check-agent-docs` after documentation/rule/skill updates.
- [x] 11.13 Run the new automation documentation drift check.
- [x] 11.14 Run `openspec validate add-dbus-automation-spine --strict`.
- [x] 11.15 Run `openspec validate --changes --strict`.
- [x] 11.16 Run `git diff --check`.
