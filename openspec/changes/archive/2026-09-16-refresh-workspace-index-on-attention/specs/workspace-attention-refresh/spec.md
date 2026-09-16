## ADDED Requirements

### Requirement: Workspace surfaces refresh at user-attention moments
The system SHALL refresh the command-palette file index and the workspace tree when the application window becomes active and when the command palette is opened. Each surface MUST refresh through the bounded, debounced, generation-guarded refresh path it already owns; no additional refresh mechanism may be introduced for these triggers. Refreshing MUST NOT be driven by a periodic idle timer.

#### Scenario: Returning to the window refreshes both surfaces
- **WHEN** the application window becomes active after being inactive
- **THEN** a palette file-index rebuild and a workspace tree refresh are requested
- **AND** each runs through that surface's existing refresh path

#### Scenario: Opening the palette refreshes the index
- **WHEN** the command palette is opened
- **THEN** a file-index rebuild is requested

#### Scenario: Opening the palette does not wait for the refresh
- **WHEN** the command palette is opened and a refresh is requested or already running
- **THEN** the palette opens against the currently installed index without waiting
- **AND** the main thread performs no filesystem traversal

#### Scenario: Refresh completion is observable through existing readiness
- **WHEN** an attention-triggered refresh has been requested and has not settled
- **THEN** the existing command-palette index and workspace-refresh readiness blockers report not settled
- **AND** no additional readiness predicate is required to observe it

### Requirement: Attention refresh is throttled by one process-wide adaptive interval
Attention-triggered refresh SHALL be throttled by a minimum interval that is shared across every window in the process and keyed on the workspace folder set, so that multiple windows over one workspace do not each refresh it. The interval MUST be derived from the observed wall time of the previous refresh, bounded below by a floor and above by a ceiling, so that a slow filesystem throttles itself. A trigger arriving inside the interval MUST be refused rather than queued.

#### Scenario: Rapid activation does not refresh repeatedly
- **WHEN** the window is activated several times in quick succession
- **THEN** at most one refresh is requested for that workspace folder set
- **AND** the suppressed triggers do not accumulate into later refreshes

#### Scenario: Multiple windows over one workspace refresh once
- **WHEN** two windows are open over the same workspace folder set and both become active in quick succession
- **THEN** the folder set is refreshed at most once for that interval

#### Scenario: A slow refresh lengthens the interval
- **WHEN** a refresh takes substantially longer than a previous one
- **THEN** the next permitted refresh is deferred proportionally to the observed duration
- **AND** the deferral never exceeds the configured ceiling

#### Scenario: A fast refresh does not shrink below the floor
- **WHEN** a refresh completes in negligible time
- **THEN** the next permitted refresh is still deferred by at least the configured floor

### Requirement: Attention refresh yields to in-flight user work
Attention-triggered refresh SHALL be refused while any editor is saving, a window-close transaction is in flight, or a draft autosave is pending, so that refresh work cannot occupy the shared background worker pool ahead of operations that protect user work. A refusal MUST NOT be queued for later replay.

#### Scenario: A save in progress refuses a refresh
- **WHEN** the window becomes active while an editor save is in flight
- **THEN** no refresh is requested
- **AND** the save completes without waiting on refresh work

#### Scenario: A file dialog closing does not trigger work ahead of a save
- **WHEN** a native or portal file dialog closes and the window becomes active as part of a Save As
- **THEN** the resulting save is not queued behind a refresh

#### Scenario: A close transaction refuses a refresh
- **WHEN** the window becomes active while a close transaction is disabling input across its draft and session yields
- **THEN** no refresh is requested

#### Scenario: Refusal is not replayed
- **WHEN** a refresh is refused because user work was in flight and that work then completes
- **THEN** no refresh is started as a consequence of the earlier refusal
- **AND** the next attention moment is what starts one
