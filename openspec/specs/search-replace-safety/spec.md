# search-replace-safety Specification

## Purpose
TBD - created by archiving change data-home-persistence-contracts. Update Purpose after archive.

## Requirements

### Requirement: Replace All writes files atomically and skips explicitly unsafe targets
The system SHALL apply Replace All writes through atomic temp-file-then-rename file updates rather than in-place overwrites. The workflow MUST skip targets the UI marks unsafe to replace, such as open files with unsaved changes, and MUST report those skipped paths without modifying them.

#### Scenario: Replace All skips an explicitly unsafe file
- **WHEN** the Replace All workflow includes a file that the UI marks unsafe to modify in place
- **THEN** the system skips that file during replacement
- **AND** the file is reported as skipped instead of being modified

#### Scenario: Replace All updates a safe file through an atomic write
- **WHEN** Replace All modifies a safe target file
- **THEN** the file is rewritten through an atomic write path
- **AND** the workflow does not rely on an in-place partial overwrite

### Requirement: Replace All preserves each replaced file's identity metadata
The system SHALL preserve each replaced file's on-disk identity metadata when
Replace All rewrites it through the shared atomic write path. A file rewritten by
Replace All MUST keep its prior permission (mode) bits and SHALL keep ownership,
POSIX ACLs, and extended attributes on a best-effort basis, matching the
guarantee for in-editor saves and undo restores.

#### Scenario: Replacing inside a restrictive file keeps its permissions
- **WHEN** Replace All rewrites a `0600` file in the workspace
- **THEN** the rewritten file on disk is still `0600`

#### Scenario: Undo restore keeps the file's permissions
- **WHEN** the user undoes a Replace All that rewrote an executable file
- **THEN** the restored file on disk is still marked executable

### Requirement: Replace All validates stale search results per file before writing
The system SHALL verify that each candidate file still matches the searched line content before applying replacements for that file. If the file has changed since the search result was produced, the system MUST skip that file rather than partially applying stale replacements.

#### Scenario: Stale search result skips the whole file
- **WHEN** Replace All reaches a file whose current line content no longer matches the recorded search result
- **THEN** the system skips replacement for that file
- **AND** it does not partially apply a subset of that file's stale replacements

### Requirement: Cancelling Replace All rolls back already-applied file writes
The system SHALL attempt to restore already-written files to their original content when cancellation interrupts a Replace All run. The user SHALL be able to cancel a run in flight: while a run is in flight, the Replace Preview confirm control SHALL become a Cancel Replace All control, and `win.cancel-search-panel-replacements` SHALL request the same cancellation. Cancellation SHALL be observed at the next chunk boundary. The file in progress MUST be left untouched (its durable write fails before rename). Files already written MUST be restored by streaming their original bytes from their journal bodies, with the same digest and identity checks as Undo, and MUST NOT require the disk reserve. If rollback cannot restore every file, the journal MUST be shrunk to the remaining files, stay active, and offer Undo for them. A run whose worker is still waiting for the journal guard MUST observe cancellation there and end without touching a file. Closing the window or quitting the application during a run SHALL ask whether to Keep Running, Close When Finished, or Stop and Close (cancel and roll back). Any close SHALL wait for the run's terminal.

#### Scenario: Cancellation restores earlier files
- **WHEN** Replace All is cancelled after one or more files were already updated
- **THEN** the system restores those already-updated files from the preserved original bytes
- **AND** cancellation does not leave a partially applied multi-file replace result when rollback succeeds

#### Scenario: User cancels a long run in the middle of a large file
- **WHEN** the user activates Cancel Replace All while the run is streaming the third of five files
- **THEN** the third file keeps its original bytes and no temp file remains beside it
- **AND** the first two files are restored from their bodies and the journal is cleared

#### Scenario: Rollback cannot restore one file
- **WHEN** a rollback finds that one already-written file was edited by another program after it was replaced
- **THEN** that file is not overwritten, the journal stays active for it, and Undo is offered for it

#### Scenario: Window closes during a run
- **WHEN** the user closes the window while Replace All is running and chooses Stop and Close
- **THEN** the run is cancelled, its written files are rolled back, and the window closes only after the run's terminal
- **AND** every file holds its original bytes, or its replaced bytes with an active undo entry

#### Scenario: Close when finished
- **WHEN** the user closes the window during a run and chooses Close When Finished
- **THEN** the run completes, its undo journal stays active, and the window then closes through the normal close-safety path

### Requirement: Successful Replace All persists a temporary undo backup for same-session recovery
The system SHALL persist original file bytes for a successful Replace All run to `$XDG_DATA_HOME/lushtext/replace-backup.json` so the user can undo that replace within the same safety window.

#### Scenario: Successful Replace All stores an undo backup
- **WHEN** Replace All completes successfully for one or more files
- **THEN** the system persists an undo backup containing the original file bytes
- **AND** the UI exposes an immediate undo path for that replace result

#### Scenario: Undo restores the original file bytes
- **WHEN** the user invokes the Replace All undo action while the undo backup is still available
- **THEN** the system restores the affected files from the persisted original bytes
- **AND** the replacement result is reversed without requiring the user to re-run search first

### Requirement: Replace All undo backup lifetime is bounded to the active safety window
The system SHALL treat `replace-backup.json` as temporary safety state rather than durable user history. The persisted backup MUST be cleared when the search panel closes, when undo completes, and when a later app session starts with stale backup state from an older run.

#### Scenario: Closing the search panel clears the persisted undo backup
- **WHEN** the search panel is closed after a replace run created an undo backup
- **THEN** the persisted undo backup is deleted
- **AND** the replace undo path does not outlive the panel-close safety boundary

#### Scenario: Startup clears stale replace backup from an earlier session
- **WHEN** the app starts and stale persisted Replace All backup data exists from an earlier session
- **THEN** the stale backup is deleted during startup
- **AND** the new session does not inherit an old Replace All undo state

### Requirement: Replace All uses stable target write coordination
The system SHALL coordinate Replace All writes and undo restores through the same stable target write guard used by file-backed editor saves. Replace All MUST acquire the guard before reading a target file's original bytes and MUST keep the guard through the corresponding atomic write. Undo restore MUST acquire the same guard before checking current bytes and restoring originals.

#### Scenario: Replace All waits for in-progress save
- **WHEN** Replace All reaches a file that an editor save is currently writing
- **THEN** Replace All waits for the save's stable target guard
- **AND** it validates the file contents after the save finishes before applying replacements

