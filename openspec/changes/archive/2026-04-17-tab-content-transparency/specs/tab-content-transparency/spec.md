## ADDED Requirements

### Requirement: Transparency preference is always available in editor appearance settings
The system SHALL expose a global `Transparency` preference in `Preferences > Editor > Appearance`. That preference SHALL always be visible, including when the current opacity is `100%`.

#### Scenario: Preferences show the transparency control at the default value
- **WHEN** the user opens `Preferences > Editor > Appearance` on a fresh install or while the opacity is still at its default value
- **THEN** the dialog shows a `Transparency` preference
- **AND** the preference is visible without any prior opt-in or hidden-state trigger

### Requirement: Transparency uses a Fedora-style row with percentage readout and popover slider
The system SHALL present the `Transparency` preference as a row with a live percentage readout and a popover-hosted slider, rather than as an always-expanded inline slider.

#### Scenario: Opening the control reveals the slider
- **WHEN** the user activates the `Transparency` preference row
- **THEN** the UI shows a popover containing a slider for adjusting opacity
- **AND** the row continues to show the current opacity as a percentage

#### Scenario: The percentage readout updates with the slider
- **WHEN** the user changes the transparency slider value
- **THEN** the percentage readout updates during that interaction to reflect the new value

### Requirement: Transparency changes apply immediately and persist
The system SHALL persist the selected transparency value across launches. Changing the control SHALL update the active tab-content surfaces immediately without requiring an app restart.

#### Scenario: Changing the setting updates the current tab content immediately
- **WHEN** the user changes the transparency value while an editor tab or Markdown preview is visible
- **THEN** the current tab content updates in the same session
- **AND** the app does not require a restart before the new transparency is visible

#### Scenario: The selected transparency value is restored on restart
- **WHEN** the user sets a non-default transparency value, closes the app, and reopens it
- **THEN** the same transparency value is restored
- **AND** tab content renders with that restored value

### Requirement: Editor and Markdown preview backgrounds share the same transparency behavior
The system SHALL apply the selected transparency value to the editor document background and to the Markdown preview background. Both surfaces MUST continue to follow the active light or dark appearance and the active editor color scheme rather than switching to a hardcoded neutral background.

#### Scenario: Source editing reflects the selected transparency
- **WHEN** the user views a normal text or code tab after setting a non-default transparency value
- **THEN** the editor document background renders with that transparency
- **AND** the editor text remains readable against the selected color scheme

#### Scenario: Markdown preview reflects the same transparency
- **WHEN** the user opens Markdown preview for a document after setting a non-default transparency value
- **THEN** the preview background renders with the same transparency value as the editor surface
- **AND** switching between source and preview does not reset or diverge from the selected transparency

#### Scenario: Theme changes preserve transparency behavior
- **WHEN** the user changes the editor color scheme or the app appearance changes between light and dark
- **THEN** the editor and Markdown preview continue using the selected transparency value
- **AND** their background tint updates to remain consistent with the newly active appearance

### Requirement: Non-document chrome remains opaque
The system SHALL keep all non-document chrome opaque while tab content transparency is enabled. This includes the top chrome, side panels, bottom chrome, minimap, and document-adjacent helper chrome.

#### Scenario: Shell chrome stays opaque while transparency is enabled
- **WHEN** the user enables a non-default transparency value and views the main window
- **THEN** the header bar and tab-strip chrome remain opaque
- **AND** the workspace sidebar and properties panel remain opaque
- **AND** the status bar and search-panel chrome remain opaque

#### Scenario: Editor helpers stay opaque while document surfaces change
- **WHEN** the user enables a non-default transparency value on an editor tab
- **THEN** the minimap remains opaque
- **AND** infobars and in-editor find or replace chrome remain opaque
- **AND** only the document-reading surfaces adopt the selected transparency
