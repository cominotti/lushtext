## ADDED Requirements

### Requirement: Search retirement budgets are release-invariant
Every destructive step performed by a retired workspace-search generation SHALL consume its per-turn retirement budget in debug and release builds. Correctness-affecting mutations MUST occur outside debug-only assertions, one retirement turn MUST release no more than 250 owned items, and unfinished state MUST remain scheduled until it reaches a terminal empty state without touching the current generation.

#### Scenario: One release-profile turn retires a large generation
- **WHEN** a retired generation owns more than 250 unique file rows, file groups, match rows, cached rows, and shared accepted-result references
- **THEN** one retirement turn removes at most 250 actual owned items in a release build
- **AND** the remaining retired state stays pending for a later turn

#### Scenario: Repeated retirement turns finish
- **WHEN** bounded retirement turns continue after a large generation is detached
- **THEN** each turn charges every successful removal before proceeding to another item
- **AND** the retired generation eventually becomes empty without changing current rows, identities, readiness, or detached-generation backpressure

#### Scenario: Debug assertions are disabled
- **WHEN** the same retirement workflow is compiled without debug assertions
- **THEN** its actual container and reference-count deltas remain identical to debug semantics
- **AND** no required mutation depends on an assertion expression being evaluated

### Requirement: Whole-buffer snapshot accumulation has bounded GTK work
Whole-buffer snapshots SHALL accumulate fixed-size chunks without repeatedly reallocating or copying the already captured document on GTK. Final coalescing, transformation, and destruction of document-sized plain data MUST occur off GTK under the workflow's existing admission and disposal ownership, while only GTK-owned buffer access remains on the main thread.

#### Scenario: A large snapshot needs many chunks
- **WHEN** a save, encoding, session, or other admitted workflow snapshots a supported large editor buffer
- **THEN** each GTK turn extracts only its configured character slice into a newly owned bounded chunk
- **AND** the chunk-header collection reserves from the initial O(1) character count instead of repeatedly growing on GTK
- **AND** no GTK turn copies or reallocates an amount proportional to all text accumulated so far

#### Scenario: Snapshot capture finishes
- **WHEN** the last chunk of an accepted large snapshot is captured
- **THEN** GTK transfers guarded chunk ownership to the admitted worker
- **AND** final coalescing or transformation does not construct or finally destroy the document-sized result in GTK dispatch
- **AND** save admission continues to span capture, coalescing, formatting, durable write, terminal acceptance, and exact-once permit release

#### Scenario: Snapshot becomes stale or is rejected
- **WHEN** generation, page lifetime, cancellation, overflow, or downstream admission invalidates a partially or fully captured snapshot
- **THEN** the snapshot stops before another stale GTK slice
- **AND** its remaining plain chunks and any coalesced payload retire through bounded off-GTK ownership

#### Scenario: Small snapshot uses the direct path
- **WHEN** the buffer fits the existing small-payload threshold
- **THEN** the implementation MAY use the direct snapshot path
- **AND** it preserves the same freshness, cancellation, and save-permit semantics as the chunked path

### Requirement: Workspace-search requests share immutable scope snapshots
Each workspace-search generation SHALL own one immutable shared folder snapshot. Request construction, active-plus-latest replacement, and periodic polling MUST clone only constant-size shared ownership rather than deep-cloning every PathBuf, while a scope change MUST create and supersede with a new generation.

#### Scenario: Polling a large workspace scope
- **WHEN** an active search over many workspace folders is polled repeatedly
- **THEN** each poll reuses the generation's immutable shared folder snapshot
- **AND** polling does not allocate or clone the full folder vector every 50 milliseconds

#### Scenario: Workspace scope changes during search
- **WHEN** the selected workspace scope changes while a search generation is active
- **THEN** a new generation receives a new immutable snapshot
- **AND** the prior generation cannot observe the new scope or publish stale results into it
