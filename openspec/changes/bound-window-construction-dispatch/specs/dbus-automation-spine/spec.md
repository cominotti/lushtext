## ADDED Requirements

### Requirement: Automation exposes bounded window-construction evidence
The automation snapshot SHALL project the shell-geometry window-construction
evidence as a bounded `window_construction` object containing only integers:
the toolkit startup, LushText startup, window-construction, and first-frame
span durations in microseconds, the largest dispatch before the first frame
(`null` in a build without the responsiveness watchdog feature, never `0`),
the pending and total construction-stage counts, the number of settings writes
before the first frame, and the settings writer's pending, flushed, and
dropped-equal counters. It SHALL NOT contain paths, setting values, or
document content. When construction is staged, a pending stage SHALL be a
readiness blocker for every predicate that depends on the staged surface. The
fields SHALL be documented in `docs/automation.md` and
`docs/automation-reference.md`.

#### Scenario: Snapshot reports construction evidence
- **WHEN** an automation client reads the snapshot after the first presented frame
- **THEN** `window_construction` reports every span and counter as integers and nothing else

#### Scenario: Staged surface blocks dependent readiness
- **WHEN** a readiness predicate depends on a surface whose construction stage is still pending
- **THEN** the wait reports the pending stage as its blocker until the stage completes
