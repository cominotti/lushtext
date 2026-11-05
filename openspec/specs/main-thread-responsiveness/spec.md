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
minimap marker scans, lossy encoding analysis, GtkSourceView syntax analysis of
long paragraphs, and feature work triggered by edits that grow a paragraph.
Each workflow MUST do one of the following:
- use a bounded/chunked GTK snapshot plus worker processing;
- enforce a documented large-input threshold with a user-visible paused,
  limited or reduced state;
- reuse already available load-time analysis data.

Features whose GTK cost grows with paragraph length MUST be gated by the
document scale policy before the paragraph reaches them, at load and in the
same signal emission as an edit that makes a paragraph exceed the feature's
threshold.

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

#### Scenario: Syntax analysis never meets a giant paragraph
- **WHEN** a document whose longest paragraph exceeds the syntax paragraph
  threshold is loaded, or an edit creates such a paragraph
- **THEN** the language is cleared and highlighting is off before any GTK idle
  can run syntax analysis on that paragraph
- **AND** the user sees the reduced-features notice instead of a multi-second
  stall

#### Scenario: Edit-time growth is evaluated within a bounded handler
- **WHEN** an insertion or deletion changes paragraph lengths
- **THEN** the profile update reads only the inserted run and a constant number
  of line lengths whose cost does not grow with line length
- **AND** it never copies or scans the document

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
Installing a decoded document above the synchronous installation threshold SHALL use bounded main-loop slices. Outside a protected paragraph's hidden tail, slice boundaries SHALL align to paragraph ends, as GTK defines them: just after `\n`, `\r`, `\r\n` (never between its CR and LF) or U+2029. The paragraph-boundary function MUST be the single shared one whose agreement with `pango_find_paragraph_boundary` is tested. GTK text layout validates whole paragraphs, so a slice that stops inside a *visible* paragraph forces later slices to re-lay-out everything already installed in that paragraph. That is quadratic total work and can stall recovery of single-line documents for minutes. A visible paragraph longer than the slice byte budget SHALL therefore be installed (and, during clearing, deleted) in one turn. The protected-view policy bounds such a paragraph by its visible-length limit. The editor SHALL remain non-editable, and projections that would amplify each insertion SHALL remain suspended, until the complete current generation is installed or the operation is cancelled.

In a document under Protected View, a paragraph whose tail is folded SHALL be installed as follows:
- its hidden tail in bounded slices of at most the protected tail slice budget, each inserted with the protected tag applied in the same `insert_with_tags` call, so tail text is never untagged across a main-loop dispatch and never laid out untagged, and every LushText `insert-text` handler treats the slice as installation, not as an edit;
- its delimiter before its tail, so the paragraph is never the buffer's last line while the tail grows (ledger A79), and its visible head, up to the fold point, in one turn after the last tail slice, so no tail slice re-lays out the head; when the tail fits one slice, the head immediately before it in the same turn, with no boundary.

A slice MAY end inside such a paragraph only at or after its fold point. A protected body whose hidden text could exceed one tail slice SHALL take this sliced path at any size, and its slices SHALL wait at the paced idle priority, below the view's line validation. Each installation or clearing dispatch of a protected body SHALL stay within the settle budget B_settle on the reference responsiveness lane.

The synchronous installation threshold SHALL be the measured largest size whose insert fits half of `B_SETTLE_US`. A document whose densest validation batch holds more than `V_SLICE` visible characters, a measured constant, SHALL be sliced even when its byte size is below the threshold. Each of its slices SHALL hold at most `V_SLICE` visible characters, and its next slice SHALL NOT be dispatched before the frame that lays out the previous one. The validation that a slice's new lines cause then fits half of `B_SETTLE_US` in either wrap mode. The slice plan is one pure function, shared by load and whole-buffer replacement.

#### Scenario: Large decoded text is installed
- **WHEN** an admitted load returns text above the synchronous installation threshold
- **THEN** GTK inserts the text in bounded slices with scheduling points between them, paragraph-aligned everywhere except inside protected hidden tails
- **AND** syntax, minimap, history, draft, monitor, and modified-state finalization run only after the complete current generation is present

#### Scenario: Giant single-paragraph content avoids quadratic re-layout
- **WHEN** a recovered draft or loaded file that is not under Protected View contains one visible paragraph larger than the slice byte budget
- **THEN** that paragraph is installed in a single turn while any multi-paragraph remainder keeps bounded paragraph-aligned slices
- **AND** previously installed paragraphs are not re-validated by later slices

#### Scenario: Giant protected paragraph is installed in hidden-tail slices
- **WHEN** a document under Protected View contains a paragraph whose hidden tail is larger than the protected tail slice budget
- **THEN** the tail is inserted in tagged slices of at most the protected tail slice budget, each in its own main-loop dispatch, after the paragraph's delimiter is in, and the head is inserted once, untagged, after the last of them
- **AND** the paragraph never shows more than its head at any dispatch boundary, and no dispatch of the install exceeds B_settle on the reference lane at 100, 250, or 500 MB with the accessibility bridge off or on

#### Scenario: CR-only and U+2029-separated documents are sliced
- **WHEN** an admitted document above the synchronous threshold separates its paragraphs only with `\r`, or only with U+2029
- **THEN** installation uses multiple bounded slices, each ending just after a delimiter
- **AND** the installed buffer is byte-identical to the decoded text

#### Scenario: CRLF pairs survive slicing
- **WHEN** a slice's byte budget ends between the `\r` and `\n` of a CRLF pair
- **THEN** the slice ends after the `\n`, so the pair stays one GTK paragraph delimiter

#### Scenario: Load is cancelled during installation
- **WHEN** the tab closes, reloads, or advances generation between installation slices, including between two hidden-tail slices of one paragraph
- **THEN** remaining slices stop without applying final loaded state, and partial content is cleared through the bounded clear plan
- **AND** admission ownership and retained decoded text are released

#### Scenario: Small load remains direct
- **WHEN** decoded text is below the synchronous installation threshold, its densest validation batch holds at most `V_SLICE` visible characters, and it hides at most one tail slice of text
- **THEN** the existing direct installation path may run in one GTK turn
- **AND** it observes the same generation and finalization rules as chunked installation

#### Scenario: Dense short-line text is paced by visible characters
- **WHEN** an admitted load returns text below the byte threshold whose densest validation batch holds more than `V_SLICE` visible characters
- **THEN** it is installed in paragraph-aligned slices of at most `V_SLICE` visible characters, each dispatched only after the frame that lays out the previous one
- **AND** each slice's validation runs in that frame's layout, as A37 requires, within half of `B_SETTLE_US`

#### Scenario: A short-line document with many characters stays direct
- **WHEN** an admitted load below the byte threshold has more than `V_SLICE` visible characters in total but no validation batch over `V_SLICE`
- **THEN** it is installed directly in one GTK turn

#### Scenario: Post-open formatting does not invalidate every line
- **WHEN** EditorConfig resolves a tab width for a document being opened or reloaded, or the end-of-document overscroll of a dense document is updated after a viewport resize
- **THEN** the tab width is applied after any clear of old text and before the first insert
- **AND** no default-style property of a dense document's view changes in a dispatch after the install that is not paced or recorded as a toolkit-floor exception

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
The system SHALL render Markdown preview through a generation-owned GTK-free plan and bounded GTK projection slices. Automatic rendering MUST enforce deterministic source, event/node, embed, and per-slice work budgets, and MUST expose a limited or paused terminal state when an input exceeds a global budget.

