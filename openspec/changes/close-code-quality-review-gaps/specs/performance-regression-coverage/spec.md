## ADDED Requirements

### Requirement: Final code-quality readiness has direct boundary evidence
The project SHALL retain deterministic direct evidence for every release, failure-path, ownership, ingestion, and working-set boundary closed by this change. Tests MUST assert actual container deltas, retained ownership, retry state, high-water values, GTK progress, and user-visible terminal behavior rather than relying only on self-reported counters or elapsed-time thresholds.

#### Scenario: Search retirement runs without debug assertions
- **WHEN** a focused release-semantic widget or proof scenario retires more than 250 items in each relevant ownership category
- **THEN** actual before-and-after ownership proves each turn releases between one and 250 items and eventually drains
- **AND** current-generation isolation and detached-generation bounds remain green

#### Scenario: Large snapshots and Local History restore run
- **WHEN** ASCII and multibyte documents above 10 MiB exercise chunked capture, save, restore, Undo Restore, cancellation, and teardown
- **THEN** exact bytes, per-slice limits, admission, no GTK-side full-body clone or coalescing, exact-once permit release, and off-GTK final disposal are proven
- **AND** an independent main-loop sentinel progresses before completion

#### Scenario: Replace and recent metadata hit failure races
- **WHEN** deterministic seams trigger pre-rename failure, after-metadata growth, exact-limit input, cap-plus-one input, and failure-heavy completion metadata
- **THEN** live undo accounting, journal state, untouched retryable files, bounded reads, exact totals, and diagnostic sample count and byte limits are asserted
- **AND** private document contents do not appear in samples

#### Scenario: File indexing approaches both byte ceilings
- **WHEN** long, Unicode, duplicate-canonical, sparse-directory, exact-boundary, one-over-boundary, and cancelled traversals run
- **THEN** conservative peak build ownership stays within its declared build budget and installed output stays within its retained budget
- **AND** truncation reason, deterministic partial output, and cleanup are asserted

#### Scenario: Dense UI and shared-context workloads run
- **WHEN** large bookmark inventories, rapid bookmark queries, multibyte minimap inputs, maximum-cell Markdown tables, many workspace folders, and image floods are exercised
- **THEN** source, projection, generation, byte, row, and image limits remain bounded while latest work wins
- **AND** shared path storage and independent main-loop progress are directly proven

### Requirement: Readiness closeout runs the complete repository gates
This change SHALL NOT be considered complete until the standard validation, lint, unit, integration, property, widget, performance-smoke, strict OpenSpec, and diff-integrity gates all pass on the implementation snapshot. Targeted release-semantic coverage MUST additionally run with debug assertions disabled, and benchmark comparisons MUST use the same toolchain, profile, machine-load conditions, and storage class without absolute cross-machine timing gates.

#### Scenario: Implementation is ready to archive
- **WHEN** every task is marked complete
- **THEN** make check, make lint-advisory, make test-unit, make test-int, make test-prop, make test-widget, and make performance-smoke all pass
- **AND** openspec validate --all --strict --no-interactive and git diff --check pass

#### Scenario: Release-only recurrence is checked
- **WHEN** the focused search-retirement regression is executed with debug assertions disabled
- **THEN** it passes by measuring actual ownership deltas
- **AND** the promoted lint provides a workspace-wide permanent recurrence guard

#### Scenario: Performance comparison is reported
- **WHEN** file-index or other materially changed GTK-free hot paths receive Criterion coverage
- **THEN** baseline and changed measurements use the same environment and report distributions or effect size
- **AND** no copied absolute time from another machine becomes a pass or fail threshold
