## ADDED Requirements

### Requirement: The durable-write protocol lives in an I/O-free verified core
The system SHALL express the durable-write protocol as an I/O-free state
machine. Given the protocol state and the outcome of the last operation, it
decides the next filesystem action. The protocol covers temp creation,
content, metadata, temp sync, rename, parent sync, cleanup, the before- and
after-rename classification, the copy and move primitives, and durable
rename. A thin shell SHALL execute each action through the private filesystem
backend, one backend call per action. Kani harnesses SHALL check the core over
every sequence of operation outcomes and crash points, and prove:

- after any crash, the destination holds the complete previous bytes or the
  complete new bytes;
- a before-rename error implies the previous bytes are still visible;
- an after-rename error implies the new bytes are visible;
- no reachable state places new bytes in a file wider than the destination's
  permissions.

#### Scenario: Crash atomicity is proved on the production core
- **WHEN** the Kani harness explores every outcome and crash sequence of the core
- **THEN** no reachable post-crash state exposes a torn destination

#### Scenario: The shell adds no decisions
- **WHEN** the shell executes the core's actions
- **THEN** each action maps to exactly one backend call and every outcome is fed back to the core unchanged

### Requirement: Durable copy and move are distinct primitives
The filesystem boundary SHALL provide a durable copy primitive that never
removes its source, and a durable move primitive that removes its source only
after the destination is durable. No primitive whose name says "copy" MUST
remove its source.

#### Scenario: Copy keeps the source
- **WHEN** a caller durably copies a file
- **THEN** the source still exists after success

#### Scenario: Move removes the source only after the destination is durable
- **WHEN** a durable move fails before the destination's parent sync completes
- **THEN** the source remains in place

### Requirement: The leftover sweep covers every app-data directory that holds durable writes
The startup leftover sweep SHALL cover every LushText app-data directory in
which durable writes create temp files, including style schemes, format-upgrade
backups, and the drafts set-aside location, under the same provably-ours,
stale-only, bounded rules. The temp-name format SHALL have one owner, which
both the name builder and the sweep predicate use, and write labels SHALL come
from a closed set.

#### Scenario: A stale leftover in the style-schemes directory is swept
- **WHEN** startup finds a stale provably-ours temp file in the style-schemes directory
- **THEN** it is removed under the same rules as the other app-data directories
