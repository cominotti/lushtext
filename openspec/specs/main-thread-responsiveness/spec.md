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

### Requirement: Session restore bounds GTK work and load planning
The window SHALL restore persisted tabs through a generation-owned bounded coordinator. Startup session and draft loading MUST reserve bounded progress capacity that ordinary long-lived disposal owners cannot consume, and the complete measured eager-preload graph MUST fit that reservation before crossing to GTK. The coordinator MUST create no more than the configured number of pages in one GTK turn, MUST admit no more than the configured number of file-backed load-planning operations concurrently, and MUST retain remaining tabs as compact restore descriptors. Tab-derived palette, sidebar, recent-open, status, and related projections MUST remain deferred until one terminal accepted rebuild. Before those compact descriptors are available, close persistence MUST merge the bounded persisted descriptor set with current pages by stable identity rather than serialize only the not-yet-restored shell state.

#### Scenario: Long-lived undo ownership fills ordinary disposal capacity
- **WHEN** startup follows a crash-interrupted Replace All whose retained undo owner leaves too little ordinary disposal capacity for the conservative recovery preload reservation
- **THEN** session and draft loading reserves the independent bounded progress lane and continues
- **AND** the undo owner may remain retryable without starving startup recovery

#### Scenario: Window closes before startup descriptors are available
- **WHEN** close safety runs while startup recovery is waiting or loading before compact session descriptors have reached GTK
- **THEN** close reloads the bounded persisted descriptors and merges current pages through a linear stable-identity index
- **AND** the empty or partial shell state does not overwrite the unrestored session
- **AND** any newly edited untitled tab remains represented alongside the persisted descriptors and its flushed draft body

#### Scenario: Early-close session evidence cannot be preserved
- **WHEN** close reloads pending persisted session descriptors but recovery diagnostics report that quarantine or preservation failed
- **THEN** the original session evidence remains untouched in place
- **AND** session replacement is rejected, the close transaction is aborted, and the window becomes usable for a later retry

#### Scenario: Measured preload ownership exceeds the startup reservation
- **WHEN** eager draft bodies fit the body-content budget but their keys and collection capacity make the complete retained graph exceed progress-lane ownership
- **THEN** startup demotes enough eager bodies to compact lazy markers before shrinking the reservation
- **AND** if metadata alone would exceed the reservation, startup discards only preload hints and lets restored pages use the existing serialized lazy reader

#### Scenario: Large session requires several GTK turns
- **WHEN** a valid session contains more tabs than one restore-turn budget
- **THEN** the window creates pages over multiple scheduled GTK turns
- **AND** input and unrelated main-loop sources can run between restore batches
- **AND** tab order remains equal to the persisted session order

#### Scenario: File-backed restore exceeds planning capacity
- **WHEN** more file-backed tabs await restore than the configured in-flight planning limit
- **THEN** only the admitted subset owns load-planning work
- **AND** later descriptors remain compact until success, failure, cancellation, or editor teardown releases capacity

#### Scenario: Derived projections publish once
- **WHEN** session restoration creates file-backed and untitled tabs across several batches
- **THEN** per-tab open paths do not rebuild aggregate palette, sidebar, recent-open, or status projections
- **AND** one terminal current-generation rebuild publishes the complete accepted tab state

#### Scenario: Restore intent survives batching
- **WHEN** the persisted session selects a later tab or command-line activation targets an opened document while restore is in progress
- **THEN** active-tab priority, unavailable-file retry state, cursor and scroll restoration, and lazy draft markers preserve their existing semantics
- **AND** stale restore callbacks cannot override the accepted current selection

#### Scenario: Window closes during restore
- **WHEN** the window lifetime ends with queued descriptors or admitted plans remaining
- **THEN** the restore generation is cancelled and all permits and projection deferral state are released exactly once
- **AND** no later completion creates a page or publishes projections into the closed window

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

### Requirement: Large file installation yields in bounded GTK slices
Installing a decoded document above the synchronous installation threshold SHALL use bounded main-loop slices whose boundaries align to paragraph ends (just after a newline). GTK text layout validates whole paragraphs, so a slice that stops inside a paragraph forces later slices to re-lay-out everything already installed in that paragraph — quadratic total work that can stall recovery of single-line documents for minutes. A single paragraph longer than the slice byte budget SHALL be installed (and, during clearing, deleted) in one turn, because GTK cannot lay out a partial paragraph incrementally regardless of how the mutation is sliced. The editor SHALL remain non-editable and projections that would amplify each insertion SHALL remain suspended until the complete current generation is installed or the operation is cancelled.

