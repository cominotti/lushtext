## MODIFIED Requirements

### Requirement: Automation exposes bounded window-construction evidence
The automation snapshot SHALL project the shell-geometry window-construction evidence as a bounded
`window_construction` object that contains only integers and nulls:
- the toolkit startup, LushText startup, window-construction, and first-frame span durations, in
  microseconds;
- the largest dispatch before the first frame;
- the first-frame span of the staged composition;
- the composition-complete span;
- the largest composition dispatch;
- the pending and total composition-stage counts;
- the number of forced stages;
- the number of settings writes before the first frame;
- the settings writer's pending, flushed, and dropped-equal counters.

Every watchdog-derived field SHALL be `null` in a build without the responsiveness watchdog
feature, never `0`. The object SHALL NOT contain paths, setting values, or document content.

While any composition stage of the active window is pending, the readiness blocker
`window-composition` SHALL be active. It SHALL be derived from that window's composition evidence,
so it clears when composition completes or the window goes away. It SHALL block `app-startup`,
`idle`, and every predicate that depends on a staged surface, including `session-restore-complete`,
`visual-geometry-settled`, and `accessibility-settled`. It SHALL NOT block
`window-actions-exported`, so automation can issue actions during composition. The fields and the
blocker SHALL be documented in `docs/automation.md` and `docs/automation-reference.md`.

#### Scenario: Snapshot reports construction evidence
- **WHEN** an automation client reads the snapshot after composition is complete
- **THEN** `window_construction` reports every span and counter as integers or nulls, with `stages_pending` equal to `0`, and nothing else

#### Scenario: Staged surface blocks dependent readiness
- **WHEN** a readiness predicate depends on a surface whose composition stage is still pending
- **THEN** the wait reports `window-composition` as its blocker until that stage completes

#### Scenario: A forced stage is counted
- **WHEN** an automation client activates an action whose surface was not yet composed
- **THEN** the action succeeds, and `forced_stages` increases by the number of stages completed early

#### Scenario: Actions stay reachable during composition
- **WHEN** an automation client waits for `window-actions-exported` while composition is still pending
- **THEN** the wait succeeds without waiting for `window-composition`
