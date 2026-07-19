## ADDED Requirements

### Requirement: Replace All undo payload accounting is exact across failures
The live Replace All undo-byte counter SHALL equal the payload bytes of entries currently retained by the in-memory undo owner. A pre-rename failure that removes an undo entry MUST reclaim its admission charge and incremental-journal entry, while an ambiguous after-rename durability failure MUST retain both the entry and its charge. High-water metrics MUST remain monotonic observations and MUST NOT serve as the live counter.

#### Scenario: First target fails before rename
- **WHEN** the first deterministically ordered target is admitted and journaled but its durable write fails before rename
- **THEN** the target remains unchanged and its in-memory and active-journal undo entry are removed
- **AND** the exact payload charge is reclaimed for later targets

#### Scenario: A later target fits reclaimed capacity
- **WHEN** each of two entries fits alone but not together and the first entry is removed after a pre-rename failure
- **THEN** the second valid target is admitted and replaced
- **AND** it is not falsely rejected by stale undo-byte accounting

#### Scenario: Durability fails after rename
- **WHEN** replacement reaches the after-rename ambiguous-durability state
- **THEN** its undo entry, journal evidence, and live payload charge remain retained for recovery
- **AND** cancellation rollback and journal-before-mutation ordering remain intact

### Requirement: Undo reads enforce the file cap during ingestion
Replace All undo SHALL read current target contents through the filesystem boundary's bounded byte reader with MAX_REPLACE_FILE_BYTES. Metadata MAY be used as an early hint but MUST NOT be the allocation boundary, and a target that exceeds the cap during ingestion MUST receive no restore write and MUST remain retryable in the backup owner.

#### Scenario: Target grows after metadata planning
- **WHEN** a target is within the cap during metadata planning but grows beyond it before or during ingestion
- **THEN** the bounded reader allocates no more than the configured limit and performs no restore write
- **AND** the target is classified as skipped and remains in remaining backup state

#### Scenario: Target is exactly at the limit
- **WHEN** the current target body contains exactly MAX_REPLACE_FILE_BYTES
- **THEN** undo proceeds through ordinary content and identity comparison
- **AND** it is not rejected solely for reaching the exact limit

#### Scenario: Bounded read encounters I/O failure
- **WHEN** target ingestion fails for an I/O reason other than the size limit
- **THEN** undo reports a bounded failure classification
- **AND** the target remains retained for a later retry

### Requirement: Replace and Undo completion metadata is bounded
Replace All and Undo SHALL return exact aggregate counts plus a deterministic diagnostic sample containing at most 32 entries and 32 KiB of retained path and message bytes per result. Only affected paths required to reconcile currently open tabs MAY cross to GTK as a complete path set; failure-heavy, skipped-heavy, rollback-heavy, all-failed, and long-path workloads MUST NOT return metadata proportional to every processed target or join every error into one message.

#### Scenario: Ten thousand targets fail or are skipped
- **WHEN** Replace All or Undo processes a failure-heavy set at the supported result cap
- **THEN** exact success, failure, skip, and remaining counts are reported
- **AND** diagnostic messages and paths retain at most 32 samples and 32 KiB in deterministic order

#### Scenario: Some affected files are open
- **WHEN** worker completion needs to refresh open editor tabs
- **THEN** it returns only the intersection between affected files and the worker's immutable open-tab identity snapshot
- **AND** closed-file paths are represented by counts or the bounded diagnostic sample

#### Scenario: Completion is stale
- **WHEN** a large completion loses generation or window freshness before GTK accepts it
- **THEN** its plain metadata owner is retired off GTK
- **AND** no stale open-tab refresh or status projection occurs