#### Scenario: Large decoded text is installed
- **WHEN** an admitted load returns text above the synchronous installation threshold
- **THEN** GTK inserts the text in bounded paragraph-aligned slices with scheduling points between them
- **AND** syntax, minimap, history, draft, monitor, and modified-state finalization run only after the complete current generation is present

#### Scenario: Giant single-paragraph content avoids quadratic re-layout
- **WHEN** a recovered draft or loaded file contains one paragraph larger than the slice byte budget
- **THEN** that paragraph is installed in a single turn while any multi-paragraph remainder keeps bounded newline-aligned slices
- **AND** previously installed paragraphs are not re-validated by later slices

#### Scenario: Load is cancelled during installation
- **WHEN** the tab closes, reloads, or advances generation between installation slices
- **THEN** remaining slices stop without applying final loaded state
- **AND** admission ownership and retained decoded text are released

#### Scenario: Small load remains direct
- **WHEN** decoded text is below the synchronous installation threshold
- **THEN** the existing direct installation path may run in one GTK turn
- **AND** it observes the same generation and finalization rules as chunked installation

### Requirement: File reads enforce allocation limits at ingestion
File loading MUST enforce the supported byte limit while reading, not solely through earlier metadata. Growth or replacement after the metadata phase MUST terminate with a typed size or freshness outcome without allocating beyond bounded sentinel overhead.

#### Scenario: File grows after load planning
- **WHEN** a file becomes larger than the supported limit after metadata admission but before or during its read
- **THEN** ingestion stops at the configured limit plus bounded detection overhead
- **AND** no oversized decoded payload reaches GTK

#### Scenario: File identity changes after planning
- **WHEN** the stable file facts no longer match the admitted load plan
- **THEN** the stale read result is rejected or safely replanned
- **AND** it cannot replace a newer editor generation

### Requirement: Large transient UI state avoids long GTK ownership transitions
The system SHALL keep filtering, installation, cache rebuilding, and destruction work proportional to large retained text or collections out of one uninterrupted GTK main-loop turn. Accepted UI state MUST remain generation- and lifetime-checked, and stale large payloads MUST be released without making GTK perform their final allocator teardown.

#### Scenario: Large local-history preview is accepted
- **WHEN** a current local-history snapshot exceeds the synchronous preview-install threshold
- **THEN** its text is installed in bounded UTF-8-safe GTK slices with main-loop progress between slices
- **AND** Copy and Restore become available only after the current generation finishes installing

#### Scenario: Notes query has no early matches
- **WHEN** a Notes browser query must examine the entire admitted source before returning few or no matches
- **THEN** matching runs outside GTK with cooperative cancellation
- **AND** GTK receives only the bounded current result projection

#### Scenario: Broad workspace reconciliation finishes
- **WHEN** a child-store reconciliation accepts thousands of rows
- **THEN** terminal cache rebuilding performs linear work without repeated scans or index shifts for previously cached rows
- **AND** the GTK thread does not execute a quadratic terminal phase after bounded model splices

#### Scenario: Large palette index is replaced or rejected
- **WHEN** full or incremental command-palette indexing leaves an old or stale large index without another owner
- **THEN** the index's final destruction runs on the bounded worker lane
- **AND** generation comparison, replay ordering, and visible results remain owned by GTK

### Requirement: Plain-data disposal admission never blocks GTK
Document-sized plain-Rust payload retirement SHALL reserve non-blocking worker-destruction capacity before the value is transferred onto GTK, with explicit job-count and retained-byte bounds where payload weight is knowable. Callers with a conservative upper bound SHALL reserve before construction; data-dependent worker results SHALL remain worker-owned until their measured reservation succeeds. The reservation MUST remain attached through replaceable UI ownership so final destruction performs a guaranteed non-blocking worker handoff. Ordinary retained ownership and startup/Notes progress ownership MUST use independently bounded lanes so long-lived ordinary owners cannot permanently starve recovery or Notes browsing. When capacity is unavailable, each producer MUST retain at most one compact latest request and one retry or capacity-wakeup source, never an unreserved document-sized payload on GTK. GTK-owned objects MUST remain on GTK and retire through their existing bounded GTK paths.

