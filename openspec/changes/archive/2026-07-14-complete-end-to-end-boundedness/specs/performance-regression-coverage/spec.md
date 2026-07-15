## ADDED Requirements

### Requirement: Palette source construction has deterministic boundedness evidence
The project SHALL test and benchmark palette source construction independently from query scoring. Evidence MUST cover flat-directory size, retained index entries, aggregate note entry and byte limits, canonical aliases, cancellation checkpoints, active and pending request counts, stale result disposal, and deterministic truncation diagnostics.

#### Scenario: Huge file and note corpora are exercised
- **WHEN** scale fixtures exceed the file-index and note-source limits
- **THEN** recorded retained memory and result counts remain within the documented bounds
- **AND** traversal and sidecar loading remain cancellable without unbounded pending requests

#### Scenario: Canonical exclusion precedes bounded selection
- **WHEN** tests use an open symlink alias as the best workspace match and a distinct lower-ranked match
- **THEN** the alias appears only under `Open Tabs`
- **AND** the distinct file fills the bounded workspace result slot

### Requirement: Buffer replacement responsiveness has layered evidence
The project SHALL cover bounded clear and replacement sessions with plain policy tests, GTK widget tests, current-generation cancellation tests, and calibrated large-Unicode diagnostics. Coverage MUST include eviction, draft recovery, local-history restore and undo, save-time formatting rewrite, disposal, stale generation, projection suppression, exact terminal cleanup, and final text equivalence.

#### Scenario: Large replacements preserve main-loop progress
- **WHEN** the responsiveness harness clears and replaces representative large Unicode buffers
- **THEN** it records bounded per-turn slice sizes and main-loop progress
- **AND** final content and workflow state match the accepted source exactly

#### Scenario: Every terminal path releases ownership
- **WHEN** replacement sessions complete, fail, become stale, or lose their editor
- **THEN** sources, retained text, projection suppression, and workflow guards are released exactly once
- **AND** no partial body becomes saveable or accepted as complete

### Requirement: Cleanup continuation and tree reconciliation have scale evidence
The project SHALL add deterministic coverage for draft directories with more than one cleanup page and workspace directories with thousands of changed rows. Draft evidence MUST prove eventual coverage across retained prefixes, failures, directory churn, and restart. Tree evidence MUST report planned and applied batch sizes, main-loop turns, supersession, cache finalization, and readiness completion.

#### Scenario: Later orphan survives behind a retained prefix
- **WHEN** more than one full cleanup page of live bodies precedes a later orphan and the process restarts between passes
- **THEN** durable continuation eventually reaches and revalidates the orphan
- **AND** no live or ambiguous body is deleted

#### Scenario: Large refresh is superseded
- **WHEN** a broad-directory reconciliation is replaced after one or more GTK batches
- **THEN** the stale plan stops within one bounded checkpoint
- **AND** the current plan alone owns final cache, state, and readiness evidence
