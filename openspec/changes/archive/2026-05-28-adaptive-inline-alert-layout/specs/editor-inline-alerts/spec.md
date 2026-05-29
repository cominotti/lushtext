# Spec Delta: editor-inline-alerts

## MODIFIED Requirements

### Requirement: Inline alert controls remain horizontally grouped in constrained widths
The system SHALL keep visible editor inline-alert controls adjacent in a horizontal group. The action group MAY wrap onto its own row beneath the alert message when the editor column is too narrow to fit the message and controls on one line, but it MUST wrap as one atomic unit. Alert message text MAY wrap above the controls, and visible controls MUST NOT be split into separate rows or separated into unrelated alert regions.

#### Scenario: Warning controls remain adjacent in a narrow editor
- **WHEN** a warning inline alert with primary, secondary, and dismiss controls is shown in a narrow editor column
- **THEN** all visible controls remain in one horizontal group
- **AND** each visible control receives a positive allocation

#### Scenario: Error controls remain adjacent in a narrow editor
- **WHEN** an error inline alert with retry and dismiss controls is shown in a narrow editor column
- **THEN** retry and dismiss remain in one horizontal group
- **AND** each visible control receives a positive allocation

#### Scenario: Action group wraps as one unit beneath the message
- **WHEN** an inline alert's message and action group cannot fit on one line
- **THEN** the entire action group wraps onto its own row beneath the message
- **AND** the action group's buttons remain in one horizontal group on that row

## ADDED Requirements

### Requirement: Editor inline alerts adapt message and action placement to editor width
The system SHALL present the alert message and the action group on a single horizontal line when the editor column is wide enough to fit both, and SHALL wrap the action group onto its own row beneath the message when the column is not wide enough. The adaptive layout MUST be implemented with a supported libadwaita wrapping container and MUST NOT reintroduce `GtkInfoBar`.

#### Scenario: Wide editor shows message and actions on one line
- **WHEN** an inline alert is shown in an editor column wide enough for the message and action group
- **THEN** the message and the action group occupy the same horizontal line
- **AND** the action group is aligned to the trailing edge

#### Scenario: Narrow editor wraps actions beneath the message
- **WHEN** an inline alert is shown in an editor column too narrow for the message and action group on one line
- **THEN** the action group wraps onto its own row beneath the message
- **AND** the message text remains readable and wraps within the column

#### Scenario: Adaptive layout uses a supported wrapping container
- **WHEN** the editor inline alert template is loaded
- **THEN** the message and action group are hosted by a libadwaita `AdwWrapBox`
- **AND** the template does not instantiate `GtkInfoBar`
