## ADDED Requirements

### Requirement: Periodic local-history capture uses live buffer policy
The system SHALL classify periodic local-history capture from the current editor buffer rather than only the backing file's last known size. Current buffers conservatively above 10 MiB MUST use save-boundary-only history, buffers above 50 MiB MUST remain unavailable for history, and eligible smaller buffers MUST use the shared direct-or-chunked snapshot policy.

#### Scenario: Originally small document grows above 10 MiB
- **WHEN** a file loaded below the full-history threshold grows conservatively above 10 MiB while modified
- **THEN** the periodic timer skips full-buffer capture
- **AND** successful save-boundary capture remains the next eligible automatic history point

#### Scenario: Live buffer grows above 50 MiB
- **WHEN** a modified file-backed editor's current buffer grows conservatively above 50 MiB
- **THEN** local-history capture becomes unavailable for that live state
- **AND** the periodic callback does not copy or persist the buffer

#### Scenario: Eligible buffer requires chunking
- **WHEN** an eligible periodic capture is above the synchronous snapshot threshold but within the full-history policy
- **THEN** text is captured in bounded main-loop slices
- **AND** persistence starts only after path and edit generations still match

### Requirement: Periodic history rejects stale snapshots
The system MUST discard a periodic snapshot if the editor closes, changes file identity, changes periodic generation, becomes ineligible, or is edited during chunked capture. A stale snapshot MUST NOT be written into either the old or new document lineage.

#### Scenario: File identity changes during capture
- **WHEN** Save As or in-app rename changes the editor's file identity while a periodic snapshot is in progress
- **THEN** the stale completion is rejected before persistence
- **AND** it is not attributed to the wrong local-history lineage

#### Scenario: Editor closes during capture
- **WHEN** an editor is destroyed before its periodic chunked snapshot completes
- **THEN** the weak editor completion performs no persistence
- **AND** no callback retains the closed editor indefinitely
