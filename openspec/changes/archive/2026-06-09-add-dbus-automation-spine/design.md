## Context

LushText already has a strong base for desktop automation:

- `LushtextApplication` is a `GtkApplication`/`GApplication` with `HANDLES_OPEN`.
- The app and windows implement `gio::ActionMap` and `gio::ActionGroup`.
- A private headless Mutter run can already introspect `/dev/cominotti/lushtext/window/1` through `org.gtk.Actions`.
- Existing smoke lanes isolate XDG state, run private D-Bus sessions, drive exported window actions, use AT-SPI for visible editables, capture screenshots, and preserve artifacts.
- Widget tests cover much of the UI state, but agent-owned smoke and debugging still need brittle inference for search text, active state, workflow readiness, and broad command coverage.

The goal is not to replace GTK, AT-SPI, or existing tests. The goal is to make the same real app easier to drive and verify:

```text
┌──────────────────────────────────────────────────────────┐
│ Human, agent, CI smoke script, or external desktop tool   │
└──────────────┬────────────────┬────────────────┬─────────┘
               │                │                │
       ┌───────▼───────┐ ┌──────▼──────┐ ┌──────▼────────┐
       │ GTK/GIO       │ │ AT-SPI      │ │ LushText       │
       │ actions       │ │ visible UI  │ │ Automation DBus│
       └───────┬───────┘ └──────┬──────┘ └──────┬────────┘
               │                │                │
               └────────────────▼────────────────┘
                                │
                     ┌──────────▼──────────┐
                     │ LushtextWindow       │
                     │ workflow modules     │
                     └──────────┬──────────┘
                                │
                     ┌──────────▼──────────┐
                     │ services + model     │
                     └──────────────────────┘
```

The Flatpak permission posture is a fixed constraint for this change: the Flatpak manifest keeps full filesystem access. Portal work in this proposal is diagnostic and smoke-oriented only.

## Goals / Non-Goals

**Goals:**

- Make major user workflows externally drivable through stable GTK/GIO actions wherever the operation is a normal user command.
- Add a narrow app-owned D-Bus inspection surface for read-only state, readiness, and events that are difficult or brittle to infer from pixels or AT-SPI.
- Provide parameterized actions for high-value workflows that currently need keyboard typing or AT-SPI text mutation.
- Create a single action catalog that maps visible commands, menus, shortcuts, command-palette entries, action IDs, parameter/state types, enabled rules, and tests.
- Expand headless Mutter smoke helpers into scenario scripts that can assert state before and after actions.
- Keep AT-SPI metadata stable enough for accessibility users and automation tools.
- Add user-facing and developer-facing documentation for every exposed automation surface, with a drift gate that fails when code and docs diverge.
- Preserve existing widget, visual, accessibility, crash-recovery, portal/sandbox, and performance lanes instead of inventing an unbounded `test-e2e` lane.

**Non-Goals:**

- No portals-only migration.
- No narrowing of Flatpak filesystem permissions.
- No arbitrary widget-tree mutation over D-Bus.
- No coordinate-only UI driving.
- No replacement of widget tests with screenshot-only tests.
- No production dependency on AT-SPI being available.
- No broad remote-control interface that can read full document contents unless the content is already visible through an explicit user workflow and bounded for diagnostics.

## Decisions

### 1. Keep GTK/GIO actions as the mutation surface

User-visible operations remain normal `gio::SimpleAction` or `ActionEntry` actions. This keeps menus, shortcuts, command palette entries, notifications, D-Bus callers, and tests aligned.

Examples:

- `begin-search-with-text(s)`
- `set-search-query(s)`
- `run-workspace-search(a{sv})` or smaller typed action variants
- `select-tab(u)` or `select-tab-by-id(s)` if stable tab IDs are introduced
- `set-preview-mode(s)` or stateful preview actions where they match user behavior

Alternatives considered:

- Use only a new D-Bus method layer for commands. Rejected because it would split user-visible behavior from menus and shortcuts.
- Keep using AT-SPI text entry for all parameterized inputs. Rejected because it is correct for visible UI proof, but too brittle as the main command path.

### 2. Add a read-only app-owned D-Bus inspection surface

The custom D-Bus interface is for observation, readiness, and events by default. It exposes bounded state snapshots, not arbitrary UI mutation.

Candidate interface:

```text
Bus name:    dev.cominotti.lushtext
Object path: /dev/cominotti/lushtext/automation1
Interface:   dev.cominotti.lushtext.Automation1

Methods:
  GetSnapshot() -> a{sv}
  GetActionCatalog() -> a{sv}
  WaitForIdle(timeout_ms: u32) -> a{sv}
  WaitForState(predicate: a{sv}, timeout_ms: u32) -> a{sv}

Properties:
  InterfaceVersion: u32
  BuildProfile: s
  AutomationEnabled: b

Signals:
  SnapshotChanged(changed_keys: as)
  WorkflowStarted(workflow_id: s, detail: a{sv})
  WorkflowFinished(workflow_id: s, result: a{sv})
  NotificationPublished(kind: s, text: s)
```

Implementation should prefer GLib/GIO D-Bus registration first because LushText already runs on the GLib main loop and already uses GIO actions. A `zbus` spike is acceptable if it cleanly integrates with the GTK main context without adding a second runtime or blocking the UI. If `zbus` is added, it must be isolated to an automation adapter module and Flatpak cargo sources must be regenerated.

