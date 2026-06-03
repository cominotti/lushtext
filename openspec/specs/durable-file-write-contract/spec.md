# durable-file-write-contract Specification

## Purpose
Define the low-level filesystem durability contract shared by editor saves, JSON persistence, drafts, local history, style-scheme writes, and Replace All file rewrites.

## Requirements
### Requirement: Atomic overwrites follow the full crash-durability ordering
The system SHALL persist every replace-an-existing-path write through a
temp-file-then-rename sequence that, in order, writes the new bytes to a uniquely
named temp file in the destination's own directory, flushes and `fsync`s that
temp file, renames it over the destination, and then `fsync`s the destination's
parent directory. The temp file MUST live in the same directory as the
destination so the rename stays on one filesystem and remains atomic.

#### Scenario: Successful atomic write leaves no temp leftovers
- **WHEN** a persistence caller atomically writes new bytes over an existing path
- **THEN** the destination contains exactly the new bytes
- **AND** no temp file remains in the destination directory after the write returns

#### Scenario: Power-loss never exposes a half-written destination
- **WHEN** the process or machine stops at any point during an atomic write
- **THEN** the destination on disk holds either the complete previous bytes or the complete new bytes
- **AND** the destination is never observed as a truncated or partially written file

### Requirement: Atomic overwrites preserve the destination's identity metadata
When the destination already exists, the system SHALL copy that destination's
identity metadata onto the temp file before the rename so the replaced file keeps
its prior on-disk identity. The preserved metadata MUST include the permission
(mode) bits and SHALL, on a best-effort basis, include ownership (uid/gid),
POSIX access ACLs, and user and security extended attributes. Best-effort
metadata that the running process is not permitted to set MUST NOT fail the write.

#### Scenario: Executable bit survives an overwrite
- **WHEN** an existing file marked executable is atomically overwritten
- **THEN** the resulting file is still marked executable

#### Scenario: Restrictive permissions survive an overwrite
- **WHEN** an existing `0600` file is atomically overwritten
- **THEN** the resulting file is still `0600` and is not widened to a more permissive mode

#### Scenario: Best-effort metadata that cannot be set does not fail the write
- **WHEN** the destination carries ownership or attributes the running process lacks permission to reapply
- **THEN** the atomic write still completes and updates the destination bytes
- **AND** the unreproducible metadata is skipped rather than aborting the save

### Requirement: Atomic overwrite temp metadata is safe and fully synced before rename
The system SHALL prepare temp-file metadata before the temp file replaces the destination. When the destination already exists, the temp file MUST be created with permissions that are no more permissive than the destination's standard permission bits, MUST receive the required destination metadata before rename, and MUST be `fsync`ed after all content and metadata mutations. A successful atomic write MUST NOT report success until the temp file's final content and required metadata are synced and the destination parent directory is synced after rename.

#### Scenario: Private destination does not get a permissive temp sibling
- **WHEN** an atomic overwrite targets an existing `0600` file
- **THEN** the temp file containing the new bytes is never created with permissions wider than the destination's standard permission bits
- **AND** the resulting destination remains `0600`

#### Scenario: Metadata mutation is covered by the temp-file sync
- **WHEN** an atomic overwrite copies destination mode bits, ownership, ACLs, or xattrs onto the temp file
- **THEN** the final temp-file `fsync` happens after those metadata mutations
- **AND** the write is not renamed into place before that final temp-file sync succeeds

#### Scenario: Metadata sync failure remains before-rename
- **WHEN** the final temp-file sync after metadata application fails
- **THEN** the write returns a before-rename failure
- **AND** the previous destination bytes and metadata remain in place

### Requirement: New-file creation uses default permissions
When the destination does not already exist, the system SHALL create it with the
platform's default permissions for a freshly created file and MUST NOT attempt to
inherit metadata from any unrelated path.

#### Scenario: Newly created file uses default permissions
- **WHEN** an atomic write targets a path that did not previously exist
- **THEN** the new file is created with default permissions
- **AND** no metadata-inheritance step is applied

### Requirement: Write failures are classified as before-rename or after-rename
The system SHALL distinguish a failure that occurs before the destination rename
from a failure that occurs after it. A before-rename failure (temp write, flush,
temp `fsync`, or rename itself) MUST leave the destination's previous bytes
intact and MUST remove the temp file. An after-rename failure (the parent
directory `fsync` failing once the new bytes are already in place) MUST report
that the new bytes were written but are not yet proven durable, rather than
reporting that the write was lost.

#### Scenario: Before-rename failure keeps the previous bytes
- **WHEN** an atomic write fails while writing, syncing, or renaming the temp file
- **THEN** the destination still contains its previous bytes
- **AND** the temp file is removed

