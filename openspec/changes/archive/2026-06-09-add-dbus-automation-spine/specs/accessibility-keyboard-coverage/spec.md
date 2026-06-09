## ADDED Requirements

### Requirement: Accessibility metadata SHALL be stable automation metadata
The project SHALL treat stable accessible names, roles, descriptions, and states as part of both the accessibility contract and the automation contract for visible interactive controls. Automation helpers MAY use AT-SPI to interact with visible editables and controls, but the metadata MUST remain meaningful for assistive technology users first.

#### Scenario: Search and replace controls expose stable names
- **WHEN** the in-tab search or replace UI is visible
- **THEN** the search entry, replacement entry, option controls, navigation controls, replace controls, and close control expose stable accessible names and roles
- **AND** automation can target the intended editable without relying only on role order or widget depth

#### Scenario: Major transient surfaces expose stable metadata
- **WHEN** command palette, search panel, notes browser, bookmarks dialog, local-history dialog, encoding controls, file-health dialog, save-changes dialog, or file/sidebar context menus are visible
- **THEN** their primary controls, close/dismiss affordances, result lists, empty states, and destructive actions expose stable accessible metadata
- **AND** constrained geometry does not hide the reachable action controls from the accessibility tree

#### Scenario: Accessibility metadata covers state extremes
- **WHEN** an accessibility-enabled smoke scenario exercises no-context, representative populated, dense or awkward, and constrained-geometry states for a supported surface
- **THEN** accessible names and roles remain stable enough for assistive technology and automation to identify the surface, item region, primary actions, and dismissal controls

### Requirement: AT-SPI automation SHALL complement action and D-Bus assertions
The accessibility smoke lane SHALL use AT-SPI to prove visible UI accessibility and to fill gaps that are inherently visual or editable, while relying on GTK/GIO actions and the automation D-Bus snapshot for app-owned command and readiness assertions where available.

#### Scenario: AT-SPI editable text is used only for visible editables
- **WHEN** a scenario needs to set text in a visible entry that does not yet have a parameterized action
- **THEN** the helper uses AT-SPI editable-text APIs with stable accessible names
- **AND** the resulting app state is verified through the automation snapshot or visible UI assertion

#### Scenario: Action-driven workflows still verify accessibility
- **WHEN** a scenario opens search, notes, command palette, preview, workspace, or properties through GTK/GIO actions
- **THEN** the accessibility lane also verifies that the resulting visible controls remain accessible by name, role, state, and focus path

#### Scenario: Missing accessibility runtime stays diagnostic
- **WHEN** AT-SPI or accessibility services are unavailable
- **THEN** the lane reports a clear skip reason
- **AND** action/D-Bus state assertions do not claim that accessibility behavior was verified

### Requirement: Accessibility automation documentation SHALL stay synchronized
The project SHALL document the accessible names and roles that are intentionally stable automation anchors, and validation MUST catch drift when those anchors change.

#### Scenario: Stable accessibility anchors are documented
- **WHEN** developers read the automation or accessibility documentation
- **THEN** each stable accessible anchor used by smoke helpers is listed with its visible surface, role, expected name, and owning workflow
- **AND** the documentation explains whether the anchor is stable public behavior or helper-internal diagnostic metadata

#### Scenario: Accessibility anchor drift fails checks
- **WHEN** a stable accessible name, role, or helper target changes
- **THEN** the relevant widget test, accessibility smoke assertion, or documentation drift check fails until the docs and helpers are updated together