#### Scenario: Undo waits for in-progress write
- **WHEN** Replace All undo targets a file that another in-app write is currently replacing
- **THEN** undo waits for the same stable target guard
- **AND** it does not restore stale bytes over an in-flight write

### Requirement: Replace All enforces file and undo memory caps
The system SHALL bound Replace All resources with explicit service-level limits.

- **Per-file limit.** A target file larger than `MAX_REPLACE_FILE_BYTES`, or one whose replacement output would be larger, MUST be skipped and reported with the reason `LargerThanOpenTier`, and the report MUST name the limit in IEC units. `MAX_REPLACE_FILE_BYTES` is `DocumentScale::REFUSE_TO_OPEN`, 512 MiB, the largest file LushText opens; it is referenced, not copied.
- **Undo data.** Undo data is no longer held in memory. A file's undo body and its rewritten output MUST each be admitted against the free space of their filesystems plus `REPLACE_DISK_RESERVE_BYTES` (1 GiB), except for a file that `main`'s admission rule would have admitted. Files that are not admitted MUST be skipped and reported with the reason `NotEnoughDiskSpace` before they are modified. Undo and cancellation rollback MUST need only the free space their writes take, never the reserve.
- **GTK-side owner.** The undo descriptor owner that crosses to GTK MUST stay within `MAX_REPLACE_UNDO_RETAINED_BYTES`.

#### Scenario: Oversized replace target is skipped
- **WHEN** Replace All includes a matching file larger than the per-file replace cap
- **THEN** the file is skipped without modification
- **AND** the result reports that path as skipped

#### Scenario: Undo payload cap skips later files before write
- **WHEN** admitting another file's undo body or output would leave less than the reserve free, and `main`'s admission rule would not have admitted that file
- **THEN** that file is skipped before any replacement write occurs
- **AND** already-written files remain undoable

#### Scenario: A 400 MiB file is replaced
- **WHEN** Replace All includes a 400 MiB UTF-8 file with ample free disk space
- **THEN** the file is replaced and is undoable
- **AND** the worker's memory peak does not grow with the file's size

#### Scenario: Replacement output above the open tier
- **WHEN** a file within the per-file limit would grow past it after its replacements
- **THEN** the file is skipped without modification and listed as larger than 512 MiB, the largest file LushText opens

#### Scenario: Undo with little free space
- **WHEN** the user runs Undo with less free space than the reserve but enough for the restored files' writes
- **THEN** every file is restored, as on `main`

### Requirement: Replace All builds changed text without full line-vector amplification
The system SHALL avoid constructing a full per-line collection of owned strings, byte ranges, or equivalent metadata during Replace All. It SHALL also avoid holding a whole file, or the whole replacement output, in memory.

- **Before reading.** Before line discovery it MUST validate the replacement count and the recorded range bounds.
- **Chunked source.** It MUST read the source in chunks of `REPLACE_STREAM_CHUNK_BYTES` and validate UTF-8 incrementally, carrying an incomplete trailing sequence into the next chunk.
- **One source-order pass.** It MUST build the replacement output in one source-order pass from sorted recorded replacements, carrying a CRLF split across a chunk boundary and streaming any line longer than a chunk without buffering it.
- **Retained memory.** Only a line that a replacement targets may be buffered, and that line is at most `MAX_SEARCH_MATCH_LINE_BYTES` long. Retained edit metadata MUST remain proportional to accepted replacements rather than total source-line count.

#### Scenario: Large accepted file avoids line-vector allocation
- **WHEN** Replace All processes a file within the per-file cap but large enough to stress allocation
- **THEN** it does not split or index the entire file into a per-line vector
- **AND** it still validates stale search results before writing

#### Scenario: Dense short-line file stays within retained metadata bounds
- **WHEN** an accepted file near the byte cap contains millions of short lines but no more than the configured replacement-count limit
- **THEN** line discovery streams only the boundaries needed by the sorted replacements
- **AND** retained line or edit metadata remains bounded by accepted replacement count rather than source-line count

#### Scenario: Streaming construction preserves line semantics
- **WHEN** replacements target LF, CRLF, final unterminated, Unicode, and empty lines
- **THEN** streaming construction produces the same changed bytes and stale-line decisions as the reference behavior
- **AND** durable journal-before-mutation and cancellation ordering remain unchanged

#### Scenario: Chunk seams do not change the result
- **WHEN** a chunk boundary falls inside a multi-byte UTF-8 sequence, between the `\r` and `\n` of a CRLF, or inside a targeted line
- **THEN** the output, the stale-line decisions and the digests equal those of the whole-buffer reference for every chunk split

### Requirement: Replace All undo journal is incremental and durable per file
The system SHALL persist Replace All undo state as incremental per-file durable entries rather than rewriting the entire growing backup after each file. Before a file is modified, the following MUST all be durable:

- a raw body file holding that file's original bytes, in the journal's `bodies/` subdirectory;
- an entry of kind `replace-all-undo-stream-entry` recording the body file name, the length and SHA-256 digest of both the original and the replaced bytes, and the target's identity facts;
- the armed incremental manifest.

The replaced bytes MUST NOT be persisted or retained. The journal MUST be prepared, removing any previous run's journal, before the run writes its first body. Entries MUST be removed before their bodies, so no entry references a missing body. A body without an entry MUST NOT deactivate the journal; cleanup removes it. A journal holding only descriptors MUST only ever be shrunk, never rebuilt from memory. Cleanup on undo, search-panel close, and startup MUST remove the journal directory, its bodies, and any stale legacy backup file.

#### Scenario: Per-file entry is durable before replacement
- **WHEN** Replace All is about to rewrite a target file
- **THEN** that file's undo body and entry are durably persisted first
- **AND** the workflow does not rewrite previously persisted entries

#### Scenario: Journal persistence scales linearly
- **WHEN** Replace All successfully rewrites many files
- **THEN** the number of durable journal entry writes grows linearly with the number of touched files
- **AND** the workflow does not serialize the full backup once per file

#### Scenario: Startup clears stale journal formats
- **WHEN** the app starts with stale Replace All undo state from a prior session
- **THEN** it deletes the new journal directory and the legacy `replace-backup.json` file
- **AND** the new session does not inherit old Replace All undo state

#### Scenario: The journal stores the original once
- **WHEN** Replace All rewrites a 100 MiB file
- **THEN** the journal holds one body of 100 MiB and a small entry, and no copy of the replaced bytes

#### Scenario: A crash before an entry keeps earlier files undoable
- **WHEN** LushText is killed after the fourth file's body is written but before its entry, and relaunched
- **THEN** Undo is offered for the three files already replaced, and the fourth body is removed by cleanup

