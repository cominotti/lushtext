## ADDED Requirements

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
