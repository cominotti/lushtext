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

### Requirement: Replace All preserves each replaced file's identity metadata
The system SHALL preserve each replaced file's on-disk identity metadata when
Replace All rewrites it through the shared atomic write path. A file rewritten by
Replace All MUST keep its prior permission (mode) bits and SHALL keep ownership,
POSIX ACLs, and extended attributes on a best-effort basis, matching the
guarantee for in-editor saves and undo restores.

#### Scenario: Replacing inside a restrictive file keeps its permissions
- **WHEN** Replace All rewrites a `0600` file in the workspace
- **THEN** the rewritten file on disk is still `0600`

#### Scenario: Undo restore keeps the file's permissions
- **WHEN** the user undoes a Replace All that rewrote an executable file
- **THEN** the restored file on disk is still marked executable

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

### Requirement: Replace All uses stable target write coordination
The system SHALL coordinate Replace All writes and undo restores through the same stable target write guard used by file-backed editor saves. Replace All MUST acquire the guard before reading a target file's original bytes and MUST keep the guard through the corresponding atomic write. Undo restore MUST acquire the same guard before checking current bytes and restoring originals.

#### Scenario: Replace All waits for in-progress save
- **WHEN** Replace All reaches a file that an editor save is currently writing
- **THEN** Replace All waits for the save's stable target guard
- **AND** it validates the file contents after the save finishes before applying replacements

#### Scenario: Undo waits for in-progress write
- **WHEN** Replace All undo targets a file that another in-app write is currently replacing
- **THEN** undo waits for the same stable target guard
- **AND** it does not restore stale bytes over an in-flight write

### Requirement: Replace All enforces file and undo memory caps
The system SHALL bound Replace All memory exposure with explicit service-level caps. A single target file larger than `10 * 1024 * 1024` bytes MUST be skipped and reported. The total persisted undo payload for one Replace All run MUST NOT exceed `64 * 1024 * 1024` bytes; files that would exceed that cap MUST be skipped and reported before they are modified.

#### Scenario: Oversized replace target is skipped
- **WHEN** Replace All includes a matching file larger than the per-file replace cap
- **THEN** the file is skipped without modification
- **AND** the result reports that path as skipped

#### Scenario: Undo payload cap skips later files before write
- **WHEN** adding another file's original and replacement bytes would exceed the total undo payload cap
- **THEN** that file is skipped before any replacement write occurs
- **AND** already-written files remain undoable

### Requirement: Replace All builds changed text without full line-vector amplification
The system SHALL avoid constructing a full `Vec<String>` of every line in a target file during Replace All. For accepted files, it MUST validate UTF-8 using the project's established large-file validation approach and build the replacement output in a bounded single-pass representation from the recorded replacement ranges.

#### Scenario: Large accepted file avoids line-vector allocation
- **WHEN** Replace All processes a file within the per-file cap but large enough to stress allocation
- **THEN** it does not split the entire file into a vector of owned line strings
- **AND** it still validates stale search results before writing

### Requirement: Replace All undo journal is incremental and durable per file
The system SHALL persist Replace All undo state as incremental per-file durable entries rather than rewriting the entire growing backup after each file. Each file's undo entry MUST be written and synced before that file is modified. Cleanup on undo, search-panel close, and startup MUST remove both the new journal directory and any stale legacy backup file.

#### Scenario: Per-file entry is durable before replacement
- **WHEN** Replace All is about to rewrite a target file
- **THEN** that file's undo entry is durably persisted first
- **AND** the workflow does not rewrite previously persisted entries

#### Scenario: Journal persistence scales linearly
- **WHEN** Replace All successfully rewrites many files
- **THEN** the number of durable journal entry writes grows linearly with the number of touched files
- **AND** the workflow does not serialize the full backup once per file

#### Scenario: Startup clears stale journal formats
- **WHEN** the app starts with stale Replace All undo state from a prior session
- **THEN** it deletes the new journal directory and the legacy `replace-backup.json` file
- **AND** the new session does not inherit old Replace All undo state

### Requirement: Replace All undo-journal corruption is isolated and diagnostic
The system SHALL handle malformed, partial, or unsupported Replace All undo-journal state as recoverable metadata. Corrupt journal entries MUST be preserved when possible, reported through recovery diagnostics, and MUST NOT be treated as a valid undo source.

#### Scenario: Malformed journal entry is preserved and excluded
- **WHEN** startup or search-panel initialization finds a malformed Replace All undo-journal entry
- **THEN** the malformed entry is quarantined or left untouched when quarantine fails
- **AND** the entry is not offered as an undo source
- **AND** a recovery diagnostic is recorded

#### Scenario: Partial journal does not imply successful undo state
- **WHEN** an undo-journal directory contains only some required metadata for a Replace All run
- **THEN** the system does not expose the Replace All undo action for that incomplete run
- **AND** surviving journal files are preserved or cleaned only after diagnostic handling succeeds

#### Scenario: Legacy backup corruption is reported during cleanup
- **WHEN** stale legacy `replace-backup.json` exists but cannot be parsed or removed at startup
- **THEN** the system records a diagnostic with the backup path and failure kind
- **AND** the new session does not claim that stale Replace All undo is available

### Requirement: Replace All journal cleanup is restart-safe
The system SHALL make cleanup of stale, completed, or invalid Replace All undo-journal state restart-safe. Cleanup MUST be ordered so that a crash during cleanup cannot convert a stale or corrupt journal into an active undo affordance after restart.

#### Scenario: Cleanup marker prevents resurrecting stale undo
- **WHEN** startup begins cleaning a stale Replace All journal and the process terminates before cleanup completes
- **THEN** relaunching LushText continues cleanup or reports the incomplete cleanup
- **AND** it does not expose the stale journal as an active undo affordance

#### Scenario: Undo completion removes journal durably
- **WHEN** the user successfully undoes a Replace All run
- **THEN** the journal is removed or marked inactive through a durable cleanup path
- **AND** restarting LushText cannot offer that completed undo again

#### Scenario: Cleanup failure remains diagnostic
- **WHEN** Replace All journal cleanup fails because of filesystem permissions or transient I/O errors
- **THEN** the system reports a diagnostic and retries later
- **AND** it does not spin in a tight retry loop

### Requirement: Replace All journal reliability has layered automated coverage
The project SHALL add deterministic service, integration, smoke, and property or fuzz-adjacent coverage for malformed undo journals, partial journals, restart-safe cleanup, and stale legacy backup handling.

#### Scenario: Service tests cover malformed journal entries
- **WHEN** service tests load malformed Replace All journal entries
- **THEN** the loader excludes those entries from active undo state
- **AND** it preserves or quarantines the original bytes and returns diagnostics

#### Scenario: Integration tests cover cleanup interruption
- **WHEN** tests simulate a crash or restart between stale journal cleanup steps
- **THEN** the next startup continues cleanup or reports incomplete cleanup
- **AND** the stale journal is never exposed as active undo state

#### Scenario: Generated journal states never produce invalid undo
- **WHEN** generated partial, duplicated, or corrupt journal directory states are loaded
- **THEN** the system never offers undo for a journal that lacks complete validated original bytes

#### Scenario: Crash smoke records stale journal handling when exercised
- **WHEN** crash recovery smoke creates or encounters Replace All journal state
- **THEN** the smoke artifacts record whether the journal was active, cleaned, quarantined, or skipped
