## ADDED Requirements

### Requirement: Recent Visibility Follows Live Tab Reality

The Open popover SHALL exclude only file-backed documents that are currently mounted in live editor tabs. Stale duplicate-detection, canonical-path, failed-load, or previously detached tab bookkeeping MUST NOT hide persisted recent documents once no matching live tab remains.

#### Scenario: Closed same-session documents reappear despite stale identity bookkeeping

- **WHEN** a local file-backed document is opened successfully and recorded as recent
- **AND** every tab for that document is closed in the same application session
- **THEN** opening the Open popover shows that document as an eligible recent row
- **AND** no stale open-path or canonical-path identity keeps it hidden

#### Scenario: Startup-loaded recents remain visible with no restored tabs

- **WHEN** recent-document persistence contains existing local file-backed documents
- **AND** LushText starts with no restored file-backed tabs
- **THEN** opening the Open popover shows those persisted rows
- **AND** the empty state is not shown as a substitute for valid rows

#### Scenario: Real open and close workflows keep recents synchronized

- **WHEN** a document is opened through file chooser, sidebar, command palette, desktop activation, CLI activation, or recent-row activation
- **AND** the tab is later closed through tab close, close action, close-tab-for-path, bulk close, or delete/rename workflows
- **THEN** the Open popover visibility filter reflects the remaining live tabs only
- **AND** closed recent rows reappear without restarting LushText

### Requirement: Recent Open Regression Coverage Is Broad

The implementation SHALL include regression coverage across pure services, window state, GTK widgets, D-Bus/automation action paths, visual geometry, and accessibility-relevant popover states for recent-open synchronization.

#### Scenario: Regression tests cover stale identity edge cases

- **WHEN** the recent-open regression suite runs
- **THEN** it covers stale display paths, stale canonical paths, duplicate path spellings, failed loads, cancelled loads, Save As, sidebar rename/delete, session restore, app startup from persisted recents, open while the popover is visible, close while the popover is visible, and multiple recent rows where all or only some are still open

#### Scenario: Regression tests cover visible state extremes

- **WHEN** Open popover UI and smoke tests run
- **THEN** they cover no eligible recents, one closed recent, representative recents, many recents, awkward/deep path labels, all recent documents currently open, all recent documents closed, constrained geometry, keyboard navigation, accessible roles/names, and item-region-only scrolling
