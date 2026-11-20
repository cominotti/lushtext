# Spec Delta

## ADDED Requirements

### Requirement: A confirmed Replace All waits for undo-reservation capacity
When a confirmed Replace All cannot reserve its undo-descriptor memory because other work holds the shared disposal capacity, the system SHALL keep the confirmed selection and the claimed transaction and SHALL start the run once capacity allows. It MUST NOT discard the selection or ask the user to preview again because of that refusal.

#### Scenario: Confirm while a file-index build holds the lane
- **WHEN** the user confirms a Replace All of three files while a workspace file-index build holds the disposal capacity the undo reservation needs
- **THEN** no file is touched while the build holds it
- **AND** once the build releases it, the run starts without another preview or Confirm, and every file holds the replaced text and is undoable

#### Scenario: A file becomes unsaved while the run waits
- **WHEN** a confirmed run is waiting for capacity and one of its files, already open in a tab, is edited without saving
- **THEN** that file is skipped as open with unsaved changes when the run starts, and the others are replaced

#### Scenario: Every file becomes unsaved while the run waits
- **WHEN** every file of a waiting run has been edited without saving in an open tab by the time capacity allows it
- **THEN** no file is written, and the report lists each file as open with unsaved changes

#### Scenario: Capacity is still short when it next changes
- **WHEN** capacity changes while a run waits but is still not enough for its reservation
- **THEN** the run keeps waiting, with its selection and transaction held

### Requirement: A run waiting for capacity can be cancelled with nothing written
A run waiting for capacity SHALL behave as a run in flight: Cancel, and Stop and Close from the close-during-run question, MUST end the wait at once with no file written and the earlier Undo still offered. Hiding the panel or starting a new search MUST NOT cancel it. Destroying the window MUST end the wait with no file written and release its transaction.

#### Scenario: Cancel while waiting
- **WHEN** the user activates Cancel Replace All while the run waits for capacity
- **THEN** the wait ends at once, no file changes, the status says nothing was changed, and the Undo of the earlier Replace All is still available and restores its files

#### Scenario: Escape while waiting
- **WHEN** the user hides the search panel while the run waits for capacity
- **THEN** the run still starts once capacity allows and replaces the files

#### Scenario: Close while waiting
- **WHEN** the user closes the window while the run waits for capacity
- **THEN** the close-during-run question is shown, and Stop and Close closes the window with no file written

#### Scenario: The window is destroyed while waiting
- **WHEN** the panel of a waiting run is disposed
- **THEN** the wait is cancelled, no file of that run is written even after capacity frees, and another window can then start a Replace All

#### Scenario: A cancelled wait keeps the earlier skip report
- **WHEN** an earlier Replace All offered Show Skipped Files and the user cancels a later run while it waits for capacity
- **THEN** Show Skipped Files still lists the earlier run's skipped files

### Requirement: A run waiting for capacity holds the application's transaction
While a confirmed run waits for capacity in one window, an apply or Undo in another window SHALL be refused with the same status message as during a run, and Undo SHALL stay unavailable in the waiting window. The earlier Undo of every window SHALL stay installed until the waiting run is admitted, and is retired only then.

#### Scenario: Apply in another window while one waits
- **WHEN** window B's confirmed run is waiting for capacity and the user confirms a Replace All in window A
- **THEN** window A's apply is refused with the "Another window is running Replace All" message, and no file is touched by it

#### Scenario: Undo in another window while one waits
- **WHEN** window A offers Undo for an earlier Replace All, window B's confirmed run is waiting for capacity, and the user activates Undo in window A
- **THEN** window A's Undo is refused with the same message, and its files keep the earlier run's text

### Requirement: The wait for capacity is reported accessibly
While a confirmed run waits for capacity, the system SHALL keep a waiting message in the status bar for the whole wait, except while a workspace search shows its own progress, announce it once, describe it in the Cancel Replace All button's accessible description, and report it in the workflow evidence and the automation snapshot. The `search-complete` readiness SHALL stay pending.

#### Scenario: State while waiting
- **WHEN** a confirmed run is waiting for capacity
- **THEN** the button reads Cancel Replace All, the status bar says the run is waiting for memory, the automation snapshot reports `content_search.replace_admission_deferred` true and `replace_in_flight` true, and the waiting message is announced once
- **AND** `search-complete` readiness is pending, and the waiting window's Undo is unavailable

#### Scenario: State after the run starts
- **WHEN** capacity allows the waiting run to start
- **THEN** `replace_admission_deferred` is false, and the waiting text gives way to the run's progress

#### Scenario: A new search during the wait
- **WHEN** the user runs a new workspace search while a confirmed run waits for capacity
- **THEN** the search's own progress is shown while it runs, the status bar shows the waiting message again within a second of the search ending, and the run still starts once capacity allows
