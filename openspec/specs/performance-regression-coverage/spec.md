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
