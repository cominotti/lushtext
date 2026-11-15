## ADDED Requirements

### Requirement: A retry-rescued widget test is diagnosed and fixed
A widget test reported `FLAKY` on CI SHALL be reproduced in isolation (under CPU starvation when the failure is a wait), classified (synchronous UI flip, asynchronous completion, or production race), and fixed at its cause, failing first. A retry, a widened budget bound, or a longer timeout MUST NOT be the fix, except a longer timeout for an asynchronous completion whose wait was below its class budget in the gtk-testing rules. The fix holds after 20 consecutive isolated starved passes.

#### Scenario: Asynchronous wait under starvation
- **WHEN** a test times out waiting for a `spawn_blocking_then` completion on a starved runner
- **THEN** the fix gives the wait a budget from the widget-wiring rules or removes the race
- **AND** 20 consecutive isolated runs pinned to one CPU with two busy loops pass

#### Scenario: Budget bound exceeded on one mode
- **WHEN** a resident-memory test exceeds its charge bound on one of two measured modes
- **THEN** the charge is recalibrated from the measured growth, or the measurement is made independent of allocator granularity and of asynchronous setup work that overlaps its window
- **AND** the bound itself is unchanged
- **AND** every constant derived from a recalibrated charge, and every assertion over it, still holds without lowering what the user is shown

#### Scenario: Production race found
- **WHEN** the diagnosis shows the test exposes a product defect
- **THEN** the defect is fixed in the product with a failing-first test, and the original test is kept

### Requirement: A retry-rescued test that does not reproduce locally is still diagnosed
When no local run reproduces a `FLAKY` failure, the diagnosis SHALL come from the CI log and instrumentation, the failing-first test SHALL be a deterministic test of the diagnosed mechanism, and the regression proof SHALL be the affected shard without that test's `FLAKY` line on at least eight CI runs.

#### Scenario: Not reproducible locally
- **WHEN** 20 starved isolated runs of a once-failing test all pass
- **THEN** the diagnosis names the timed-out predicate from the CI log and a deterministic test reproduces the diagnosed mechanism red before the fix
- **AND** the affected shard runs without that test's `FLAKY` line on at least eight CI runs of the branch
