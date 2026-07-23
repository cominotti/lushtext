## ADDED Requirements

### Requirement: Closed-file bookmark previews are one-active and one-latest
Each open Notes browser SHALL own at most one active closed-file bookmark excerpt load and one latest pending compact request. Selecting a newer preview MUST cancel the active request and replace the pending request without launching another worker until the active request reaches a terminal outcome. Excerpt loading MUST check cancellation during bounded ingestion and line scanning, and only the current browser lifetime, preview generation, and selected bookmark identity may publish preview state.

#### Scenario: Rapid selection outpaces a slow closed-file load
- **WHEN** the user selects several closed-file bookmarks before the first excerpt load terminates
- **THEN** the browser retains one active load and at most the latest compact pending request
- **AND** intermediate selections do not accumulate worker jobs or excerpt payloads

#### Scenario: Active excerpt observes cancellation
- **WHEN** a newer selection cancels a closed-file excerpt during bounded read or line scanning
- **THEN** obsolete work stops at a bounded cancellation checkpoint
- **AND** the latest pending request becomes eligible only after the active terminal is observed

#### Scenario: Stale terminal reaches the browser
- **WHEN** an obsolete excerpt load returns after the selection or browser lifetime changed
- **THEN** it cannot replace preview content, loading state, or the Open action target
- **AND** only the still-current latest request may publish

#### Scenario: Notes browser closes under preview pressure
- **WHEN** the dialog closes with active and pending closed-file preview work
- **THEN** active work is cancelled and pending work is discarded
- **AND** no later completion retains or mutates the closed browser

#### Scenario: Bookmark source is already open
- **WHEN** the selected bookmark can be previewed from its live editor
- **THEN** the browser uses the existing live excerpt path without starting a closed-file worker
- **AND** obsolete closed-file work is cancelled or discarded