#### Scenario: Rewriting reads the body it saved
- **WHEN** Replace All writes a target's new bytes
- **THEN** they are generated from the journal body, and the rename happens only if the body's digest equals the recorded original digest and the output's digest equals the digest computed while the body was written

### Requirement: Replace All undo-journal corruption is isolated and diagnostic
The system SHALL handle malformed, partial, or unsupported Replace All undo-journal state as recoverable metadata. Corrupt journal entries MUST be preserved when possible, reported through recovery diagnostics, and MUST NOT be treated as a valid undo source.

#### Scenario: Malformed journal entry is preserved and excluded
- **WHEN** startup or search-panel initialization finds a malformed Replace All undo-journal entry
- **THEN** the malformed entry is quarantined or left untouched when quarantine fails
- **AND** the entry is not offered as an undo source
- **AND** a recovery diagnostic is recorded

#### Scenario: Partial journal does not imply successful undo state
- **WHEN** an undo-journal directory contains only some required metadata for a Replace All run
- **THEN** the system does not expose the Replace All undo action for that incomplete run
- **AND** surviving journal files are preserved or cleaned only after diagnostic handling succeeds

#### Scenario: Legacy backup corruption is reported during cleanup
- **WHEN** stale legacy `replace-backup.json` exists but cannot be parsed or removed at startup
- **THEN** the system records a diagnostic with the backup path and failure kind
- **AND** the new session does not claim that stale Replace All undo is available

### Requirement: Replace All journal cleanup is restart-safe
The system SHALL make cleanup of stale, completed, or invalid Replace All undo-journal state restart-safe. Cleanup MUST be ordered so that a crash during cleanup cannot convert a stale or corrupt journal into an active undo affordance after restart.

#### Scenario: Cleanup marker prevents resurrecting stale undo
- **WHEN** startup begins cleaning a stale Replace All journal and the process terminates before cleanup completes
- **THEN** relaunching LushText continues cleanup or reports the incomplete cleanup
- **AND** it does not expose the stale journal as an active undo affordance

#### Scenario: Undo completion removes journal durably
- **WHEN** the user successfully undoes a Replace All run
- **THEN** the journal is removed or marked inactive through a durable cleanup path
- **AND** restarting LushText cannot offer that completed undo again

#### Scenario: Cleanup failure remains diagnostic
- **WHEN** Replace All journal cleanup fails because of filesystem permissions or transient I/O errors
- **THEN** the system reports a diagnostic and retries later
- **AND** it does not spin in a tight retry loop

### Requirement: Replace All journal reliability has layered automated coverage
The project SHALL add deterministic service, integration, smoke, and property or fuzz-adjacent coverage for malformed undo journals, partial journals, restart-safe cleanup, and stale legacy backup handling.

#### Scenario: Service tests cover malformed journal entries
- **WHEN** service tests load malformed Replace All journal entries
- **THEN** the loader excludes those entries from active undo state
- **AND** it preserves or quarantines the original bytes and returns diagnostics

#### Scenario: Integration tests cover cleanup interruption
- **WHEN** tests simulate a crash or restart between stale journal cleanup steps
- **THEN** the next startup continues cleanup or reports incomplete cleanup
- **AND** the stale journal is never exposed as active undo state

#### Scenario: Generated journal states never produce invalid undo
- **WHEN** generated partial, duplicated, or corrupt journal directory states are loaded
- **THEN** the system never offers undo for a journal that lacks complete validated original bytes

#### Scenario: Crash smoke records stale journal handling when exercised
- **WHEN** crash recovery smoke creates or encounters Replace All journal state
- **THEN** the smoke artifacts record whether the journal was active, cleaned, quarantined, or skipped

### Requirement: Replace Preview enforces row and byte budgets
The system SHALL generate Replace Preview through explicit service-level limits of at most 16,384 generated rows (a presentation cap rounded up from 10,000 by `power-of-two-thresholds`) and `64 * 1024 * 1024` conservatively charged preview bytes. The generator MUST use saturating accounting, MUST admit only complete preview rows, and MUST return a typed outcome that distinguishes generated, omitted-eligible, source-truncated, and otherwise invalid matches.

#### Scenario: Preview remains within both limits
- **WHEN** all eligible search matches fit within the row and byte budgets
- **THEN** the outcome contains every eligible complete preview row
- **AND** the omitted-eligible count is zero

#### Scenario: Next row would exceed the byte budget
- **WHEN** admitting another complete preview row would exceed the byte budget
- **THEN** the generator stops admitting rows before exceeding the limit
- **AND** that match and later eligible matches are counted as omitted
- **AND** no partially constructed row is exposed

#### Scenario: Search reaches the row budget
- **WHEN** more eligible matches exist than the preview row budget permits
- **THEN** the outcome contains at most the configured row count
- **AND** the remaining eligible matches are reported as omitted

#### Scenario: Truncated source line remains ineligible
- **WHEN** a search match contains only a bounded source-line excerpt
- **THEN** Replace Preview does not generate an apply-capable row from that excerpt
- **AND** the outcome distinguishes it from rows omitted solely by the preview budget

### Requirement: Omitted preview matches cannot be applied implicitly
The system SHALL allow confirmation only for checked rows generated by the current accepted preview outcome. Matches omitted by row or byte limits, skipped source rows, stale generations, and unchecked rows MUST NOT enter the apply request.

#### Scenario: Confirm a truncated preview subset
- **WHEN** the current preview reports generated rows plus omitted eligible matches and the user confirms all generated rows
- **THEN** the apply request contains only those generated rows
- **AND** omitted matches are not applied
- **AND** the UI does not describe the operation as replacing every search match

#### Scenario: User unchecks generated rows
- **WHEN** the user unchecks some rows in a bounded preview and confirms
- **THEN** only the still-checked generated rows are passed to Replace All
- **AND** omitted and unchecked rows remain untouched

#### Scenario: Search changes before confirmation
- **WHEN** query, replacement, options, results, or panel generation changes after a preview was generated
- **THEN** the old preview and checked state are invalidated together
- **AND** the stale rows cannot be confirmed

### Requirement: Replace Preview reuses immutable data without changing semantics
The system SHALL avoid per-row duplication of immutable original-line and literal replacement data where shared ownership is safe. Regex-expanded replacement text MUST remain row-specific, and every accepted row MUST retain enough complete original and replacement data for stale validation and apply correctness.

#### Scenario: Literal replacement is shared across many rows
- **WHEN** a literal replacement preview contains many accepted matches using the same replacement text
- **THEN** the immutable literal replacement is stored once and shared by those rows
- **AND** applying any selected row uses the exact literal text