A block larger than one projection slice MUST NOT stop planning. Planning SHALL cut a projection batch only where no inline state is open, MAY cut inside a block at such a checkpoint, and MUST emit an accessible omission marker and continue planning when the smallest unit delimited by those checkpoints still exceeds a per-slice budget. An omission MUST be scoped to that overflowing unit: a unit inside an open container MUST be replaced within that container while its sibling units still render, and only a top-level block with no interior checkpoint MAY be replaced whole. Every emitted batch MUST stay within the per-slice event and byte budgets. Cross-turn projection state SHALL be bounded by structural nesting depth plus at most one in-flight embedded-block buffer charged against a named retention ceiling for that block's kind, and MUST NOT grow with document size. A retention ceiling MUST NOT change which presentation a projection-side widget budget selects for an embedded block, so each ceiling MUST be expressed in the same unit as the widget budget governing that kind of block. Global source, event, retained-byte, embed-descriptor, structural-depth, and inline-footnote budgets MUST remain terminal stop conditions, and an omitted unit's parsed work MUST still be charged to them.

#### Scenario: Dense Markdown exceeds the global event budget
- **WHEN** a document is small enough for automatic preview but expands into more render events or GTK nodes than the global render-event budget
- **THEN** planning terminates at that global budget with an explicit limited preview state
- **AND** GTK does not build the unbounded remainder in one callback

#### Scenario: Accepted plan needs many GTK nodes
- **WHEN** a current render plan contains more nodes than one projection slice permits
- **THEN** GTK applies it over bounded main-loop turns
- **AND** input, repaint, and other completions can run between slices

#### Scenario: One block exceeds a projection slice
- **WHEN** a single top-level block needs more events or bytes than one projection slice permits
- **THEN** planning continues past that block instead of discarding the rest of the document
- **AND** every following block is planned and projected

#### Scenario: An oversized block has an inline-safe checkpoint
- **WHEN** an oversized block can be cut at a point where only block containers are open
- **THEN** the block is projected across several bounded turns
- **AND** each emitted batch stays within the per-slice event and byte budgets
- **AND** no batch boundary splits an open inline span

#### Scenario: One unit inside an open container overflows
- **WHEN** a single row, list item, code-block text run, quoted paragraph, or definition body exceeds a per-slice budget and has no interior checkpoint
- **THEN** only that unit is replaced by one accessible omission marker inside the still-open container
- **AND** the container's other units are still rendered in source order
- **AND** planning continues after the container closes

#### Scenario: A top-level block has no inline-safe checkpoint
- **WHEN** a top-level block such as one very dense paragraph or heading exceeds a per-slice budget and contains no point where only block containers are open
- **THEN** that block is replaced by one accessible omission marker
- **AND** planning continues with the following block
- **AND** the terminal state reports a complete preview with a count of omissions rather than a stopped preview

#### Scenario: An open embedded block crosses its retention ceiling
- **WHEN** an open embedded block would exceed the retention ceiling for its own kind — retained text bytes for a code block, or cell count for a table
- **THEN** planning stops retaining that block's remaining content at the crossing point and records the remainder as unretained for that block
- **AND** that record carries the unretained source byte and cell counts so projection-side widget budgets still evaluate the block's true total size
- **AND** it is treated as internal accounting rather than a user-visible omission: no marker is rendered for it, it does not count toward the reported omission total, and a plan whose only such records are these publishes the complete terminal
- **AND** batches already emitted for that block remain valid and unchanged
- **AND** planning continues after the block closes

#### Scenario: Continuation state crosses projection turns
- **WHEN** a projection turn ends inside a block that continues in a later turn
- **THEN** the retained continuation holds only open-container descriptors, scalar flow state, and at most one charged in-flight embedded-block buffer
- **AND** the following turn validates the continuation it holds against the continuation the next batch expects
- **AND** a mismatch resolves to an explicit terminal state instead of corrupted rendered content

#### Scenario: Preview generation changes during projection
- **WHEN** the document, preview mode, or page lifetime changes while a plan or projection session is active
- **THEN** all later work owned by the stale generation is discarded
- **AND** it cannot insert widgets, tags, placeholders, or terminal state into the newer preview
- **AND** any retained continuation buffer is released without freeing document-sized text on the GTK thread

#### Scenario: A global budget is exceeded
- **WHEN** source bytes, retained events, retained bytes, embed descriptors, structural depth, or inline-footnote expansion exceeds its global budget
- **THEN** planning stops at that budget with the existing limited or paused terminal state
- **AND** the reported reason names that global budget rather than a per-unit omission

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

### Requirement: Window construction and the dispatches before the first frame fit the settle budget
The system SHALL keep each of the following within the settle-dispatch budget B_settle recorded by
the responsiveness policy constants (working value 16 ms), each one separately:
- LushText-owned application startup work;
- the work an `open` or `activate` handler performs before the main loop runs;
- every GTK main-loop dispatch from that handler until the window's composition is complete,
  including each window's first presented frame and every frame dispatch that shows newly revealed
  content.

The budget SHALL hold on the reference lane (private `mutter --headless`, `GSK_RENDERER=cairo`, at
most 16 cores, release build) for a fresh profile and a steady-state profile, under both the
keyfile and the dconf GSettings backends, and for a steady-state profile with a restored workspace
folder and the properties pane shown, in runs that meet the reference precondition: the 1-minute
load average below 4 for 30 s before the run, no process at 80 % CPU or more, and no other agent's
build, test, or proof process running. A run that loses the precondition SHALL be discarded.

The `open` and `activate` handlers SHALL perform no window construction before the main loop runs:
they SHALL hold the application, record the request, and schedule the first composition stage. A
request that arrives before the window exists SHALL merge into the pending one rather than create
a second window. The hold SHALL be released once the window belongs to the application, or when
the application quits first.

The window SHALL be composed in a fixed sequence of dispatches, each within B_settle:
1. warming the one-time text and icon state;
2. constructing the whole window, with the content the first frame does not need invisible, and
   its breakpoints installed;
3. realizing and presenting it, in one dispatch, so that no frame of the window runs while it is
   realized but not mapped;
4. after its first frame has painted, one dispatch that reveals the content page, the workspace
   sidebar, and the document-properties panel, in that order, and admits the activation's file
   opens in bounded turns.

The window's first frame SHALL be a frame painted while the window is mapped. Every place that
reports or gates on the first frame SHALL use that frame: the construction evidence, the settings
writer's first-frame gate, and the measured lane. Composition SHALL complete on the first frame
painted after the reveal dispatch. A concealed revealer's child SHALL be invisible while it is
concealed.

When an action, accelerator, activation file open, session-restore completion, or automation call
needs a staged surface visible, focused, or allocated, the system SHALL first complete the stages
that surface needs, in order and synchronously; the forced dispatch and the frame that follows it
SHALL each fit B_settle. Reading or projecting onto a staged surface SHALL NOT force a stage. The
operation SHALL then behave exactly as it does on a fully composed window. Accelerators and actions
SHALL NOT be dropped during composition. Activation file opens SHALL be admitted in bounded turns.

The content fill-in is the time from the window's first frame to the later of composition complete
and the first frame painted after the activation's file opens have run. On every reference run it
SHALL be at most 40 ms, the maintainer's accepted fill-in, which lies within five reference frames
(5 × the 120 Hz reference frame, 41,665 µs), and its median over a verdict run SHALL be at most
three reference frames (24,999 µs). Neither is a new budget constant, and neither relaxes
B_settle. Every dispatch until the later of composition complete and that activation frame SHALL
fit B_settle. The window's first frame SHALL be painted no later than the first frame of the
staged composition it replaces, measured the same way.

Toolkit-owned startup (`gtk_init`) SHALL be measured and reported separately, and is not counted
against this budget. The one-time cost of the first toplevel's realize SHALL be counted against
the budget of the dispatch that incurs it. If that cost alone exceeds B_settle on the reference
lane, the measured lane SHALL report the dispatch as over budget, and the programme record SHALL
record it against the ledger axiom that pins it.

Software rendering on the headless lane overstates paint and present relative to a GPU, so a pass
there is conservative for those phases; the gate SHALL stay the headless lane and SHALL NOT run
against the live desktop.

#### Scenario: Steady-state launch constructs the window within budget
- **WHEN** the release build opens any file on a profile whose settings were already migrated
- **THEN** every dispatch from the `open` handler until composition is complete is at most B_settle, the first presented frame and each composition frame included

