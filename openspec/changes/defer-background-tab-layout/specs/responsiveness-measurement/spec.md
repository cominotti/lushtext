## ADDED Requirements

### Requirement: The lane measures a switch to a never-shown large tab
`eviction/reselect` SHALL end with a `switch-hidden` interaction phase: select
a tab the run never selected before, wait for `file-open-complete`, and scroll
it, recording the phase's stalls and input-to-frame latencies like any other
interaction phase.

#### Scenario: The switch is measured
- **WHEN** `eviction/reselect` runs and its earlier phases succeeded
- **THEN** the row carries a `switch-hidden` phase of kind `interaction`
