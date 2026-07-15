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
pull-request path unless a check is proven cheap and stable. Every GitHub
Actions job that runs performance coverage, including scheduled, manual, and
release-triggered jobs, MUST stay within the repository's 30-minute job budget.

#### Scenario: Pull request lane stays bounded
- **WHEN** default pull-request CI runs
- **THEN** it runs only cheap performance compilation or smoke checks suitable
  for routine feedback
- **AND** every job in the workflow declares or inherits a timeout of 30 minutes
  or less

#### Scenario: Release benchmark report stays bounded
- **WHEN** a `v*` release tag or release benchmark recovery dispatch runs the
  release benchmark report workflow
- **THEN** the workflow generates and uploads the release benchmark report asset
  within a 30-minute job timeout
- **AND** it uses a release-safe benchmark/report scope rather than an unbounded
  full Criterion suite

#### Scenario: Deeper performance run is available
- **WHEN** maintainers need higher confidence after performance-sensitive changes
- **THEN** scheduled or manual benchmark reports can run deeper benchmark
  coverage and preserve artifacts
- **AND** each GitHub Actions job in that diagnostic path still stays within the
  30-minute timeout ceiling by scoping, splitting, or otherwise bounding the work

#### Scenario: Full benchmark diagnostics stay outside release publication
- **WHEN** a full Criterion report cannot be proven to finish within the
  30-minute job budget
- **THEN** it MUST NOT be required as a tag publication workflow
- **AND** release publication uses a smaller report scope that still records
  useful benchmark evidence

### Requirement: Streaming benchmark harnesses preserve backpressure semantics
Benchmarks for streaming services that emit through channels SHALL avoid
producer/receiver deadlocks. A benchmark that uses a bounded channel MUST drain
that channel concurrently with the producer or explicitly make backpressure the
measured behavior with a bounded completion condition.

#### Scenario: Content search benchmark drains while searching
- **WHEN** the content-search benchmark measures a fixture that can emit more
  events than the channel capacity
- **THEN** the benchmark drains search events while `content_search::search(...)`
  is still running
- **AND** Criterion can complete warmup, sample collection, analysis, and report
  generation without waiting for a post-return drain that can never be reached

#### Scenario: Raw search throughput benchmark does not measure channel backpressure
- **WHEN** a benchmark is intended to measure raw search traversal or matching
  throughput rather than UI backpressure behavior
- **THEN** it uses an unbounded or otherwise non-blocking collection strategy
- **AND** the benchmark name or comments make clear that backpressure is outside
  that measurement

#### Scenario: Backpressure behavior is covered separately
- **WHEN** bounded-channel backpressure is intentionally part of the
  content-search contract
- **THEN** the project keeps a focused test or benchmark that proves the worker
  and receiver cooperate without deadlock
- **AND** the fixture has an explicit completion bound suitable for CI

### Requirement: Workflow timeout policy is enforced
The project SHALL provide deterministic validation that no GitHub Actions job
has a timeout above 30 minutes.

#### Scenario: Workflow declares an excessive timeout
- **WHEN** a workflow job declares `timeout-minutes` greater than 30
- **THEN** the policy check fails and identifies the workflow file, job, and
  configured timeout

#### Scenario: Diagnostic workflow needs more total work
- **WHEN** a diagnostic workflow needs more than 30 minutes of total coverage
- **THEN** it is split into multiple bounded jobs or scopes
- **AND** no individual job exceeds the 30-minute ceiling

#### Scenario: Release recovery does not extend timeouts
- **WHEN** a release workflow fails because a job cannot finish in 30 minutes
- **THEN** recovery changes the job scope, fixture, benchmark harness, or
  workflow split
- **AND** it does not raise the job timeout above 30 minutes

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

### Requirement: Transient file-load bounds have deterministic scale coverage
The project SHALL test and benchmark file-load admission, bounded ingestion, and GTK installation with concurrent small, large, and individually oversized supported files. Evidence MUST distinguish queued scalar requests, active payload weight, retained decoded results, and installed editor residency.

