# accessibility-keyboard-coverage Specification

## Purpose
Define LushText's accessibility and keyboard smoke coverage so core interactive
surfaces remain usable through assistive technologies and keyboard-only flows.

## Requirements
### Requirement: Key interactive surfaces expose stable accessibility metadata
The project SHALL provide coverage for stable accessible names, roles, and
descriptions on controls that users and assistive technologies rely on.

#### Scenario: Search controls are accessible by name and role
- **WHEN** the in-tab search UI is visible in a real accessibility-enabled
  session
- **THEN** the search entry and close/control buttons expose stable accessible
  names and roles

#### Scenario: Persistent shell controls are accessible
- **WHEN** the main window is visible
- **THEN** workspace toggle, document-properties toggle, tab controls, primary
  menu controls, status controls, and editor action buttons expose meaningful
  accessibility metadata

#### Scenario: Dialog actions are accessible
- **WHEN** save-changes, local-history, notes, preferences, or file-related
  dialogs are visible
- **THEN** their primary controls and destructive actions expose stable names,
  roles, and states

### Requirement: Accessibility metadata SHALL be stable automation metadata
The project SHALL treat stable accessible names, roles, descriptions, and states
as part of both the accessibility contract and the automation contract for
visible interactive controls. Automation helpers MAY use AT-SPI to interact with
visible editables and controls, but the metadata MUST remain meaningful for
assistive technology users first.

#### Scenario: Search and replace controls expose stable names
- **WHEN** the in-tab search or replace UI is visible
- **THEN** the search entry, replacement entry, option controls, navigation
  controls, replace controls, and close control expose stable accessible names
  and roles
- **AND** automation can target the intended editable without relying only on
  role order or widget depth

#### Scenario: Major transient surfaces expose stable metadata
- **WHEN** command palette, search panel, notes browser, bookmarks dialog,
  local-history dialog, encoding controls, file-health dialog,
  save-changes dialog, or file/sidebar context menus are visible
- **THEN** their primary controls, close/dismiss affordances, result lists,
  empty states, and destructive actions expose stable accessible metadata
- **AND** constrained geometry does not hide the reachable action controls from
  the accessibility tree

#### Scenario: Accessibility metadata covers state extremes
- **WHEN** an accessibility-enabled smoke scenario exercises no-context,
  representative populated, dense or awkward, and constrained-geometry states
  for a supported surface
- **THEN** accessible names and roles remain stable enough for assistive
  technology and automation to identify the surface, item region, primary
  actions, and dismissal controls

### Requirement: Keyboard-only workflows are smoke-tested
The project SHALL cover representative user workflows using keyboard input only.

#### Scenario: Search workflow is keyboard-operable
- **WHEN** the user opens search through the keyboard, types a query, navigates
  matches, and closes search
- **THEN** focus returns to the expected editor
- **AND** the app does not trap focus in hidden search controls

#### Scenario: Command and sidebar workflows are keyboard-operable
- **WHEN** the user opens the command palette, switches workspace/sidebar or
  properties visibility, and returns to editing using keyboard actions
- **THEN** each action updates the visible UI state
- **AND** focus restoration follows the documented behavior

#### Scenario: Save-changes dialog is keyboard-operable
- **WHEN** a close flow presents a save-changes dialog
- **THEN** the user can reach Save, Discard, Cancel, and any multi-document
  selection controls using keyboard navigation
- **AND** activating those controls produces the same outcome as pointer
  activation

### Requirement: Accessibility smoke runs outside the accessibility-disabled widget harness
The project SHALL provide an accessibility-enabled smoke lane that complements
the default widget harness rather than relying on it.

#### Scenario: Accessibility bridge is enabled for the smoke lane
- **WHEN** the accessibility smoke lane runs
- **THEN** it does not set `NO_AT_BRIDGE=1`
- **AND** it verifies the app through AT-SPI or the host accessibility API
  available in the session

#### Scenario: Accessibility smoke artifacts are preserved
- **WHEN** the accessibility smoke lane completes
- **THEN** it records the queried accessible tree subset, focus path, and any
  warnings needed to diagnose failures

#### Scenario: Missing accessibility runtime skips clearly
- **WHEN** the host lacks the accessibility services required by the smoke lane
- **THEN** the lane reports a clear skip reason and does not claim that real
  accessibility behavior was verified

### Requirement: AT-SPI automation SHALL complement action and D-Bus assertions
The accessibility smoke lane SHALL use AT-SPI to prove visible UI accessibility
and to fill gaps that are inherently visual or editable, while relying on
GTK/GIO actions and the automation D-Bus snapshot for app-owned command and
readiness assertions where available.

#### Scenario: AT-SPI editable text is used only for visible editables
- **WHEN** a scenario needs to set text in a visible entry that does not yet
  have a parameterized action
- **THEN** the helper uses AT-SPI editable-text APIs with stable accessible
  names
- **AND** the resulting app state is verified through the automation snapshot or
  visible UI assertion

#### Scenario: Action-driven workflows still verify accessibility
- **WHEN** a scenario opens search, notes, command palette, preview, workspace,
  or properties through GTK/GIO actions
- **THEN** the accessibility lane also verifies that the resulting visible
  controls remain accessible by name, role, state, and focus path

#### Scenario: Missing accessibility runtime stays diagnostic
- **WHEN** AT-SPI or accessibility services are unavailable
- **THEN** the lane reports a clear skip reason
- **AND** action/D-Bus state assertions do not claim that accessibility behavior
  was verified

### Requirement: Accessibility automation documentation SHALL stay synchronized
The project SHALL document the accessible names and roles that are intentionally
stable automation anchors, and validation MUST catch drift when those anchors
change.

#### Scenario: Stable accessibility anchors are documented
- **WHEN** developers read the automation or accessibility documentation
- **THEN** each stable accessible anchor used by smoke helpers is listed with
  its visible surface, role, expected name, and owning workflow
- **AND** the documentation explains whether the anchor is stable public
  behavior or helper-internal diagnostic metadata

#### Scenario: Accessibility anchor drift fails checks
- **WHEN** a stable accessible name, role, or helper target changes
- **THEN** the relevant widget test, accessibility smoke assertion, or
  documentation drift check fails until the docs and helpers are updated
  together
