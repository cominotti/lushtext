## ADDED Requirements

### Requirement: Bookmark gutter marks expose direct editing
The system SHALL open a bookmark edit dialog when the user activates an existing bookmark gutter mark in a saved, loaded editor. The dialog MUST identify the activated bookmark, show its current label and 1-based line number, and leave source file bytes unchanged while editing bookmark metadata.

#### Scenario: Activate a bookmark gutter mark
- **WHEN** the active saved editor contains a bookmark gutter mark
- **AND** the user activates that bookmark mark in the line-mark gutter
- **THEN** the system opens a bookmark edit dialog for that bookmark
- **AND** the dialog shows the bookmark label if one exists
- **AND** the dialog shows the bookmark line using the 1-based line number users expect

#### Scenario: Activate a non-bookmark line mark
- **WHEN** the user activates a line mark that is not a LushText bookmark
- **THEN** the system does not open the bookmark edit dialog
- **AND** persisted bookmark data remains unchanged

### Requirement: Bookmark edit dialog can reassign a bookmark line
The system SHALL let users update an existing bookmark's label and line number from the bookmark edit dialog. Saving a valid edit MUST move the same bookmark identity to the target line, refresh live bookmark presentation, and persist through the existing bookmark sidecar save path. Invalid edits MUST keep the existing bookmark unchanged.

#### Scenario: Save a bookmark with a new label
- **WHEN** the bookmark edit dialog is open for an existing bookmark
- **AND** the user changes the label and saves
- **THEN** the same bookmark record stores the new normalized label
- **AND** later bookmark tooltips, browse surfaces, and navigation labels show the updated label

#### Scenario: Move a bookmark to another line
- **WHEN** the bookmark edit dialog is open for an existing bookmark
- **AND** the user enters a valid target line that does not already contain another bookmark
- **AND** the user saves
- **THEN** the same bookmark identity moves to the target line
- **AND** the gutter indicator and minimap marker move to that line
- **AND** reopening the file restores the bookmark at the new line

#### Scenario: Reject an out-of-range target line
- **WHEN** the bookmark edit dialog is open for an existing bookmark
- **AND** the user enters a target line outside the active buffer's line range
- **AND** the user attempts to save
- **THEN** the system keeps the dialog open and reports validation feedback
- **AND** the bookmark line and label remain unchanged

#### Scenario: Reject a target line with another bookmark
- **WHEN** the bookmark edit dialog is open for one bookmark
- **AND** the user enters a target line that already contains a different bookmark
- **AND** the user attempts to save
- **THEN** the system keeps the dialog open and reports validation feedback
- **AND** neither bookmark is merged, overwritten, or removed
