## MODIFIED Requirements

### Requirement: Long-running performance coverage is gated separately
The project SHALL keep expensive performance validation outside the default fast pull-request path unless a check is proven cheap and stable. Every GitHub Actions job that runs performance coverage, including scheduled, manual, and release-triggered jobs, MUST stay within the repository's 30-minute job budget.

#### Scenario: Pull request lane stays bounded
- **WHEN** default pull-request CI runs
- **THEN** it runs only cheap performance compilation or smoke checks suitable for routine feedback
- **AND** every job in the workflow declares or inherits a timeout of 30 minutes or less

#### Scenario: Release benchmark report stays bounded
- **WHEN** a `v*` release tag or release benchmark recovery dispatch runs the release benchmark report workflow
- **THEN** the workflow generates and uploads the release benchmark report asset within a 30-minute job timeout
- **AND** it uses a release-safe benchmark/report scope rather than an unbounded full Criterion suite

#### Scenario: Deeper performance run is available
- **WHEN** maintainers need higher confidence after performance-sensitive changes
- **THEN** scheduled or manual benchmark reports can run deeper benchmark coverage and preserve artifacts
- **AND** each GitHub Actions job in that diagnostic path still stays within the 30-minute timeout ceiling by scoping, splitting, or otherwise bounding the work

#### Scenario: Full benchmark diagnostics stay outside release publication
- **WHEN** a full Criterion report cannot be proven to finish within the 30-minute job budget
- **THEN** it MUST NOT be required as a tag publication workflow
- **AND** release publication uses a smaller report scope that still records useful benchmark evidence

## ADDED Requirements

### Requirement: Streaming benchmark harnesses preserve backpressure semantics
Benchmarks for streaming services that emit through channels SHALL avoid producer/receiver deadlocks. A benchmark that uses a bounded channel MUST drain that channel concurrently with the producer or explicitly make backpressure the measured behavior with a bounded completion condition.

#### Scenario: Content search benchmark drains while searching
- **WHEN** the content-search benchmark measures a fixture that can emit more events than the channel capacity
- **THEN** the benchmark drains search events while `content_search::search(...)` is still running
- **AND** Criterion can complete warmup, sample collection, analysis, and report generation without waiting for a post-return drain that can never be reached

#### Scenario: Raw search throughput benchmark does not measure channel backpressure
- **WHEN** a benchmark is intended to measure raw search traversal or matching throughput rather than UI backpressure behavior
- **THEN** it uses an unbounded or otherwise non-blocking collection strategy
- **AND** the benchmark name or comments make clear that backpressure is outside that measurement

#### Scenario: Backpressure behavior is covered separately
- **WHEN** bounded-channel backpressure is intentionally part of the content-search contract
- **THEN** the project keeps a focused test or benchmark that proves the worker and receiver cooperate without deadlock
- **AND** the fixture has an explicit completion bound suitable for CI

### Requirement: Workflow timeout policy is enforced
The project SHALL provide deterministic validation that no GitHub Actions job has a timeout above 30 minutes.

#### Scenario: Workflow declares an excessive timeout
- **WHEN** a workflow job declares `timeout-minutes` greater than 30
- **THEN** the policy check fails and identifies the workflow file, job, and configured timeout

#### Scenario: Diagnostic workflow needs more total work
- **WHEN** a diagnostic workflow needs more than 30 minutes of total coverage
- **THEN** it is split into multiple bounded jobs or scopes
- **AND** no individual job exceeds the 30-minute ceiling

#### Scenario: Release recovery does not extend timeouts
- **WHEN** a release workflow fails because a job cannot finish in 30 minutes
- **THEN** recovery changes the job scope, fixture, benchmark harness, or workflow split
- **AND** it does not raise the job timeout above 30 minutes
