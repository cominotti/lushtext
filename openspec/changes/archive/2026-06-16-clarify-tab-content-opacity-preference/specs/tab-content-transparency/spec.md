## MODIFIED Requirements

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