#### Scenario: First launch constructs the window within budget
- **WHEN** the release build opens a file on a fresh profile under the keyfile GSettings backend
- **THEN** every dispatch from the `open` handler until composition is complete is at most B_settle
- **AND** no GSettings backend write occurs before the first presented frame

#### Scenario: The open handler defers all window work and keeps the application alive
- **WHEN** the application handles `open` or `activate` with no existing window
- **THEN** the handler returns to the main loop without instantiating a window template, realizing a surface, or presenting a window
- **AND** the application keeps running until the window is constructed

#### Scenario: A second request before the window exists
- **WHEN** a second `open` arrives before the first request's window has been constructed
- **THEN** one window is constructed, and it opens the files of both requests

#### Scenario: A deferred surface is needed early
- **WHEN** a user presses F9, or an automation client activates `win.toggle-sidebar`, before the stage owning that surface has run
- **THEN** the stages that surface needs complete before the action proceeds, and the dispatch and the frame after it each fit B_settle
- **AND** the action's visible effect, state, and automation snapshot equal those on a fully composed window

#### Scenario: Projections during composition force nothing
- **WHEN** the window allocates, refreshes its status bar, or renders notifications before the sidebar or content stages have run
- **THEN** no stage is completed early

#### Scenario: Files opened during composition
- **WHEN** the activation names files, or session restore completes, before the content stage has run
- **THEN** each file opens as a tab through the ordinary open path, in bounded turns, with the same duplicate-path, draft-claim, load-priority, and focus behaviour as on a composed window

#### Scenario: The first realize exceeds the budget on its own
- **WHEN** the reference lane measures the realize dispatch above B_settle
- **THEN** the lane reports that dispatch as over budget
- **AND** the programme record states that the budget is not met and names the toolkit axiom and the upstream report

#### Scenario: No frame runs on the unmapped window
- **WHEN** the window is realized and presented by its composition
- **THEN** every `after-paint` of its frame clock fires while the window is mapped
- **AND** the first frame the construction evidence and the lane report is that first mapped frame

#### Scenario: Revealed content appears in one frame
- **WHEN** the window's first frame has painted with every reveal stage still pending
- **THEN** one dispatch reveals the content page, the workspace sidebar, and the properties panel, and composition completes on the next painted frame

#### Scenario: Content fills in within five reference frames
- **WHEN** the release build opens a file on any reference-lane profile, including the populated one
- **THEN** the time from the first frame to the first painted frame after the activation's file opens ran, with every revealed surface shown, is at most 40 ms, within five reference frames
- **AND** the median of that time over the verdict run is at most three reference frames

### Requirement: GSettings backend writes stay off the construction and interaction paths
The system SHALL NOT execute a GSettings backend write synchronously on the
GTK thread during window construction or before the first presented frame.
Imperative LushText settings writes SHALL go through one application-wide
coalescing writer that keeps the new value in memory immediately, drops a
value equal to the last persisted value, and flushes at most once per
persistence window, no earlier than the first presented frame. Until a value
is flushed, every LushText read of that key SHALL return it, and every
LushText subscriber to that key's changes SHALL be notified when it is
enqueued, so effects delivered through another settings instance stay
immediate. The writer's
flush SHALL run on a path that keeps every GTK dispatch within its budget, as
established by the GSettings backend axiom probes. The last window's `close_request` and
application shutdown SHALL flush pending values synchronously, after any
in-flight flush, so persisted window state is exact after close; closing a
window while others remain SHALL NOT flush synchronously.
Persisted keys, values, defaults, and the split-view migration result SHALL
remain unchanged.

#### Scenario: Resizing does not write per notification
- **WHEN** the user resizes the window and GTK emits many `default-width` and `default-height` notifications
- **THEN** no notification handler performs a backend write on the GTK thread
- **AND** at most one flush per persistence window persists the latest size

#### Scenario: Shell toggles do not write synchronously
- **WHEN** the user toggles the workspace sidebar, document properties, Markdown preview, search panel, or colour scheme
- **THEN** the visible effect is immediate
- **AND** the setting is persisted by the coalescing writer, not by a synchronous backend write in the toggle's dispatch

#### Scenario: Close persists the exact state
- **WHEN** the last window closes with pending writer values
- **THEN** the persisted window size, maximized state, and surface visibility equal the values at close

#### Scenario: Closing one of several windows does not stall the others
- **WHEN** a window closes while another LushText window remains open
- **THEN** the close performs no synchronous backend write
- **AND** the closed window's final values are flushed by the writer on its ordinary path

#### Scenario: Effects through other settings instances stay immediate
- **WHEN** the user changes invisible-character display or the workspace sidebar preset from Preferences
- **THEN** open editor pages and windows reflect the new value in the same interaction, before any flush

#### Scenario: Equal values are not rewritten
- **WHEN** a restore or toggle computes a value equal to the last persisted value
- **THEN** the writer performs no backend write for that key

### Requirement: Protected View interactions and settles stay within named budgets
For documents in Protected View, every interaction dispatch SHALL stay within `B_interact`. Interaction dispatches are scroll, key, click, tab switch, toggles, find navigation, inspector navigation and fold-marker activation. Input-to-presented-frame latency SHALL stay within two reference frames. Every settle dispatch SHALL stay within `B_settle`. Settle dispatches are install, restore, draft recovery, eviction reload, external reload, a word-wrap toggle's re-layout, find-batch publication and whole-line clipboard writes. The one declared exception is the single GTK turn that inserts a hidden tail longer than the slice budget ("Large file installation yields in bounded GTK slices" keeps such a paragraph in one turn): until `sliced-protected-install` (C3) is adopted, that turn is a declared envelope whose measured cost the verdict records as C3's gate input, not a `B_settle` guarantee. Its visible head SHALL still be bounded by `head_cap`, so the envelope covers only the copy of the hidden run into the buffer, never its layout. Both budgets SHALL be read from `model/responsiveness_budget.rs` (`B_INTERACT_US`, `B_SETTLE_US`, `REFERENCE_FRAME_US`, `INPUT_TO_FRAME_FRAMES`); Protected View SHALL NOT restate their values. PR-gated widget tests SHALL assert the preconditions that make the expensive toolkit paths unreachable:
- at most `head_cap` visible characters per paragraph, and at most `V_BATCH` per window of `R_fold` paragraphs;
- `editable=false` and `cursor-visible=false`, applied before the first installed byte;
- the `move-cursor` intercept;
- no search context and no minimap attached; syntax as C1's `DocumentScale` allows;
- invisibles off while a fold exists.

#### Scenario: Precondition gates on a protected document
- **WHEN** the widget suite opens each synthetic protected fixture (2k/10k/50k/250k/1.9M-character paragraphs, the dense wrap-off fixture, CR-only, U+2029-separated, and a prose fixture of 600-character lines)
- **THEN** every precondition holds after install, after tab switch, after a word-wrap toggle and after eviction reload

#### Scenario: Recovery of two giant single-line drafts
- **WHEN** startup recovery restores two drafts of 32 MiB each whose last paragraph holds more than 33 million characters
- **THEN** no dispatch other than the declared one-turn hidden-tail inserts exceeds `B_settle`, no paragraph of more than `head_cap` visible characters is ever laid out, and the one-turn inserts are recorded against the envelope

