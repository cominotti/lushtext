## ADDED Requirements

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