#### Scenario: Disposal lane is saturated from GTK
- **WHEN** all disposal workers and reserved capacity are occupied and a GTK callback requests another document-sized plain-data operation
- **THEN** reservation fails immediately before that value crosses onto GTK
- **AND** only the compact latest request or worker-held result remains eligible for retry
- **AND** the GTK callback does not wait for a worker or blocking channel send

#### Scenario: Producer is superseded while disposal is full
- **WHEN** one producer already owns a compact pending request and a newer request replaces it before capacity returns
- **THEN** the producer retains at most one latest compact request and one wakeup source
- **AND** any previously published document-sized value keeps its reservation until accepted transfer or final worker destruction
- **AND** superseded data is not restored to current generation state

#### Scenario: Aggregate weighted pressure drains
- **WHEN** several preview, palette, notes, history, search, and undo producers submit weighted plain-data destruction concurrently
- **THEN** admitted job and byte high-water marks stay within the documented lane policy, with exclusive progress for an overweight job when applicable
- **AND** pre-admitted nested final destructors run off GTK
- **AND** every accepted owner or compact pending request is eventually released or cancelled on owner teardown

#### Scenario: Browse Notes starts while hidden palette sources fill ordinary capacity
- **WHEN** the installed file index and command-palette note source retain enough ordinary capacity that a fresh maximum Notes source would not fit
- **THEN** Browse Notes reserves its independently bounded progress lane and starts source construction
- **AND** hidden palette ownership cannot leave the dialog deferred indefinitely

#### Scenario: Browse Notes is activated repeatedly
- **WHEN** Browse Notes is activated again while its dialog is still alive or waiting for progress capacity
- **THEN** the existing dialog is re-presented instead of creating another source owner or wakeup
- **AND** at most one 64 MiB browser source reservation remains attributable to that window

#### Scenario: Live editor note metadata exceeds the deferred-request budget
- **WHEN** open editor paths or bookmark labels would make the pre-admission Notes snapshot exceed its retained-byte limit
- **THEN** later snapshot material is omitted before cloning it into the deferred request
- **AND** the request remains within the documented byte bound and carries typed truncation evidence into the published source

#### Scenario: Pure-drop workflows leave the shared completion lane
- **WHEN** a `Send` payload requires only final destruction and no GTK result handling
- **THEN** it uses the proven plain-disposal contract rather than holding a generic worker slot for a no-op GTK completion
- **AND** lifecycle-specific blocking destructors remain on their owning worker path

### Requirement: Document-sized GTK buffer replacement yields in bounded slices
Any workflow that clears or replaces document-sized editor content SHALL use one editor-owned bounded GTK mutation session above the synchronous threshold. The session MUST carry weak editor ownership, workflow-specific freshness identity, source ownership, projection suppression, and one typed terminal outcome. While a replacement is partial, the editor MUST remain non-editable and non-saveable, and modified, eviction, history, draft, cursor, monitor, and projection finalization MUST occur only after the complete current generation is installed or safely cancelled.

#### Scenario: Large clean editor is evicted
- **WHEN** memory policy accepts eviction of a clean reloadable editor whose buffer exceeds the synchronous replacement threshold
- **THEN** GTK clears the buffer in bounded main-loop slices
- **AND** the editor is marked evicted and its residency is released only after the current clear session completes

#### Scenario: Large recovery or history body is installed
- **WHEN** draft recovery, local-history restore, or local-history undo replaces a large buffer
- **THEN** GTK clears and inserts text through bounded slices with scheduling points between them
- **AND** no partial body becomes editable, saveable, or visible as a completed restore

#### Scenario: Save formatting rewrites a large live buffer
- **WHEN** save-time formatting produces document-sized text different from the live buffer
- **THEN** the accepted text is installed through the same bounded replacement contract
- **AND** save finalization cannot apply to a newer edit, path, save, or load generation

#### Scenario: Replacement becomes stale between slices
- **WHEN** the editor closes, changes workflow generation, or otherwise invalidates an active replacement
- **THEN** remaining slices stop and release their source and retained text exactly once
- **AND** the workflow reports a typed cancellation or failure without publishing successful terminal state

#### Scenario: Small replacement remains direct
- **WHEN** both the existing buffer and replacement text are below the calibrated synchronous threshold
- **THEN** the workflow MAY replace text in one GTK turn
- **AND** it observes the same freshness and terminal-finalization rules as a sliced replacement

