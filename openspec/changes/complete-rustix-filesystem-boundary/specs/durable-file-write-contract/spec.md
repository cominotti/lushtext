## ADDED Requirements

### Requirement: Durable writes use shared filesystem backend primitives
The durable write implementation SHALL perform raw filesystem actions through private backend primitives owned by the filesystem boundary. The durable write state machine may own sequencing, failure classification, metadata plans, target write coordination, and streaming writer orchestration, but it MUST NOT duplicate raw platform operations that are available from the shared backend.

#### Scenario: Atomic replace delegates raw platform operations
- **WHEN** an atomic replacement creates a temp file, writes content, applies metadata, syncs the temp file, renames into place, removes a failed temp file, or syncs the parent directory
- **THEN** raw platform calls are performed through shared private filesystem backend helpers
- **AND** the public durable-write behavior exposed through the filesystem write boundary remains unchanged

#### Scenario: Durable rename delegates namespace mutation and sync
- **WHEN** a durable rename moves a file or directory and syncs affected parent directories
- **THEN** rename and directory sync operations use the shared private filesystem backend helpers
- **AND** production callers reach the operation through the public filesystem boundary rather than a durable-write implementation module

### Requirement: Durable write backend consolidation preserves every safety guarantee
Backend consolidation MUST preserve the existing durable-write requirements for atomic overwrite ordering, destination identity metadata preservation, safe temp metadata, final temp sync after metadata, default permissions for new files, before/after-rename failure classification, non-swallowed sync errors, stable target coordination, durable copy fallback, and streaming serialization.

#### Scenario: Consolidated backend keeps before-rename failure semantics
- **WHEN** a write, metadata application, temp-file sync, or rename fails before the destination replacement is committed
- **THEN** the previous destination bytes remain in place
- **AND** the caller receives the same before-rename failure classification as before consolidation

#### Scenario: Consolidated backend keeps after-rename failure semantics
- **WHEN** the destination rename succeeds but parent-directory sync fails
- **THEN** the caller receives the same after-rename durability warning classification as before consolidation
- **AND** the failure is not reported as an unwritten save

#### Scenario: Consolidated backend preserves identity metadata
- **WHEN** an existing destination with restrictive mode bits, ownership, ACLs, or extended attributes is atomically overwritten
- **THEN** required mode bits are preserved and best-effort identity metadata preservation still runs before the final temp-file sync
- **AND** unsupported or permission-denied best-effort metadata does not fail the write

### Requirement: Durable copy fallback keeps source metadata through shared backend support
Durable copy fallback SHALL continue to copy source bytes and source identity metadata to the destination and SHALL not remove the source until the destination write and destination parent sync have completed. Any metadata probing, temp metadata application, source cleanup, and source parent sync used by copy fallback MUST route through the shared private filesystem backend.

#### Scenario: Copy fallback carries source metadata after backend consolidation
- **WHEN** durable copy fallback copies a source file over an existing destination with different mode bits
- **THEN** the resulting destination takes the source file's mode bits
- **AND** the source is removed only after the destination content, destination metadata, and destination parent sync have completed

#### Scenario: Copy fallback failure keeps source in place
- **WHEN** copy fallback fails before the destination write and destination parent sync complete
- **THEN** the source file remains in place
- **AND** the operation reports an error instead of deleting the source

### Requirement: Durable write implementation remains private to filesystem boundary callers
Production callers SHALL reach durable byte writes, streaming writes, durable directory creation, durable rename, durable copy fallback, parent-directory sync, stable target coordination, and durable failure classification through the filesystem boundary. The durable-write implementation module MUST remain private implementation detail and MUST NOT be imported by production callers.

#### Scenario: Editor save reaches durable behavior through filesystem write
- **WHEN** editor save resolves a write target, acquires the stable target guard, and writes document bytes
- **THEN** it calls the filesystem write boundary
- **AND** it does not import durable-write implementation helpers directly

#### Scenario: Replace All reaches durable behavior through filesystem write
- **WHEN** Replace All or Replace All undo reads and writes target files
- **THEN** it acquires the same stable target guard and atomic replacement behavior through the filesystem write boundary
- **AND** it does not import durable-write implementation helpers directly

#### Scenario: Persistence callers stream through filesystem write
- **WHEN** JSON state, drafts, notes, bookmarks, saved searches, session data, local history, or style-scheme persistence writes data
- **THEN** it uses the filesystem write boundary for durable byte or streaming writes
- **AND** it inherits the same backend-consolidated durability guarantees
