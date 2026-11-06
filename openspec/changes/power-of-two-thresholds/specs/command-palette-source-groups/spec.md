## MODIFIED Requirements

### Requirement: Palette source inventories are bounded and superseding
The system SHALL keep palette source construction bounded before query scoring begins. File-index construction MUST retain at most 131,072 canonical files (a presentation cap rounded up from 100,000 by `power-of-two-thresholds`), MUST avoid materializing an unbounded flat directory, and MUST cooperatively cancel superseded traversal. Note-source construction MUST retain at most 16,384 entries (a presentation cap rounded up from 10,000 by `power-of-two-thresholds`, so every note `main` retained is retained) and at most 64 MiB of aggregate searchable UTF-8 note text, MUST report deterministic truncation diagnostics, and MUST NOT retain bodies beyond those limits. File-index and note-source coordinators SHALL each own at most one active request plus one compact latest request.

#### Scenario: Huge flat directory exceeds remaining index capacity
- **WHEN** one workspace directory contains more visible entries than the remaining 131,072-file index capacity
- **THEN** traversal retains only bounded directory and index state while selecting the admitted entries
- **AND** it does not materialize and sort the complete flat directory before enforcing the index limit

#### Scenario: Workspace scope changes during index construction
- **WHEN** a newer workspace scope requests an index while an older traversal is active
- **THEN** the older traversal observes cooperative cancellation
- **AND** the coordinator retains only the latest compact scope request rather than queueing full indexes

#### Scenario: Note corpus exceeds aggregate bounds
- **WHEN** eligible note sidecars exceed either 16,384 entries or 64 MiB of aggregate searchable UTF-8 text
- **THEN** source construction stops retaining additional note bodies according to deterministic source order
- **AND** the palette remains usable with a visible bounded-truncation diagnostic

#### Scenario: Note refresh is superseded repeatedly
- **WHEN** note or bookmark edits request several refreshes while one sidecar load is active
- **THEN** only one latest compact refresh request remains pending
- **AND** stale workers release loaded bodies without replacing the current source