#### Scenario: Regex captures expand differently
- **WHEN** regex replacement captures produce different text for different matches
- **THEN** each accepted preview row retains its own expanded replacement
- **AND** shared-storage optimization does not collapse distinct expansions

#### Scenario: Original lines remain valid stale snapshots
- **WHEN** accepted preview rows share original-line storage with the search cache
- **THEN** every row still compares against the complete original line before file mutation
- **AND** the existing whole-file stale-result skip behavior remains intact

### Requirement: Preview rows use stable direct identity
The system SHALL assign each streamed search match a stable identity scoped to its search generation and SHALL resolve preview display, checked state, and activation by direct identity lookup. GTK row binding and checkbox activation MUST NOT linearly scan all preview rows or allocate display-path strings to rediscover a match.

#### Scenario: Bind a preview row in a dense result list
- **WHEN** GTK binds any generated preview row among thousands of results
- **THEN** it resolves that row through one bounds-checked identity lookup
- **AND** binding cost does not grow linearly with total preview rows

#### Scenario: Toggle a preview checkbox
- **WHEN** the user toggles a generated preview row's checkbox
- **THEN** checked state is updated by the row's stable match identity
- **AND** the action does not search by path, line number, and byte range

#### Scenario: New search reuses numeric identities
- **WHEN** a later search generation assigns identities that overlap old numeric values
- **THEN** the old generation's mapping and checked state cannot affect the new rows
- **AND** all accepted state remains generation-scoped

### Requirement: Bounded preview feedback remains readable and accessible
The Replace Preview UI SHALL expose generated, checked, omitted, and skipped state without fake result rows. Search controls, omission feedback, row checkboxes, and confirmation MUST remain reachable in empty, representative, many-result, awkward-path, and constrained-window states, with only the result-item region scrolling.

#### Scenario: Byte-limited preview shows omission feedback
- **WHEN** preview generation stops at the byte budget
- **THEN** the panel visibly reports how many eligible matches were omitted
- **AND** the confirmation control describes the checked generated subset
- **AND** equivalent state is available to accessibility and automation consumers

#### Scenario: No eligible preview rows
- **WHEN** every search row is source-truncated or invalid for safe preview
- **THEN** the panel shows an explicit no-eligible-preview state
- **AND** confirmation is disabled
- **AND** search and dismissal controls remain usable

#### Scenario: Dense preview in constrained geometry
- **WHEN** many generated rows with long Unicode names and deep paths are shown in a narrow window
- **THEN** row text remains bounded within the panel
- **AND** no horizontal scrollbar is introduced
- **AND** controls and omission feedback do not scroll away with result items

### Requirement: Replace Preview resource policy has layered coverage
The project SHALL add pure service, property, widget, accessibility, visual-geometry, and scale tests for zero matches, representative literal and regex matches, exactly-at-limit inputs, 16,384 matches (the result cap, a presentation cap rounded up from 10,000), large replacement text, stale generations, awkward paths, Unicode, and constrained layouts.

#### Scenario: Scale fixture avoids quadratic row binding
- **WHEN** a performance fixture binds and toggles rows in a 16,384-match preview
- **THEN** identity resolution remains constant-time per row
- **AND** the fixture detects a regression to full-preview scans

#### Scenario: Property tests preserve apply equivalence
- **WHEN** generated literal and regex cases fit within the preview budget
- **THEN** applying accepted rows produces the same file text as the existing safe replacement semantics
- **AND** storage sharing does not change ranges, captures, or stale validation

### Requirement: Search diagnostics never contain private document text
Search and Replace Preview diagnostics, warnings, automation state, and typed failures MUST NOT include document substrings, complete match text, replacement expansions, or other buffer contents. Invalid preview rows SHALL be represented by bounded counts and non-content reason classes.

#### Scenario: Regex no longer matches an extracted range
- **WHEN** Replace Preview cannot re-match a recorded range
- **THEN** the outcome increments a typed invalid-row reason without logging the original substring
- **AND** UI feedback may report only bounded counts and non-private metadata

#### Scenario: Diagnostic logging is enabled at default level
- **WHEN** search warnings are written to stderr or the user-session journal
- **THEN** messages contain no matched or surrounding document text
- **AND** file paths or line numbers are included only when required by the existing diagnostic policy

#### Scenario: Invalid rows coexist with valid preview rows
- **WHEN** a preview contains both valid replacements and invalid stale ranges
- **THEN** confirmation still includes only valid current rows
- **AND** the invalid summary reveals no private source or replacement contents

### Requirement: Workspace search owns one worker group and one latest request
The search panel SHALL own at most one active workspace-search controller/walker group and one latest pending compact request. A superseding query MUST cancel the active generation and replace the pending request, but MUST NOT launch another worker group until the active result stream reaches a terminal disconnected state.

#### Scenario: User types several queries rapidly
- **WHEN** several newer valid queries arrive before the active workspace search observes cancellation
- **THEN** the panel retains only the latest pending compact request
- **AND** no overlapping replacement controller/walker group is started

#### Scenario: Cancelled search disconnects
- **WHEN** the active search reaches its cancelled or disconnected terminal state
- **THEN** the panel revalidates and starts the latest pending request, if any
- **AND** intermediate superseded requests never consume traversal workers

#### Scenario: Panel closes with active and pending search
- **WHEN** the panel lifetime ends while one search is active and another is pending
- **THEN** the active search is cancelled and the pending request is discarded
- **AND** neither generation can later publish results or readiness state

### Requirement: Accepted search matches have immutable generation identity
The system SHALL seal each accepted search result set into one immutable generation-owned snapshot shared by list projection, Replace Preview, checked-row state, and apply planning. Sharing MUST preserve stable match identity, preview budgets, explicit selection, and stale-file validation without copying the whole match vector on GTK.

#### Scenario: Replace Preview uses a shared result snapshot
- **WHEN** Replace Preview begins for the current accepted search generation
- **THEN** it references the same immutable match snapshot used by current result identity
- **AND** building the preview does not duplicate every `SearchMatch` on the GTK thread

#### Scenario: Search generation changes during preview construction
- **WHEN** a newer search result snapshot is accepted before the old preview completes
- **THEN** the old snapshot may remain alive only for its bounded in-flight owner
- **AND** its completion cannot replace, check, or apply matches in the newer generation