### Requirement: Command-palette search is one-active and one-latest
The command palette SHALL run at most one background search and retain at most one latest superseding request as compact query state. New input MUST cancel or supersede obsolete work cooperatively, and only the current generation may update rows, searching state, accessibility state, or readiness.

#### Scenario: Rapid typing outpaces search completion
- **WHEN** several query generations arrive while one full-index search is active
- **THEN** intermediate pending requests are replaced by the latest query
- **AND** at most one active worker and one compact pending request are retained

#### Scenario: Active search observes cancellation
- **WHEN** a newer query supersedes an active search
- **THEN** candidate scoring stops at a bounded cancellation checkpoint
- **AND** the latest request starts after active ownership is released

#### Scenario: Stale completion reaches GTK
- **WHEN** an obsolete search completes after a newer generation exists
- **THEN** it changes neither visible results nor searching/accessibility state
- **AND** readiness remains pending only for current active or queued work

#### Scenario: Palette closes during search
- **WHEN** the palette closes with active or pending query work
- **THEN** cancellation releases retained search state
- **AND** no later completion reopens or mutates the closed surface

### Requirement: Markdown preview rendering is bounded across planning and projection
The system SHALL render Markdown preview through a generation-owned GTK-free plan and bounded GTK projection slices. Automatic rendering MUST enforce deterministic source, event/node, embed, and per-slice work budgets, and MUST expose a limited or paused terminal state when an input exceeds those budgets.

#### Scenario: Dense Markdown stays below the byte pause threshold
- **WHEN** a document is small enough for automatic preview but expands into more render events or GTK nodes than the configured render budget
- **THEN** planning terminates at the deterministic budget with an explicit limited preview state
- **AND** GTK does not build the unbounded remainder in one callback

#### Scenario: Accepted plan needs many GTK nodes
- **WHEN** a current render plan contains more nodes than one projection slice permits
- **THEN** GTK applies it over bounded main-loop turns
- **AND** input, repaint, and other completions can run between slices

#### Scenario: Preview generation changes during projection
- **WHEN** the document, preview mode, or page lifetime changes while a plan or projection session is active
- **THEN** all later work owned by the stale generation is discarded
- **AND** it cannot insert widgets, tags, placeholders, or terminal state into the newer preview

### Requirement: Markdown image work has bounded generation ownership
Local-image preview work SHALL be admitted by current render generation under explicit count and byte limits. Excess, oversized, stale, or failed image work MUST resolve to an accessible placeholder or stale discard without allowing queued image payloads to grow with embed count.

#### Scenario: Preview contains many local images
- **WHEN** one accepted render contains more local-image descriptors than its image admission budget
- **THEN** only the bounded admitted subset may own active or queued image payloads
- **AND** remaining embeds render deterministic accessible placeholders

#### Scenario: Stale image completes after rerender
- **WHEN** an image decode or load completes after a newer preview generation owns the surface
- **THEN** the completion releases its payload ownership and is ignored
- **AND** the newer preview remains unchanged

### Requirement: Large search projection transitions yield to GTK
Search-result replacement, retirement, cache cleanup, and Replace Preview handoff SHALL avoid whole-result cloning or teardown in one GTK turn. Retired generations MUST be detached immediately and disposed in bounded slices that cannot mutate a newer generation.

#### Scenario: Ten thousand results are replaced
- **WHEN** a new search generation supersedes a result model at the configured maximum result count
- **THEN** the old generation is no longer current immediately
- **AND** its rows and auxiliary caches are retired over bounded main-loop turns

#### Scenario: Replace Preview starts from accepted results
- **WHEN** the user enters Replace Preview for a large accepted result generation
- **THEN** worker construction receives a shared immutable result snapshot without cloning every match on GTK
- **AND** preview identity and checked-row semantics still refer to that exact generation

#### Scenario: New results arrive during old-generation disposal
- **WHEN** a disposal slice runs after a newer generation has populated visible results or caches
- **THEN** the slice removes only state owned by the retired generation
- **AND** current rows, match identities, and readiness remain intact

### Requirement: Document-sized rejected payloads retire outside GTK dispatch
When Replace Preview or Markdown work becomes stale, rejected, superseded, unchecked, or otherwise unprojected, the system SHALL detach it from current generation state immediately. The final destruction of document-sized plain-Rust outcomes, plans, queued sources, and projection tails MUST run away from GTK dispatch through bounded worker ownership. GTK-owned buffers, widgets, models, tags, and links MUST remain on the GTK thread and MUST retire through bounded main-loop slices that cannot mutate the current generation.

