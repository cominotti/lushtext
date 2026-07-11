## ADDED Requirements

### Requirement: Draft body lifetime is bounded across tabs
The system SHALL extend chunked draft snapshot responsiveness through the write stage so a multi-tab draft pass does not retain all completed buffer strings simultaneously. GTK snapshot work MUST yield in bounded chunks, and the next complete body MUST NOT be accumulated while the previous complete body is still retained for persistence.

#### Scenario: Snapshot and write stages apply backpressure
- **WHEN** a draft pass contains more than one large dirty editor
- **THEN** completion of one chunked snapshot hands that body to background persistence before the next complete body is retained
- **AND** GTK input and repaint remain schedulable between snapshot chunks and worker completions

#### Scenario: Pending autosave coalesces during pipeline work
- **WHEN** another first-dirty or periodic autosave request arrives while the bounded pipeline is active
- **THEN** the window records one pending rerun
- **AND** it does not start a conflicting snapshot or manifest writer