### Requirement: Replace Preview confirmation and retirement stay payload-bounded
Replace Preview SHALL keep current checked-match identity incrementally and SHALL NOT scan, filter, or synchronously destroy a near-limit preview outcome in the GTK confirmation path. Confirmation MUST detach the current immutable outcome and checked identity set, partition selected replacements away from GTK, retire unchecked or rejected payloads away from GTK, and invoke Replace All only with rows selected from the still-current preview and search generation. Replaced, invalidated, stale, and exited preview state MUST use the applicable bounded retirement path rather than final document-sized destruction on GTK.

#### Scenario: User confirms a near-limit checked subset
- **WHEN** the current preview contains near-limit replacement data and only a subset of stable match identities remains checked
- **THEN** the GTK action captures current identity without filtering the full outcome
- **AND** worker processing returns only the checked generated replacements to the normal Replace All callback
- **AND** unchecked replacement payloads are destroyed away from GTK

#### Scenario: Preview changes during confirmation selection
- **WHEN** query, replacement, search result, preview, or panel generation changes while worker-side selection is active
- **THEN** the selected stale rows are not passed to Replace All
- **AND** their payload is retired without changing the newer preview

#### Scenario: Entering a new preview replaces a visible outcome
- **WHEN** a new preview request starts while a prior near-limit preview outcome is visible
- **THEN** the prior outcome and checked identity detach from current state immediately
- **AND** their GTK-owned projection and plain-data payload follow their bounded retirement paths

#### Scenario: All generated rows are unchecked
- **WHEN** every generated preview row is unchecked before confirmation
- **THEN** no replacement enters the apply callback
- **AND** the full rejected outcome is retired away from GTK

### Requirement: Replace All undo payload accounting is exact across failures
The live Replace All undo-byte counter SHALL equal the body bytes of entries currently retained by the journal, and the live descriptor-weight counter SHALL equal the weight of descriptors retained by the in-memory undo owner. A pre-rename failure that removes an undo entry MUST remove its body and reclaim its disk admission and descriptor charge, while an ambiguous after-rename durability failure MUST retain the entry, the body and both charges. High-water metrics MUST remain monotonic observations and MUST NOT serve as the live counter.

#### Scenario: First target fails before rename
- **WHEN** the first deterministically ordered target is admitted and journaled but its durable write fails before rename
- **THEN** the target remains unchanged and its in-memory and active-journal undo entry are removed
- **AND** the exact payload charge is reclaimed for later targets

#### Scenario: A later target fits reclaimed capacity
- **WHEN** each of two entries fits alone but not together and the first entry is removed after a pre-rename failure
- **THEN** the second valid target is admitted and replaced
- **AND** it is not falsely rejected by stale undo-byte accounting

#### Scenario: Durability fails after rename
- **WHEN** replacement reaches the after-rename ambiguous-durability state
- **THEN** its undo entry, journal evidence, and live payload charge remain retained for recovery
- **AND** cancellation rollback and journal-before-mutation ordering remain intact

### Requirement: Undo reads enforce the file cap during ingestion
Replace All undo SHALL stream the current target through the filesystem boundary, bounded by `MAX_REPLACE_FILE_BYTES`, to compute its length and SHA-256 digest. It MUST then decide:

- a file whose length and digest equal the original's is already restored;
- a file whose length and digest equal the replaced bytes' is restored by streaming the body, and only if the body's digest equals the original's and the target's identity facts did not change during the check;
- any other file is skipped and stays retryable.

Metadata MAY be used as an early hint but MUST NOT be the bound. A target that exceeds the cap during the stream MUST receive no restore write and MUST remain retryable in the backup owner.

#### Scenario: Target grows after metadata planning
- **WHEN** a target is within the cap during metadata planning but grows beyond it before or during ingestion
- **THEN** the bounded stream reads no more than the configured limit and performs no restore write
- **AND** the target is classified as skipped and remains in remaining backup state

#### Scenario: Target is exactly at the limit
- **WHEN** the current target body contains exactly MAX_REPLACE_FILE_BYTES
- **THEN** undo proceeds through ordinary content and identity comparison
- **AND** it is not rejected solely for reaching the exact limit

#### Scenario: Bounded read encounters I/O failure
- **WHEN** target ingestion fails for an I/O reason other than the size limit
- **THEN** undo reports a bounded failure classification
- **AND** the target remains retained for a later retry

### Requirement: Replace and Undo completion metadata is bounded
Replace All and Undo SHALL return exact aggregate counts plus a deterministic diagnostic sample containing at most 32 entries and 32 KiB of retained path and message bytes per result.

Replace All SHALL also return a `ReplaceSkipReport`:

- exact counts per `ReplaceSkipReason`;
- a list of skipped files (path, reason, first skipped line) in deterministic order, bounded by `MAX_REPLACE_SKIP_LIST_BYTES` of charged path bytes;
- a count of skipped files past that bound that are not listed.

Only affected paths required to reconcile currently open tabs MAY cross to GTK as a complete path set. Failure-heavy, skipped-heavy, rollback-heavy, all-failed, and long-path workloads MUST NOT return metadata beyond these bounds or join every error into one message.

#### Scenario: Ten thousand targets fail or are skipped
- **WHEN** Replace All or Undo processes a failure-heavy set at the supported result cap
- **THEN** exact success, failure, skip, and remaining counts are reported
- **AND** diagnostic messages and paths retain at most 32 samples and 32 KiB in deterministic order
- **AND** the skip list holds every skipped file whose path fits the skip-list bound and states how many are not listed

#### Scenario: Some affected files are open
- **WHEN** worker completion needs to refresh open editor tabs
- **THEN** it returns only the intersection between affected files and the worker's immutable open-tab identity snapshot
- **AND** closed-file paths are represented by counts or the bounded diagnostic sample

#### Scenario: Completion is stale
- **WHEN** a large completion loses generation or window freshness before GTK accepts it
- **THEN** its plain metadata owner is retired off GTK
- **AND** no stale open-tab refresh or status projection occurs

### Requirement: In-tab Replace is unavailable in Protected View
In Protected View the in-tab search bar SHALL hide or disable its replace row and the replace actions. `win.begin-replace` (Ctrl+H) SHALL open find only and publish the Protected View notice. No `GtkSourceSearchContext` SHALL be attached to the protected buffer. Workspace Replace All SHALL keep its existing skip rules for open documents: a modified open document, including one that entered Protected View through an edit, is skipped. When Replace All rewrites the file of an open, unmodified protected document, the existing automatic reload of that tab SHALL install the new content through the fold-at-insertion executor and re-derive the Protected View decision.

