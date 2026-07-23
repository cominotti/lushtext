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

### Requirement: Accessibility smoke SHALL verify the app-wide surface matrix
The accessibility smoke lane SHALL expand from a small stable-anchor check into a reviewable surface matrix that verifies major LushText workflows through AT-SPI or the host accessibility API, while preserving clear skip behavior for unsupported hosts.

#### Scenario: Surface matrix includes major workflows
- **WHEN** `make accessibility-smoke` runs on a supported host
- **THEN** it verifies accessibility anchors for the shell, editor, Open popover, command palette, in-tab search, workspace search, workspace sidebar/file tree, notes/bookmarks, local history, document properties, Markdown preview, preferences, save/close dialogs, and representative context menus
- **AND** each scenario records the surface, expected role, expected name, focus path where relevant, and artifact path

#### Scenario: Matrix covers state extremes
- **WHEN** a covered surface supports no-context, representative populated, dense or awkward, or constrained-geometry states
- **THEN** at least one accessibility smoke scenario or focused companion scenario verifies the applicable extremes
- **AND** the artifacts prove reachable commands, readable empty states, item-region-only scrolling, preserved headers/close/actions, and absence of fake rows or unrelated context

#### Scenario: Unsupported host gaps do not count as verified
- **WHEN** the host lacks AT-SPI, Mutter, D-Bus, PipeWire, Python bindings, or another required accessibility runtime
- **THEN** the lane reports a clear skip reason
- **AND** skipped matrix entries are not recorded as passing accessibility coverage
### Requirement: Accessibility smoke SHALL prove editor text behavior where supported
The accessibility smoke lane SHALL explicitly verify the main editor text surface rather than relying only on shell controls or action/D-Bus state.

#### Scenario: Active editor has accessible focus and text identity
- **WHEN** the smoke lane opens a representative text fixture
- **THEN** the active editor surface is discoverable through the accessibility tree or documented host API
- **AND** its focus path, editable/read-only state, and bounded document identity match the active tab

#### Scenario: Caret and selection are tested
- **WHEN** the smoke lane types or selects text in the active editor through supported accessibility or keyboard mechanisms
- **THEN** caret or selection state is observable through AT-SPI, accessibility focus, or a documented platform fallback
- **AND** the app-owned automation snapshot confirms the active document state without exposing unbounded text

#### Scenario: Editor limitation is explicit
- **WHEN** GtkSourceView or the host accessibility stack cannot expose a required editor text detail
- **THEN** the smoke lane records the unsupported detail as a caveat
- **AND** a manual verification checklist covers the gap until an automated path exists
### Requirement: Accessibility smoke SHALL verify dynamic states and announcements
The accessibility smoke lane SHALL verify stateful controls, busy/error states, and user-meaningful announcements for representative workflows.

#### Scenario: Toggle and disclosure states update
- **WHEN** accessibility smoke toggles workspace sidebar, document properties, search options, preview mode, focus mode, folder expansion, notes panes, or other disclosure controls
- **THEN** the corresponding accessible state changes after the UI settles
- **AND** hidden controls are not reported as visible or focusable after dismissal

#### Scenario: Alerts and workflow outcomes announce
- **WHEN** the smoke lane triggers an inline alert, search no-result state, search result state, replace completion, recovery warning, or save/load error fixture
- **THEN** the accessibility artifacts include the alert or announcement evidence available from the platform
- **AND** the recorded message is bounded and user-meaningful

#### Scenario: High-frequency updates are not noisy
- **WHEN** the smoke lane exercises typing or repeated progress updates
- **THEN** it verifies that LushText does not emit a separate announcement for every intermediate keystroke or heartbeat
- **AND** final or milestone announcements remain available when the workflow settles
### Requirement: Keyboard-only smoke SHALL cover all primary workflows
Keyboard-only coverage SHALL exercise the primary LushText workflows without pointer input, using normal shortcuts, focus navigation, menus, command palette entries, and action paths.

#### Scenario: Keyboard-only user can edit and recover focus
- **WHEN** a keyboard-only scenario opens a document, edits text, opens search, navigates matches, opens and closes a transient surface, and returns to editing
- **THEN** focus returns to the active editor or documented fallback after each transient surface closes
- **AND** hidden controls do not trap focus

#### Scenario: Keyboard-only user can operate list-heavy surfaces
- **WHEN** a keyboard-only scenario operates Open popover rows, command palette results, workspace search results, notes/bookmarks rows, local-history snapshots, and workspace file-tree rows
- **THEN** rows can be reached, activated, dismissed, and navigated without pointer input
- **AND** row actions with hover affordances have keyboard, context-menu, or command-palette alternatives

#### Scenario: Keyboard-only user can complete destructive and close flows
- **WHEN** a keyboard-only scenario triggers unsaved close, delete, rename, restore, Replace All, or discard operations
- **THEN** Save, Discard, Cancel, destructive, undo, and checkbox controls are reachable and expose stable names, roles, and states
- **AND** activation produces the same result as pointer activation through normal safety flows
### Requirement: Accessibility smoke artifacts SHALL be reviewable and synchronized
Accessibility smoke SHALL preserve bounded artifacts that make failures diagnosable and SHALL keep scenario documentation synchronized with helper behavior.

#### Scenario: Each scenario writes a manifest
- **WHEN** an accessibility smoke scenario finishes, fails, or skips
- **THEN** it writes a bounded manifest or summary naming the fixture, actions, readiness waits, AT-SPI assertions, focus assertions, warning scan, environment, and skip or failure reason
- **AND** the manifest avoids unbounded document text and private persistence identifiers

#### Scenario: Stable anchors are documented and drift-checked
- **WHEN** a stable accessibility anchor, helper flag, scenario name, or expected role changes
- **THEN** automation reference or accessibility documentation drift checks fail until the docs and helper assertions are updated together
- **AND** docs distinguish stable public accessibility metadata from diagnostic helper internals

#### Scenario: Warning scan includes accessibility regressions
- **WHEN** accessibility smoke completes
- **THEN** unexpected GTK, GDK, Libadwaita, AT-SPI, accessibility, assertion, or application warnings fail the lane
- **AND** known compositor-shutdown noise is allowlisted narrowly with preserved logs

### Requirement: Warning allowlist classification has a single source of truth
The accessibility smoke lane's warning allowlist classification SHALL be
defined exactly once in a shared module and imported by every scan and
summary path that classifies warning lines. The lane MUST NOT embed
duplicate copies of the classification predicate whose bodies can be edited
independently.

#### Scenario: Scan and summary paths classify identically
- **WHEN** the smoke lane classifies warning lines during the final warning
  scan and again while composing the summary artifact
- **THEN** both paths call the same shared classification predicate
- **AND** a warning line cannot be allowlisted by one path and unexpected by
  the other

#### Scenario: An allowlist change is single-site
- **WHEN** a maintainer adds, narrows, or removes an allowlist entry (for
  example a new compositor-shutdown noise pattern)
- **THEN** the change is made in one shared module
- **AND** no second embedded copy of the predicate exists to fall out of
  sync

#### Scenario: Consolidation preserves classification behavior
- **WHEN** the shared module replaces the previously duplicated predicates
- **THEN** ANSI style sequences are still stripped before classification
- **AND** every previously allowlisted line class remains allowlisted and
  every previously unexpected line class remains unexpected
