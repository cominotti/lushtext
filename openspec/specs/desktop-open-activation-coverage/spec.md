# desktop-open-activation-coverage Specification

## Purpose
Define LushText's desktop and CLI open-activation coverage so file launches,
metadata, and invalid-path handling stay aligned with user-facing behavior.

## Requirements
### Requirement: Application open activation is covered
The test suite SHALL cover the `ApplicationImpl::open` path used by file
manager, desktop, and CLI activation.

#### Scenario: Single file activation opens a tab
- **WHEN** the application receives a desktop or CLI open activation for a
  supported file
- **THEN** it opens or reuses an application window
- **AND** the file is opened in an editor tab with matching content

#### Scenario: Multiple file activation opens multiple tabs
- **WHEN** the application receives one open activation containing multiple
  supported files
- **THEN** each file is opened or focused according to normal duplicate-tab
  rules
- **AND** the app does not create duplicate tabs for the same canonical path

#### Scenario: Activation reuses the existing window
- **WHEN** LushText is already running
- **AND** a new file activation arrives
- **THEN** the existing application window is reused when possible
- **AND** the activated document becomes visible without creating an unnecessary
  second primary window

### Requirement: Activation handles invalid and inaccessible paths safely
The test suite SHALL cover activation paths that cannot be opened successfully.

#### Scenario: Missing file activation reports failure
- **WHEN** the application receives an open activation for a missing file
- **THEN** it reports an error through the normal user feedback path
- **AND** it does not create a misleading saved or clean editor tab for that
  missing path

#### Scenario: Inaccessible file activation does not crash
- **WHEN** the application receives an open activation for a path it cannot read
- **THEN** it keeps the app responsive
- **AND** it shows access failure feedback without corrupting open-path
  bookkeeping

### Requirement: Stale Failed-Tab Activation Regressions Are Covered
The test suite SHALL cover desktop and CLI activation behavior when existing
tabs include failed-load placeholders from session restore or earlier
activation attempts.

#### Scenario: Activation opens over restored failed placeholder
- **WHEN** a regression test seeds or constructs a failed restored tab for a
  path
- **AND** a later `ApplicationImpl::open` activation requests the same path
  after it becomes readable
- **THEN** the test verifies that the old failed tab remains present
- **AND** the requested file opens in a new selected tab with matching content

#### Scenario: Failed placeholder bookkeeping is cleared
- **WHEN** a regression test drives a load failure through the normal open path
- **THEN** the test verifies that duplicate-detection bookkeeping no longer
  contains the failed path
- **AND** a later activation for that path succeeds after the file becomes
  readable

#### Scenario: Modified failed placeholder does not block activation
- **WHEN** a regression test creates a failed-load tab that preserves modified
  buffer content
- **AND** a later activation requests the same path
- **THEN** the test verifies that the modified failed tab remains recoverable
- **AND** the activated file still opens and becomes selected

### Requirement: Activation Ordering And Duplicate Regressions Are Covered
The test suite SHALL cover activation ordering so explicit desktop or CLI opens
continue to cooperate with session restore, canonical duplicate detection, and
multi-file activation.

#### Scenario: Explicit activation remains selected after restored failure settles
- **WHEN** a regression test starts LushText with an explicit file activation
  and a prior session containing another tab that later fails to load
- **THEN** the test verifies that the explicit file remains the selected tab
- **AND** the restored failed tab does not steal focus after its error appears

#### Scenario: Successful duplicate activation still deduplicates
- **WHEN** a regression test activates an already loaded file, including a
  canonical duplicate such as a symlink when supported by the fixture
- **THEN** the test verifies that LushText focuses the existing loaded document
- **AND** it does not create duplicate tabs for the same canonical file

#### Scenario: Multi-file activation continues after an unsupported input
- **WHEN** a regression test sends one activation containing an unsupported
  URI-shaped `gio::File` and one readable local file
- **THEN** the test verifies visible failure feedback for the unsupported input
- **AND** the local file still opens with matching content

### Requirement: Non-Path Open Inputs Are Covered
The test suite SHALL include coverage for application open inputs that are valid
`gio::File` values but do not expose a local path.

