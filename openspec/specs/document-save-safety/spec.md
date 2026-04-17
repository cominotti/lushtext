# document-save-safety Specification

## Purpose
TBD - created by archiving change data-home-persistence-contracts. Update Purpose after archive.
## Requirements
### Requirement: File-backed saves write atomically and keep the document dirty on failure
The system SHALL write file-backed saves using an atomic temp-file-then-rename path instead of overwriting the destination in place. If the write or finalization fails, the document MUST remain modified so the user still has an obvious unsaved-work signal and can retry safely.

#### Scenario: Successful save replaces the destination atomically
- **WHEN** the user saves a file-backed document successfully
- **THEN** the destination file is updated through an atomic write path
- **AND** the editor transitions to an unmodified state only after the write succeeds

#### Scenario: Failed save leaves the document marked modified
- **WHEN** the user saves a file-backed document and the write fails
- **THEN** the editor remains marked modified
- **AND** the user receives failure feedback instead of losing the unsaved-work signal

### Requirement: Very large saves still write one consistent document snapshot
The system SHALL capture one consistent buffer snapshot before the background save write begins. For very large documents that require incremental snapshot capture, the system MUST prevent user edits from mutating the bytes being written mid-save.

#### Scenario: Very large save writes one stable snapshot
- **WHEN** the user saves a very large modified document whose buffer snapshot is captured incrementally
- **THEN** the bytes written to disk reflect one consistent document snapshot
- **AND** intervening user edits do not race into the in-flight write

### Requirement: Save As adopts the destination only after a successful write
The system SHALL treat `Save As` as a state transition that completes only after the destination write succeeds. On failure, the editor MUST keep its prior identity and MUST NOT register the destination as the active document path.

#### Scenario: Successful Save As adopts the destination path
- **WHEN** `Save As` completes successfully
- **THEN** the editor adopts the destination path as its active file identity
- **AND** later window bookkeeping treats that destination as the open document path

#### Scenario: Failed Save As leaves the prior identity untouched
- **WHEN** `Save As` fails before the destination write is finalized
- **THEN** the editor keeps its prior file or untitled identity
- **AND** the failed destination is not treated as the active open document

### Requirement: Modified documents require an explicit close or discard decision
The system SHALL require an explicit `Save`, `Discard`, or `Cancel` decision before a modified document is closed. The save-on-close path MUST NOT silently treat untitled documents as saved; untitled documents MUST be saved through `Save As` or explicitly discarded.

#### Scenario: Closing a modified file-backed tab prompts for a decision
- **WHEN** the user closes a modified file-backed tab or window
- **THEN** the system presents a save-changes decision flow
- **AND** the tab or window does not close until the user chooses `Save`, `Discard`, or `Cancel`

#### Scenario: Save-on-close blocks untitled documents from being treated as saved
- **WHEN** the user chooses the save path for a close flow that includes an untitled modified document
- **THEN** the close flow does not proceed as though that untitled document was saved
- **AND** the untitled draft remains available until the user uses `Save As` or explicitly discards it

### Requirement: External file changes surface a reload path without silently overwriting editor content
The system SHALL monitor file-backed documents for external on-disk changes and surface a warning when the backing file's mtime changes. The system MUST keep the current editor buffer intact until the user explicitly chooses to discard changes and reload from disk.

#### Scenario: External modification shows a warning
- **WHEN** another program changes the backing file of an open file-backed document
- **THEN** the editor shows a warning that the file changed on disk
- **AND** the current buffer content is not silently replaced

#### Scenario: Discard and reload restores current on-disk bytes
- **WHEN** the user chooses to discard local changes and reload after an external modification warning
- **THEN** the editor reloads the file from disk
- **AND** the warning is cleared for the newly loaded on-disk content