#### Scenario: After-rename failure reports written-but-not-durable
- **WHEN** the rename succeeds but the parent directory `fsync` fails
- **THEN** the caller is told the bytes were written but durability could not be confirmed
- **AND** the failure is not reported as a lost or unwritten save

### Requirement: Durability sync failures are never silently ignored
The system SHALL propagate every `fsync`/`sync_all` error from the temp file,
the destination directory, and newly created directory-tree entries. A failed
durability sync MUST surface as an error or warning to the caller and MUST NOT be
swallowed into a silent success.

#### Scenario: Temp-file sync failure aborts before rename
- **WHEN** the temp file's `fsync` fails
- **THEN** the write returns a before-rename failure and the destination is untouched

#### Scenario: Directory sync failure is surfaced, not swallowed
- **WHEN** a directory `fsync` fails after the rename
- **THEN** the caller receives an after-rename durability error
- **AND** the operation does not report a silent success

### Requirement: Atomic write coordination is stable for a target path
The system SHALL coordinate in-app writes by stable resolved target identity rather than by the destination inode that may be replaced by `rename`. Editor save, Save As, Replace All, and Replace All undo MUST acquire the same write coordination guard for the same canonical target before reading or writing file bytes. The coordination guard MUST NOT require read-write access to the destination file itself.

#### Scenario: Save and Replace All serialize for the same target
- **WHEN** an editor save and a Replace All operation target the same canonical file at the same time
- **THEN** one operation waits for the other's write coordination guard
- **AND** their reads and atomic writes do not interleave

#### Scenario: Symlink and canonical path share one guard
- **WHEN** one operation addresses a file through a symlink and another addresses the resolved target path
- **THEN** both operations resolve to the same write coordination identity
- **AND** they cannot write concurrently as independent targets

#### Scenario: Read-only destination can still be coordinated
- **WHEN** the destination file is readable but not writable and its parent directory permits replacement
- **THEN** acquiring the write coordination guard does not fail solely because the file cannot be opened read-write

### Requirement: Durable copy fallback preserves source identity
When durable copy is used as a fallback for a rename that cannot be completed directly, the destination SHALL inherit the source file's content and identity metadata. The source file MUST NOT be removed until the destination bytes, destination metadata, destination directory entry, and destination parent-directory sync have completed.

#### Scenario: Copy fallback carries source mode over existing destination
- **WHEN** durable copy fallback copies a `0644` source over an existing `0600` destination
- **THEN** the destination contains the source bytes
- **AND** the destination mode is `0644`

#### Scenario: Copy fallback keeps source until destination is durable
- **WHEN** durable copy fallback fails before the destination write and parent-directory sync are complete
- **THEN** the source file remains in place
- **AND** the operation reports an error instead of deleting the source

### Requirement: Durable writes support streaming serialization
The system SHALL provide a durable write path that lets callers stream serialized content into the temp file while preserving the same safe temp metadata, final temp sync, rename, parent-directory sync, and before/after-rename failure classification as byte-slice atomic writes.

#### Scenario: JSON state writes without a prebuilt byte vector
- **WHEN** a JSON persistence caller saves a state document
- **THEN** the caller can serialize directly into the durable temp writer
- **AND** the write inherits the same metadata and fsync contract as `atomic_write_bytes`

#### Scenario: Streaming failure stays before-rename
- **WHEN** the streaming writer closure fails before rename
- **THEN** the destination remains unchanged
- **AND** the temp file is removed

### Requirement: Durable write entry points are owned by the filesystem boundary
The system SHALL expose durable byte-slice writes, durable streaming writes, durable directory creation, parent-directory sync, durable rename, durable copy fallback, stable write coordination, and before/after-rename failure classification through the internal filesystem boundary. Production callers MUST NOT import or call durable write implementation helpers directly when a filesystem-boundary operation exists.

#### Scenario: Editor save calls the filesystem write boundary
- **WHEN** a file-backed editor save writes document content to disk
- **THEN** it invokes the internal filesystem write boundary for target identity resolution, write coordination, atomic replacement, and failure classification
- **AND** it preserves the existing dirty-state behavior for before-rename and after-rename failures

#### Scenario: JSON and draft persistence stream through the boundary
- **WHEN** JSON state, draft content, session state, local history, notes, bookmarks, or saved-search data is persisted
- **THEN** the caller uses the filesystem boundary durable byte or streaming write operation
- **AND** it receives the same metadata preservation, temp sync, rename, parent-directory sync, and failure classification contract as editor saves

#### Scenario: Direct durable implementation calls are removed
- **WHEN** the migration is complete
- **THEN** production callers no longer import durable-write implementation helpers directly
- **AND** any remaining durable-write implementation module is private to the filesystem boundary
