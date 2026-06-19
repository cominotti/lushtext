## ADDED Requirements

### Requirement: Automation SHALL support accessibility scenario readiness
The automation spine SHALL expose bounded readiness and state needed by accessibility scenarios to drive normal user-visible actions and wait for accessibility-sensitive UI settlement without mutating private widgets.

#### Scenario: Accessibility scenarios wait on named readiness
- **WHEN** an accessibility smoke scenario opens or changes editor, search, workspace search, Open popover, command palette, notes, local history, properties, preview, preferences, or save/close surfaces
- **THEN** it can wait on the narrowest relevant readiness predicate before querying AT-SPI
- **AND** fixed sleeps are used only as a documented fallback when no app-owned predicate exists

#### Scenario: Accessibility-sensitive state is bounded
- **WHEN** an accessibility scenario requests an automation snapshot
- **THEN** the snapshot reports only bounded state such as visible surfaces, active tab identity class, result counts, selected row identity class, busy/error state, and active transient surface
- **AND** it does not expose unbounded document text, note bodies, local-history contents, complete search results, or private persistence identifiers

#### Scenario: Surface readiness includes accessibility metadata updates
- **WHEN** a UI transition, row rebind, dialog presentation, preview render, search completion, or announcement-producing workflow updates accessible metadata
- **THEN** the relevant readiness predicate does not report ready until accessible names, roles, states, and focus targets have settled
- **AND** timeout failures preserve state, logs, and AT-SPI artifacts

### Requirement: Automation actions SHALL drive accessibility smoke through normal UI paths
Accessibility smoke scenarios SHALL use documented GTK/GIO actions and visible editables rather than private widget mutation, and automation support SHALL keep those paths aligned with normal user behavior.

#### Scenario: Visible workflows use public actions where possible
- **WHEN** accessibility smoke opens search, workspace search, Open popover, command palette, notes, local history, properties, preview, focus mode, minimap, or preferences
- **THEN** it uses documented app/window actions or keyboard shortcuts that share behavior with normal UI controls
- **AND** resulting UI state is verified through both automation state and accessibility-tree assertions

#### Scenario: Text entry uses visible editables
- **WHEN** accessibility smoke must type into a visible entry or editor
- **THEN** it uses keyboard input, AT-SPI editable-text APIs, or a documented action that routes through the normal workflow
- **AND** the final app-owned state is verified without relying only on coordinates or widget depth

#### Scenario: Private widget mutation is rejected
- **WHEN** a proposed accessibility helper attempts to mutate private GTK widget state outside user-visible actions or visible editables
- **THEN** the automation contract does not treat that path as a stable accessibility scenario API
- **AND** the design is changed to use normal actions, keyboard input, or bounded read-only observation

### Requirement: Automation artifacts SHALL include accessibility proof hooks
Automation scenario artifacts SHALL preserve enough bounded accessibility context to diagnose failures in accessibility and visual accessibility lanes.

#### Scenario: Artifact summary reads accessibility results
- **WHEN** `scripts/lushtext-automation.py artifact-summary` reads accessibility smoke artifacts
- **THEN** it reports passed, failed, skipped, or unsupported AT-SPI assertions, focus assertions, warning-scan status, scenario manifests, and environment details
- **AND** it keeps output bounded for terminal use

#### Scenario: Scenario manifests identify proof chain
- **WHEN** an accessibility-backed automation scenario writes a manifest
- **THEN** it names the normal action or keyboard step, readiness predicate, automation state assertion, AT-SPI assertion, screenshot or visual assertion when present, and warning scan
- **AND** failures identify which proof link failed

#### Scenario: Documentation covers accessibility automation fields
- **WHEN** automation snapshot fields, readiness predicates, helper flags, or artifact fields are added for accessibility scenarios
- **THEN** `docs/automation.md`, `docs/automation-reference.md`, action catalog coverage, and drift checks are updated in the same change
- **AND** unsupported host limitations are documented as diagnostics, not passing proof
