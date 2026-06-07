## ADDED Requirements

### Requirement: Local-history index corruption preserves snapshot bodies
The system SHALL isolate malformed local-history lineage metadata from valid snapshot bodies. A malformed lineage index MUST be preserved when possible, reported through recovery diagnostics, and MUST NOT cause snapshot text files to be deleted merely because the index could not be parsed.

#### Scenario: Malformed local-history index does not delete snapshot text
- **WHEN** a local-history lineage index cannot be parsed during startup or history browsing
- **THEN** the original index is preserved through quarantine or left untouched when quarantine fails
- **AND** snapshot text files under that lineage remain on disk

#### Scenario: Repairable lineage index is rebuilt conservatively
- **WHEN** snapshot files contain enough deterministic metadata to rebuild the lineage index
- **THEN** the system writes a repaired index through the durable JSON path
- **AND** it records a recovery diagnostic describing the repair

#### Scenario: Ambiguous lineage repair is skipped
- **WHEN** snapshot files cannot be mapped into a deterministic newest-first lineage
- **THEN** the system does not invent snapshot ordering or identities
- **AND** it preserves the lineage data with a diagnostic that manual inspection may be required

### Requirement: Local-history lineage migrations are retryable and merge-safe
The system SHALL record pending local-history lineage migrations before or as part of the post-rename lineage migration workflow. If migration, merge, or cleanup fails, the pending state MUST survive restart and be retried during startup reconciliation.

#### Scenario: Pending local-history migration survives restart
- **WHEN** an in-app file or directory rename succeeds but local-history lineage migration fails before completion
- **THEN** a pending migration record remains in app data
- **AND** restarting LushText retries the local-history migration

#### Scenario: Target lineage is written before source cleanup
- **WHEN** local-history migration moves snapshots from an old identity to a new identity
- **THEN** the target lineage index and snapshot bodies are durably written before the old lineage is removed
- **AND** cleanup failure leaves retryable diagnostic state

#### Scenario: Save As never consumes pending rename lineage
- **WHEN** a document has pending local-history rename migration state and the user later uses Save As to a different path
- **THEN** the Save As path starts a separate lineage
- **AND** the pending rename migration remains tied only to the original in-app rename

### Requirement: Local-history reconciliation is bounded and conservative
The system SHALL reconcile duplicate or orphaned local-history lineages conservatively during startup and browsing. Reconciliation MUST be bounded in time and data volume, MUST preserve the newest durable snapshots when deterministic, and MUST preserve evidence instead of deleting ambiguous non-empty lineage data.

#### Scenario: Duplicate lineages merge deterministically
- **WHEN** old and new local-history lineages both exist and snapshot identifiers are deterministic
- **THEN** the system merges the lineages while preserving newest-first order and retention caps
- **AND** it removes the obsolete lineage only after the merged target is durably written

#### Scenario: Corrupt duplicate lineage is quarantined
- **WHEN** one duplicate lineage is malformed and the other is valid
- **THEN** the malformed lineage is quarantined or preserved with diagnostics
- **AND** the valid lineage remains browsable

#### Scenario: Reconciliation work is capped
- **WHEN** startup sees many local-history lineages or very large snapshot stores
- **THEN** reconciliation applies explicit scan and time budgets
- **AND** unfinished work is recorded for later retry instead of blocking startup indefinitely

### Requirement: Local-history reliability has layered automated coverage
The project SHALL add deterministic service, integration, widget, property or fuzz-adjacent, and performance coverage for local-history index corruption, repair, retryable migration, duplicate reconciliation, and bounded startup behavior.

#### Scenario: Service tests cover corrupt indexes and intact snapshots
- **WHEN** service tests load a malformed lineage index with intact snapshot text files
- **THEN** the result preserves snapshot bodies and returns recovery diagnostics
- **AND** repair occurs only when deterministic evidence is present

#### Scenario: Integration tests cover migration retry
- **WHEN** tests simulate an in-app rename whose local-history migration fails after source rename
- **THEN** a pending migration record survives restart
- **AND** a later successful retry preserves all expected snapshots

#### Scenario: Widget tests cover partial history browsing
- **WHEN** local-history browsing opens with one corrupt lineage and one valid lineage
- **THEN** the valid lineage remains browsable
- **AND** visible partial-recovery feedback is shown

#### Scenario: Generated duplicate sets never drop the last copy
- **WHEN** generated local-history duplicate and orphan states are reconciled
- **THEN** the reconciler never deletes the last non-empty snapshot body before a durable merged target exists

#### Scenario: Performance tests cover bounded reconciliation
- **WHEN** the performance lane runs recovery fixtures with many lineages
- **THEN** it records reconciliation timing and confirms startup remains within the documented budget
