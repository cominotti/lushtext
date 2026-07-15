## ADDED Requirements

### Requirement: Command-palette search is one-active and one-latest
The command palette SHALL run at most one background search and retain at most one latest superseding request as compact query state. New input MUST cancel or supersede obsolete work cooperatively, and only the current generation may update rows, searching state, accessibility state, or readiness.

#### Scenario: Rapid typing outpaces search completion
- **WHEN** several query generations arrive while one full-index search is active
- **THEN** intermediate pending requests are replaced by the latest query
- **AND** at most one active worker and one compact pending request are retained

#### Scenario: Active search observes cancellation
- **WHEN** a newer query supersedes an active search
- **THEN** candidate scoring stops at a bounded cancellation checkpoint
- **AND** the latest request starts after active ownership is released

#### Scenario: Stale completion reaches GTK
- **WHEN** an obsolete search completes after a newer generation exists
- **THEN** it changes neither visible results nor searching/accessibility state
- **AND** readiness remains pending only for current active or queued work

#### Scenario: Palette closes during search
- **WHEN** the palette closes with active or pending query work
- **THEN** cancellation releases retained search state
- **AND** no later completion reopens or mutates the closed surface