#### Scenario: Stale Replace Preview worker returns a near-limit outcome
- **WHEN** a preview outcome becomes stale before its worker completion reaches GTK
- **THEN** GTK rejects it without synchronously destroying its row and text payload
- **AND** bounded worker retirement releases the final plain-data owner

#### Scenario: Markdown projection is superseded with batches remaining
- **WHEN** a newer render generation invalidates a plan with unprojected event batches
- **THEN** the idle callback stops before applying another stale batch
- **AND** the remaining plain-Rust batch tail is destroyed away from GTK dispatch

#### Scenario: Detached Markdown GTK state drains
- **WHEN** an old rendered buffer, embeds, or link targets are detached from the visible preview
- **THEN** they remain GTK-owned and are released within the configured per-turn character and item budgets
- **AND** no retirement slice changes the current rendered generation

### Requirement: Markdown detached generations apply explicit backpressure
Ordinary Markdown rerendering SHALL retain at most two detached GTK render generations and at most one latest pending render request behind retirement. When the detached-generation cap is reached, the system MUST replace older pending requests with the latest generation, MUST retire the superseded pending plain data away from GTK, and MUST defer new ordinary detachment and projection until retirement falls below the cap. Preview readiness MUST remain pending while current planning, projection, admitted image work, detached retirement, or the latest pending render request exists.

#### Scenario: Rapid edits outpace Markdown retirement
- **WHEN** repeated edits request more renders than bounded GTK retirement can drain
- **THEN** detached render ownership never exceeds two ordinary generations
- **AND** at most the latest pending render request is retained
- **AND** the newest current request resumes after retirement creates capacity

#### Scenario: Pending render is superseded repeatedly
- **WHEN** several newer render requests arrive while detached state is at the cap
- **THEN** intermediate pending sources are replaced rather than queued
- **AND** their final plain-data destruction does not occur in the GTK callback

#### Scenario: Preview closes under retirement pressure
- **WHEN** the preview closes with planning, pending, projection, image, and detached retirement work present
- **THEN** every obsolete generation is invalidated and releases ownership through its applicable bounded path
- **AND** no later completion reopens or mutates the closed preview

### Requirement: Workspace-search event turns count every received event
The workspace-search GTK consumer SHALL receive and dispatch at most 250 channel events per scheduled turn. Every successfully received `Match`, `Progress`, `ResultCap`, `Error`, and `Done` event MUST consume one unit before variant-specific handling; channel disconnection MAY terminate the turn without consuming an event unit.

#### Scenario: Progress burst contains no matches
- **WHEN** more than 250 progress events are ready before one consumer turn
- **THEN** that turn receives at most 250 events
- **AND** remaining events wait for a later scheduled turn

#### Scenario: Mixed event burst reaches the cap
- **WHEN** match, progress, result-cap, and error events are interleaved in a ready channel
- **THEN** their combined received count is at most 250 for the turn
- **AND** visible result and diagnostic semantics remain unchanged

#### Scenario: Terminal event is received within budget
- **WHEN** `Done` is the next event before the turn reaches 250 received events
- **THEN** it consumes one event unit and terminates the active search normally
- **AND** the latest pending search may proceed through the existing flight contract

### Requirement: Sliced buffer mutation is reentrancy-safe before GTK calls
For every non-empty sliced delete or insert, the editor-owned replacement session SHALL establish mutation-started state before invoking a signal-emitting GTK mutation API. The session MUST NOT remain mutably borrowed across that GTK call, and continuation MUST revalidate session identity afterward. Synchronous reentrant cancellation or supersession MUST clean partial state exactly once, publish no successful terminal state for the invalidated generation, and leave only the accepted current content editable and saveable.

#### Scenario: First changed signal supersedes replacement synchronously
- **WHEN** the first non-empty sliced mutation emits `changed` and a synchronous handler starts a newer replacement
- **THEN** the older session is already marked as having mutated
- **AND** its cancellation path performs exact partial cleanup without a borrow conflict
- **AND** final buffer content equals only the newer accepted source

#### Scenario: Mutation returns after reentrant cancellation
- **WHEN** control returns from a GTK delete or insert after the session was cancelled reentrantly
- **THEN** the stale continuation does not record progress or schedule another slice
- **AND** projection suppression, editability, saveability, retained text, and terminal ownership are released exactly once

