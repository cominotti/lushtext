# search-replace-safety Specification

## Purpose
TBD - created by archiving change data-home-persistence-contracts. Update Purpose after archive.
## Requirements
### Requirement: Replace All writes files atomically and skips explicitly unsafe targets
The system SHALL apply Replace All writes through atomic temp-file-then-rename file updates rather than in-place overwrites. The workflow MUST skip targets the UI marks unsafe to replace, such as open files with unsaved changes, and MUST report those skipped paths without modifying them.

#### Scenario: Replace All skips an explicitly unsafe file
- **WHEN** the Replace All workflow includes a file that the UI marks unsafe to modify in place
- **THEN** the system skips that file during replacement
- **AND** the file is reported as skipped instead of being modified

#### Scenario: Replace All updates a safe file through an atomic write
- **WHEN** Replace All modifies a safe target file
- **THEN** the file is rewritten through an atomic write path
- **AND** the workflow does not rely on an in-place partial overwrite

### Requirement: Replace All validates stale search results per file before writing
The system SHALL verify that each candidate file still matches the searched line content before applying replacements for that file. If the file has changed since the search result was produced, the system MUST skip that file rather than partially applying stale replacements.

#### Scenario: Stale search result skips the whole file
- **WHEN** Replace All reaches a file whose current line content no longer matches the recorded search result
- **THEN** the system skips replacement for that file
- **AND** it does not partially apply a subset of that file's stale replacements

### Requirement: Cancelling Replace All rolls back already-applied file writes
The system SHALL attempt to restore already-written files to their original content when cancellation interrupts a Replace All run.

#### Scenario: Cancellation restores earlier files
- **WHEN** Replace All is cancelled after one or more files were already updated
- **THEN** the system restores those already-updated files from the preserved original bytes
- **AND** cancellation does not leave a partially applied multi-file replace result when rollback succeeds

### Requirement: Successful Replace All persists a temporary undo backup for same-session recovery
The system SHALL persist original file bytes for a successful Replace All run to `$XDG_DATA_HOME/lushtext/replace-backup.json` so the user can undo that replace within the same safety window.

#### Scenario: Successful Replace All stores an undo backup
- **WHEN** Replace All completes successfully for one or more files
- **THEN** the system persists an undo backup containing the original file bytes
- **AND** the UI exposes an immediate undo path for that replace result

#### Scenario: Undo restores the original file bytes
- **WHEN** the user invokes the Replace All undo action while the undo backup is still available
- **THEN** the system restores the affected files from the persisted original bytes
- **AND** the replacement result is reversed without requiring the user to re-run search first

### Requirement: Replace All undo backup lifetime is bounded to the active safety window
The system SHALL treat `replace-backup.json` as temporary safety state rather than durable user history. The persisted backup MUST be cleared when the search panel closes, when undo completes, and when a later app session starts with stale backup state from an older run.

#### Scenario: Closing the search panel clears the persisted undo backup
- **WHEN** the search panel is closed after a replace run created an undo backup
- **THEN** the persisted undo backup is deleted
- **AND** the replace undo path does not outlive the panel-close safety boundary

#### Scenario: Startup clears stale replace backup from an earlier session
- **WHEN** the app starts and stale persisted Replace All backup data exists from an earlier session
- **THEN** the stale backup is deleted during startup
- **AND** the new session does not inherit an old Replace All undo state