#### Scenario: Session restore requests many large files
- **WHEN** a scale fixture restores more large tabs than the transient budget can admit
- **THEN** recorded active payload ownership never exceeds policy except for one documented exclusive load
- **AND** remaining requests stay compact and eventually progress or become stale

#### Scenario: Cancellation releases capacity
- **WHEN** admitted and queued loads are cancelled in varied completion orders
- **THEN** every permit is released exactly once
- **AND** the next current request can progress without leaked capacity

#### Scenario: Chunked installation remains responsive
- **WHEN** the responsiveness harness installs representative large Unicode documents
- **THEN** it records slice counts, retained payload bounds, and main-loop progress
- **AND** final buffer contents match the decoded source exactly

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

### Requirement: Bounded palette search has equivalence and scale coverage
The project SHALL prove bounded top-result selection and one-active/one-latest execution against a full-sort reference on representative and generated corpora. Coverage MUST include Unicode normalization, equal scores, empty queries, source deduplication, cancellation, rapid input, and the maximum indexed-file corpus.

#### Scenario: Bounded selector is compared with a reference
- **WHEN** generated candidate/query/result-limit combinations run through bounded and full-sort implementations
- **THEN** both produce the same selected identities and deterministic order
- **AND** the bounded implementation retains no more than the configured result count per source

#### Scenario: Maximum index receives rapid queries
- **WHEN** the 100,000-file fixture receives more queries than workers can complete
- **THEN** measurements report one active search, at most one pending query, cancellation progress, and final-query latency
- **AND** obsolete full-index jobs do not accumulate in the generic worker FIFO

#### Scenario: Diagnostic privacy regression is exercised
- **WHEN** an invalid Replace Preview range is generated under captured tracing output
- **THEN** typed invalid counts remain correct
- **AND** captured diagnostics contain none of the private source or replacement sentinel text

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

### Requirement: Remaining interactive pipelines have deterministic boundedness evidence
The project SHALL maintain runnable policy, integration, GTK/widget, and benchmark evidence for save admission, Markdown render/application and image limits, workspace-search single-flight ownership, result retirement/handoff, incremental editor-memory accounting, and lossy-encoding analysis. Evidence MUST measure the resource bound directly where possible rather than treating elapsed time alone as proof.

#### Scenario: Multi-tab save high-water evidence runs
- **WHEN** a fixture closes or saves several large modified documents
- **THEN** evidence records queued compact requests, admitted bytes, exclusive overweight behavior, and maximum simultaneously retained save payloads
- **AND** the observed high-water mark satisfies sequential close-save and byte-budget contracts

#### Scenario: Dense Markdown and image pressure evidence runs
- **WHEN** fixtures render dense event streams and more local images than the configured budgets
- **THEN** evidence records maximum events/nodes per GTK slice and outstanding image count/bytes
- **AND** stale generations and limited placeholders reach exact terminal states

#### Scenario: Rapid workspace queries run at scale
- **WHEN** a fixture supersedes slow searches repeatedly
- **THEN** evidence observes no more than one active controller/walker group and one latest compact request
- **AND** only the latest current generation can publish terminal results

#### Scenario: Large result handoff and retirement run at scale
- **WHEN** a maximum-sized accepted result set enters Replace Preview and is then superseded
- **THEN** evidence records zero whole-vector GTK clones and bounded rows/cache entries retired per slice
- **AND** preview/check/apply identities remain generation-correct

#### Scenario: Many-tab edits remain below enforcement threshold
- **WHEN** a scale fixture performs ordinary edits with many open tabs below the memory upper threshold
- **THEN** evidence records one scalar record update without a full-tab scan or candidate-vector allocation per edit
- **AND** threshold crossing still triggers deterministic current eviction policy

#### Scenario: Encoding analysis equivalence and throughput run
- **WHEN** benchmarks and property fixtures analyze lossless and lossy UTF-16, Windows-1252, and Shift_JIS inputs at representative sizes
- **THEN** optimized results remain semantically equivalent to actual encoding
- **AND** benchmark evidence records throughput without per-scalar allocation/setup amplification