### Requirement: Search retirement budgets are release-invariant
Every destructive step performed by a retired workspace-search generation SHALL consume its per-turn retirement budget in debug and release builds. Correctness-affecting mutations MUST occur outside debug-only assertions, one retirement turn MUST release no more than 250 owned items, and unfinished state MUST remain scheduled until it reaches a terminal empty state without touching the current generation.

#### Scenario: One release-profile turn retires a large generation
- **WHEN** a retired generation owns more than 250 unique file rows, file groups, match rows, cached rows, and shared accepted-result references
- **THEN** one retirement turn removes at most 250 actual owned items in a release build
- **AND** the remaining retired state stays pending for a later turn

#### Scenario: Repeated retirement turns finish
- **WHEN** bounded retirement turns continue after a large generation is detached
- **THEN** each turn charges every successful removal before proceeding to another item
- **AND** the retired generation eventually becomes empty without changing current rows, identities, readiness, or detached-generation backpressure

#### Scenario: Debug assertions are disabled
- **WHEN** the same retirement workflow is compiled without debug assertions
- **THEN** its actual container and reference-count deltas remain identical to debug semantics
- **AND** no required mutation depends on an assertion expression being evaluated

### Requirement: Whole-buffer snapshot accumulation has bounded GTK work
Whole-buffer snapshots SHALL accumulate fixed-size chunks without repeatedly reallocating or copying the already captured document on GTK. Final coalescing, transformation, and destruction of document-sized plain data MUST occur off GTK under the workflow's existing admission and disposal ownership, while only GTK-owned buffer access remains on the main thread.

#### Scenario: A large snapshot needs many chunks
- **WHEN** a save, encoding, session, or other admitted workflow snapshots a supported large editor buffer
- **THEN** each GTK turn extracts only its configured character slice into a newly owned bounded chunk
- **AND** the chunk-header collection reserves from the initial O(1) character count instead of repeatedly growing on GTK
- **AND** no GTK turn copies or reallocates an amount proportional to all text accumulated so far

#### Scenario: Snapshot capture finishes
- **WHEN** the last chunk of an accepted large snapshot is captured
- **THEN** GTK transfers guarded chunk ownership to the admitted worker
- **AND** final coalescing or transformation does not construct or finally destroy the document-sized result in GTK dispatch
- **AND** save admission continues to span capture, coalescing, formatting, durable write, terminal acceptance, and exact-once permit release

#### Scenario: Snapshot becomes stale or is rejected
- **WHEN** generation, page lifetime, cancellation, overflow, or downstream admission invalidates a partially or fully captured snapshot
- **THEN** the snapshot stops before another stale GTK slice
- **AND** its remaining plain chunks and any coalesced payload retire through bounded off-GTK ownership

#### Scenario: Small snapshot uses the direct path
- **WHEN** the buffer fits the existing small-payload threshold
- **THEN** the implementation MAY use the direct snapshot path
- **AND** it preserves the same freshness, cancellation, and save-permit semantics as the chunked path

### Requirement: Workspace-search requests share immutable scope snapshots
Each workspace-search generation SHALL own one immutable shared folder snapshot. Request construction, active-plus-latest replacement, and periodic polling MUST clone only constant-size shared ownership rather than deep-cloning every PathBuf, while a scope change MUST create and supersede with a new generation.

#### Scenario: Polling a large workspace scope
- **WHEN** an active search over many workspace folders is polled repeatedly
- **THEN** each poll reuses the generation's immutable shared folder snapshot
- **AND** polling does not allocate or clone the full folder vector every 50 milliseconds

#### Scenario: Workspace scope changes during search
- **WHEN** the selected workspace scope changes while a search generation is active
- **THEN** a new generation receives a new immutable snapshot
- **AND** the prior generation cannot observe the new scope or publish stale results into it

### Requirement: Workspace search bounds traversal identity ownership
Each workspace-search generation SHALL normalize its immutable ordered folder scope into a bounded traversal plan before scanning. A single effective traversal root and multiple roots proven disjoint MUST NOT retain one visited-file identity per scanned file. Exact duplicate and covered roots MUST be scanned only once while result attribution preserves the original configured folder precedence. Any unresolved alias fallback that still requires per-file identity tracking MUST enforce explicit entry and conservative path-byte limits and MUST terminate with typed incomplete-search feedback before either limit is exceeded.

#### Scenario: Single-root no-match search visits a huge tree
- **WHEN** one workspace folder contains more files than the result cap but none match the query
- **THEN** search retained identity state remains independent of the number of visited files
- **AND** cancellation, progress, and normal completion preserve their existing semantics

