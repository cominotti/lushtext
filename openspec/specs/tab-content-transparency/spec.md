## Purpose
Persist and restore the user's tab-content transparency preference while keeping document surfaces visually consistent with the active GtkSourceView theme and keeping non-document chrome opaque.

## Requirements

### Requirement: Transparency preference is always available in editor appearance settings
The system SHALL expose a global tab-content opacity preference labeled `Background Opacity` in `Preferences > Editor > Appearance`. That preference SHALL always be visible, including when the current opacity is `100%`, and its visible percentage SHALL describe opacity rather than transparency.

#### Scenario: Preferences show the opacity control at the default value
- **WHEN** the user opens `Preferences > Editor > Appearance` on a fresh install or while the opacity is still at its default value
- **THEN** the dialog shows a `Background Opacity` preference
- **AND** the preference shows `100%` as the fully opaque default
- **AND** the preference is visible without any prior opt-in or hidden-state trigger

### Requirement: Transparency uses a Fedora-style row with percentage readout and popover slider
The system SHALL present the `Background Opacity` preference as a Fedora-style row with a live opacity percentage readout and a popover-hosted slider, rather than as an always-expanded inline slider. The row subtitle MUST explain that lower values make editor and Markdown preview document backgrounds more transparent.

#### Scenario: Opening the control reveals the slider
- **WHEN** the user activates the `Background Opacity` preference row
- **THEN** the UI shows a popover containing a slider for adjusting opacity
- **AND** the row continues to show the current opacity as a percentage

#### Scenario: The percentage readout updates with the slider
- **WHEN** the user changes the opacity slider value
- **THEN** the percentage readout updates during that interaction to reflect the new opacity value
- **AND** the percentage is not inverted into a transparency value

#### Scenario: Lower values are explained as more transparent
- **WHEN** the row subtitle is visible
- **THEN** it explains that lower values make editor and Markdown preview backgrounds more transparent
- **AND** it does not imply that `85%` means `85%` transparent

### Requirement: Transparency changes apply immediately and persist
The system SHALL persist the selected opacity value across launches. Changing the `Background Opacity` control SHALL update the active tab-content surfaces immediately without requiring an app restart.

#### Scenario: Changing the setting updates the current tab content immediately
- **WHEN** the user changes the opacity value while an editor tab or Markdown preview is visible
- **THEN** the current tab content updates in the same session
- **AND** the app does not require a restart before the new opacity is visible

#### Scenario: The selected opacity value is restored on restart
- **WHEN** the user sets a non-default opacity value, closes the app, and reopens it
- **THEN** the same opacity value is restored
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

### Requirement: Transparency-derived style schemes are cached under app data and regenerated safely
The system SHALL cache derived opacity-aware GtkSourceView style schemes under `$XDG_DATA_HOME/lushtext/style-schemes/` when non-opaque tab-content transparency needs a scheme variant that does not already exist in the active style-scheme search path. Missing cached derived schemes MUST regenerate automatically instead of disabling the transparency feature.

#### Scenario: First non-opaque use creates or reuses a derived cached scheme
- **WHEN** the user applies a transparency value below `100%` for a base style scheme that does not already have the required derived opacity variant loaded
- **THEN** the system creates or reuses a derived style-scheme file under the app data directory
- **AND** the editor can apply that derived scheme for the current tab-content transparency value

#### Scenario: Deleted derived cache is regenerated on demand
- **WHEN** a previously used derived transparency style-scheme file is missing from the app data cache
- **THEN** the system regenerates the needed derived scheme automatically the next time that opacity variant is required
- **AND** tab-content transparency continues to work without requiring the user to reset the preference
