## ADDED Requirements

### Requirement: Watcher pressure has deterministic retained-state coverage
The project SHALL test and benchmark watcher delivery when event production is slower than, equal to, and faster than GTK consumption. Coverage MUST report the path cap, mailbox state, full-refresh promotions, notices consumed per poll, and retained refresh-plan size.

#### Scenario: Sustained producer pressure exceeds consumer rate
- **WHEN** a fixture emits many debounced event batches without GTK polling
- **THEN** retained event state remains bounded
- **AND** the next poll observes a path notice or conservative full refresh representing the burst

#### Scenario: Bulk rename storm is replayed
- **WHEN** a scale fixture emits awkward Unicode, overlapping, duplicate, and deeply nested rename paths
- **THEN** normalization and merge timing are recorded off the GTK path
- **AND** GTK-side work remains bounded by one notice and the configured path cap

#### Scenario: Repeated errors remain bounded
- **WHEN** watcher failures repeat faster than the UI can render feedback
- **THEN** retained diagnostic state remains constant-space
- **AND** current-generation recovery or manual Refresh stays available