#### Scenario: Overlapping roots cover the same file
- **WHEN** ordered workspace folders include duplicates, descendants, or canonical aliases that cover the same file
- **THEN** the normalized traversal plan avoids duplicate scanning where coverage is resolved
- **AND** an admitted result is attributed according to the first configured folder that owned it before normalization

#### Scenario: Alias identity cannot be resolved completely
- **WHEN** unavailable or uncanonicalizable roots require fallback file-identity tracking to prevent duplicate results
- **THEN** the fallback ledger retains no more than its documented entry and path-byte budgets
- **AND** reaching either budget stops with explicit incomplete-search feedback rather than silently publishing a complete result

### Requirement: Decoded document and recovery bodies retain off-GTK disposal ownership
Every document-sized decoded file body and recovered draft body SHALL reserve bounded plain-data disposal capacity before the body crosses from worker or aggregate preload ownership onto GTK. The reservation MUST remain attached through weak-owner checks, generation validation, direct or sliced buffer installation, cancellation, teardown, and eligible accepted-baseline transfer. A stale, rejected, superseded, ineligible, or otherwise terminal body MUST perform its final plain-Rust destruction on the admitted disposal worker, and document-sized bodies MUST NOT use the statically-small unreserved sentinel path.

#### Scenario: File-load completion loses its editor
- **WHEN** a supported large decoded file body reaches main-loop completion after the editor weak reference can no longer be upgraded
- **THEN** the guarded result is rejected without destroying the body in GTK dispatch
- **AND** its final destructor runs through the pre-admitted disposal worker

#### Scenario: Sliced file installation is cancelled
- **WHEN** a newer load generation or editor teardown cancels a large installation between GTK slices
- **THEN** the installer releases transient load admission exactly once
- **AND** the remaining guarded decoded body is finally destroyed off GTK

#### Scenario: Draft body becomes stale before replacement
- **WHEN** an eager or lazy recovered draft body loses ticket freshness before or during bounded replacement
- **THEN** no partial or terminal restored state is published
- **AND** the body's guard survives until worker-side final destruction

#### Scenario: Accepted body seeds a clean baseline
- **WHEN** file-load or draft policy retains the accepted installed body as an eligible local-history baseline
- **THEN** ownership is transferred without a full-body clone or unguarded unwrap
- **AND** later baseline replacement or editor teardown still performs final plain-data destruction off GTK

### Requirement: Large-file decoding and analysis are cooperatively cancellable
After bounded file ingestion, encoding detection, decoding, line-ending classification, and file-health analysis for a large admitted document SHALL observe cancellation at explicit bounded work boundaries wherever the underlying operation supports incremental progress. Once cancellation is observed, the worker MUST stop subsequent analysis, publish only the existing typed cancelled terminal, and release its transient ownership exactly once. A successful uncancelled load MUST preserve exact decoded content, encoding metadata, line-ending classification, and file-health findings.

#### Scenario: Cancellation arrives during incremental decoding
- **WHEN** a large admitted document is cancelled while byte classification or decoding is in progress
- **THEN** the worker stops at a bounded cancellation checkpoint without starting later exhaustive analysis
- **AND** no decoded result is installed for the cancelled generation

#### Scenario: Cancellation arrives during health analysis
- **WHEN** decoding completes but cancellation occurs while line-ending or file-health evidence is being accumulated
- **THEN** the analysis terminates without publishing a partial health result
- **AND** the load's transient permit and retained bytes are released exactly once

#### Scenario: Supported encodings cross chunk boundaries
- **WHEN** UTF-8, BOM or BOM-less UTF-16, or a supported fallback encoding contains multibyte characters across processing chunk boundaries
- **THEN** the uncancelled result exactly matches the reference decoding and metadata
- **AND** cancellation checks do not split, replace, or lose valid scalar content

#### Scenario: Small file uses a direct path
- **WHEN** an admitted document is below the calibrated incremental-processing threshold
- **THEN** it may use a direct decode and analysis path
- **AND** it preserves the same pre/post cancellation, exact-result, and terminal ownership semantics

#### Scenario: A codec operation cannot yield internally
- **WHEN** one existing library operation cannot expose incremental progress
- **THEN** cancellation is checked immediately before and after that operation
- **AND** the implementation does not claim an absolute cancellation-latency guarantee for that interval
