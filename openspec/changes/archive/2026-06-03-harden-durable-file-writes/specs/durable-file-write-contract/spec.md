## ADDED Requirements

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