#### Scenario: Ctrl+H on a protected document
- **WHEN** the user presses Ctrl+H with a protected document active
- **THEN** the find bar opens without a replace row, no search context exists on the buffer, and the notice is shown

### Requirement: Replace All never acts on matches past the search budget
When a workspace search was limited by its retained-byte budget, Replace Preview and Replace All SHALL operate only on the retained matches, within Replace All's own unchanged safety budget (preview rows, preview bytes, per-file and undo caps). The preview summary SHALL state how many matches past the search limit are not included. No match that the user was not shown SHALL ever be replaced. The Replace All journal's startup scan bound (`JOURNAL_SCAN_MAX_ENTRIES`) SHALL be asserted at compile time to be at least `MAX_REPLACE_PREVIEW_ROWS`, the most files one run can touch.

#### Scenario: Replace All after a limited search
- **WHEN** the user enters Replace Preview after a search that found more matches than it retained
- **THEN** the preview lists only retained matches, subject to the preview's own limits
- **AND** its summary states the number of matches past the search limit that are not included
- **AND** confirming replaces only checked preview rows

#### Scenario: A search without a limit is unchanged
- **WHEN** the search retained every match it found
- **THEN** the preview summary carries no search-limit clause

#### Scenario: More retained matches than the preview admits
- **WHEN** an unlimited search retained more matches than Replace Preview's row or byte budget admits
- **THEN** the preview lists the admitted prefix and states the omitted count through its existing summary
- **AND** confirming replaces only checked preview rows, never an omitted match

### Requirement: In-tab Replace All uses the safe GtkSourceView binding
The in-tab search bar's Replace All SHALL call GtkSourceView through the safe `sourceview5` binding, which returns the number of matches it replaced. A match-free Replace All SHALL be a successful no-op. A `GError` SHALL be logged, never unwrapped, under the search-diagnostics requirement. The match counter SHALL be refreshed in every case, and the call SHALL use no `unsafe` code.

#### Scenario: Replace All with no match is a no-op
- **WHEN** the user runs Replace All in the in-tab search bar and the query matches nothing in the document
- **THEN** the document is unchanged and the application does not panic
- **AND** the match counter still reflects zero matches

#### Scenario: Replace All replaces every match
- **WHEN** the user runs Replace All and the query matches several places
- **THEN** every match is replaced and the counter shows no remaining matches

#### Scenario: A GtkSourceView error leaves the document and counter consistent
- **WHEN** GtkSourceView reports a `GError` for the Replace All, for example for a malformed regular-expression replacement
- **THEN** the document is unchanged and the application does not panic
- **AND** the match counter is refreshed

### Requirement: Replace All streams every file up to the open tier in constant memory
Replace All SHALL process each file with a working set independent of its size: chunk buffers, at most one targeted line, digest states, and edit records proportional to accepted replacements. Pass A SHALL stream the target into its undo body while dry-running the replacement into a digest. Pass B SHALL stream the body through the replacement into the target's durable write. The durable-write protocol core SHALL NOT change.

#### Scenario: Memory is flat across file sizes
- **WHEN** Replace All rewrites one 16 MiB file and, separately, one 512 MiB file, with the same replacements
- **THEN** the worker's peak memory growth for the 512 MiB file is within the growth for the 16 MiB file times 1.05 plus 1 MiB

### Requirement: GTK retains only undo descriptors
The undo state that crosses to GTK SHALL hold descriptors only: the path, the body name, the lengths, the digests and the identity facts. It SHALL never hold file content.

#### Scenario: GTK holds no file bytes for undo
- **WHEN** Replace All completes over files totalling 1 GiB
- **THEN** the undo owner retained on GTK holds no file content, and its weight is proportional to the number of files

### Requirement: Replace All refuses a target changed during the run
Replace All SHALL record a target's identity facts (size, modification time in nanoseconds, device and inode) when it acquires the target's write guard. It SHALL refuse to commit the file when those facts differ at the end of pass A or at the end of pass B. A refused file MUST keep its current bytes. It MUST be skipped with the reason `ChangedDuringReplace`, and its undo body and entry MUST be removed.

#### Scenario: Another program appends to the file during pass B
- **WHEN** another program appends to a 200 MiB target while Replace All is writing its new bytes
- **THEN** no rename happens, the file keeps the other program's bytes, and it is listed as changed while replacing

### Requirement: Replace All loses no file that main replaced
Disk admission SHALL waive the reserve for every file `main` would have replaced. A pure ledger SHALL replay `main`'s rule: two passes in path order, at `MAIN_REPLACE_FILE_BYTES` then `MAIN_MAX_REPLACE_FILE_BYTES` for source and output; each admitted file charges source plus output against `MAIN_REPLACE_UNDO_BYTES` and its entry weight against `MAX_REPLACE_UNDO_RETAINED_BYTES`; a pre-rename failure reclaims both. Processing SHALL keep the two passes, the second up to the per-file limit.

#### Scenario: Little free space
- **WHEN** a Replace All run that `main` replaced in full runs with less free space than the reserve, but enough for its writes
- **THEN** every file is replaced, as on `main`

#### Scenario: A small file whose output grows past 10 MiB
- **WHEN** a 9 MiB file's replacements make it 11 MiB, and it sorts before 10 MiB files that `main` replaced within the 64 MiB undo payload
- **THEN** it is processed in the second pass, as on `main`, and every file `main` replaced is replaced

#### Scenario: A large file early in path order
- **WHEN** a 300 MiB file sorts before small files that `main` replaced, and the disk cannot hold the 300 MiB file's body plus the reserve
- **THEN** the 300 MiB file is listed as skipped for disk space and every small file is replaced

### Requirement: Every skipped file carries one typed reason
Every file a Replace All run does not replace SHALL carry exactly one `ReplaceSkipReason`: `UnsavedChanges`, `ChangedSinceSearch`, `ChangedDuringReplace`, `NotUtf8`, `LineTooLong`, `InvalidPreview`, `LargerThanOpenTier`, `NotEnoughDiskSpace`, `Unreadable`, `WriteFailed` or `UndoJournalFailed`. Counts per reason SHALL be exact, and partly replaced files SHALL be counted apart from skipped ones.

#### Scenario: A file skipped at preview time
- **WHEN** a file's only matches are on lines longer than the search excerpt
- **THEN** the run reports that file once, with the reason `LineTooLong`

### Requirement: A file with two skip reasons carries the first
A file that qualifies for two skip reasons SHALL carry the first that applies in this order: `UnsavedChanges`; then `LineTooLong` and `InvalidPreview`, for a file none of whose matches was eligible; then the worker's reasons in the order it checks them.

