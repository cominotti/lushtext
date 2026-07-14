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

### Requirement: Reusable task boundary preserves responsiveness
Fitting LushText background workflows SHALL use `gtk-lush-tasks` for bounded
worker execution and GLib-main-loop completion delivery after the Phase 3
migration. GTK-owned snapshots, large-buffer chunking, durable-write ordering,
and domain-specific freshness checks MUST remain in the owning workflow when
they express app behavior rather than reusable task dispatch.

#### Scenario: Worker dispatch uses gtk-lush-tasks
- **WHEN** a LushText workflow schedules blocking filesystem work, canonical
  probes, expensive pure analysis, or persistence through a fitting
  `spawn_blocking_then`-style path
- **THEN** the workflow uses `gtk-lush-tasks` for worker scheduling and
  main-thread completion delivery
- **AND** it does not keep a duplicate app-local worker dispatcher for fitting
  call sites

#### Scenario: GTK snapshots still happen before worker scheduling
- **WHEN** a migrated workflow needs editor text, widget visibility, active
  file paths, selected rows, or other GTK-owned state
- **THEN** it captures that state on the GTK thread before scheduling worker
  code
- **AND** the worker receives owned non-GTK data

#### Scenario: App-owned freshness checks remain visible
- **WHEN** a migrated completion depends on current tab identity, document
  path, search generation, encoding request, undo generation, persistence
  ordering, or another workflow-specific state
- **THEN** that check remains visible in the owning LushText module or is
  represented through an explicit typed helper from `gtk-lush-tasks`
- **AND** older worker results cannot overwrite newer visible state

#### Scenario: Data safety is not weakened by extraction
- **WHEN** migrated persistence or save-adjacent workflows complete after
  newer state exists
- **THEN** durable-write, retry, dirty-state, and latest-state-wins semantics
  remain equivalent to the pre-migration behavior
- **AND** data-safety review findings are fixed before archive

### Requirement: Draft body lifetime is bounded across tabs
The system SHALL extend chunked draft snapshot responsiveness through the write stage so a multi-tab draft pass does not retain all completed buffer strings simultaneously. GTK snapshot work MUST yield in bounded chunks, and the next complete body MUST NOT be accumulated while the previous complete body is still retained for persistence.

#### Scenario: Snapshot and write stages apply backpressure
- **WHEN** a draft pass contains more than one large dirty editor
- **THEN** completion of one chunked snapshot hands that body to background persistence before the next complete body is retained
- **AND** GTK input and repaint remain schedulable between snapshot chunks and worker completions

#### Scenario: Pending autosave coalesces during pipeline work
- **WHEN** another first-dirty or periodic autosave request arrives while the bounded pipeline is active
- **THEN** the window records one pending rerun
- **AND** it does not start a conflicting snapshot or manifest writer

### Requirement: Editor-sized GTK work uses live bounds and freshness
The system SHALL base periodic editor workflows on current constant-time buffer size information rather than only load-time file metadata. Any chunked snapshot that can overlap edits MUST carry enough editor, path, lifetime, and buffer-generation state to reject mixed or stale content before background work begins.

#### Scenario: Small file grows before periodic work
- **WHEN** an editor loaded from a small file grows beyond a workflow's live threshold before a periodic callback runs
- **THEN** the callback uses the current buffer classification
- **AND** it does not perform a direct whole-buffer copy based only on the old file size

#### Scenario: Buffer changes during chunked capture
- **WHEN** the user edits while a chunked periodic snapshot is being assembled
- **THEN** the completion is rejected as stale
- **AND** the mixed snapshot is not persisted or analyzed as a coherent document state

### Requirement: Search selection prefill is bounded before copy
The system SHALL prefill in-editor Find and Replace queries only from non-empty selections of at most 1,024 characters. The selection length MUST be checked before materializing its text, and an oversized selection MUST leave the current query unchanged while the search surface remains usable.

#### Scenario: Short selection prefills Find
- **WHEN** the user opens Find with a non-empty selection of at most 1,024 characters
- **THEN** the search entry is populated with that exact selection
- **AND** the query is focused and selected for editing

#### Scenario: Large selection does not allocate query text
- **WHEN** the user opens Find or Replace with a selection longer than 1,024 characters
- **THEN** the editor does not copy that selection into an owned query string
- **AND** the existing query remains unchanged and focused

#### Scenario: Unicode selection uses character count
- **WHEN** the selection contains multibyte Unicode characters
- **THEN** prefill eligibility is determined by character count rather than raw UTF-8 byte count
- **AND** an accepted selection is copied without splitting a character

