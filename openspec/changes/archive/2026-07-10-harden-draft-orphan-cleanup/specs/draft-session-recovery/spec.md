## ADDED Requirements

### Requirement: Draft orphan cleanup reports typed conservative outcomes
The system SHALL inspect and execute draft orphan cleanup through typed results that distinguish confirmed manifest removals, confirmed file deletions, already-absent files, retained items, scan or status failures, deletion failures, manifest-write failures, and unfinished bounded work. Cleanup counts and success diagnostics MUST include only confirmed committed actions.

#### Scenario: Orphan draft file deletion succeeds
- **WHEN** a bounded trusted cleanup pass finds a draft body with no entry in the latest manifest and deletion succeeds
- **THEN** the outcome reports that file as confirmed deleted
- **AND** the cleaned count includes it exactly once

#### Scenario: Orphan draft file deletion fails
- **WHEN** deletion of a confirmed orphan draft body fails
- **THEN** the outcome retains the path with a typed failure
- **AND** the cleaned count does not include that file
- **AND** a later deferred pass may retry it

#### Scenario: Draft directory scan fails
- **WHEN** the drafts directory exists but cannot be scanned consistently
- **THEN** cleanup returns a scan failure without executing a partial destructive plan
- **AND** it does not report the directory as clean

### Requirement: Cleanup distinguishes missing artifacts from metadata errors
The system SHALL use recovery-aware path status for draft body presence decisions. Only a confirmed missing body MAY make its matching manifest entry eligible for cleanup; permission, metadata, symlink, or other I/O errors MUST retain the entry and produce diagnostics.

#### Scenario: Manifest body is confirmed missing
- **WHEN** path status confirms that a manifest entry's draft body does not exist
- **THEN** the entry becomes eligible for merge-safe manifest removal
- **AND** it is not treated as a body deletion

#### Scenario: Manifest body status is unreadable
- **WHEN** path status for a manifest entry fails because metadata cannot be inspected
- **THEN** the manifest entry remains present
- **AND** the cleanup outcome reports the status failure
- **AND** no destructive decision is inferred from that error

### Requirement: Cleanup revalidates latest recovery state before mutation
The system MUST revalidate an orphan candidate against the latest persisted manifest and current path status before deletion or manifest removal. A cleanup plan MUST carry entry fingerprints or equivalent generation evidence so a newer draft with the same ID cannot be removed by stale work.

#### Scenario: New manifest entry appears after inspection
- **WHEN** inspection identifies an orphan body but a new manifest entry for that draft ID is committed before execution
- **THEN** execution skips deleting the body
- **AND** the newer recovery entry remains intact

#### Scenario: Draft body reappears after missing-body inspection
- **WHEN** inspection identifies a manifest entry's body as missing but a body is written before manifest cleanup commits
- **THEN** execution rechecks the path and retains the manifest entry
- **AND** stale cleanup does not detach the new body from its recovery metadata

#### Scenario: Same draft ID has a newer generation
- **WHEN** the latest manifest contains the same draft ID with newer saved-generation metadata than the inspected fingerprint
- **THEN** cleanup does not remove the newer entry
- **AND** the stale plan is reported as skipped

### Requirement: Manifest cleanup is durable before visible acceptance
The system SHALL remove confirmed missing-body entries through the serialized durable manifest update path. The window MUST merge only removals that were committed to the latest manifest; a manifest-write failure MUST leave visible state retryable and MUST NOT be presented as successful cleanup.

#### Scenario: Manifest cleanup commit succeeds
- **WHEN** a confirmed missing-body fingerprint still matches and the durable manifest update succeeds
- **THEN** the outcome includes the exact committed removal
- **AND** the window may remove that matching entry from its current manifest state

#### Scenario: Manifest cleanup commit fails
- **WHEN** the durable manifest update for confirmed missing entries fails
- **THEN** the outcome reports no committed manifest removals for that update
- **AND** the window retains its entries and surfaces retryable recovery feedback

### Requirement: Orphan cleanup remains bounded and non-blocking
The system SHALL keep orphan inspection and mutation off the GTK main thread and SHALL inspect no more than the configured bounded entry count in one pass. If eligible work remains, the outcome MUST record that fact for a later deferred retry rather than looping synchronously.

#### Scenario: Damaged directory exceeds the scan bound
- **WHEN** a drafts directory contains more candidates than one cleanup pass permits
- **THEN** the pass inspects only the configured maximum
- **AND** the outcome records that more work may remain
- **AND** startup and the GTK main loop remain usable

#### Scenario: Untrusted startup state skips cleanup
- **WHEN** startup recovery cannot trust the draft manifest
- **THEN** orphan cleanup is not executed
- **AND** ambiguous draft bodies and metadata remain preserved for repair or diagnosis

### Requirement: Draft orphan cleanup has deterministic fault coverage
The project SHALL add service and integration tests for missing files, metadata errors, unreadable directories, bounded scans, delete failures, manifest-write failures, concurrent same-ID updates, and partial successful outcomes.

#### Scenario: Generated failure combinations never over-report cleanup
- **WHEN** tests inject combinations of scan, status, deletion, and manifest-write failures
- **THEN** every reported removal corresponds to a confirmed action
- **AND** ambiguous or failed artifacts remain represented as retained or retryable