#### Scenario: An unsaved file with only long-line matches
- **WHEN** a file open with unsaved changes has only matches on lines longer than the search excerpt
- **THEN** the run reports it once, with the reason `UnsavedChanges`

### Requirement: The skip report includes partly replaced files and all-unsaved runs
A replaced file whose matches on long lines were not replaced SHALL be listed as partly replaced, with the count of matches left. A run whose files are all open with unsaved changes SHALL still produce the report, without starting a worker.

#### Scenario: A file partly replaced
- **WHEN** a file has three matches on short lines and one on a 10 KiB line
- **THEN** three matches are replaced, and the file is listed as partly replaced with `LineTooLong`, one match left, and the editor action

#### Scenario: Every file has unsaved changes
- **WHEN** every file of a Replace All run is open with unsaved changes
- **THEN** no file is written, and Show Skipped Files lists each file with `UnsavedChanges` and the editor action

### Requirement: A Skipped Files dialog lists the skip report
When a run skips files, the panel's completion state SHALL show the skipped count and a Show Skipped Files control. The control SHALL open a dialog whose virtualized list groups rows by reason, with exact counts. Each row SHALL show the file name, its folder and a reason sentence, and never document content. A final row SHALL state how many skipped files are not listed.

#### Scenario: Three kinds of skip
- **WHEN** a run skips one open file with unsaved changes, one Latin-1 file and one 600 MiB file
- **THEN** the dialog lists three rows under three reason groups, each with its file name, folder and reason
- **AND** the status message reports the replaced count and "3 skipped"

#### Scenario: Thousands of skipped files
- **WHEN** a run skips 16,000 files
- **THEN** the dialog lists every file whose path fits the skip-list bound, the counts per reason are exact, and opening the dialog stays within `B_settle`

### Requirement: Open and replace in editor finishes a skipped file without writing it
A Skipped Files row SHALL offer Open and Replace in Editor when the pure availability policy allows it. `win.replace-in-editor(s)` SHALL open the file through the normal open path, or select its tab, and place the cursor at the first skipped match. It SHALL then reveal the in-tab replace bar, prefilled with the run's query, options and replacement, not the panel's current fields. It SHALL NOT replace or save anything by itself.

#### Scenario: Unsaved open file
- **WHEN** the user activates Open and Replace in Editor on a row skipped for unsaved changes
- **THEN** that file's tab is selected, the replace bar shows the query and the replacement, and the document is unchanged until the user applies the replacement

#### Scenario: The panel was edited after the run
- **WHEN** the user changes the workspace query after a run and then activates Open and Replace in Editor on one of its rows
- **THEN** the replace bar is prefilled with the run's query and replacement, not the edited ones

### Requirement: The editor action is offered only where the editor can replace
The policy SHALL never offer the action for `LargerThanOpenTier` or `Unreadable`. It SHALL NOT offer it for `LineTooLong` when the source line exceeds `4 × L_PROTECT_EDIT` bytes, because that document opens in Protected View, where in-tab Replace is unavailable. It SHALL offer the action for every other reason.

#### Scenario: A file over the open tier
- **WHEN** a row is skipped as larger than the largest file LushText opens
- **THEN** the row offers no editor action and its reason names 512 MiB

#### Scenario: A line certainly in Protected View
- **WHEN** a row is skipped as `LineTooLong` for a 300,000-byte line
- **THEN** the row offers no editor action and says the document opens view-only

### Requirement: Regex replacements are translated for the in-tab bar
In regex mode the prefilled pattern SHALL be the workspace pattern, and the replacement SHALL be translated from Rust `regex` syntax to GtkSourceView syntax. Each reference, read as Rust `regex` reads it (`$name` or `${name}`, where a name of digits is a group number), SHALL become `\g<N>` or `\g<name>`; `$$` SHALL become `$`, and a literal backslash SHALL be escaped. A template naming a group the pattern does not have SHALL leave the replacement empty, with an explanation in the bar.

#### Scenario: Regex replacement with captures
- **WHEN** the workspace query is the regex `(\w+)@example` with replacement `$1@example.org`
- **THEN** the in-tab bar is prefilled with the same pattern, regex mode on, and the replacement `\g<1>@example.org`

#### Scenario: A group number followed by a digit
- **WHEN** the replacement is `${1}0`
- **THEN** the prefilled replacement is `\g<1>0`, which GtkSourceView expands to group 1 followed by `0`

### Requirement: Replace All reports progress for long runs
While a Replace All run is in flight for longer than the search-progress delay (500 ms), the status bar SHALL show its progress: files done of the total, and MiB done of the total. A heartbeat renews the progress lease. The progress SHALL be announced once when it first appears, and the terminal message SHALL replace it.

#### Scenario: A 512 MiB file
- **WHEN** Replace All runs on a 512 MiB file
- **THEN** the status bar shows the MiB processed, rising, until the completion message replaces it

### Requirement: Legacy inline journal entries remain undoable
A journal of the previous entry kind (`replace-all-undo-entry`, with inline original and replaced strings) SHALL still load and undo, bounded by the legacy payload limit of 64 MiB (`MAIN_REPLACE_UNDO_BYTES`). Its descriptors SHALL hold no file content: Undo SHALL re-read one legacy entry at a time on a worker. New runs SHALL write only `replace-all-undo-stream-entry` entries. A manifest that mixes the two kinds SHALL be treated as malformed: it is preserved and offers no undo.

#### Scenario: Crash before an upgrade
- **WHEN** LushText is upgraded after a crash that left an active legacy journal
- **THEN** the relaunched build offers Undo, and Undo restores the original bytes

### Requirement: Recovery removes the crashed run's temp files
The journal manifest SHALL record the process id and launch nonce of the launch that last wrote targets through it. On startup recovery, Undo and discard, the system SHALL remove the temp files beside each journaled target whose names match the exact pattern, the Replace All tag and the recorded (process id, launch nonce) pair, when that pair is not the current launch's. Only the journaled targets' own directories SHALL be read, each once. Every other sweep keeps the 24-hour age rule.

#### Scenario: Killed while writing a 512 MiB file
- **WHEN** LushText is killed during pass B of a 512 MiB file and relaunched
- **THEN** the half-written temp beside the target is removed and the target keeps its original bytes
- **AND** the run's Undo, offered for the files it did replace, counts that file as already restored and writes nothing to it

### Requirement: An Undo in a later launch owns its temp files
Before its first restore write, an Undo that runs in a later launch than the one the manifest records SHALL sweep the recorded pair's temp files and then re-arm the manifest with its own pair. A process id alone SHALL NOT identify a launch, because a relaunch inside a pid namespace can reuse it.

