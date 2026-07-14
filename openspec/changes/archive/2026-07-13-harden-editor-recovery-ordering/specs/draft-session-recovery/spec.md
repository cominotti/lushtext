## ADDED Requirements

### Requirement: Every asynchronous draft restore is freshness-gated
The system MUST associate every asynchronous untitled and file-backed draft restore with a ticket containing the expected draft identity, editor lifetime, file path, dirty or edit generation, load generation, and resolved manifest entry. A completion MUST apply recovered text, show restore-specific feedback, or delete stale recovery state only while every applicable ticket field remains current.

#### Scenario: User edits while an ordinary restore read is pending
- **WHEN** an editor changes after an asynchronous draft restore starts but before its completion reaches GTK
- **THEN** the older completion does not replace the current buffer
- **AND** it does not clear, delete, or reclassify the preserved draft

#### Scenario: File identity or load generation changes during restore
- **WHEN** an editor is reused, reloaded, renamed, or assigned a different path while draft resolution is pending
- **THEN** the completion is rejected before applying content or feedback
- **AND** recovery state belonging to the original request remains available for an eligible later restore

#### Scenario: Restore entry is replaced while resolution is pending
- **WHEN** the manifest entry for a draft changes after restore begins
- **THEN** the old resolution cannot apply or delete the newer entry
- **AND** the latest entry remains authoritative

### Requirement: Draft mutations follow user-intent order
The system SHALL assign monotonically ordered intent to draft autosave upserts and draft deletion caused by Save, discard, close resolution, or stale-recovery cleanup. Draft body mutation and manifest mutation MUST execute through one ordered persistence workflow so an older autosave cannot recreate a draft after a later deletion intent.

#### Scenario: Save completes while autosave is writing
- **WHEN** an autosave operation is active and the corresponding editor successfully saves afterward
- **THEN** the Save-triggered deletion is ordered after the older autosave body and manifest work
- **AND** the final durable state contains neither the deleted manifest entry nor a resurrected draft body

#### Scenario: Autosave completion arrives after a deletion intent
- **WHEN** an older body write or completion reaches the coordinator after Save or discard has advanced the draft mutation generation
- **THEN** the obsolete upsert is rejected or ordered before the authoritative deletion
- **AND** it cannot become startup-restorable metadata

#### Scenario: New edit follows a completed deletion intent
- **WHEN** the user edits again after a Save-triggered draft deletion
- **THEN** the newer dirty generation may enqueue a later autosave upsert
- **AND** the ordering guard does not suppress legitimate recovery for the new unsaved work

#### Scenario: Ordered mutation fails
- **WHEN** a draft body, manifest upsert, or deletion operation fails
- **THEN** the workflow reports the failure without marking an uncommitted generation protected
- **AND** retry preserves the same intent order and the existing one-complete-body bound
