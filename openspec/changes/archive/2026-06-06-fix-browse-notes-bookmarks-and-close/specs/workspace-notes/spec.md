## ADDED Requirements

### Requirement: Notes browser exposes reliable dismissal controls
The system SHALL provide a visible Close affordance in the workspace-scoped `Browse Notes...` surface. The close affordance MUST be available in populated and empty browser states, and keyboard dismissal MUST NOT require the user to first click inside the dialog content.

#### Scenario: Close populated notes browser from the sidebar page
- **WHEN** `Browse Notes...` is open with at least one result
- **AND** the sidebar page is visible
- **THEN** the user can invoke a visible Close control to dismiss the dialog

#### Scenario: Close populated notes browser from the preview page
- **WHEN** `Browse Notes...` is open with at least one result
- **AND** the preview page is visible
- **THEN** the user can invoke a visible Close control to dismiss the dialog

#### Scenario: Close empty notes browser
- **WHEN** `Browse Notes...` opens in its empty state
- **THEN** the user can invoke a visible Close control to dismiss the dialog

#### Scenario: Dismiss notes browser with Escape after opening
- **WHEN** `Browse Notes...` is open
- **AND** the user presses `Escape` without first clicking inside the dialog
- **THEN** the dialog closes
