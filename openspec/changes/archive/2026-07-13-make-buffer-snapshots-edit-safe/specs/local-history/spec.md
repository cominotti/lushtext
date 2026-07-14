## ADDED Requirements

### Requirement: Periodic local-history scheduling is superseding and disposable
Each editor SHALL own at most one scheduled periodic local-history timer and at most one active chunked periodic snapshot. Rescheduling, save, path change, ineligibility, or disposal MUST remove or supersede older sources without retaining them until their original deadlines.

#### Scenario: Repeated clean and dirty transitions occur within five minutes
- **WHEN** an editor repeatedly starts and ends modified cycles before the periodic interval expires
- **THEN** only the latest eligible timer remains scheduled
- **AND** obsolete timer callbacks do not accumulate for the old cycles

#### Scenario: Periodic snapshot is superseded by an edit
- **WHEN** the source buffer changes during an active periodic snapshot
- **THEN** the snapshot is cancelled and releases its admission permit and GTK resources
- **AND** at most one later periodic schedule represents the current cycle

#### Scenario: Editor is disposed with history work pending
- **WHEN** an editor closes while its timer or snapshot is pending
- **THEN** both sources are removed or rendered inert immediately
- **AND** no later callback retains or mutates the disposed editor
