## ADDED Requirements

### Requirement: Explicit Document Activation Opens Requested Local Files
LushText SHALL honor local-file open activations delivered by the desktop, file manager, or CLI. An explicitly activated local file MUST open in a focused editor tab unless an existing successfully loaded or currently loading tab already owns the same document according to normal duplicate detection.

#### Scenario: Explicit activation opens and focuses the requested file
- **WHEN** the application receives an open activation for a readable local file
- **THEN** LushText opens or reuses an application window
- **AND** the activated file is opened in an editor tab
- **AND** that editor tab becomes the selected tab

#### Scenario: Successful duplicate activation focuses existing document
- **WHEN** the application receives an open activation for a local file that is already open successfully
- **THEN** LushText focuses the existing document tab
- **AND** it does not create an additional tab for the same canonical document

#### Scenario: Pending duplicate activation does not create parallel loads
- **WHEN** the application receives an open activation for a local file whose first load is still pending
- **THEN** LushText treats the pending tab as the active owner of that document
- **AND** it does not start a second parallel load for the same path

### Requirement: Failed Restore Placeholders Do Not Block Explicit Activation
LushText SHALL keep failed restored or previously failed tabs visible as diagnostic placeholders, but those failed placeholders MUST NOT be treated as successful duplicate owners for later explicit desktop or CLI activation of the same path.

#### Scenario: Explicit activation bypasses stale failed placeholder
- **WHEN** a restored tab for a path is showing a file-open failure
- **AND** the application later receives an explicit open activation for the same path
- **THEN** LushText keeps the failed tab and its error message visible
- **AND** it opens the activated path in a new editor tab when the path is now readable
- **AND** the newly opened tab becomes the selected tab

#### Scenario: Failed placeholder does not reserve open-path bookkeeping
- **WHEN** a file load fails before the document reaches a loaded state
- **THEN** LushText removes any provisional duplicate-detection keys for that path
- **AND** a later explicit activation for the same path is not rejected as a duplicate of the failed placeholder

#### Scenario: Failed modified placeholder remains recoverable without blocking activation
- **WHEN** a failed-load tab preserves modified buffer content for user safety
- **THEN** LushText keeps that buffer and its recovery affordances available
- **AND** it does not treat the failed tab as the successfully opened owner of the failed path

### Requirement: Non-Path Activation Inputs Fail Visibly
LushText SHALL handle every `gio::File` received through application open activation. Inputs that cannot be represented as a local filesystem path MUST produce user-visible failure feedback and MUST NOT be silently ignored.

#### Scenario: Unsupported URI activation reports failure
- **WHEN** the application receives an open activation for a `gio::File` that has a URI but no local path
- **THEN** LushText reports that the URI cannot be opened through the local-file editor path
- **AND** it does not create a bogus saved document tab for that URI
- **AND** it does not crash or hang

#### Scenario: Unsupported URI does not block valid files in same activation
- **WHEN** one open activation contains both an unsupported non-path URI and a readable local file
- **THEN** LushText reports the unsupported URI failure
- **AND** it still opens and focuses the readable local file