#### Scenario: Killed during an Undo's restore
- **WHEN** LushText is killed while an Undo in a later launch restores a 512 MiB file, and relaunched
- **THEN** that Undo's half-written temp is removed, and Undo is offered again for the files it had not restored

#### Scenario: The relaunch reuses the crashed process's pid
- **WHEN** the relaunched process has the same pid as the launch that crashed
- **THEN** the crashed launch's temp files are still removed, and the current launch's are kept

### Requirement: One Replace All undo journal per application
The undo journal SHALL belong to the application, not to a window. Starting a Replace All in any window SHALL retire the undo affordance in every window, and a window that loses one SHALL say so once. While a Replace All, its rollback or an Undo is in flight in any window, an apply or Undo in another window SHALL be refused with a status message rather than queued.

#### Scenario: Another window runs Replace All
- **WHEN** window A offers Undo for a Replace All and the user runs a Replace All in window B
- **THEN** window A no longer offers Undo and shows one status message saying the earlier Undo is no longer available

#### Scenario: Apply while another window's run is in flight
- **WHEN** a Replace All is running in window B and the user confirms one in window A
- **THEN** window A's run is refused with a status message, and no file is touched by it

### Requirement: A confirmed Replace All waits for undo-reservation capacity
When a confirmed Replace All cannot reserve its undo-descriptor memory because other work holds the shared disposal capacity, the system SHALL keep the confirmed selection and the claimed transaction and SHALL start the run once capacity allows. It MUST NOT discard the selection or ask the user to preview again because of that refusal.

#### Scenario: Confirm while a file-index build holds the lane
- **WHEN** the user confirms a Replace All of three files while a workspace file-index build holds the disposal capacity the undo reservation needs
- **THEN** no file is touched while the build holds it
- **AND** once the build releases it, the run starts without another preview or Confirm, and every file holds the replaced text and is undoable

#### Scenario: A file becomes unsaved while the run waits
- **WHEN** a confirmed run is waiting for capacity and one of its files, already open in a tab, is edited without saving
- **THEN** that file is skipped as open with unsaved changes when the run starts, and the others are replaced

#### Scenario: Every file becomes unsaved while the run waits
- **WHEN** every file of a waiting run has been edited without saving in an open tab by the time capacity allows it
- **THEN** no file is written, and the report lists each file as open with unsaved changes

#### Scenario: Capacity is still short when it next changes
- **WHEN** capacity changes while a run waits but is still not enough for its reservation
- **THEN** the run keeps waiting, with its selection and transaction held

### Requirement: A run waiting for capacity can be cancelled with nothing written
A run waiting for capacity SHALL behave as a run in flight: Cancel, and Stop and Close from the close-during-run question, MUST end the wait at once with no file written and the earlier Undo still offered. Hiding the panel or starting a new search MUST NOT cancel it. Destroying the window MUST end the wait with no file written and release its transaction.

#### Scenario: Cancel while waiting
- **WHEN** the user activates Cancel Replace All while the run waits for capacity
- **THEN** the wait ends at once, no file changes, the status says nothing was changed, and the Undo of the earlier Replace All is still available and restores its files

#### Scenario: Escape while waiting
- **WHEN** the user hides the search panel while the run waits for capacity
- **THEN** the run still starts once capacity allows and replaces the files

#### Scenario: Close while waiting
- **WHEN** the user closes the window while the run waits for capacity
- **THEN** the close-during-run question is shown and says that no file has changed yet, and Stop and Close closes the window with no file written

#### Scenario: The window is destroyed while waiting
- **WHEN** the panel of a waiting run is disposed
- **THEN** the wait is cancelled, no file of that run is written even after capacity frees, and another window can then start a Replace All

#### Scenario: A cancelled wait keeps the earlier skip report
- **WHEN** an earlier Replace All offered Show Skipped Files and the user cancels a later run while it waits for capacity
- **THEN** Show Skipped Files still lists the earlier run's skipped files

### Requirement: A run waiting for capacity holds the application's transaction
While a confirmed run waits for capacity in one window, an apply or Undo in another window SHALL be refused with the same status message as during a run, and Undo SHALL stay unavailable in the waiting window. The earlier Undo of every window SHALL stay installed until the waiting run is admitted, and is retired only then.

#### Scenario: Apply in another window while one waits
- **WHEN** window B's confirmed run is waiting for capacity and the user confirms a Replace All in window A
- **THEN** window A's apply is refused with the "Another window is running Replace All" message, and no file is touched by it

#### Scenario: Other windows' Undo journals do not keep a run waiting
- **WHEN** other windows hold earlier Undo journals that, beside the run's undo reservation, exceed the disposal capacity, and nothing else holds it
- **THEN** the confirmed run starts at once, and retires those windows' Undo as it starts

#### Scenario: A journal recovery does not race a waiting or starting run
- **WHEN** a window's startup recovery of an earlier active journal is waiting or in flight while a confirmed run waits or is admitted
- **THEN** the recovery never ends the waiting run as superseded, and no window offers the journal the admitted run replaced as its Undo

#### Scenario: Undo in another window while one waits
- **WHEN** window A offers Undo for an earlier Replace All, window B's confirmed run is waiting for capacity, and the user activates Undo in window A
- **THEN** window A's Undo is refused with the same message, and its files keep the earlier run's text

### Requirement: The wait for capacity is reported accessibly
While a confirmed run waits for capacity, the system SHALL keep a waiting message in the status bar for the whole wait, except while a workspace search shows its own progress, announce it once, describe it in the Cancel Replace All button's accessible description, and report it in the workflow evidence and the automation snapshot. The `search-complete` readiness SHALL stay pending.

#### Scenario: State while waiting
- **WHEN** a confirmed run is waiting for capacity
- **THEN** the button reads Cancel Replace All, the status bar says the run is waiting for memory, the automation snapshot reports `content_search.replace_admission_deferred` true and `replace_in_flight` true, and the waiting message is announced once
- **AND** `search-complete` readiness is pending, and the waiting window's Undo is unavailable

#### Scenario: State after the run starts
- **WHEN** capacity allows the waiting run to start
- **THEN** `replace_admission_deferred` is false, and the waiting text gives way to the run's progress

#### Scenario: A new search during the wait
- **WHEN** the user runs a new workspace search while a confirmed run waits for capacity
- **THEN** the search's own progress is shown while it runs, the status bar shows the waiting message again within a second of the search ending, and the run still starts once capacity allows
