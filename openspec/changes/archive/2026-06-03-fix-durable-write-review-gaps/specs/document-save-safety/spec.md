## ADDED Requirements

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
