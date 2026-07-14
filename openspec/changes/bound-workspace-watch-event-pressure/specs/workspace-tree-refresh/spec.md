## ADDED Requirements

### Requirement: Watcher event delivery uses a bounded coalescing mailbox
The watcher backend SHALL normalize tree-changing events outside GTK and merge them into one bounded pending notice. The notice SHALL contain either a capped unique path set or a conservative full-refresh marker; exceeding any retained-path or queued-update bound MUST promote to full refresh rather than silently losing visible changes.

#### Scenario: Event burst stays below the path cap
- **WHEN** create, remove, and rename events produce a unique changed-path set within the configured cap
- **THEN** the mailbox retains one deduplicated bounded path notice
- **AND** GTK receives no access-only or duplicate paths

#### Scenario: Event burst exceeds the path cap
- **WHEN** unique tree-changing paths exceed the configured cap before GTK consumes them
- **THEN** the pending notice becomes a full-refresh marker
- **AND** additional paths do not grow retained memory

#### Scenario: Producer outruns GTK polling
- **WHEN** several backend batches arrive before the next GTK poll
- **THEN** they merge into the same bounded notice
- **AND** no unbounded channel backlog is created

#### Scenario: Error and disconnect arrive with pending changes
- **WHEN** bounded changes, backend errors, or disconnection overlap
- **THEN** the mailbox preserves a bounded current error/disconnect state and conservative refresh need
- **AND** repeated identical errors do not grow retained state

### Requirement: GTK consumes bounded watcher work per turn
Each workspace-section poll callback SHALL take at most one bounded watcher notice and SHALL keep refresh-side pending paths under the same cap. A full-refresh marker MUST dominate accumulated targeted paths, while manual Refresh and current tree interaction remain available.

#### Scenario: GTK consumes a path notice
- **WHEN** the poll callback receives a bounded changed-path notice
- **THEN** it performs work proportional to at most the configured cap
- **AND** it returns control to the main loop without draining an unbounded producer queue

#### Scenario: Targeted refresh accumulation crosses the cap
- **WHEN** refresh debouncing accumulates more unique paths than the targeted-refresh cap
- **THEN** the pending plan becomes one full refresh
- **AND** the path set is released instead of continuing to grow

#### Scenario: Full refresh is already pending
- **WHEN** later targeted watcher notices arrive while a full refresh is pending
- **THEN** those paths are not accumulated separately
- **AND** the one pending full refresh remains authoritative
