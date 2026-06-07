## ADDED Requirements

### Requirement: External file modifications are covered end to end
The test suite SHALL cover the editor's live file monitor path from an actual
on-disk external modification through the user-visible inline alert.

#### Scenario: External modification shows the changed-on-disk warning
- **WHEN** a file-backed document is open in LushText
- **AND** another process modifies the backing file on disk
- **THEN** the editor shows the external-change warning for that document
- **AND** the current buffer content is not silently replaced

#### Scenario: External modification preserves local unsaved edits
- **WHEN** the editor has unsaved local edits
- **AND** another process modifies the backing file on disk
- **THEN** the warning is shown
- **AND** the unsaved local buffer content remains visible until the user chooses
  an explicit action

#### Scenario: External monitor test waits on behavior, not sleeps
- **WHEN** the monitor coverage waits for the changed-on-disk warning
- **THEN** it waits for an observable predicate with a timeout
- **AND** it does not pass by calling the notification reducer directly without
  exercising the file monitor path

### Requirement: Reload and discard actions are covered
The test suite SHALL cover the actions a user can take after an external file
change warning is displayed.

#### Scenario: Discard and reload restores disk bytes
- **WHEN** an external-change warning is visible
- **AND** the user chooses the discard-and-reload action
- **THEN** the editor reloads the current bytes from disk
- **AND** the warning is cleared for the newly loaded content

#### Scenario: Dismissing the warning does not reload
- **WHEN** an external-change warning is visible
- **AND** the user dismisses the warning without reloading
- **THEN** the editor keeps the current buffer content
- **AND** the document remains in the correct modified or unmodified state

### Requirement: LushText's own saves do not create false external-change alerts
The test suite SHALL prove that successful LushText saves update monitor state
so the app does not warn about its own durable writes.

#### Scenario: Own save suppresses stale warning
- **WHEN** LushText saves a modified file-backed document successfully
- **THEN** the editor updates its known backing-file state
- **AND** the file monitor does not show a changed-on-disk warning caused by that
  same save

#### Scenario: Failed save keeps unsaved-work signal
- **WHEN** LushText attempts to save and the write fails
- **THEN** the document remains modified
- **AND** the absence of an external-change warning does not hide the save
  failure feedback
