## Why

LushText already exposes many GTK window actions and has strong widget, visual, accessibility, crash-recovery, and portal/sandbox smoke lanes, but automated agents still need brittle AT-SPI or pixel inference for several common workflows. This change makes LushText unusually observable and scriptable through a deliberate D-Bus automation spine while preserving the existing user-facing GTK behavior and keeping the current full filesystem permission posture.

## What Changes

- Add a documented automation spine that combines GTK/GIO actions, a narrow app-owned read-only D-Bus inspection interface, stable AT-SPI metadata, and scenario-driven smoke helpers.
- Add parameterized user-level actions for workflows that are currently only partially drivable over D-Bus, starting with search/query, workspace search, tab/surface selection, preview state, notes/bookmarks, and readiness/idle checkpoints where appropriate.
- Add a machine-readable action catalog that maps visible commands to action names, parameters, state, enablement rules, menu/shortcut surfaces, and test coverage.
- Add a typed read-only automation state surface for active document, tabs, visible surfaces, search state, workspace/search/notes state summaries, last notifications, and readiness events.
- Expand smoke tooling so agents can run scenario scripts that combine action activation, read-only state assertions, AT-SPI checks, screenshots, warning scans, and artifacts.
- Add extensive user-facing and developer-facing documentation for every exposed action, D-Bus member, state field, signal, helper flag, scenario, and safety boundary, backed by drift checks that fail when docs are stale.
- Validate desktop D-Bus/app activation metadata and desktop actions where they are harmless and proven, without weakening existing CLI, file-manager, or MIME open behavior.
- Keep Flatpak using full filesystem permissions. Portal-related work is limited to diagnostics, runtime reporting, chooser/screenshot support, and smoke evidence; this change does not migrate LushText to portals-only or narrow filesystem access.
- No breaking changes to user-visible workflows, file persistence formats, keyboard shortcuts, or existing test lanes.

## Capabilities

### New Capabilities
- `dbus-automation-spine`: Defines LushText's app-owned automation contract: action catalog, parameterized actions, read-only D-Bus state, readiness/events, scenario runner behavior, and safety boundaries.

### Modified Capabilities
- `accessibility-keyboard-coverage`: Extend accessibility coverage so AT-SPI metadata becomes a stable automation and accessibility contract across major workflow surfaces.
- `desktop-open-activation-coverage`: Extend desktop activation coverage to include D-Bus action introspection/activation and any validated desktop D-Bus activation metadata.
- `desktop-visual-smoke-coverage`: Extend visual smoke coverage to use the automation spine for pre-capture state assertions and scenario matrices.
- `menu-workflow-coverage`: Extend menu/action workflow coverage so all visible commands are represented in the action catalog and covered through user-visible invocation paths.
- `portal-sandbox-workflow-coverage`: Clarify that this change preserves full filesystem permissions and only adds harmless portal/sandbox diagnostics and smoke assertions.

## Impact

- Affected Rust areas: `crates/lushtext-core/src/app.rs`, `crates/lushtext-core/src/ui/window/`, command/action registration, search/search-panel workflows, notes/bookmark workflows, sidebar/workspace workflows, notification/status plumbing, and optional new automation service modules.
- Affected test and smoke areas: `crates/lushtext/tests/widget/**`, `scripts/run-visual-smoke.sh`, `scripts/run-accessibility-smoke.sh`, `scripts/run-crash-recovery-smoke.sh`, `scripts/run-portal-sandbox-smoke.sh`, `.agents/skills/gtk-agentic-debugging/scripts/**`, `docs/end-user-coverage.md`, and scheduled `end-user-smoke` artifacts.
- Affected documentation: new automation guide/reference docs, `README.md`, `AGENTS.md`, `.agents/rules/*.md`, relevant `.agents/skills/*` references, and documentation drift checks.
- Potential dependencies: a Rust D-Bus service helper such as `zbus` for typed app-owned interfaces, plus regenerated Flatpak cargo sources if a dependency is added.
- Desktop metadata impact: may add validated desktop actions or D-Bus activation metadata only after proving native, Flatpak, Snap, CLI, MIME, and file-manager launch behavior remains correct.
- Packaging impact: Flatpak `--filesystem=host` remains the shipping baseline for this change.
