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
