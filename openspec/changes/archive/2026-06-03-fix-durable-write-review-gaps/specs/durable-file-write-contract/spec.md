## ADDED Requirements

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
