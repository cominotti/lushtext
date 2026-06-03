## ADDED Requirements

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
