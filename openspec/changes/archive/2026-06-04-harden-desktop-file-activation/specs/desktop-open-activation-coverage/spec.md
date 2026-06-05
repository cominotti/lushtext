## ADDED Requirements

### Requirement: Stale Failed-Tab Activation Regressions Are Covered
The test suite SHALL cover desktop and CLI activation behavior when existing tabs include failed-load placeholders from session restore or earlier activation attempts.

#### Scenario: Activation opens over restored failed placeholder
- **WHEN** a regression test seeds or constructs a failed restored tab for a path
- **AND** a later `ApplicationImpl::open` activation requests the same path after it becomes readable
- **THEN** the test verifies that the old failed tab remains present
- **AND** the requested file opens in a new selected tab with matching content

#### Scenario: Failed placeholder bookkeeping is cleared
- **WHEN** a regression test drives a load failure through the normal open path
- **THEN** the test verifies that duplicate-detection bookkeeping no longer contains the failed path
- **AND** a later activation for that path succeeds after the file becomes readable

#### Scenario: Modified failed placeholder does not block activation
- **WHEN** a regression test creates a failed-load tab that preserves modified buffer content
- **AND** a later activation requests the same path
- **THEN** the test verifies that the modified failed tab remains recoverable
- **AND** the activated file still opens and becomes selected

### Requirement: Activation Ordering And Duplicate Regressions Are Covered
The test suite SHALL cover activation ordering so explicit desktop or CLI opens continue to cooperate with session restore, canonical duplicate detection, and multi-file activation.

#### Scenario: Explicit activation remains selected after restored failure settles
- **WHEN** a regression test starts LushText with an explicit file activation and a prior session containing another tab that later fails to load
- **THEN** the test verifies that the explicit file remains the selected tab
- **AND** the restored failed tab does not steal focus after its error appears

#### Scenario: Successful duplicate activation still deduplicates
- **WHEN** a regression test activates an already loaded file, including a canonical duplicate such as a symlink when supported by the fixture
- **THEN** the test verifies that LushText focuses the existing loaded document
- **AND** it does not create duplicate tabs for the same canonical file

#### Scenario: Multi-file activation continues after an unsupported input
- **WHEN** a regression test sends one activation containing an unsupported URI-shaped `gio::File` and one readable local file
- **THEN** the test verifies visible failure feedback for the unsupported input
- **AND** the local file still opens with matching content

### Requirement: Non-Path Open Inputs Are Covered
The test suite SHALL include coverage for application open inputs that are valid `gio::File` values but do not expose a local path.

#### Scenario: Non-path URI activation is not silent
- **WHEN** a regression test invokes `ApplicationImpl::open` with a non-path URI file
- **THEN** the test verifies that user-visible feedback is published
- **AND** no fake path-backed editor tab is created for the unsupported URI

#### Scenario: Reused window receives URI failure feedback
- **WHEN** LushText already has a window
- **AND** a non-path URI activation arrives
- **THEN** the test verifies that the existing window remains responsive
- **AND** the unsupported input is reported through that window's feedback surface
