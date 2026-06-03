# unsaved-close-safety-coverage Specification

## Purpose
Define LushText's unsaved-close safety coverage so tab and window close flows
protect drafts, saves, and modified user data through user-facing decisions.

## Requirements
### Requirement: File-backed close decisions are covered through window close flows
The test suite SHALL cover modified file-backed documents through the same tab
and window close-request paths used by users.

#### Scenario: Cancel keeps modified file open
- **WHEN** a modified file-backed tab is closed through the user close path
- **AND** the user chooses Cancel in the save-changes dialog
- **THEN** the tab remains open
- **AND** the document remains modified

#### Scenario: Save closes only after successful write
- **WHEN** a modified file-backed tab is closed through the user close path
- **AND** the user chooses Save
- **THEN** the tab closes only after the durable save succeeds
- **AND** the document's draft is removed after the successful save

#### Scenario: Save failure keeps the tab open
- **WHEN** a modified file-backed tab is closed through the user close path
- **AND** the save attempt fails
- **THEN** the tab remains open
- **AND** the document remains modified with failure feedback visible

#### Scenario: Discard closes and removes the draft
- **WHEN** a modified file-backed tab is closed through the user close path
- **AND** the user chooses Discard
- **THEN** the tab closes without writing the local edits
- **AND** the associated draft is removed intentionally

### Requirement: Untitled close decisions are covered
The test suite SHALL cover modified untitled documents so save-on-close never
treats them as saved without a successful Save As destination.

#### Scenario: Saving an untitled document requires Save As
- **WHEN** a modified untitled tab is closed through the user close path
- **AND** the user chooses Save
- **THEN** the close flow does not treat the document as saved until Save As
  completes successfully
- **AND** the untitled draft remains available while no durable destination
  exists

#### Scenario: Cancel keeps untitled draft
- **WHEN** a modified untitled tab is closed through the user close path
- **AND** the user chooses Cancel
- **THEN** the tab remains open
- **AND** its draft remains recoverable

#### Scenario: Discard removes untitled draft
- **WHEN** a modified untitled tab is closed through the user close path
- **AND** the user chooses Discard
- **THEN** the tab closes
- **AND** its draft is removed intentionally

### Requirement: Multi-tab window close decisions are covered
The test suite SHALL cover window close-request behavior when several modified
tabs require one coordinated decision.

#### Scenario: Multi-tab Cancel keeps the window open
- **WHEN** the window close-request path finds multiple modified tabs
- **AND** the user chooses Cancel
- **THEN** the window remains open
- **AND** all modified tabs remain available

#### Scenario: Selected tabs save and unchecked tabs discard
- **WHEN** the multi-tab save-changes dialog lists modified documents
- **AND** the user saves selected rows while leaving other rows unchecked
- **THEN** selected file-backed documents are saved before close completion
- **AND** unchecked documents are discarded intentionally with their drafts
  removed

#### Scenario: In-flight save blocks close completion
- **WHEN** any tab is already saving or a close-triggered save is still pending
- **THEN** the window close flow remains inhibited until the save result is
  known
- **AND** close completion is not reported early

#### Scenario: Confirmed window close persists session state
- **WHEN** the user confirms a window close decision
- **THEN** session and draft cleanup state is persisted consistently before the
  app exits or destroys the window
