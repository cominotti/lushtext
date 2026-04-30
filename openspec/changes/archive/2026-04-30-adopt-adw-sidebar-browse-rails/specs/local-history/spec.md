## ADDED Requirements

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
- **WHEN** the active saved document is larger than 50 MB
- **THEN** the system does not offer the Local History browser
- **AND** the Adwaita sidebar rail is not shown as a bypass around the large-file policy
