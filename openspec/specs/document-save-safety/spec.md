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

### Requirement: File-backed saves preserve the destination's identity metadata
The system SHALL preserve a file-backed document's on-disk identity metadata
across a save that overwrites an existing file. After a successful save, the file
MUST retain its prior permission (mode) bits and SHALL retain ownership, POSIX
ACLs, and extended attributes on a best-effort basis, so editing a file does not
silently change who can read or execute it.

#### Scenario: Saving an executable script keeps it executable
- **WHEN** the user edits and saves a file that was marked executable
- **THEN** the saved file on disk is still marked executable

#### Scenario: Saving a private file keeps its restrictive permissions
- **WHEN** the user edits and saves a file whose mode is `0600`
- **THEN** the saved file on disk is still `0600` and is not widened to be group- or world-readable

### Requirement: Save failures distinguish unwritten changes from undurable writes
The system SHALL report a save that failed before the destination was replaced
differently from a save whose bytes reached the destination but whose directory
durability could not be confirmed. A before-rename failure MUST tell the user the
changes were not written and keep the document modified. An after-rename
durability failure MUST tell the user the changes are on disk but not yet
confirmed durable, and MUST keep the document modified so a retry can re-attempt
the directory flush, rather than presenting a generic lost-save error.

#### Scenario: Pre-rename failure reports unwritten changes
- **WHEN** a save fails while writing or renaming the temp file
- **THEN** the editor reports that the changes were not written
- **AND** the document remains marked modified

#### Scenario: Post-rename durability failure reports a distinct warning
- **WHEN** a save replaces the destination but the directory durability sync fails
- **THEN** the editor surfaces a durability warning that the changes are on disk but not yet confirmed durable
- **AND** the document remains marked modified so the user can retry
- **AND** the failure is not presented as an indistinguishable lost save

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

### Requirement: Symlink-backed saves update the resolved target
The system SHALL preserve symlink semantics for file-backed saves. When a document is opened through a symlink, saving the document MUST update the resolved target file and MUST NOT replace the symlink path with a regular file. The editor MUST keep a coherent display path and canonical write target so duplicate detection and write coordination treat the symlink and target as the same document.

#### Scenario: Saving through a symlink keeps the symlink
- **WHEN** the user opens a document through a symlink and saves changes
- **THEN** the symlink still exists as a symlink
- **AND** the resolved target file contains the saved bytes

#### Scenario: Unresolvable symlink target fails before replace
- **WHEN** a symlink-backed document's resolved target cannot be written
- **THEN** the save fails before replacing the symlink path
- **AND** the document remains marked modified

### Requirement: Save snapshot strategy follows live buffer size
The system SHALL choose synchronous or chunked save snapshot capture from the live editor buffer size, not only from the last known on-disk file size. Unknown-size buffers and buffers at or above the large snapshot threshold MUST use chunked snapshotting so a large untitled document or a file that grew in memory does not block the GTK main thread with one full-buffer copy.

#### Scenario: Large untitled buffer snapshots in chunks
- **WHEN** an untitled modified document grows beyond the large save snapshot threshold
- **THEN** saving captures the buffer in bounded main-loop chunks
- **AND** the editor remains protected from one long synchronous full-buffer copy

#### Scenario: File that grew in memory snapshots in chunks
- **WHEN** a file-backed document was small on disk but the live buffer grows beyond the large save snapshot threshold
- **THEN** saving uses chunked snapshot capture based on the live buffer size

### Requirement: Async load results are generation guarded
The system SHALL prevent stale asynchronous load results from applying after a newer load or reopen starts for the same editor. Each load request MUST carry a generation or equivalent identity, and the main-thread completion path MUST ignore results that do not match the editor's current load generation.

#### Scenario: Older load cannot overwrite newer content
- **WHEN** an editor starts loading file A and then starts loading file B before file A completes
- **THEN** file A's later completion is ignored
- **AND** the editor displays file B's content and metadata

#### Scenario: Cancelled load stays cancelled after a newer load starts
- **WHEN** a load is cancelled and another load begins
- **THEN** the cancelled load cannot become uncancelled by the new request's token reset
- **AND** its completion path does not mutate the editor

### Requirement: Durability-unconfirmed saves remain retryable
The system SHALL keep a file-backed document marked modified when the saved bytes reach disk but durability cannot be confirmed. The warning path MUST NOT clear draft recovery, close the tab, or adopt a Save As destination as fully committed until a later save succeeds or the user explicitly discards.

#### Scenario: Durability warning keeps save retry available
- **WHEN** a save reaches the destination but the parent-directory sync fails
- **THEN** the editor reports a durability warning
- **AND** the document remains modified so the user can save again

### Requirement: Save payload admission precedes complete buffer capture
The system SHALL acquire conservative byte-weighted admission before capturing a complete document snapshot for an asynchronous save. A queued save MUST retain only compact scalar and weak identity state, and its payload ownership MUST remain charged until snapshot, transformation, encoding, and durable-write inputs are consumed or discarded.

#### Scenario: Several saves would exceed the payload budget
- **WHEN** admitting another save's conservative snapshot and encoding charge would exceed the process save-payload budget
- **THEN** that save remains queued without capturing complete document text
- **AND** it is reconsidered after earlier payload ownership is released

#### Scenario: One supported save exceeds the shared budget
- **WHEN** one supported document has a conservative save charge larger than the ordinary shared budget
- **THEN** it runs only as the exclusive admitted save payload
- **AND** no second document-sized save payload overlaps it

#### Scenario: Queued save becomes stale
- **WHEN** an editor closes, changes save generation, changes destination identity, or no longer needs the queued save before admission
- **THEN** the compact request is skipped or removed
- **AND** it consumes neither document payload budget nor worker capacity

### Requirement: Multi-document close saves are ordered and recovery-safe
When a close decision saves multiple modified documents, the system SHALL complete those saves sequentially and SHALL NOT capture the next complete document body while the preceding close save still owns its payload. The close flow MUST preserve existing dirty-state, durability-warning, Save As, draft-recovery, and explicit-discard semantics.

#### Scenario: Window close saves several modified files
- **WHEN** the user chooses to save several modified file-backed tabs during window close
- **THEN** the flow admits, snapshots, writes, and releases one selected document before admitting the next
- **AND** the window closes only after every selected save succeeds

#### Scenario: A close save fails before replacement
- **WHEN** one selected close save fails before replacing its destination
- **THEN** the document remains modified and recoverable
- **AND** the window and remaining selected tabs do not close as though all saves succeeded

#### Scenario: A close save has unconfirmed durability
- **WHEN** one selected close save reaches disk but its durability cannot be confirmed
- **THEN** the document remains modified and its draft recovery is retained
- **AND** later close saves and final window closure do not proceed as though the warning were a successful terminal save
