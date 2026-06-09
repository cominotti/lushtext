## ADDED Requirements

### Requirement: D-Bus action exposure is covered
Desktop activation coverage SHALL include D-Bus introspection and activation checks for LushText's public app and window actions. The checks MUST prove that externally visible actions remain aligned with the action catalog and that action activation does not bypass normal application behavior.

#### Scenario: App and window actions are introspectable
- **WHEN** LushText runs in a private D-Bus and headless Mutter session
- **THEN** the app-level `org.gtk.Actions` object lists documented app actions
- **AND** the window-level `org.gtk.Actions` object lists documented window actions
- **AND** each externally supported action appears in the action catalog with matching parameter and state types

#### Scenario: D-Bus activation drives normal action path
- **WHEN** a smoke helper activates a documented public action over D-Bus
- **THEN** LushText executes the same workflow as the corresponding in-app control, shortcut, menu item, or command-palette entry
- **AND** the automation snapshot or widget state verifies the expected result

#### Scenario: Action state remains observable
- **WHEN** a stateful action such as sidebar, properties, minimap, focus mode, preview, or fullscreen state changes
- **THEN** the exported action state and automation snapshot agree after the app settles

### Requirement: Desktop D-Bus activation metadata is validated before adoption
If this change adds `DBusActivatable=true`, desktop actions, or additional desktop-entry D-Bus metadata, the project SHALL validate the metadata against native, staged development, Flatpak, Snap, CLI, MIME, and file-manager activation behavior before accepting it.

#### Scenario: Desktop D-Bus activation preserves file opening
- **WHEN** a generated, staged, Flatpak, or Snap desktop entry advertises D-Bus activation
- **THEN** opening one or more local files through the desktop or file manager still reaches `ApplicationImpl::open`
- **AND** duplicate-tab, failed-placeholder, unsupported-URI, and explicit-selection behavior remains covered by the existing activation requirements

#### Scenario: Desktop actions launch supported commands
- **WHEN** a desktop entry advertises additional desktop actions
- **THEN** each advertised action maps to a documented app action
- **AND** invoking it through GLib or desktop tools activates the expected behavior without requiring a pre-existing window unless that requirement is documented

#### Scenario: Metadata is not enabled without proof
- **WHEN** D-Bus activation metadata cannot be proven for a packaging target
- **THEN** the change leaves that metadata disabled for the target or documents the blocker
- **AND** existing `Exec` and MIME activation behavior remains unchanged

### Requirement: D-Bus activation documentation SHALL stay synchronized
The project SHALL document how to inspect and activate supported app/window actions through D-Bus, including object paths, action scopes, parameter examples, stateful actions, and known packaging differences.

#### Scenario: Developer docs include D-Bus activation examples
- **WHEN** developers read the automation documentation
- **THEN** it includes working examples for listing actions, describing an action, activating a simple action, activating a parameterized action, and reading stateful action state

#### Scenario: Desktop activation docs track metadata
- **WHEN** desktop entry D-Bus activation metadata, desktop actions, or application IDs change
- **THEN** README, automation docs, packaging docs, and validation scripts are updated in the same change
