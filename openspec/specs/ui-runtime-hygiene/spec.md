# ui-runtime-hygiene Specification

## Purpose
TBD - created by archiving change close-consistency-and-decomposition-gaps. Update Purpose after archive.
## Requirements
### Requirement: Registered callbacks are invoked outside held RefCell borrows
UI code that stores registered callbacks in `RefCell` collections SHALL
clone the callbacks (or otherwise end the borrow) before invoking them, so a
callback that re-enters registration or other borrowing methods cannot panic
the GTK thread. Iterating a callback collection while its `RefCell` borrow
is held across the invocations MUST NOT remain in production paths.

#### Scenario: Callback re-entering registration does not panic
- **WHEN** a registered file-loaded, bookmark, sidebar-section, or
  create/rename callback re-enters a method that mutably borrows the same
  callback storage
- **THEN** no `BorrowError`/`BorrowMutError` panic occurs
- **AND** callback invocation order and payloads are unchanged for existing
  consumers

#### Scenario: Existing clone-then-call sites remain the pattern
- **WHEN** new UI code adds a registered-callback invocation
- **THEN** it follows the clone-then-call shape already used by the
  codebase's compliant sites
- **AND** review guidance names the pattern so regressions are caught

### Requirement: Best-effort cleanup emits diagnostics
Background best-effort cleanup paths that intentionally ignore failures
SHALL emit a `tracing` diagnostic when the cleanup fails, naming the target
path or resource, so silent-orphan debugging has a trail. Ignoring the
failure for workflow purposes remains correct; ignoring it silently does
not.

#### Scenario: Cancelled temp-item cleanup failure is logged
- **WHEN** cancelling an inline new-item flow fails to remove its
  placeholder file or directory on the background thread
- **THEN** a warning-level diagnostic records the path and error
- **AND** the user-facing cancel flow is otherwise unchanged

### Requirement: Ordered-save lock poisoning degrades to recovery
The session save-ordering lock SHALL recover from poisoning instead of
panicking a second time, so a prior panic during one ordered save cannot
turn a later close-time session save into a lost session snapshot. Recovery
MUST keep the ordering state consistent or conservatively rebuild it.

#### Scenario: Close-time save survives a poisoned ordering lock
- **WHEN** a previous ordered session save panicked and poisoned the
  ordering lock, and the window later saves the session at close time
- **THEN** the close-time save acquires the ordering state through the
  existing unpoisoned-recovery helper and completes
- **AND** ordering semantics for subsequent saves remain correct

