## MODIFIED Requirements

### Requirement: Local history respects large-file safety policy
The system SHALL apply LushText’s existing large-file safety thresholds to local history. For files above 8 MiB and at or below 64 MiB, the system MUST limit history capture to save-boundary snapshots. For files above 64 MiB, the system MUST make local history unavailable and MUST not capture or preview historical snapshots for that document.

#### Scenario: Reduced history capture for very large but still openable files
- **WHEN** the active saved document is larger than 8 MiB and not larger than 64 MiB
- **THEN** the system limits local-history capture to save-boundary snapshots

#### Scenario: Local history unavailable for huge files
- **WHEN** the active saved document is larger than 64 MiB
- **THEN** the system does not offer local-history browsing for that document
- **AND** the system does not create new local-history snapshots for that document

### Requirement: Local-history retention stays bounded across documents
The system SHALL keep local-history retention bounded by trimming the oldest stored snapshots after newer ones are recorded. The shipped retention policy MUST keep at most 64 snapshots for one document lineage and at most 256 snapshots across the whole app-data history store. Both are retention tiers rounded up to a power of two, so an upgrade from the former 48 and 240 trims no stored snapshot.

#### Scenario: One document lineage trims its oldest snapshots after the per-document cap
- **WHEN** a document's local-history lineage grows beyond 64 stored snapshots
- **THEN** the oldest snapshots in that lineage are removed
- **AND** the newest snapshots remain available for browsing and restore

#### Scenario: Global retention trims the oldest snapshots across all lineages
- **WHEN** the total number of stored local-history snapshots across the app exceeds 256
- **THEN** the oldest stored snapshots across all lineages are trimmed
- **AND** newer snapshots remain available across the retained lineages

### Requirement: Local-history snapshots use the native Adwaita sidebar rail
The system SHALL present local-history snapshot entries through an `AdwSidebar` rail rather than a hand-built `GtkListBox` rail. The sidebar rail MUST preserve newest-first ordering, async preview loading, compact navigation handoff, Copy, Restore, empty snapshot handling, preview-error handling, safety snapshots, and large-file availability gating.

#### Scenario: Browse snapshots in the Adwaita sidebar rail
- **WHEN** the user opens Local History for an eligible saved document with stored snapshots
- **THEN** the browser shows snapshot entries in an `AdwSidebar` rail
- **AND** the entries remain ordered newest-first
- **AND** each entry preserves the semantic snapshot metadata currently shown in the browser

#### Scenario: Select a snapshot from the sidebar rail
- **WHEN** the user selects a snapshot item in the Local History sidebar rail
- **THEN** the browser starts loading that snapshot preview asynchronously
- **AND** stale preview loads from earlier selections cannot update the preview after the selection changes

#### Scenario: Activate a snapshot from a compact sidebar rail
- **WHEN** the Local History browser is collapsed and the user activates a snapshot item in the sidebar rail
- **THEN** the browser navigates to the preview page for that selected snapshot
- **AND** the back affordance returns to the snapshot rail

#### Scenario: Copy and restore stay bound to the selected sidebar item
- **WHEN** the user selects a snapshot item in the Local History sidebar rail and the preview loads successfully
- **THEN** Copy copies that selected snapshot's text when text exists
- **AND** Restore captures the current buffer as a safety snapshot before applying that selected snapshot

#### Scenario: Empty and error states remain explicit
- **WHEN** the selected snapshot item represents an empty historical snapshot
- **THEN** the preview shows the explicit empty-snapshot explanation instead of a blank content area
- **AND** the restore action remains available

#### Scenario: Huge files still cannot open local-history browsing
- **WHEN** the active saved document is larger than 64 MiB
- **THEN** the system does not offer the Local History browser
- **AND** the Adwaita sidebar rail is not shown as a bypass around the large-file policy

### Requirement: Periodic local-history capture uses live buffer policy
The system SHALL classify periodic local-history capture from the current editor buffer rather than only the backing file's last known size. Current buffers conservatively above 8 MiB MUST use save-boundary-only history, buffers above 64 MiB MUST remain unavailable for history, and eligible smaller buffers MUST use the shared direct-or-chunked snapshot policy.

#### Scenario: Originally small document grows above 8 MiB
- **WHEN** a file loaded below the full-history threshold grows conservatively above 8 MiB while modified
- **THEN** the periodic timer skips full-buffer capture
- **AND** successful save-boundary capture remains the next eligible automatic history point

#### Scenario: Live buffer grows above 64 MiB
- **WHEN** a modified file-backed editor's current buffer grows conservatively above 64 MiB
- **THEN** local-history capture becomes unavailable for that live state
- **AND** the periodic callback does not copy or persist the buffer

#### Scenario: Eligible buffer requires chunking
- **WHEN** an eligible periodic capture is above the synchronous snapshot threshold but within the full-history policy
- **THEN** text is captured in bounded main-loop slices
- **AND** persistence starts only after path and edit generations still match
