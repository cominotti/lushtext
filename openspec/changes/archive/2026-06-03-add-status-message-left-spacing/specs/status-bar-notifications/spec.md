## ADDED Requirements

### Requirement: Status-bar message area is visually separated from the workspace toggle
The system SHALL keep a small horizontal gap between the workspace-sidebar toggle and the status-bar message area. The gap MUST be outside the flashing message-area background, so visible notification flashes begin after the gap and do not visually merge with the workspace toggle.

#### Scenario: Info flash starts after the workspace toggle gap
- **WHEN** an info status-bar notification becomes visible
- **THEN** the message-area flash begins after a small horizontal gap from the workspace-sidebar toggle
- **AND** the gap does not use the message-area flash background
- **AND** the workspace-sidebar toggle remains outside the flash background

#### Scenario: Severity flashes preserve the left gap
- **WHEN** warning and error status-bar notifications become visible
- **THEN** each message-area flash preserves the same small horizontal gap from the workspace-sidebar toggle
- **AND** the document metadata controls remain outside the flash background

#### Scenario: Message text keeps its compact alignment
- **WHEN** a status-bar notification is rendered
- **THEN** the message text keeps its existing compact offset inside the message area
- **AND** the added gap does not increase the status-bar height
