## ADDED Requirements

### Requirement: The Skipped Files dialog is accessible
The dialog SHALL have an accessible name, and its reason groups SHALL be headings with exact counts. Each row SHALL expose the file name as its label and the folder and reason as its description, and each Open and Replace in Editor button SHALL name the file in its description. Show Skipped Files SHALL be reachable by keyboard from the completion row. Escape SHALL close the dialog and return focus to it.

#### Scenario: Keyboard-only review of skipped files
- **WHEN** a keyboard user finishes a Replace All that skipped files
- **THEN** Tab reaches Show Skipped Files, Enter opens the dialog, each row's action is reachable, and Escape returns focus to Show Skipped Files

### Requirement: Long Replace All runs are announced once
The Cancel Replace All control SHALL keep the confirm button's keyboard position. Replace All progress SHALL be announced once when it first appears, never on a heartbeat. The completion message SHALL be announced once and SHALL state the skipped count.

#### Scenario: Screen reader during a long run
- **WHEN** a Replace All run lasts ten seconds
- **THEN** a screen reader hears the progress announced once and the completion once, not once per heartbeat
