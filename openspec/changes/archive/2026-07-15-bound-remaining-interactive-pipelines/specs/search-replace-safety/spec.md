## ADDED Requirements

### Requirement: Workspace search owns one worker group and one latest request
The search panel SHALL own at most one active workspace-search controller/walker group and one latest pending compact request. A superseding query MUST cancel the active generation and replace the pending request, but MUST NOT launch another worker group until the active result stream reaches a terminal disconnected state.

#### Scenario: User types several queries rapidly
- **WHEN** several newer valid queries arrive before the active workspace search observes cancellation
- **THEN** the panel retains only the latest pending compact request
- **AND** no overlapping replacement controller/walker group is started

#### Scenario: Cancelled search disconnects
- **WHEN** the active search reaches its cancelled or disconnected terminal state
- **THEN** the panel revalidates and starts the latest pending request, if any
- **AND** intermediate superseded requests never consume traversal workers

#### Scenario: Panel closes with active and pending search
- **WHEN** the panel lifetime ends while one search is active and another is pending
- **THEN** the active search is cancelled and the pending request is discarded
- **AND** neither generation can later publish results or readiness state

### Requirement: Accepted search matches have immutable generation identity
The system SHALL seal each accepted search result set into one immutable generation-owned snapshot shared by list projection, Replace Preview, checked-row state, and apply planning. Sharing MUST preserve stable match identity, preview budgets, explicit selection, and stale-file validation without copying the whole match vector on GTK.

#### Scenario: Replace Preview uses a shared result snapshot
- **WHEN** Replace Preview begins for the current accepted search generation
- **THEN** it references the same immutable match snapshot used by current result identity
- **AND** building the preview does not duplicate every `SearchMatch` on the GTK thread

#### Scenario: Search generation changes during preview construction
- **WHEN** a newer search result snapshot is accepted before the old preview completes
- **THEN** the old snapshot may remain alive only for its bounded in-flight owner
- **AND** its completion cannot replace, check, or apply matches in the newer generation
