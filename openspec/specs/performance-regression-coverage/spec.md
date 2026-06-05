# performance-regression-coverage Specification

## Purpose
Define LushText's user-facing performance coverage so lightweight smoke checks,
large-file behavior, and deeper benchmark paths remain explicit.

## Requirements
### Requirement: User-facing performance budgets are documented and runnable
The project SHALL provide documented performance smoke coverage for workflows
whose regressions would be visible to users.

#### Scenario: Performance smoke command exists
- **WHEN** a maintainer lists development validation commands
- **THEN** there is a documented command for lightweight performance smoke checks
  distinct from full Criterion benchmark reporting

#### Scenario: Performance report records environment and fixtures
- **WHEN** the performance smoke command runs
- **THEN** it records hardware or runner identity when available, toolkit
  versions, build profile, fixture sizes, thresholds, and measured timings

#### Scenario: Thresholds are coarse and reviewable
- **WHEN** a performance threshold is enforced
- **THEN** the threshold is documented with its baseline rationale
- **AND** failures include enough measured data to decide whether the regression
  is code, fixture, or runner noise

### Requirement: Core latency and throughput paths are covered
The performance smoke lane SHALL cover the user-visible workflows most likely
to create perceived slowness.

#### Scenario: Startup and file-open latency are measured
- **WHEN** the performance smoke lane runs
- **THEN** it measures application startup or first-window readiness and opening
  representative small and medium text documents

#### Scenario: Workspace indexing and search are measured
- **WHEN** the performance smoke lane runs against a representative workspace
  fixture
- **THEN** it measures file indexing, command-palette file search, and
  workspace-wide content search

#### Scenario: Save and replace workflows are measured
- **WHEN** the performance smoke lane runs
- **THEN** it measures representative save, Save As, Replace All, and undo
  workflows without requiring destructive writes to user data

### Requirement: Large-file and memory-pressure behavior remains covered
The test and performance lanes SHALL cover LushText's user-facing degradation
behavior for large files and many open buffers.

#### Scenario: Large-file thresholds are verified through UI-observable behavior
- **WHEN** documents cross the syntax-disable, undo-disable, or refuse-to-load
  thresholds
- **THEN** tests verify the corresponding user-visible feedback and editor
  capability state rather than only testing the pure threshold helper

#### Scenario: Very large save snapshot behavior remains responsive
- **WHEN** a very large modified buffer is saved
- **THEN** coverage proves that the save uses a consistent snapshot
- **AND** the UI remains protected from concurrent edits or duplicate save
  requests while the write is pending

#### Scenario: Buffer eviction and reload are covered under memory pressure
- **WHEN** total open buffer memory exceeds the configured budget
- **THEN** unmodified background tabs can be evicted according to policy
- **AND** reselecting an evicted tab reloads its content without losing user
  data or open-path bookkeeping

### Requirement: Main-thread responsiveness regressions are covered
The performance and test lanes SHALL cover workflows where a regression would
move filesystem I/O, large snapshots, or expensive pure analysis back onto the
GTK thread. Coverage MUST include deterministic service or unit tests for pure
ordering behavior, widget tests for user-visible asynchronous state, and a
lightweight performance-smoke path for coarse main-loop stall detection where
the behavior is practical to measure.

#### Scenario: Async persistence ordering is tested
- **WHEN** Replace All undo backup save or clear work is delayed by a test
  fixture or narrow test hook
- **THEN** automated coverage proves the search panel updates visible undo
  state before the delayed disk operation completes
- **AND** stale save-after-clear and clear-after-save completions cannot restore
  inactive undo UI state

#### Scenario: Chunked draft snapshots are tested
- **WHEN** a dirty editor buffer exceeds the autosave synchronous snapshot
  threshold
- **THEN** automated coverage proves the snapshot is collected through the
  chunked path
- **AND** failed asynchronous draft writes leave the editor eligible for a later
  autosave attempt

#### Scenario: Stale asynchronous analysis results are tested
- **WHEN** Replace preview generation, Save As canonical refresh, or lossy
  encoding analysis completes after its originating request is no longer
  current
- **THEN** automated coverage proves the stale result is ignored
- **AND** the visible UI state remains tied to the newest request

#### Scenario: Performance smoke includes main-loop stall coverage
- **WHEN** the lightweight performance smoke lane runs after this change
- **THEN** it includes at least one coarse responsiveness check for a workflow
  that previously risked a long GTK tick
