## ADDED Requirements

### Requirement: Parallel widget runs report session-hygiene failures
`widget-shards.py parallel` and `run-widget-tests.sh` SHALL recognize the helper's `SESSION-HYGIENE:` verdict and dedicated exit status. They SHALL report it as a named class beside `FLAKY-SHARD:` and `OOM-KILL:`, SHALL fail the run, and SHALL NOT retry it as a flaky shard.

#### Scenario: One shard's session activates the accessibility bus
- **WHEN** a shard's session fails the guard with `accessibility-bus` while its tests pass
- **THEN** the closing summary prints `SESSION-HYGIENE: <shard>: accessibility-bus` and the run exits nonzero
- **AND** the shard is not rerun
