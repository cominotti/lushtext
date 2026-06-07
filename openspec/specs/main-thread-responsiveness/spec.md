# main-thread-responsiveness Specification

## Purpose
Define LushText's GTK main-thread responsiveness contract for workflows that
touch editor text, filesystem state, or expensive analysis from UI actions.

## Requirements
### Requirement: GTK workflows separate snapshots from blocking work
The system SHALL keep filesystem I/O, canonical path probes, and unbounded pure
analysis out of GTK signal handlers, timer callbacks, and immediate UI action
paths. GTK-only state such as widget properties and `TextBuffer` contents MUST
be collected on the GTK thread, but any collection that can grow with document
size SHALL use a bounded fast path, a chunked main-loop snapshot, or an explicit
large-input fallback before worker-thread processing begins.

#### Scenario: Worker code does not touch GTK state
- **WHEN** an asynchronous responsiveness workflow needs editor text, widget
  visibility, active file paths, or search-panel state
- **THEN** the workflow captures that GTK-owned state on the GTK thread before
  scheduling worker work
- **AND** the worker receives only owned non-GTK data such as strings, paths,
  query specifications, or model records

#### Scenario: Large buffer snapshot yields before persistence or analysis
- **WHEN** a workflow needs text from a buffer large enough to exceed the
  synchronous snapshot threshold
- **THEN** the snapshot is gathered in bounded GTK main-loop slices
- **AND** user input, repaint, and pending async completions can run between
  snapshot slices

#### Scenario: Small buffer snapshot remains direct and bounded
- **WHEN** a workflow needs text from a buffer known to be below the synchronous
  snapshot threshold
- **THEN** the workflow may capture the text directly on the GTK thread
- **AND** the captured text is handed to worker or persistence logic without
  performing filesystem I/O in the same GTK callback

### Requirement: Replace All undo persistence does not block interaction
The system SHALL update Replace All undo UI state immediately on the GTK thread
while saving, replacing, or deleting persisted undo safety state on a background
thread. Disk persistence and cleanup MUST preserve the active safety-window
semantics already required by the search-replace safety contract.

#### Scenario: Undo backup save is scheduled after UI state updates
- **WHEN** Replace All completes with an undo backup that must be persisted by
  the search panel
- **THEN** the search panel exposes the undo affordance from in-memory state
  without waiting for the disk save to finish
- **AND** the persisted backup save runs through the background filesystem
  workflow

#### Scenario: Undo backup clear is scheduled without blocking panel close
- **WHEN** the search panel closes or the Replace All undo safety window ends
- **THEN** the visible undo state is cleared on the GTK thread
- **AND** persisted undo state is deleted on a background thread
- **AND** the close or clear action does not wait for slow storage

#### Scenario: Stale undo persistence result cannot resurrect old state
- **WHEN** an older undo-backup save or clear operation completes after a newer
  undo generation has already been installed
- **THEN** the completion is ignored for visible UI state
- **AND** disk cleanup ordering still removes stale persisted backup state for
  the inactive generation

### Requirement: Draft autosave snapshots are bounded and retryable
The system SHALL keep regular dirty-draft autosave from monopolizing the GTK
thread when one or more modified editors contain large buffers. Autosave MUST
capture dirty editor contents in bounded chunks when needed, write drafts on a
background thread, and leave failed or stale captures eligible for a later
autosave attempt.

#### Scenario: Large dirty draft is captured in chunks before write
- **WHEN** the periodic autosave sweep finds a modified dirty editor whose
  buffer exceeds the synchronous snapshot threshold
- **THEN** the editor text is captured in bounded main-loop slices
- **AND** the draft file and manifest write run on a background thread after
  the snapshot completes

#### Scenario: Multiple dirty tabs do not create one long GTK tick
- **WHEN** the autosave sweep finds multiple modified dirty editors
- **THEN** each large snapshot yields between bounded chunks
- **AND** the sweep does not copy every dirty buffer to owned strings in one
  uninterrupted GTK timer callback

#### Scenario: Failed draft write remains retryable
- **WHEN** a draft snapshot is accepted but the background draft write fails
- **THEN** the editor remains eligible for a later autosave attempt
- **AND** existing recovery data is not deleted solely because the asynchronous
  write failed

#### Scenario: Close-time draft safety is preserved
- **WHEN** the user closes the window while unsaved dirty draft state remains
- **THEN** the app still flushes recovery data before the close completes
- **AND** any responsiveness optimization MUST NOT weaken the existing
  close-time crash-recovery guarantee

### Requirement: Save As path bookkeeping refreshes asynchronously
The system SHALL avoid synchronous canonical path probes in the GTK completion
path after a successful Save As. Save As MUST update the visible document path
immediately from the selected destination and refresh canonical open-path
bookkeeping through a background task whose result is applied only if the editor
still represents the same saved path.

#### Scenario: Save As success does not wait for canonicalization
- **WHEN** a Save As write succeeds for a document
- **THEN** the tab title, visible file path, and document state update from the
  chosen destination on the GTK thread
- **AND** canonical path lookup for open-path bookkeeping runs in the
  background

#### Scenario: Stale Save As canonical refresh is ignored
- **WHEN** a Save As canonical refresh completes after the editor has been
  renamed, closed, or saved to a different path
- **THEN** the stale result does not change the window's open-path bookkeeping
- **AND** duplicate-tab prevention remains consistent for the editor's current
  path

### Requirement: Replace preview generation is asynchronous and stale-safe
The system SHALL generate potentially expensive Replace preview data away from
the GTK action path. Entering preview mode MUST capture the current query,
replacement text, and search results on the GTK thread, run pure preview
generation on a worker thread when the input is non-trivial, and apply the
preview only if the search generation is still current.

#### Scenario: Large Replace preview does not block the search panel
- **WHEN** the user enters Replace preview mode with many search matches
- **THEN** the search panel can show a pending preview state without generating
  all preview rows synchronously in the activation callback
- **AND** pure preview construction runs on a background thread

#### Scenario: Stale Replace preview result is discarded
- **WHEN** the query, replacement text, search results, or panel lifetime
  changes before an asynchronous preview result returns
- **THEN** the stale preview result is discarded
- **AND** the search panel keeps the state for the newer generation

### Requirement: Expensive preview and analysis workflows degrade before blocking input
The system SHALL protect user input from large Markdown preview preprocessing,
minimap marker scans, and lossy encoding analysis. Each workflow MUST either
use a bounded/chunked GTK snapshot plus worker processing, enforce a documented
large-input threshold with a user-visible paused or limited state, or reuse
already available load-time analysis data.

#### Scenario: Markdown preview avoids unbounded preprocessing in one GTK turn
- **WHEN** Markdown preview refresh is requested for a document whose text or
  generated event stream would exceed the configured automatic-preview budget
- **THEN** the app either preprocesses bounded owned input asynchronously or
  shows an explicit paused/limited preview state
- **AND** it does not perform unbounded Markdown preprocessing in one GTK
  callback

#### Scenario: Minimap long-line markers avoid full-buffer blocking scans
- **WHEN** long-line minimap markers are enabled for a document large enough to
  make full-buffer scanning expensive
- **THEN** marker collection uses cached load-time data, a chunked scan, or a
  documented threshold that omits expensive markers
- **AND** scrolling or editing the document is not blocked by a full-buffer scan

#### Scenario: Lossy encoding analysis returns asynchronously for non-small buffers
- **WHEN** the user chooses a save encoding that requires scanning non-small
  buffer contents for lossy conversion
- **THEN** the analysis runs after a bounded snapshot and worker processing step
- **AND** the lossy-save confirmation is shown only for the still-current
  document and encoding request