- **AND** the recorded report includes the fixture size, threshold, elapsed
  timing, and enough environment detail to interpret a regression

#### Scenario: Regression tests use the existing harness boundaries
- **WHEN** responsiveness coverage is added for GTK-visible behavior
- **THEN** widget tests use the existing headless widget harness and shared
  wait helpers
- **AND** pure service, ordering, or text-processing behavior is tested without
  requiring a display server

### Requirement: Long-running performance coverage is gated separately
The project SHALL keep expensive performance validation outside the default fast
pull-request path unless a check is proven cheap and stable.

#### Scenario: Pull request lane stays bounded
- **WHEN** default pull-request CI runs
- **THEN** it runs only cheap performance compilation or smoke checks suitable
  for routine feedback

#### Scenario: Deeper performance run is available
- **WHEN** maintainers need higher confidence before release or after
  performance-sensitive changes
- **THEN** a scheduled, manual, or release validation path runs deeper benchmark
  reports and preserves artifacts

### Requirement: Recovery hardening has bounded startup and autosave performance coverage
The performance and responsiveness lanes SHALL cover recovery-metadata loading, quarantine or repair, migration reconciliation, and first-dirty autosave so reliability hardening does not introduce user-visible startup stalls or editing jank.

#### Scenario: Startup recovery fixtures are timed
- **WHEN** the performance smoke lane runs recovery fixtures with malformed metadata, pending migrations, and duplicate sidecars
- **THEN** it records startup or first-window readiness timing
- **AND** it reports fixture counts, total metadata size, repaired or quarantined item counts, and thresholds

#### Scenario: First-dirty autosave responsiveness is measured
- **WHEN** the performance or responsiveness lane exercises first-dirty autosave on small and large buffers
- **THEN** it records main-loop stall or elapsed autosave timing
- **AND** large buffers are proven to use the chunked snapshot path

#### Scenario: Migration reconciliation is bounded
- **WHEN** recovery fixtures contain many pending sidecar or local-history migrations
- **THEN** reconciliation applies documented budgets
- **AND** the report distinguishes completed, deferred, failed, and skipped migration work

### Requirement: Reliability performance tests preserve harness boundaries
The project SHALL place recovery performance coverage at the narrowest useful layer. Pure metadata repair, duplicate reconciliation, and ledger state-machine behavior MUST be tested without a display server. GTK-visible recovery warnings and first-dirty user interaction behavior MUST use widget or smoke harnesses.

#### Scenario: Pure recovery benchmarks avoid GTK
- **WHEN** recovery_metadata, migration ledger, or reconciliation performance coverage is added
- **THEN** pure service tests or Criterion benchmarks cover the data-processing cost without launching GTK
- **AND** fixtures stay bounded and reproducible

#### Scenario: Widget responsiveness tests cover visible recovery state
- **WHEN** testing that recovery diagnostics or first-dirty autosave do not break user interaction
- **THEN** widget tests use the existing GTK harness and wait helpers
- **AND** they avoid production-length sleeps by using test hooks or shortened intervals

#### Scenario: Smoke performance stays outside fragile host dependencies
- **WHEN** crash or confined recovery smoke emits timing data
- **THEN** that timing is diagnostic unless explicitly promoted to an enforced threshold
- **AND** host-sensitive lanes do not become default PR blockers without stability review

### Requirement: Recovery hardening test load is tiered
The project SHALL separate cheap PR-friendly recovery checks from deeper scheduled or manual coverage. Default checks MUST remain bounded while still compiling and exercising the critical recovery contracts.

#### Scenario: Pull request lane runs cheap recovery coverage
- **WHEN** default PR validation runs
- **THEN** it includes service or integration tests for recovery metadata, migration ledger ordering, and first-dirty autosave logic that are cheap enough for routine feedback

#### Scenario: Scheduled lane runs deep recovery fixtures
- **WHEN** scheduled, manual, or release validation runs
- **THEN** it exercises larger corrupted metadata sets, many pending migrations, crash/restart smoke, and confined recovery smoke when host support is available
- **AND** it preserves timing and diagnostic artifacts

#### Scenario: Recovery performance regressions include enough context
- **WHEN** a recovery performance threshold fails
- **THEN** the report includes measured timings, fixture sizes, environment details, and the recovery operation being measured
- **AND** maintainers can tell whether the regression is from code, fixture scale, or host noise
