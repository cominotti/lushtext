## MODIFIED Requirements

### Requirement: Workspace-search event turns count every received event
The workspace-search GTK consumer SHALL receive and dispatch at most 256 channel events per scheduled turn (`MAX_SEARCH_EVENTS_PER_TICK`, a per-turn cap that `power-of-two-thresholds` measured at a power of two: a full turn MUST stay within `B_SETTLE_US`). Every successfully received `Match`, `Progress`, `ResultCap`, `Error`, and `Done` event MUST consume one unit before variant-specific handling; channel disconnection MAY terminate the turn without consuming an event unit.

#### Scenario: Progress burst contains no matches
- **WHEN** more than 256 progress events are ready before one consumer turn
- **THEN** that turn receives at most 256 events
- **AND** remaining events wait for a later scheduled turn

#### Scenario: Mixed event burst reaches the cap
- **WHEN** match, progress, result-cap, and error events are interleaved in a ready channel
- **THEN** their combined received count is at most 256 for the turn
- **AND** visible result and diagnostic semantics remain unchanged

#### Scenario: Terminal event is received within budget
- **WHEN** `Done` is the next event before the turn reaches 256 received events
- **THEN** it consumes one event unit and terminates the active search normally
- **AND** the latest pending search may proceed through the existing flight contract

### Requirement: Search retirement budgets are release-invariant
Every destructive step performed by a retired workspace-search generation SHALL consume its per-turn retirement budget in debug and release builds. Correctness-affecting mutations MUST occur outside debug-only assertions, one retirement turn MUST release no more than 256 owned items (`SEARCH_RETIREMENT_ROWS_PER_SLICE`, measured like the event cap), and unfinished state MUST remain scheduled until it reaches a terminal empty state without touching the current generation.

#### Scenario: One release-profile turn retires a large generation
- **WHEN** a retired generation owns more than 256 unique file rows, file groups, match rows, cached rows, and shared accepted-result references
- **THEN** one retirement turn removes at most 256 actual owned items in a release build
- **AND** the remaining retired state stays pending for a later turn

#### Scenario: Repeated retirement turns finish
- **WHEN** bounded retirement turns continue after a large generation is detached
- **THEN** each turn charges every successful removal before proceeding to another item
- **AND** the retired generation eventually becomes empty without changing current rows, identities, readiness, or detached-generation backpressure

#### Scenario: Debug assertions are disabled
- **WHEN** the same retirement workflow is compiled without debug assertions
- **THEN** its actual container and reference-count deltas remain identical to debug semantics
- **AND** no required mutation depends on an assertion expression being evaluated
