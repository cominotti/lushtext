## ADDED Requirements

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
