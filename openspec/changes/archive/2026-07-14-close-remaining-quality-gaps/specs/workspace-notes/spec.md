## ADDED Requirements

### Requirement: Notes browser source construction and querying remain bounded
The system SHALL construct the `Browse Notes...` source with explicit aggregate entry, searchable-text byte, sidecar-scan, open-editor snapshot, and recovery-diagnostic limits. The browser SHALL retain one bounded immutable source, SHALL execute note-body matching outside GTK, and SHALL retain at most one active query plus one latest compact superseding query. Source or result truncation MUST be explicit without changing workspace-scope, section ordering, canonical de-duplication, preview, or Open semantics for admitted rows.

#### Scenario: Aggregate Notes source exceeds admission limits
- **WHEN** bookmarks, folder notes, document notes, and eligible open-tab rows exceed an aggregate browser admission limit
- **THEN** source construction stops at a deterministic boundary and retains no more than the configured entry and text budgets
- **AND** the browser reports that later source material was omitted instead of presenting the admitted source as complete

#### Scenario: Open editors contain many bookmark rows
- **WHEN** GTK snapshots open-editor note and bookmark metadata before opening the browser
- **THEN** collection stops at the browser-owned open-editor snapshot bound
- **AND** no `usize::MAX` or equivalent unbounded collection bypasses worker-side admission

#### Scenario: Queries change faster than matching completes
- **WHEN** the user types several Notes queries while an earlier full-source match is active
- **THEN** the active query is cancelled cooperatively and only the latest compact pending query is retained
- **AND** stale matches never rebuild the sidebar or change the selected preview

#### Scenario: Current query exceeds the render limit
- **WHEN** the current background match finds more rows than the existing browser render cap
- **THEN** it retains and publishes only the capped ordered result indexes
- **AND** the browser preserves its existing visible refinement message and grouped row behavior

#### Scenario: Notes browser closes during source or query work
- **WHEN** the dialog is disposed while bounded source construction or query matching is active
- **THEN** current work is cancelled or discarded without a later GTK callback
- **AND** retained source, pending query, and result payloads are released
