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
The system SHALL attempt to restore already-written files to their original content when cancellation interrupts a Replace All run.

#### Scenario: Cancellation restores earlier files
- **WHEN** Replace All is cancelled after one or more files were already updated
- **THEN** the system restores those already-updated files from the preserved original bytes
- **AND** cancellation does not leave a partially applied multi-file replace result when rollback succeeds

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
The system SHALL bound Replace All memory exposure with explicit service-level caps. A single target file larger than `10 * 1024 * 1024` bytes MUST be skipped and reported. The total persisted undo payload for one Replace All run MUST NOT exceed `64 * 1024 * 1024` bytes; files that would exceed that cap MUST be skipped and reported before they are modified.

#### Scenario: Oversized replace target is skipped
- **WHEN** Replace All includes a matching file larger than the per-file replace cap
- **THEN** the file is skipped without modification
- **AND** the result reports that path as skipped

#### Scenario: Undo payload cap skips later files before write
- **WHEN** adding another file's original and replacement bytes would exceed the total undo payload cap
- **THEN** that file is skipped before any replacement write occurs
- **AND** already-written files remain undoable

### Requirement: Replace All builds changed text without full line-vector amplification
The system SHALL avoid constructing a full per-line collection of owned strings, byte ranges, or equivalent metadata during Replace All. Before line discovery, it MUST validate replacement-count and recorded-range bounds. For accepted files, it MUST validate UTF-8 using the project's established large-file validation approach and build replacement output in one source-order pass from sorted recorded replacements. Retained edit metadata MUST remain proportional to accepted replacements rather than total source-line count, while original bytes, output bytes, and durable undo bytes remain governed by their existing caps.

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

### Requirement: Replace All undo journal is incremental and durable per file
The system SHALL persist Replace All undo state as incremental per-file durable entries rather than rewriting the entire growing backup after each file. Each file's undo entry MUST be written and synced before that file is modified. Cleanup on undo, search-panel close, and startup MUST remove both the new journal directory and any stale legacy backup file.

#### Scenario: Per-file entry is durable before replacement
- **WHEN** Replace All is about to rewrite a target file
- **THEN** that file's undo entry is durably persisted first
- **AND** the workflow does not rewrite previously persisted entries

#### Scenario: Journal persistence scales linearly
- **WHEN** Replace All successfully rewrites many files
- **THEN** the number of durable journal entry writes grows linearly with the number of touched files
- **AND** the workflow does not serialize the full backup once per file

#### Scenario: Startup clears stale journal formats
- **WHEN** the app starts with stale Replace All undo state from a prior session
- **THEN** it deletes the new journal directory and the legacy `replace-backup.json` file
- **AND** the new session does not inherit old Replace All undo state

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
The system SHALL generate Replace Preview through explicit service-level limits of at most 10,000 generated rows and `64 * 1024 * 1024` conservatively charged preview bytes. The generator MUST use saturating accounting, MUST admit only complete preview rows, and MUST return a typed outcome that distinguishes generated, omitted-eligible, source-truncated, and otherwise invalid matches.

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
The project SHALL add pure service, property, widget, accessibility, visual-geometry, and scale tests for zero matches, representative literal and regex matches, exactly-at-limit inputs, 10,000 matches, large replacement text, stale generations, awkward paths, Unicode, and constrained layouts.

#### Scenario: Scale fixture avoids quadratic row binding
- **WHEN** a performance fixture binds and toggles rows in a 10,000-match preview
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