Alternatives considered:

- GIO-only D-Bus registration: lowest dependency risk, but less typed than `zbus`.
- `zbus` service interface: strong typed interface and proxy generation, but possible runtime/dependency complexity.
- Test-only in-process Rust helpers: useful for widget tests but not enough for real process smoke or agent tooling.

### 3. Make the action catalog generated or checked, not hand-maintained

The catalog should be derived from a single declarative registry or audited against registered actions and visible command definitions. It must include:

- action ID and scope (`app`, `win`, section-specific groups when relevant);
- parameter type;
- state type and current state where applicable;
- enablement rules;
- user-visible labels/menus/shortcuts/command-palette entry;
- whether it is safe for external activation;
- tests that prove wiring and behavior;
- docs anchor for user/developer reference.

The drift gate should fail if a public action or automation D-Bus member is added, removed, renamed, or changes type without updating docs and tests.

### 4. Keep documentation part of the exposed contract

Automation is only useful if the surface is stable and discoverable. This change should add a durable documentation set such as:

- `docs/automation.md`: user/agent guide for supported automation, examples with `gdbus`, scenario helper usage, safety notes, and troubleshooting.
- `docs/automation-reference.md` or a generated section: action catalog, D-Bus interface, methods, properties, signals, state fields, versioning, and compatibility.
- Updates to `docs/end-user-coverage.md`, `README.md`, `AGENTS.md`, `.agents/rules/build.md`, `.agents/rules/documentation.md`, and gtk debugging/testing skill references.

Docs must describe what is stable, what is diagnostic-only, what is gated to development/test builds, and what is intentionally not exposed.

### 5. Scenario smoke scripts are bounded, artifact-rich, and state-driven

The existing capture helpers should grow into scenario drivers rather than raw action lists. A scenario should declare:

- fixture setup;
- launch mode and isolated XDG state;
- actions to invoke;
- state predicates to wait for;
- AT-SPI assertions for visible controls;
- screenshots to capture;
- warning/error scan policy;
- output artifact manifest.

Scenario families should cover:

- no document / untitled document / file-backed document;
- one tab / many tabs / pinned tabs / failed placeholder tab;
- search and replace;
- workspace sidebar empty, populated, dense, awkward names, and constrained geometry;
- notes/bookmarks empty, populated, dense, and constrained;
- preview and document-properties surfaces;
- crash/restart recovery checkpoints;
- portal/sandbox diagnostics without permission migration.

### 6. Keep portal behavior diagnostic and permission-preserving

This change must not weaken the shipping Flatpak permission model. Portal-related work is limited to:

- reporting which portal services are present;
- proving file chooser and screenshot paths when available;
- distinguishing app bugs from host/runtime limitations;
- preserving artifacts for denials and portal errors;
- checking that broad filesystem access remains documented and intentional.

## Risks / Trade-offs

- **Risk: A custom automation interface becomes an unsafe remote-control API.** -> Keep mutation in normal user actions, make custom D-Bus read-only by default, bound content fields, document every exposed field, and gate development-only helpers.
- **Risk: D-Bus callbacks touch GTK from the wrong thread.** -> Route all state collection and action completion through the GLib main context and keep heavy work off the GTK thread.
- **Risk: The action catalog drifts from code.** -> Add a generated or audited catalog plus `make check-automation-docs`/policy checks that fail on missing docs or tests.
- **Risk: Smoke scenarios become slow and flaky.** -> Keep default PR lanes bounded, use existing host-sensitive scheduled/manual smoke lanes for full scenario matrices, and use readiness predicates instead of sleeps.
- **Risk: AT-SPI names become automation-only labels with poor accessibility value.** -> Treat accessibility metadata as user-facing accessibility text first, with automation benefiting from the same stable names.
- **Risk: DBusActivatable or desktop actions regress launch behavior.** -> Add them only after native, Flatpak, Snap, CLI, MIME, and file-manager activation proof; otherwise keep current metadata and document the blocked proof.
- **Risk: Portal diagnostics get mistaken for a portal migration.** -> Keep the Flatpak manifest permission unchanged and make the no-migration boundary explicit in proposal, specs, docs, and smoke summaries.

## Migration Plan

1. Add the action catalog/audit shape and documentation skeleton.
2. Add parameterized actions and tests for the highest-friction workflows.
3. Add the read-only automation D-Bus interface and versioned snapshot contract.
4. Update headless Mutter helpers to use actions plus snapshots before AT-SPI/pixels.
5. Expand scenario smoke matrices and artifacts.
6. Add drift gates for catalog/docs/tests.
7. Validate native, Flatpak, Snap, desktop, and smoke behavior.

Rollback is straightforward if needed: keep the GTK actions that are legitimate user commands, disable or remove the custom automation interface, and retain documentation explaining which command surfaces remain stable.

## Open Questions

- Should the custom D-Bus interface be exposed in release builds by default, or should full snapshot access require an environment variable or development build profile?
- Should content-bearing state fields include bounded excerpts, hashes, lengths, or no content at all by default?
- Should the action catalog be generated from code at build/test time, or maintained as a checked-in manifest with an audit script?
- Should desktop actions be added in this change, or only a proof report plus action catalog coverage?