### Requirement: Keystrokes in editable long paragraphs are a measured toolkit-floor exception
Documents whose longest paragraph is at most `L_PROTECT_EDIT` stay editable (maintainer decision, 2026-10-30). A keystroke inside a paragraph longer than `L_EDIT_STALL` exceeds `B_interact` on the reference machine because GtkTextView re-lays out the whole paragraph on every edit (ledger A23) and GtkTextBuffer recomputes its log attrs (A26); no LushText code is on that path. This SHALL be recorded as one explicit toolkit-floor exception, not as a relaxed budget: its record SHALL name the range (`L_EDIT_STALL` exclusive to `L_PROTECT_EDIT` inclusive), the measured per-keystroke costs of task 1.1 (median, with word wrap on: about 5 ms at 5,000 characters, 7 ms at 10,000, 9.6 ms at 20,000 and 18.5 to 21 ms at 50,000; with it off about 1.3 to 5 ms at 10,000 and 7.7 to 11.6 ms at 50,000), the axioms it rests on, and that the optional C5 owns reducing it. `B_INTERACT_US` SHALL NOT change. The responsiveness lane SHALL keep measuring it through its `keystroke` group (one paragraph of a swept length between short paragraphs, keystrokes at its start, middle and end, wrap on and off), whose fixture kind and derivation are covered by the lane's self-tests, and a later verdict that finds the cost above the recorded figures SHALL report it as a regression.

#### Scenario: The exception is recorded with its measurements
- **WHEN** a reader looks up why typing in a 20,000-character line misses `B_interact`
- **THEN** `docs/next/long-line-documents.md` and the ledger rows of A23 and A26 name the exception's range, its measured costs, the maintainer decision and C5 as its owner

#### Scenario: Keystrokes below the exception stay within budget
- **WHEN** the lane's `keystroke` group types into a paragraph of at most `L_EDIT_STALL` characters
- **THEN** every keystroke dispatch stays within `B_interact`

#### Scenario: The keystroke sweep is self-tested
- **WHEN** `make check-responsiveness-lane` runs
- **THEN** it checks the `keystroke` fixture's markers, paragraph length and delimiters, and the driver's case grid for the group

### Requirement: Protected View never snapshots a protected buffer on the GTK thread
LushText-owned Protected View work SHALL stay within declared chunk sizes, and SHALL NOT read a protected buffer's text through a GTK snapshot whose per-chunk cost grows with the chunk's depth inside a paragraph:
- find, the Line Inspector, whole-line copy and every save of a protected document SHALL read `ProtectedTextSource`, which for every whole-content install is the installed body itself, retained;
- after an edit-growth entry the source SHALL be captured at most once; the capture SHALL use a positioning whose per-chunk cost the ledger shows to be independent of depth (P8, ledger A66), or, if no such positioning exists, SHALL be recorded in the verdict as a declared envelope owned by C3;
- find SHALL deliver at most a declared number of results and bytes per GTK turn under a total result cap;
- the inspector SHALL read at most `N_inspect` characters per window, and whole-line copy SHALL write at most one chunk per dispatch;
- notice and accessible text stay within the inline-alert bounds.

None of this work SHALL copy a whole paragraph or the whole buffer on the GTK thread.

#### Scenario: First find on a large protected document
- **WHEN** the user opens find on a 100 MB protected document that was installed from disk
- **THEN** the search runs on the retained source with no snapshot turn on the GTK thread, and every result batch is published within `B_settle`

#### Scenario: Save As of a giant protected document
- **WHEN** the user saves a protected document holding a 1.9M-character paragraph with Save As
- **THEN** no GTK dispatch reads the buffer's text, and the write runs on the worker from the retained source

