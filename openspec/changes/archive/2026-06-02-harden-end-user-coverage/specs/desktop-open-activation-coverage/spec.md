## ADDED Requirements

### Requirement: Application open activation is covered
The test suite SHALL cover the `ApplicationImpl::open` path used by file
manager, desktop, and CLI activation.

#### Scenario: Single file activation opens a tab
- **WHEN** the application receives a desktop or CLI open activation for a
  supported file
- **THEN** it opens or reuses an application window
- **AND** the file is opened in an editor tab with matching content

#### Scenario: Multiple file activation opens multiple tabs
- **WHEN** the application receives one open activation containing multiple
  supported files
- **THEN** each file is opened or focused according to normal duplicate-tab
  rules
- **AND** the app does not create duplicate tabs for the same canonical path

#### Scenario: Activation reuses the existing window
- **WHEN** LushText is already running
- **AND** a new file activation arrives
- **THEN** the existing application window is reused when possible
- **AND** the activated document becomes visible without creating an unnecessary
  second primary window

### Requirement: Activation handles invalid and inaccessible paths safely
The test suite SHALL cover activation paths that cannot be opened successfully.

#### Scenario: Missing file activation reports failure
- **WHEN** the application receives an open activation for a missing file
- **THEN** it reports an error through the normal user feedback path
- **AND** it does not create a misleading saved or clean editor tab for that
  missing path

#### Scenario: Inaccessible file activation does not crash
- **WHEN** the application receives an open activation for a path it cannot read
- **THEN** it keeps the app responsive
- **AND** it shows access failure feedback without corrupting open-path
  bookkeeping

### Requirement: Desktop metadata open commands are verified against runtime behavior
Desktop integration verification SHALL connect static metadata to actual open
activation behavior.

#### Scenario: Desktop entry forwards document URIs
- **WHEN** the generated or staged desktop entry is inspected
- **THEN** its `Exec` line supports document activation with file arguments or
  URIs
- **AND** the verification proves that the same activation path opens a document
  in LushText

#### Scenario: CLI file arguments take priority over restored active selection
- **WHEN** LushText starts with explicit file arguments and a previous session
  also exists
- **THEN** the explicitly activated files are opened
- **AND** restored session state does not steal the active selection away from
  the activated file