### Requirement: Byte-compatible scans use established search primitives
The system SHALL use the existing `memchr` dependency for complete CR/LF candidate scanning in line-ending detection while preserving exact LF, CRLF, CR, mixed, and empty-input semantics. Optimized byte searches MUST remain in GTK-free service code and MUST be covered against the prior scalar behavior.

#### Scenario: Mixed line endings preserve counts
- **WHEN** decoded text contains LF, CRLF, and lone CR endings
- **THEN** the optimized scan counts each logical ending exactly once
- **AND** detection and suggested save style match the established policy

#### Scenario: CRLF does not count as lone LF
- **WHEN** a carriage return is immediately followed by a line feed
- **THEN** the pair contributes one CRLF ending
- **AND** its line feed is not counted again as LF

### Requirement: Unchanged Markdown allocation avoids embed traversal
The system SHALL cache the last processed effective Markdown text-column width together with the rendered-embed generation. A code-block width refresh MUST avoid traversing embedded widgets when both values are unchanged, while new embeds or a changed valid width MUST still receive the full width update.

#### Scenario: Repeated unchanged allocation settles cheaply
- **WHEN** GTK requests several deferred code-block refreshes with the same effective text-column width and unchanged embeds
- **THEN** only the first refresh traverses the rendered embeds
- **AND** later passes still complete readiness callbacks

#### Scenario: New code block at the same width
- **WHEN** Markdown rerenders new embedded code blocks without changing the text-column width
- **THEN** the changed embed generation forces width assignment for the new blocks
- **AND** the cache does not leave them at a narrow natural allocation

#### Scenario: Hidden preview reports zero width
- **WHEN** a preview is temporarily hidden or unallocated between valid presentations
- **THEN** the invalid width does not replace the last valid processed tuple
- **AND** the next valid allocation can repair every current code block

### Requirement: Focused responsiveness changes have regression evidence
The project SHALL add unit, property, GTK widget, visual-geometry, and performance coverage for live size classification, stale chunked capture, selection prefill boundaries, line-ending scan equivalence, and Markdown width-cache invalidation.

#### Scenario: Performance tests detect restored full scans
- **WHEN** responsiveness benchmarks run on large line-ending input and many embedded code blocks
- **THEN** they exercise the optimized candidate scan and unchanged-width fast path
- **AND** a regression to repeated scalar/full-embed scanning is observable

### Requirement: Chunked buffer snapshot positions remain valid across yields
The system MUST NOT retain `GtkTextIter` positions across main-loop turns in which the source buffer can change. Chunked snapshots SHALL use GTK-stable position ownership, SHALL detect source-buffer mutation, and SHALL remove every temporary mark, signal handler, timer source, and callback on each terminal outcome.

#### Scenario: Buffer changes between snapshot slices
- **WHEN** an insertion or deletion changes the source buffer after one chunk and before the next scheduled slice
- **THEN** the snapshot terminates as cancelled before reusing an invalid position
- **AND** no mixed-generation or partial text reaches its consumer

#### Scenario: Snapshot completes without mutation
- **WHEN** every chunk is captured from one unchanged buffer generation
- **THEN** the consumer receives the complete text exactly once
- **AND** the snapshot's temporary marks, handlers, and sources are released

#### Scenario: Editor closes during capture
- **WHEN** the source widget or owning workflow is disposed while a snapshot is pending
- **THEN** later sources perform no consumer callback or GTK mutation
- **AND** temporary snapshot state does not retain the editor indefinitely

### Requirement: Snapshot consumers handle cancellation explicitly
Every chunked snapshot consumer SHALL choose an explicit cancellation policy appropriate to its workflow. Persistence workflows MUST remain retryable, stale analysis and previews MUST be discarded or superseded, and save workflows that freeze editing MUST restore interactivity on every terminal outcome.

#### Scenario: Draft capture is cancelled by an edit
- **WHEN** draft autosave snapshotting is cancelled because the source buffer changed
- **THEN** the editor remains draft-dirty
- **AND** one coalesced later autosave remains eligible to capture the latest generation

#### Scenario: Analysis or preview is superseded
- **WHEN** encoding analysis, note preview, or local-history preparation is cancelled by newer content
- **THEN** the stale consumer result is not rendered or applied
- **AND** the owning debounce or generation policy may schedule the latest request

#### Scenario: Save snapshot reaches a terminal outcome
- **WHEN** a save freezes the editor while chunked snapshotting runs
- **THEN** success proceeds with one coherent snapshot
- **AND** cancellation or failure restores the prior editable and cursor-visible state without clearing modified content
