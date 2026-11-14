## ADDED Requirements

### Requirement: A retry-rescued widget test is diagnosed and fixed
A widget test reported `FLAKY` on CI SHALL be reproduced in isolation under CPU starvation, classified (synchronous UI flip, asynchronous completion, or production race), and fixed at its cause, failing first. A longer timeout, a retry, or a widened budget bound MUST NOT be the fix. The fix holds after 20 consecutive isolated starved passes.

#### Scenario: Asynchronous wait under starvation
- **WHEN** a test times out waiting for a `spawn_blocking_then` completion on a starved runner
- **THEN** the fix gives the wait a budget from the widget-wiring rules or removes the race
- **AND** 20 consecutive isolated runs pinned to one CPU with two busy loops pass

#### Scenario: Budget bound exceeded on one mode
- **WHEN** a resident-memory test exceeds its charge bound on one of two measured modes
- **THEN** the charge is recalibrated from the measured growth, or the measurement is made independent of allocator granularity
- **AND** the bound itself is unchanged

#### Scenario: Production race found
- **WHEN** the diagnosis shows the test exposes a product defect
- **THEN** the defect is fixed in the product with a failing-first test, and the original test is kept
