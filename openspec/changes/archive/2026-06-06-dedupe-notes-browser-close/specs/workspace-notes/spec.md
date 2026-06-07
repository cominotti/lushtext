## ADDED Requirements

### Requirement: Notes browser exposes a single adaptive close affordance
The system SHALL provide a visible Close/X affordance in the `Browse Notes...` surface without showing duplicate equivalent dialog-close controls. In populated unfolded layouts where the notes sidebar and preview are both visible at the same time, the browser MUST expose exactly one visible Close/X control for the dialog. In populated collapsed layouts, the currently visible notes-browser page MUST still have an obvious visible Close/X control available, whether that page is the sidebar or the preview. The empty notes-browser state MUST also expose one visible Close/X control. Keyboard dismissal with `Escape` MUST close the dialog immediately after opening without requiring the user to first click inside the dialog content.

#### Scenario: Unfolded notes browser has one close control
- **WHEN** `Browse Notes...` is open with at least one result
- **AND** the notes browser is wide enough to show the sidebar and preview at the same time
- **THEN** the user sees exactly one visible Close/X control for dismissing the dialog
- **AND** invoking that Close/X control dismisses the dialog

#### Scenario: Collapsed sidebar page remains visibly dismissible
- **WHEN** `Browse Notes...` is open with at least one result
- **AND** the notes browser is collapsed with the sidebar page visible
- **THEN** the user can invoke a visible Close/X control to dismiss the dialog

#### Scenario: Collapsed preview page remains visibly dismissible
- **WHEN** `Browse Notes...` is open with at least one result
- **AND** the notes browser is collapsed with the preview page visible
- **THEN** the user can invoke a visible Close/X control to dismiss the dialog
- **AND** the preview page's Back control remains navigation rather than dialog dismissal

#### Scenario: Empty notes browser remains visibly dismissible
- **WHEN** `Browse Notes...` opens in its empty state
- **THEN** the user sees exactly one visible Close/X control for dismissing the dialog
- **AND** invoking that Close/X control dismisses the dialog

#### Scenario: Escape closes notes browser immediately after opening
- **WHEN** `Browse Notes...` is open
- **AND** the user presses `Escape` without first clicking inside the dialog
- **THEN** the dialog closes
