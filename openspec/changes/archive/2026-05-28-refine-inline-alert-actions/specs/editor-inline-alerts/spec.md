## ADDED Requirements

### Requirement: Inline alert controls share one horizontal action group
The system SHALL present every visible editor inline-alert control in one horizontal action group. The dismiss control MUST appear in the same action group as workflow action buttons and MUST be ordered after retry, discard, save, normalize, or other workflow actions.

#### Scenario: Restored draft alert groups discard save and dismiss
- **WHEN** a "Draft Changes Restored" warning inline alert includes discard and save actions
- **THEN** the alert shows `Discard...`, `Save...`, and dismiss controls in one horizontal action group
- **AND** the dismiss control appears after the save control

#### Scenario: Error alert groups retry and dismiss
- **WHEN** an error inline alert includes a retry action
- **THEN** the alert shows retry and dismiss controls in one horizontal action group
- **AND** the dismiss control appears after the retry control

#### Scenario: Informational warning still exposes dismiss in the action group
- **WHEN** a warning inline alert includes no workflow action labels
- **THEN** the alert shows no empty workflow action buttons
- **AND** the alert still shows the dismiss control in the horizontal action group

### Requirement: Inline alert controls remain horizontally grouped in constrained widths
The system SHALL keep visible editor inline-alert controls adjacent in a horizontal group when the editor column narrows. Alert message text MAY wrap above the controls, but visible controls MUST NOT be split into separate rows or separated into unrelated alert regions.

#### Scenario: Warning controls remain adjacent in a narrow editor
- **WHEN** a warning inline alert with primary, secondary, and dismiss controls is shown in a narrow editor column
- **THEN** all visible controls remain in one horizontal group
- **AND** each visible control receives a positive allocation

#### Scenario: Error controls remain adjacent in a narrow editor
- **WHEN** an error inline alert with retry and dismiss controls is shown in a narrow editor column
- **THEN** retry and dismiss remain in one horizontal group
- **AND** each visible control receives a positive allocation

### Requirement: Inline alert buttons have subtle contrast against alert surfaces
The system SHALL style editor inline-alert buttons so their resting, hover, and active states are discernible against warning and error alert backgrounds. The styling MUST remain scoped to inline-alert buttons and MUST preserve Adwaita-compatible warning and error surfaces.

#### Scenario: Warning alert buttons are distinguishable from the warning background
- **WHEN** a warning inline alert is shown
- **THEN** each visible inline-alert button has a subtle surface or border that distinguishes it from the warning background
- **AND** hovering a button makes that distinction slightly stronger

#### Scenario: Error alert buttons are distinguishable from the error background
- **WHEN** an error inline alert is shown
- **THEN** each visible inline-alert button has a subtle surface or border that distinguishes it from the error background
- **AND** hovering a button makes that distinction slightly stronger

#### Scenario: Alert button styling is scoped
- **WHEN** inline-alert button styling is applied
- **THEN** the styling is scoped to buttons inside editor inline alerts
- **AND** unrelated editor, sidebar, status-bar, dialog, and search-panel buttons are not targeted by the inline-alert contrast rules
