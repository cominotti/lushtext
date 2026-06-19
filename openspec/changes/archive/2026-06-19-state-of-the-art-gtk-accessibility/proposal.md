## Why

LushText already has meaningful accessibility groundwork, but the current contract is not broad enough to call the application state-of-the-art for GTK accessibility. This change makes accessibility a first-class product spine across semantics, keyboard operation, dynamic announcements, visual accessibility, documentation, and proof artifacts.

## What Changes

- Introduce an app-wide GTK accessibility contract for names, roles, descriptions, relations, states, announcements, focus, keyboard parity, and visual accessibility.
- Audit and normalize major surfaces: main editor, tab strip, header/status controls, workspace sidebar and file tree, open popover, command palette, in-tab search, workspace search, document properties, notes/bookmarks, local history, Markdown preview, preferences, save/close dialogs, context menus, and compact/adaptive layouts.
- Add explicit editor and preview accessibility proof for text exposure, focus, caret/selection behavior, read-only states, and source/preview presentation modes.
- Expand AT-SPI smoke coverage from a small anchor set into a surface/state matrix that covers no-context, representative populated, dense/awkward, and constrained-geometry states.
- Add visual accessibility coverage for focus indication, high contrast, large text, reduced motion, color-not-only communication, opacity/readability, and narrow geometry.
- Standardize dynamic announcements for alerts, status changes, search results, long-running operations, destructive confirmations, and completed workflows without producing screen-reader noise during normal typing.
- Add developer guardrails so new icon-only controls, custom list rows, transient surfaces, and hover-only affordances cannot land without accessible metadata, keyboard parity, and tests.
- Document the stable accessibility anchors, manual verification guidance, known platform caveats, and user-facing keyboard/accessibility behavior.

## Capabilities

### New Capabilities

- `gtk-accessibility-spine`: Defines LushText's app-wide GTK accessibility semantics, surface inventory, dynamic announcements, keyboard parity, visual accessibility, and user/developer documentation contract.

### Modified Capabilities

- `accessibility-keyboard-coverage`: Expand the accessibility smoke and keyboard proof matrix so it verifies the new app-wide accessibility contract through AT-SPI and keyboard-only workflows.
- `dbus-automation-spine`: Add bounded automation support needed for accessibility scenarios to drive visible surfaces, wait for accessibility-sensitive readiness, and preserve privacy-safe artifacts.
- `desktop-visual-smoke-coverage`: Add visual accessibility smoke expectations for high contrast, large text, focus indication, reduced motion, color-not-only states, and opacity/readability.
- `visual-geometry-invariants`: Add invariant expectations for accessibility-sensitive geometry, including focus rings, persistent controls, internal scrolling, and constrained state extremes.

## Impact

- Affected UI code: `crates/lushtext-core/src/ui/**`, especially editor page, sidebar/workspace section, command palette, search bar/panel, open popover, status bar, properties panel, preferences, notes/bookmarks, local history, Markdown preview, dialogs, and transient-surface handling.
- Affected automation and smoke tooling: `scripts/run-accessibility-smoke.sh`, `.agents/skills/gtk-agentic-debugging/scripts/*`, `scripts/lushtext-automation.py`, automation docs checks, and end-user smoke artifacts.
- Affected tests: widget tests for accessible metadata/state, AT-SPI smoke scenarios, visual smoke/geometry scenarios, documentation drift checks, and policy checks.
- Affected docs/rules: `docs/automation.md`, `docs/automation-reference.md`, `docs/end-user-coverage.md`, `.agents/rules/build.md`, `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, and a new or expanded user-facing accessibility guide.
- Dependencies remain within GTK4/Libadwaita/GtkSourceView/AT-SPI and existing smoke infrastructure; no new runtime service should be required beyond documented host-sensitive accessibility tooling.