#### Scenario: Non-path URI activation is not silent
- **WHEN** a regression test invokes `ApplicationImpl::open` with a non-path URI
  file
- **THEN** the test verifies that user-visible feedback is published
- **AND** no fake path-backed editor tab is created for the unsupported URI

#### Scenario: Reused window receives URI failure feedback
- **WHEN** LushText already has a window
- **AND** a non-path URI activation arrives
- **THEN** the test verifies that the existing window remains responsive
- **AND** the unsupported input is reported through that window's feedback
  surface

### Requirement: Desktop metadata open commands are verified against runtime behavior
Desktop integration verification SHALL connect static metadata to actual open
activation behavior.

#### Scenario: Desktop entry forwards document URIs
- **WHEN** the generated or staged desktop entry is inspected
- **THEN** its `Exec` line supports document activation with file arguments or
  URIs
- **AND** the verification proves that the same activation path opens a document
  in LushText

#### Scenario: CLI file arguments take priority over restored active selection
- **WHEN** LushText starts with explicit file arguments and a previous session
  also exists
- **THEN** the explicitly activated files are opened
- **AND** restored session state does not steal the active selection away from
  the activated file

### Requirement: D-Bus action exposure is covered
Desktop activation coverage SHALL include D-Bus introspection and activation
checks for LushText's public app and window actions. The checks MUST prove that
externally visible actions remain aligned with the action catalog and that
action activation does not bypass normal application behavior.

#### Scenario: App and window actions are introspectable
- **WHEN** LushText runs in a private D-Bus and headless Mutter session
- **THEN** the app-level `org.gtk.Actions` object lists documented app actions
- **AND** the window-level `org.gtk.Actions` object lists documented window
  actions
- **AND** each externally supported action appears in the action catalog with
  matching parameter and state types

#### Scenario: D-Bus activation drives normal action path
- **WHEN** a smoke helper activates a documented public action over D-Bus
- **THEN** LushText executes the same workflow as the corresponding in-app
  control, shortcut, menu item, or command-palette entry
- **AND** the automation snapshot or widget state verifies the expected result

#### Scenario: Action state remains observable
- **WHEN** a stateful action such as sidebar, properties, minimap, focus mode,
  preview, or fullscreen state changes
- **THEN** the exported action state and automation snapshot agree after the app
  settles

### Requirement: Desktop D-Bus activation metadata is validated before adoption
The project SHALL validate any added `DBusActivatable=true`, desktop actions,
or additional desktop-entry D-Bus metadata against native, staged development,
Flatpak, Snap, CLI, MIME, and file-manager activation behavior before accepting
it.

#### Scenario: Desktop D-Bus activation preserves file opening
- **WHEN** a generated, staged, Flatpak, or Snap desktop entry advertises D-Bus
  activation
- **THEN** opening one or more local files through the desktop or file manager
  still reaches `ApplicationImpl::open`
- **AND** duplicate-tab, failed-placeholder, unsupported-URI, and
  explicit-selection behavior remains covered by the existing activation
  requirements

#### Scenario: Desktop actions launch supported commands
- **WHEN** a desktop entry advertises additional desktop actions
- **THEN** each advertised action maps to a documented app action
- **AND** invoking it through GLib or desktop tools activates the expected
  behavior without requiring a pre-existing window unless that requirement is
  documented

#### Scenario: Metadata is not enabled without proof
- **WHEN** D-Bus activation metadata cannot be proven for a packaging target
- **THEN** the change leaves that metadata disabled for the target or documents
  the blocker
- **AND** existing `Exec` and MIME activation behavior remains unchanged

### Requirement: D-Bus activation documentation SHALL stay synchronized
The project SHALL document how to inspect and activate supported app/window
actions through D-Bus, including object paths, action scopes, parameter
examples, stateful actions, and known packaging differences.

#### Scenario: Developer docs include D-Bus activation examples
- **WHEN** developers read the automation documentation
- **THEN** it includes working examples for listing actions, describing an
  action, activating a simple action, activating a parameterized action, and
  reading stateful action state

#### Scenario: Desktop activation docs track metadata
- **WHEN** desktop entry D-Bus activation metadata, desktop actions, or
  application IDs change
- **THEN** README, automation docs, packaging docs, and validation scripts are
  updated in the same change