### Requirement: The Protected View verdict is measured under the reference precondition
The Protected View budget verdict SHALL be measured with C0's responsiveness lane as it now stands (presented frames only; a run in which a measured case wrote no `frame` record fails), against both `main`'s and the change's release responsiveness binaries measured by the change's lane driver, each in its own hold of the shared measurement lock. Every run SHALL hold the measurement lock exclusively for itself alone (lock protocol rev 2: the intent lock first, so no new build starts, then the measurement lock; one run per hold), and SHALL record the load before the lock, inside it and after the run; load is accepted and recorded, not gated (maintainer decision, 2026-10-30, as for C1's lane verdict). `main` and the change SHALL be measured interleaved, so both sides see the same load. Each case SHALL run 7 repetitions in the lane's three variants (plain, AT, probe IM); cases that start a process (open, three-document restore, draft recovery) SHALL also run in the four construction profiles (fresh or steady-state, keyfile or dconf). The verdict, its run ids, the precondition evidence, `btrfs-stall-report` for each run and the toolkit versions SHALL be recorded in `docs/next/long-line-documents.md`.

#### Scenario: Responsiveness lane run
- **WHEN** the lane runs the protected cases (open, scroll, Page Down, caret keys, typing, click and double-click, find, inspector open and navigation, Copy Whole Line, Save As, tab switch, word-wrap toggle, eviction and reselection, restore of three protected documents, recovery of two 32 MiB single-line drafts, the AT bridge with a listener)
- **THEN** the largest interaction dispatch is at most `B_interact`, the largest settle dispatch other than a declared one-turn hidden-tail insert is at most `B_settle`, the one-turn inserts are recorded against the envelope, and the input-to-frame latency is at most two reference frames

#### Scenario: Load is recorded with each run
- **WHEN** a verdict run is measured
- **THEN** its record carries the load before, inside and after the lock, and `main` and the change alternate run by run

### Requirement: Post-open settle fits the settle budget per dispatch
Every main-loop dispatch from the activation turn that creates an editor page until that document is settled SHALL take at most `B_SETTLE_US`, on every opening path:
- activation, the Open dialog, the recent-documents popover and the sidebar;
- session restore and draft recovery.

A document is settled when all of the following hold:
- its load is installed and published;
- its language is attached;
- no post-open slice, debounce, worker completion or publish step is pending;
- GtkSourceView highlighting and GtkTextView validation have been quiet for the named settle-quiet period.

An input event delivered during settle SHALL be dispatched within `B_INTERACT_US` of its event time. A dispatch recorded as a toolkit-floor exception against a ledger axiom is exempt from `B_SETTLE_US`, and the input events whose dispatch overlaps it are exempt from `B_INTERACT_US`; both are reported, never dropped. Debounces started by user edits during settle are not post-open work. Dispatches that lay out a single paragraph longer than the Protected View entry threshold are excluded; they are owned by the protected-view and sliced-protected-install changes. The budgets are read from `model/responsiveness_budget.rs`.

#### Scenario: A small document settles within budget
- **WHEN** a 400-byte plain-text document is opened in any of the four construction profiles
- **THEN** every dispatch from the activation turn until the document is settled takes at most `B_SETTLE_US`

#### Scenario: An HTML document opens without a language-definition stall
- **WHEN** the first HTML document of a process is opened
- **THEN** the activation dispatch does not parse the language definition
- **AND** every dispatch until the document is settled takes at most `B_SETTLE_US`, except a language-definition parse recorded as a toolkit-floor exception against its ledger axiom

#### Scenario: Typing during settle is not starved
- **WHEN** keys are typed into a document from its load readiness until it is settled
- **THEN** each key is dispatched within `B_INTERACT_US` of its event time

#### Scenario: A dense document of short lines settles within budget
- **WHEN** a document whose lines are dense but shorter than the Protected View entry threshold is opened, with word wrap on or off
- **THEN** no validation dispatch caused by the open takes more than `B_SETTLE_US`

### Requirement: Language attach stays out of the activation dispatch
The language catalog SHALL be built off the GTK thread and handed to it, and every LushText use of a GtkSourceView language manager SHALL go through that catalog. Guessing a document's language from its path SHALL run on a worker and return a language id. The catalog SHALL be started at application startup, so that it is ready before the activation's opens. The worker's path guess SHALL agree with GtkSourceView's `guess_language` for every glob of every shipped language, and at attach time GtkSourceView's guess is authoritative.

Every site that sets a document's language (path changes, load publication, the buffer-replacement terminal and document-scale changes) SHALL request one attach stage instead of attaching in its own dispatch. The stage SHALL run in its own dispatch after the editor's first frame; for an editor that is not mapped, after it is first mapped, except a stage a chunked highlighted install requested, which SHALL NOT wait for the editor to be mapped. While the attach is pending, the document's language id SHALL already be reported to every consumer that asks for it, including the Markdown preview. The attach SHALL run off the GTK thread only where the ledger's language-parse axiom shows that this is sound, and then only on a language object that no GTK-thread code could reach before the handoff.

Where it cannot, the attach SHALL run from a `G_PRIORITY_LOW` idle, and only after the named input-quiet interval has passed with no key press, button press, scroll start or touch begin on the window, or once the named attach deadline has passed. The one exception is a stage a chunked highlighted install requested in an editor that is mapped and has the keyboard focus of the active window: that install holds the editor's typed keys until its terminal, so the stage SHALL NOT wait for input-quiet there; a chunked install in any other editor, including a background tab and the focus child of an inactive window, SHALL wait for input-quiet or the deadline. Its cost SHALL be recorded as a toolkit-floor exception against that axiom, with measured numbers and a drafted upstream report.

#### Scenario: The first open of a process does not load the language list
- **WHEN** the first document of a process is opened
- **THEN** the activation dispatch neither loads the language list nor guesses a language

#### Scenario: The language parse cannot leave the GTK thread
- **WHEN** the ledger's language-parse axiom shows that the definition parse must run on the GTK thread, and an HTML document is opened while the user is typing
- **THEN** the attach waits until no input event has arrived for the input-quiet interval, or until the attach deadline, and runs from a low-priority idle
- **AND** the programme record lists the parse as a toolkit-floor exception with its axiom id, cost and upstream report

#### Scenario: A background chunked install waits for input-quiet
- **WHEN** a large Markdown document installs chunk by chunk in a tab that is not shown, while the user types in the shown editor
- **THEN** its language does not attach until no input has arrived for the input-quiet interval, or until the attach deadline

#### Scenario: An inactive window's install waits for input-quiet
- **WHEN** a chunked install runs in an editor that is its window's focus child, while the user types in another, active window
- **THEN** its language does not attach until no input has arrived for the input-quiet interval, or until the attach deadline

#### Scenario: A Markdown document is recognised before its language attaches
- **WHEN** a Markdown document opens with the side-by-side preview shown, and its language attach is still pending
- **THEN** the preview treats it as Markdown, and never shows the not-a-Markdown placeholder for it

#### Scenario: A restored tab that is not selected
- **WHEN** session restore creates an editor page that is not selected, for a document that does not take the chunked highlighted install
- **THEN** its language attach runs only after it is first mapped, and its language id is reported meanwhile

#### Scenario: The language attaches after the first frame
- **WHEN** a document with a recognised language opens, other than into a tab that is not shown while a chunked highlighted install owns its buffer
- **THEN** its first editor frame is painted before the language attach stage runs
- **AND** a later load, reload or rename of the same editor replaces any pending attach, so a stale language is never attached

### Requirement: Highlighting attach and background analysis are bounded
Publishing an installed document SHALL NOT turn on syntax highlighting in a dispatch whose cost grows with the document. The document scale's total-size syntax threshold `S_SYNTAX` SHALL be the measured `S_BG_HIGHLIGHT`: the largest short-line total size whose background analysis keeps every dispatch within `B_SETTLE_US`. Above it, syntax highlighting is reduced with the existing reduction reason `TotalSize`, shown as for the other reductions, except for a language with a chunked cap (`document_scale::chunked_threshold`; none shipped): a document of one of them above its measured threshold and at most its cap takes the chunked highlighted install instead, and is reduced with `TotalSize` only above the cap. The budgets are not relaxed for background analysis.

#### Scenario: A large HTML document publishes without a document-sized dispatch
- **WHEN** a 5 MB short-line HTML document finishes its sliced installation
- **THEN** the publish dispatch takes at most `B_SETTLE_US`

#### Scenario: A document above the background-highlight threshold
- **WHEN** an HTML document larger than `S_BG_HIGHLIGHT` is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize`
- **AND** no background-analysis dispatch runs for it

#### Scenario: A document with a chunked cap keeps its highlighting
- **WHEN** a short-line document larger than its language's measured threshold and at most its chunked cap is opened
- **THEN** syntax highlighting stays on, the document takes the chunked highlighted install, and no dispatch of the install, publish or settle exceeds `B_SETTLE_US`

#### Scenario: A chunked cap holds for flipping keys on a worst-case document
- **WHEN** a key that changes the syntax context of the rest of a document at its language's chunked cap (a code fence or `<!--` in Markdown, `/*` in C) is typed at its start or its middle, in a document with no closer after the cursor, then its closer
- **THEN** every dispatch until quiet stays within `B_SETTLE_US`, and every ordinary key typed meanwhile within `B_INTERACT_US`

#### Scenario: A shipped Markdown document above its gate is not chunked
- **WHEN** a 400 KiB short-line Markdown document opens in a shipped build
- **THEN** syntax highlighting is reduced with the reason `TotalSize`, no chunked install runs, and no language is attached

#### Scenario: Markdown, C, Rust, Python and XML keep their gates
- **WHEN** a Markdown, C, Rust, Python or XML document larger than its measured threshold is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize` as before: the largest power of two that passes the flipping-key criteria is 128 KiB for markdown (an ordinary key inside an open `<!--` fails at 256 KiB), 64 KiB for c (a closing `/*` fails at 128 KiB), 128 KiB for rust and xml, and none for python3, so no cap exceeds that language's gate

#### Scenario: HTML, JSON, JavaScript, TypeScript, CSS, shell and YAML stay gated
- **WHEN** a document of one of those languages larger than its measured threshold is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize` as before, and no language is attached to it

### Requirement: User-started invalidations on dense documents are paced
On a document where some window of consecutive paragraphs that one validation batch can cover holds more than `V_SLICE` visible characters (the document profile's densest batch), a user-started change of word wrap, zoom, font or tab width SHALL NOT leave more than `V_SLICE` visible characters of invalid lines for one validation dispatch. A style that a buffer tag carries reaches every view of the buffer, so it SHALL NOT change what the minimap or printing shows; word wrap SHALL therefore not be carried by a buffer tag. When a change applies to several editors, the selected editor SHALL be updated first, and every other dense editor when it is next mapped.

The interaction dispatch that handles the change SHALL take at most `B_INTERACT_US`. The visible paragraphs SHALL be updated first, in a frame whose dispatch takes at most `B_SETTLE_US`. The rest of the document SHALL be updated outward, in paragraph-aligned ranges, in low-priority dispatches of at most `B_SETTLE_US` each. A newer change SHALL supersede the running one and restart from the visible paragraphs.

The mechanism is the one the ledger's restyle axiom selects. If none passes, the cost SHALL be recorded as a toolkit-floor exception against that axiom.

#### Scenario: Toggling word wrap on a dense document
- **WHEN** the user toggles word wrap on a document of 710-character lines
- **THEN** the toggle's dispatch takes at most `B_INTERACT_US`, the visible paragraphs show the new wrapping in the next frame, and every later dispatch until the document is settled takes at most `B_SETTLE_US`

#### Scenario: Zooming a dense document
- **WHEN** the user zooms in on a document of 710-character lines
- **THEN** the zoom's dispatch takes at most `B_INTERACT_US`, the visible paragraphs show the new size first, and every later dispatch until the document is settled takes at most `B_SETTLE_US`

#### Scenario: Changing the font of a dense document
- **WHEN** the user changes the editor font while a document of 710-character lines is open
- **THEN** no dispatch caused by the change in that editor takes more than `B_SETTLE_US`, and the visible paragraphs are updated first

#### Scenario: A second change during a paced update
- **WHEN** the user toggles word wrap again before the previous toggle has reached every paragraph
- **THEN** the earlier update stops without applying its remaining ranges, and the newest setting is applied from the visible paragraphs outward

#### Scenario: A document that is not dense keeps the direct path
- **WHEN** the user toggles word wrap on a document with no window over `V_SLICE` visible characters
- **THEN** the view property changes directly, as today

#### Scenario: Zooming with several dense documents open
- **WHEN** the user zooms while two dense documents are open in different tabs
- **THEN** the selected document is updated first, outward from its visible paragraphs, and the other document is updated only when it is next shown
- **AND** neither minimap is drawn at the document's font size

### Requirement: Protected tail slices never merge into a growing segment
The sliced protected install SHALL keep one zero-width text-buffer boundary between every two consecutive hidden-tail slices of a paragraph, so that GTK never merges the newly inserted run with the already installed tail by copying both (ledger A36, A76). The boundary is an anonymous left-gravity mark; alternating protected tags with identical properties is the recorded fallback if that axiom stops holding. The install also marks each protected paragraph's start, where its head goes, and its tail end, so the record of ranges covers the whole paragraph content. A tail that fits one slice needs no boundary: it goes in right after its head, in the same dispatch. Boundaries SHALL persist until their range is deleted or the buffer is disposed. They SHALL NOT be removed while char segments remain on both sides of them. The total work to install a protected tail SHALL be linear in the tail's size.

#### Scenario: Consecutive tail slices stay separate segments
- **WHEN** a hidden tail is installed in K slices
- **THEN** K − 1 boundaries separate them, the paragraph start and the tail end carry one boundary each, and no later slice's insertion copies the bytes of an earlier slice
- **AND** the whole tail still reads as one protected-tag range

#### Scenario: Boundaries do not outlive their content
- **WHEN** a buffer that held protected tails is evicted, reloaded, replaced, or closed
- **THEN** no boundary mark created by the install remains in the buffer after the clear completes

### Requirement: Protected tails are cleared in recorded slices
Clearing a buffer that holds protected tails SHALL first delete each protected paragraph's content through the boundary ranges recorded at install time, paragraph by paragraph, each from the paragraph's start forwards, as many consecutive ranges per main-loop dispatch as fit the protected clear slice budget and at least one (a deletion costs time growing with the text before it in its paragraph, ledger A76). The record of those ranges SHALL outlive the install and SHALL hold marks and byte counts only, never text. The boundaries a deleted range leaves collapsed SHALL be deleted in the same dispatch, where they no longer separate two char segments. Visible content SHALL then keep the paragraph-aligned clear slices, which no longer meet a giant paragraph. A buffer emptied another way (a direct replacement) SHALL drop its record after the text is gone. No clear dispatch SHALL exceed B_settle on the reference lane.

#### Scenario: Eviction clears a giant protected tail
- **WHEN** memory policy evicts a clean protected editor whose tail spans many recorded ranges
- **THEN** each clear dispatch deletes the first remaining recorded ranges of a paragraph that fit the clear slice budget, from its first range to its last
- **AND** residency is released only after the clear session completes

#### Scenario: A partial install is cancelled
- **WHEN** a protected install is cancelled after some tail slices were inserted
- **THEN** the cancellation clear removes exactly the installed ranges and boundaries, and no partial body becomes editable, saveable, or visible as a completed load

### Requirement: Sliced protected installation preserves document bytes exactly
Slicing SHALL only choose offsets and SHALL never transform text. After a completed sliced install, the buffer's document text, hidden text included, SHALL equal the decoded document byte for byte, for every slice budget and every placement of paragraph delimiters and multibyte characters. Saving or Save As after a sliced install SHALL write exactly the source bytes. The planner that chooses the offsets SHALL be GTK-free and checked as the requirement "The protected install slice plan is proved over a stated domain" states (Kani for the parts it proves, property tests for the rest).

While a load installation or a whole-buffer replacement owns an editor's buffer, no draft, close-time capture or local-history periodic capture SHALL snapshot that buffer: the buffer then holds neither the old document nor the new one. The existing draft SHALL stay unchanged until the installation or replacement reaches its terminal. A sliced protected install that can no longer place a paragraph's head where the body puts it SHALL fail closed, cancelling as incomplete, and a turn whose editor is gone while a fold plan is set SHALL end the install rather than insert hidden text unfolded.

#### Scenario: Mixed delimiters and multibyte text round-trip
- **WHEN** a synthetic protected document with CR-only, `\r\n` straddling a budget, U+2029, and multibyte characters at every byte phase is installed through the sliced path
- **THEN** reading the buffer's full text including hidden text returns the decoded document unchanged

#### Scenario: An undelimited folded last paragraph is never the last line
- **WHEN** a protected body whose last paragraph is folded and has no delimiter is installed, directly or sliced
- **THEN** the buffer ends with one stand-in `\n` under an invisible, non-editable terminator tag, so a point below the text maps to an iterator without walking the hidden run (ledger A79)
- **AND** the stand-in is not document text: every snapshot of the buffer's document text (save, draft, local history, preview) and the document statistics stop before it, and every clear deletes it

#### Scenario: Save As after a sliced install
- **WHEN** the user saves a protected document to a new path after a sliced install
- **THEN** the new file's bytes equal the original file's bytes

#### Scenario: A recovered folded draft is written back byte for byte
- **WHEN** a recovered draft whose folded last paragraph has no delimiter is written by the draft autosave, which snapshots the buffer
- **THEN** the body on disk equals the original draft byte for byte, without the stand-in terminator

#### Scenario: An autosave during a replacement keeps the previous draft
- **WHEN** an editor whose draft is behind its buffer starts a bounded whole-buffer replacement (a local-history restore) and an autosave tick runs before the replacement's terminal
- **THEN** the persisted draft still holds the pre-replacement text, and the next pass after the terminal writes the replaced text

#### Scenario: A broken sliced install fails closed
- **WHEN** a sliced protected install finds its paragraph-start mark deleted
- **THEN** it inserts no head after the tail and cancels as incomplete, so the partial buffer is cleared and cannot be saved or drafted

### Requirement: The protected install slice plan is proved over a stated domain
The sliced protected install SHALL take its slices from one GTK-free, I/O-free planner, and Kani harnesses SHALL check that production planner, C1's paragraph-boundary function and C2's fold placement step directly, never a copy. Each hidden-tail slice SHALL end at the previous slice's end plus `tail_slice_bytes`, or at the tail end if that is nearer, rounded down to the nearest boundary that is a char boundary and not between `\r` and `\n`; `tail_slice_bytes` SHALL be at least 4, so every slice advances. The tail and clear slice budgets SHALL be derived from B_settle and the measured per-KiB costs by one pure, integer-only function. The harnesses SHALL apply only when the adoption gate adopted this change, and only for the adopted half.

The proofs hold on these domains, which the harnesses and the planner's rustdoc SHALL state:
- the **document domain**: every valid UTF-8 string of at most 10 bytes over `a`, `é`, `€`, `𝄞`, `\r`, `\n` and U+2029, a tail slice budget and a clear slice budget of 4 to 7 bytes, `N_visible` of 1 to 3 characters, and small symbolic density parameters within the fold placement step's preconditions;
- the **giant-paragraph domain**: one paragraph whose content is at most 12 bytes of `a`, `é`, `€` and `𝄞`, ending with no delimiter, `\n`, `\r`, `\r\n` or U+2029, with the same budgets and `N_visible`, and density parameters under which the density rule does not shorten a first paragraph's head;
- every `u64` input of the budget derivation.

Larger sizes SHALL be covered by property tests of the same assertions, and no rustdoc or spec text SHALL claim the Kani result beyond these domains.

**As implemented (2026-11-02):** Kani proves the clear plan, C2's fold of one paragraph of any `u64` length, the budget derivation's freedom from panics and its admissibility, and the `should_panic` rounding-guard harness. The whole-plan scenarios below ("The slices reproduce the exact bytes", "No boundary splits a scalar or a CRLF pair", the planner half of "Every slice fits the budget derived from B_settle", the boundary half of "Boundaries and the clear plan are placed exactly", "The tag boundary falls exactly at N_visible" and "The slice count of one giant paragraph is bounded") did not fit CBMC at any domain tried and are checked by property tests on generated inputs instead, which sample those domains rather than cover them; the derivation's fit and optimality are property-tested too. This is a recorded deferral (`docs/next/formal-verification.md`), not a proof.

#### Scenario: The slices reproduce the exact bytes
- **WHEN** Kani plans any input of the document domain
- **THEN** every step advances, and the slices concatenated in order equal the input byte for byte

#### Scenario: No boundary splits a scalar or a CRLF pair
- **WHEN** Kani plans any input of the document domain
- **THEN** every slice end is a char boundary of the input and never falls between the `\r` and `\n` of a pair

#### Scenario: Every slice fits the budget derived from B_settle
- **WHEN** Kani derives the slice budget from any `u64` settle budget, per-KiB cost and margin
- **THEN** the derivation never overflows or panics, and an accepted budget is at least 4 bytes, at most the stated maximum slice, the largest size whose predicted cost, margin included, is within the settle budget, while an unattainable budget is reported as an error
- **AND** when Kani plans any input of the document domain with such a budget, every hidden-tail slice is at most that budget and every slice is wholly visible or wholly hidden

#### Scenario: Boundaries and the clear plan are placed exactly
- **WHEN** Kani plans any input of the document domain
- **THEN** a boundary follows a slice exactly when the next slice is a hidden-tail slice of the same paragraph
- **AND** the clear plan deletes each paragraph's recorded ranges from its first, each turn within the clear budget, and with the ordinary clear slices covers the buffer exactly once

#### Scenario: The tag boundary falls exactly at N_visible
- **WHEN** Kani plans one giant paragraph of the giant-paragraph domain
- **THEN** if the paragraph is longer than `N_visible` characters, the switch from visible to hidden slices falls at the end of the paragraph's `N_visible`-th character
- **AND** every byte before it is in the paragraph's head slice and every byte from it to the tail end of C2's fold range is in a hidden-tail slice, and the executor inserts the head untagged and hidden-tail slices with the protected tag

#### Scenario: The slice count of one giant paragraph is bounded
- **WHEN** Kani plans one giant paragraph whose content is `len` bytes, delimiter excluded, with tail slice budget `B`
- **THEN** the paragraph's content takes at most ⌈len / (B − 3)⌉ + 1 slices: one head slice and at most ⌈tail bytes / (B − 3)⌉ tail slices, each tail slice but the last being at least `B − 3` bytes

#### Scenario: A boundary choice that splits UTF-8 is caught
- **WHEN** the `should_panic` harness runs the production stepper with the rounding guard removed, so a tail slice ends at the raw previous end plus `B`
- **THEN** Kani finds a slice end inside a multibyte character and the harness passes as `should_panic`

### Requirement: Bounded-stage hand-offs are not held behind text validation
A hand-off between bounded stages of a LushText workflow SHALL run at
`gtk_lush_tasks::completion_priority()`, so that a text view's validation
backlog (ledger A80) cannot hold it back. A hand-off is a worker completion,
the file-load and save admission drains, or the release of a file-load or save
payload permit. Paced work whose slices must follow validation, such as the
post-open install slices at `PACED_SLICE_PRIORITY` (ledger A70), SHALL stay
below the validation idle while the editor's view lays the document out or the
tab is its window's selected tab; a paced install or whole-buffer replacement
into any other tab, whose view shows its placeholder, SHALL queue its slices
one priority step lower, `G_PRIORITY_DEFAULT_IDLE + 1`, within the same slice
budgets: nothing lays it out, and the visible document installs and lays out
first. Every
hand-off dispatch SHALL stay within `B_SETTLE_US`.

#### Scenario: A save during a validation backlog is admitted and written promptly
- **WHEN** the user presses Ctrl+S on a small document while other open
  documents are still being validated
- **THEN** the save's admission drain, its snapshot and its worker completion
  run without waiting for the backlog
- **AND** the saved bytes are on disk long before the backlog ends

#### Scenario: Install pacing still follows validation
- **WHEN** a paced installation into a shown tab queues its next slice
- **THEN** the slice waits at `PACED_SLICE_PRIORITY`, below the validation idle,
  as before
- **AND** a paced installation into a background tab queues its next slice at
  `G_PRIORITY_DEFAULT_IDLE + 1`, and switches to `PACED_SLICE_PRIORITY` from the next
  slice once its tab is selected or shown

#### Scenario: A composition stage dispatches alone
- **WHEN** a worker completion makes default-idle work ready before a window
  composition stage runs, such as the session load queuing the first restore
  turn before stage P
- **THEN** the stage runs at a priority one step above `G_PRIORITY_DEFAULT_IDLE`,
  in an iteration of its own, and the default-idle work runs in the next one
- **AND** the language catalog's one-off build runs at
  `G_PRIORITY_DEFAULT_IDLE - 2`, a priority no other source uses, so it never
  shares a stage's or a turn's dispatch

### Requirement: A background document is read without waiting for installs
After a file load's worker read, its transient load permit SHALL keep the
decoded body it still holds outside the buffer, the live editor's four-byte
residency for every character not yet installed, and the fixed per-load
overhead (never more than it was granted), and SHALL stop counting against the
concurrent-read limit, so that an installation waiting behind the visible tab
never keeps the next document from being read while the byte budget still
covers everything the overlapping loads own. An oversize load SHALL keep
running alone until it is released. The protected-residency rule SHALL still
count an installing body as a payload, and the permit SHALL be released exactly
once. A background install slice that has waited behind visible work for
`BACKGROUND_SLICE_AGE` (256 ms) SHALL run once at the visible tab's paced
priority, so no background install waits indefinitely.

#### Scenario: Overlapping installs stay within the byte budget
- **WHEN** several loads have finished reading and are installing
- **THEN** their retained charges, each covering its body and the residency of
  its characters not yet installed, fit the shared budget together
- **AND** a load larger than the budget installs with no other load admitted

#### Scenario: A background install keeps moving behind visible work
- **WHEN** a background slice waits while `G_PRIORITY_DEFAULT_IDLE` work stays
  ready
- **THEN** it runs within `BACKGROUND_SLICE_AGE` of being queued, and removing
  its source still cancels it

#### Scenario: Ten large documents are all read at once
- **WHEN** ten 8 MiB documents are opened while one of them is shown
- **THEN** every document is read and its installation begun without waiting
  for another installation to finish, within the byte budget

### Requirement: A hidden tab's editor view lays out nothing
An editor's text view SHALL show an empty placeholder buffer from the editor's
construction until its tab is first shown, and again once it is seen laying
out while its tab is hidden (its adjustment's `upper` changing while the view is
unmapped), so that no background tab validates its document (ledger A82, A83).
Showing the tab SHALL give the view its document buffer back before the tab's
first frame. The document buffer SHALL be the one every workflow reads and
writes, whatever the view shows: loading, drafts, saves, session persistence,
search, bookmarks, printing and memory accounting SHALL behave as for a shown
tab, and a detach or attach SHALL change no byte, modified flag, undo state,
caret or primary selection.

No view SHALL be given the placeholder while an assistive technology listens
to the application's events (GTK sends each swap's whole old and new text to
it, about 2 ms per MiB in one dispatch), nor while its document has more than
`VIEW_DETACH_MAX_LINES` (2^18) lines (the swap walks every line); when a
listener appears, a detached view whose document has at most
`AT_ATTACH_MAX_CHARS` (2^20) characters SHALL be attached at once. A detached
view SHALL keep Protected View's primary-selection choice for its attach, and
the kept position inside a wrapped paragraph SHALL be the display line at the
top of the view.

#### Scenario: Background documents are not laid out
- **WHEN** several large documents are opened or restored and only one tab is
  shown
- **THEN** the hidden tabs' views show their placeholders and validate nothing
- **AND** a `G_PRIORITY_DEFAULT_IDLE` source is not held back by them, even
  after a view-wide restyle of every editor

#### Scenario: A hidden tab seen laying out is detached
- **WHEN** a tab that was shown is hidden while its view still has lines to lay
  out, or is invalidated while hidden
- **THEN** its view is given the placeholder before its next validation batch

#### Scenario: A detached tab keeps its position
- **WHEN** a detached tab is shown again
- **THEN** its view shows the line it showed when hidden (or the line a
  restore, search or bookmark navigation asked for while it was hidden), and
  keeps it there while it lays the document out, until the user gives input
- **AND** the session saved meanwhile records that line

#### Scenario: No view is detached while an assistive technology listens
- **WHEN** an assistive technology listens and a hidden tab's view lays out
- **THEN** the view stays attached and lays out in GTK's own bounded batches
- **AND** a small detached view attaches as soon as the listener appears, and
  views are detached again once no listener remains

#### Scenario: A document with very many lines is never detached
- **WHEN** a hidden tab's document has more than `VIEW_DETACH_MAX_LINES` lines
- **THEN** its view stays attached

#### Scenario: A hidden tab's selection stays on PRIMARY
- **WHEN** a tab whose selection owns the primary selection is hidden and its
  view detached
- **THEN** the primary selection still offers it, and still does after the
  attach

#### Scenario: Protected View entered while detached
- **WHEN** a detached tab's document enters Protected View
- **THEN** no toolkit critical is logged and, once shown, the document offers
  nothing on the primary selection

#### Scenario: A never-shown tab prints and opens bookmarks
- **WHEN** a tab that was never shown (preview-only mode) is printed or a
  bookmark in it is opened
- **THEN** the print covers its document, and the bookmark places the caret
  and the kept line without a toolkit critical

#### Scenario: A hidden view with a minimap is detached within budget
- **WHEN** a hidden laid-out view of 204,601 lines with a minimap bound is
  re-laid out (a zoom step)
- **THEN** its detach dispatch stays within `B_SETTLE_US`

#### Scenario: Switching to a large hidden tab stays within budget
- **WHEN** the user switches to a large tab whose view laid nothing out while
  hidden
- **THEN** every dispatch of the switch and of the re-layout that follows stays
  within `B_SETTLE_US`

### Requirement: The assistive-technology mirror fails safe
The application's mirror of GTK's AT-SPI event-listener table SHALL read as
listening until the registry has answered `GetRegisteredEvents`, and for the
rest of the session when the accessibility bus address, the connection or
that call fails; it SHALL read the registry's answer as replacing its table,
and SHALL parse `EventListenerRegistered` (`(ssas)`) and
`EventListenerDeregistered` (`(ss)`) so a listener leaving is counted. When a
listener appears, detached editors SHALL be attached at most one per main-loop
dispatch, the selected tab's first, and the queue SHALL stop when no listener
remains. A detached document over `AT_ATTACH_MAX_CHARS` shown while a listener
listens sends its whole text once (recorded exception, design D7).

#### Scenario: An assistive technology starts and stops
- **WHEN** a listener registers and later deregisters
- **THEN** views are not detached while it listens, and are detached again
  once it has left

#### Scenario: The registry has not answered
- **WHEN** an editor is built before the registry's answer, or the registry
  cannot be reached
- **THEN** its view is not given the placeholder

#### Scenario: Many small hidden documents when a listener appears
- **WHEN** a listener appears while several small documents are detached
- **THEN** they attach in separate dispatches, one each

### Requirement: Hidden text never escapes the in-tab search
No paced reveal SHALL start while the in-tab search context is attached with
a non-empty query, and the query becoming non-empty, attaching a context with
a retained query, or replacing through it SHALL first reveal any text a paced
reveal still hides, so find, the occurrence count, Replace and Replace All see
the whole document. With an empty query restyles and reattaches still pace. Replace All with nothing to replace SHALL change nothing
and SHALL not fail.

#### Scenario: Replace All right after switching to a dense tab
- **WHEN** the user switches to a never-shown dense tab and runs Replace All
  at once
- **THEN** every match in the document is replaced

#### Scenario: An open search bar with an empty query keeps pacing
- **WHEN** a dense document is restyled while its search bar is open with an
  empty query
- **THEN** the restyle is paced, and typing a query reveals every paragraph
  at once

#### Scenario: Replace All matches nothing
- **WHEN** Replace All runs with a query that matches nothing
- **THEN** the document is unchanged and the application keeps running

### Requirement: Highlighted documents above their attach threshold install chunk by chunk
A document whose incoming scale allows syntax highlighting, in a language with a chunked cap (`document_scale::chunked_threshold`: the largest power of two at which, on a worst-case document with no closer after the cursor, a key that changes the syntax context of the rest of the document and its closer keep every dispatch within `B_SETTLE_US` and ordinary keys stay within `B_INTERACT_US`, at the start and the middle, and which the install fits in about 0.5 s; a language whose cap would not exceed its C6 gate has none, and no shipped language has one today), larger than that language's attach-after-install threshold and at most its cap, SHALL be installed with its language attached, in this order: a language attached to old text is removed before that text is cleared; the first paragraph-aligned chunk is inserted before the attach stage runs (a language already attached to the empty buffer may stay); the deferred attach stage attaches the language to that one chunk; and every later chunk, of at most `HIGHLIGHT_CHUNK_BYTES` (8 KiB) and ending on a paragraph boundary, is inserted only once the context engine has reported the buffer analysed to its end (ledger A84) or after one wait at `G_PRIORITY_LOW + 1`. Every installer of a whole body SHALL take this order: file load and reload, eviction reload, draft recovery, local-history restore and undo, and save-formatting mirror-back. A cancelled, superseded or disposed chunked install SHALL remove the language before clearing its partial text, and a sliced clear-only replacement SHALL remove it before clearing; a whole-buffer replacement SHALL remove it at its first clear turn, so one that ends before any turn leaves the language as it was. An attach stage requested while a chunked install owns the buffer, including by an identity change, SHALL NOT wait for the editor to be mapped. Publication SHALL NOT toggle highlighting on a chunked document. The installed text SHALL be the decoded body byte for byte.

#### Scenario: The first frame shows text
- **WHEN** a 400 KiB document opens in a language with a chunked cap above it (the chunked tests give Markdown 512 KiB)
- **THEN** the language is attached while the buffer holds at most one chunk, after the first chunk was inserted
- **AND** the document ends highlighted, its text equal to the file

#### Scenario: Each chunk is analysed before the next
- **WHEN** a chunk has gone in and the engine has not reported the buffer analysed to its end
- **THEN** the next chunk waits once at `G_PRIORITY_LOW + 1`, behind the engine's worker, and then goes in

#### Scenario: A background tab installs without being shown
- **WHEN** a large Markdown document is opened into a tab that is not selected
- **THEN** its chunked install completes with the language attached while the tab stays hidden

#### Scenario: A cancelled chunked install leaves no language behind
- **WHEN** a chunked install is cancelled after several chunks
- **THEN** its partial text is cleared with no language attached, and the editor is marked incomplete so nothing can be saved over the file

#### Scenario: A reload removes the language before clearing
- **WHEN** a highlighted large Markdown document is reloaded
- **THEN** its language is removed while the old text is still whole, and the reloaded document ends highlighted

#### Scenario: A local-history restore installs chunk by chunk
- **WHEN** a 400 KiB Markdown body is restored from local history into a Markdown editor
- **THEN** the replacement inserts it in chunks of at most 8 KiB and the language is attached while the buffer holds at most one chunk

#### Scenario: A rename while a background install waits for its attach
- **WHEN** a hidden tab's chunked install waits for its attach stage and the file is renamed
- **THEN** the install still finishes without the tab being shown

#### Scenario: A chunk the engine does not report waits once
- **WHEN** a chunk goes in and the engine reports no analysed range
- **THEN** the next chunk waits once at `G_PRIORITY_LOW + 1` and goes in, and the install finishes

#### Scenario: A replacement that ends before its first turn keeps the language
- **WHEN** a chunked whole-buffer replacement becomes stale before its first turn
- **THEN** the old text keeps its language and highlighting, and nothing re-analyses it

