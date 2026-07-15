## MODIFIED Requirements

### Requirement: Watcher pressure has deterministic retained-state coverage
The project SHALL test and benchmark watcher delivery from raw backend callback through GTK consumption when event production is slower than, equal to, and faster than GTK polling. Coverage MUST report the path cap, raw-event normalization count, contention/full-refresh promotions, mailbox state, notices consumed per poll, and retained refresh-plan size, and MUST prove that no intermediate debouncer vector or application queue grows with event count.

#### Scenario: Sustained producer pressure exceeds consumer rate
- **WHEN** a fixture emits many raw tree-changing events without GTK polling
- **THEN** retained event state remains bounded from the first callback onward
- **AND** the next poll observes a path notice or conservative full refresh representing the burst

#### Scenario: Bulk rename storm is replayed
- **WHEN** a scale fixture emits awkward Unicode, overlapping, duplicate, deeply nested, and ambiguous raw rename events
- **THEN** normalization and merge timing are recorded off the GTK path
- **AND** GTK-side work remains bounded by one notice and the configured path cap

#### Scenario: Mailbox contention remains constant-space
- **WHEN** producer callbacks overlap a held mailbox lock during a sustained event burst
- **THEN** retained contention evidence remains constant-space and promotes conservatively to full refresh
- **AND** the producer does not allocate a retry queue or block GTK consumption

#### Scenario: Repeated errors remain bounded
- **WHEN** watcher failures repeat faster than the UI can render feedback
- **THEN** retained diagnostic state remains constant-space
- **AND** current-generation recovery or manual Refresh stays available

## ADDED Requirements

### Requirement: Quality closeout has deterministic feature-matrix and scale evidence
The project SHALL verify the remaining quality closeout under both default and all-feature Rust configurations and SHALL add focused deterministic evidence for Notes admission/query ownership, local-history preview slicing, workspace bulk-cache rebuilding, command-palette index retirement, and draft-cleanup retry scheduling. Tests and benchmarks MUST assert retained-state or work bounds rather than relying only on elapsed time.

#### Scenario: Default and all-feature unit configurations compile
- **WHEN** closeout validation runs
- **THEN** the default-feature unit-test target compiles and runs the in-module draft-cleanup fault tests
- **AND** the all-feature unit, Clippy, property, and integration surfaces selected by repository policy also pass

#### Scenario: Notes source and query pressure are exercised
- **WHEN** fixtures exceed source admission and render limits while queries are superseded
- **THEN** evidence records admitted entries, searchable bytes, truncation reasons, active and pending request counts, cancellation, and published result count
- **AND** no stale or over-budget source/result is accepted

#### Scenario: Large history preview remains interactive
- **WHEN** representative Unicode snapshot text requires several preview-install slices and selection changes during installation
- **THEN** evidence records slice count, main-loop progress, cancellation, and accepted retained payload count
- **AND** final preview text exactly matches only the current snapshot

#### Scenario: Broad tree cache rebuild is measured
- **WHEN** mirrors from small sizes through the configured row cap are rebuilt
- **THEN** instrumentation or an operation-count oracle demonstrates linear rebuild work
- **AND** the benchmark separately reports terminal cache rebuild from reconciliation planning and model-splice timing

#### Scenario: Palette indexes are retired off GTK
- **WHEN** full, accepted incremental, and rejected incremental updates each release a last-owned large file index
- **THEN** deterministic test evidence shows final destruction is transferred to the worker lane
- **AND** current generation and replay behavior remain unchanged

#### Scenario: Cleanup retries cursorless work
- **WHEN** deterministic delete and manifest faults produce `has_more_work` without continuation cursors
- **THEN** window-level decision tests schedule one delayed retry from the safe beginning with bounded backoff
- **AND** a later successful outcome stops retrying and reports only confirmed cleanup
