## MODIFIED Requirements

### Requirement: Cancelling Replace All rolls back already-applied file writes
The system SHALL attempt to restore already-written files to their original content when cancellation interrupts a Replace All run. The user SHALL be able to cancel a run in flight: while a run is in flight, the Replace Preview confirm control SHALL become a Cancel Replace All control, and `win.cancel-search-panel-replacements` SHALL request the same cancellation. Cancellation SHALL be observed at the next chunk boundary. The file in progress MUST be left untouched (its durable write fails before rename). Files already written MUST be restored by streaming their original bytes from their journal bodies, with the same digest and identity checks as Undo. If rollback cannot restore every file, the journal MUST stay active and Undo MUST be offered for the remaining files. Closing the window or quitting the application during a run SHALL request cancellation and defer the close until the run's terminal.

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
- **WHEN** the user closes the window while Replace All is running
- **THEN** the run is cancelled, a status message says Replace All is being cancelled, and the window closes only after the run's terminal
- **AND** every file holds its original bytes, or its replaced bytes with an active undo entry

### Requirement: Replace All enforces file and undo memory caps
The system SHALL bound Replace All resources with explicit service-level limits.

- **Per-file limit.** A single target file larger than `MAX_REPLACE_FILE_BYTES` MUST be skipped and reported with the reason `LargerThanOpenTier`, and the report MUST name the limit in IEC units. `MAX_REPLACE_FILE_BYTES` is `DocumentScale::REFUSE_TO_OPEN`, 512 MiB, the largest file LushText opens; it is referenced, not copied.
- **Undo data.** Undo data is no longer held in memory. A file's undo body and its rewritten output MUST each be admitted against the free space of their filesystems plus `REPLACE_DISK_RESERVE_BYTES` (1 GiB), except for a file that `main`'s admission rule would have admitted. Files that are not admitted MUST be skipped and reported with the reason `NotEnoughDiskSpace` before they are modified.
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

- a raw body file holding that file's original bytes;
- an entry of kind `replace-all-undo-stream-entry` recording the body file name, the length and SHA-256 digest of both the original and the replaced bytes, and the target's identity facts;
- the armed incremental manifest.

The replaced bytes MUST NOT be persisted or retained. Entries MUST be removed before their bodies, so no entry references a missing body. Cleanup on undo, search-panel close, and startup MUST remove the journal directory, its bodies, and any stale legacy backup file.

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

#### Scenario: Rewriting reads the body it saved
- **WHEN** Replace All writes a target's new bytes
- **THEN** they are generated from the journal body, and the rename happens only if the body's digest equals the recorded original digest and the output's digest equals the digest computed while the body was written

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

## ADDED Requirements

### Requirement: Replace All streams every file up to the open tier in constant memory
Replace All SHALL process each file with a working set independent of its size: chunk buffers, at most one targeted line, digest states, and edit records proportional to accepted replacements. Pass A SHALL stream the target into its undo body while dry-running the replacement into a digest. Pass B SHALL stream the body through the replacement into the target's durable write. The durable-write protocol core SHALL NOT change.

#### Scenario: Memory is flat across file sizes
- **WHEN** Replace All rewrites one 16 MiB file and, separately, one 512 MiB file
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
Disk admission SHALL waive the reserve for exactly the files `main` would have replaced. A pure ledger replays `main`'s rule: `MAIN_REPLACE_FILE_BYTES` files first, then up to `MAIN_MAX_REPLACE_FILE_BYTES`, each charging source plus output against `MAIN_REPLACE_UNDO_BYTES` in path order. Processing SHALL keep those two passes, the second extended to the per-file limit. A waived file needs only the free space its writes take.

#### Scenario: Little free space
- **WHEN** a Replace All run that `main` replaced in full runs with less free space than the reserve, but enough for its writes
- **THEN** every file is replaced, as on `main`

#### Scenario: A large file early in path order
- **WHEN** a 300 MiB file sorts before small files that `main` replaced, and the disk cannot hold the 300 MiB file's body plus the reserve
- **THEN** the 300 MiB file is listed as skipped for disk space and every small file is replaced

### Requirement: Every skipped file carries one typed reason
Every file a Replace All run does not replace SHALL carry exactly one `ReplaceSkipReason`: `UnsavedChanges`, `ChangedSinceSearch`, `ChangedDuringReplace`, `NotUtf8`, `LineTooLong`, `InvalidPreview`, `LargerThanOpenTier`, `NotEnoughDiskSpace`, `Unreadable`, `WriteFailed` or `UndoJournalFailed`. Counts per reason SHALL be exact.

#### Scenario: A file skipped at preview time
- **WHEN** a file's only matches are on lines longer than the search excerpt
- **THEN** the run reports that file once, with the reason `LineTooLong`

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
A Skipped Files row SHALL offer Open and Replace in Editor when the pure availability policy allows it. `win.replace-in-editor(s)` SHALL open the file through the normal open path, or select its tab, and place the cursor at the first skipped match. It SHALL then reveal the in-tab replace bar, prefilled with the query, the options and the replacement. It SHALL NOT replace or save anything by itself.

#### Scenario: Unsaved open file
- **WHEN** the user activates Open and Replace in Editor on a row skipped for unsaved changes
- **THEN** that file's tab is selected, the replace bar shows the query and the replacement, and the document is unchanged until the user applies the replacement

### Requirement: The editor action is offered only where the editor can replace
The policy SHALL never offer the action for `LargerThanOpenTier` or `Unreadable`. It SHALL NOT offer it for `LineTooLong` when the source line exceeds `4 × L_PROTECT_EDIT` bytes, because that document opens in Protected View, where in-tab Replace is unavailable. It SHALL offer the action for every other reason.

#### Scenario: A file over the open tier
- **WHEN** a row is skipped as larger than the largest file LushText opens
- **THEN** the row offers no editor action and its reason names 512 MiB

#### Scenario: A line certainly in Protected View
- **WHEN** a row is skipped as `LineTooLong` for a 300,000-byte line
- **THEN** the row offers no editor action and says the document opens view-only

### Requirement: Regex replacements are translated for the in-tab bar
In regex mode the prefilled pattern SHALL be the workspace pattern. The replacement SHALL be translated from Rust `regex` syntax to GtkSourceView syntax: `$N` and `${N}` become `\N`, `${name}` becomes `\g<name>`, `$$` becomes `$`, and a literal backslash is escaped. A template that cannot be translated SHALL leave the replacement empty, with an explanation in the bar.

#### Scenario: Regex replacement with captures
- **WHEN** the workspace query is the regex `(\w+)@example` with replacement `$1@example.org`
- **THEN** the in-tab bar is prefilled with the same pattern, regex mode on, and the replacement `\1@example.org`

### Requirement: Replace All reports progress for long runs
While a Replace All run is in flight for longer than the search-progress delay (500 ms), the status bar SHALL show its progress: files done of the total, and MiB done of the total. A heartbeat renews the progress lease. The progress SHALL be announced once when it first appears, and the terminal message SHALL replace it.

#### Scenario: A 512 MiB file
- **WHEN** Replace All runs on a 512 MiB file
- **THEN** the status bar shows the MiB processed, rising, until the completion message replaces it

### Requirement: Legacy inline journal entries remain undoable
A journal of the previous entry kind (`replace-all-undo-entry`, with inline original and replaced strings) SHALL still load and undo, bounded by the legacy payload limit of 64 MiB. New runs SHALL write only `replace-all-undo-stream-entry` entries. A manifest that mixes the two kinds SHALL be treated as malformed: it is preserved and offers no undo.

#### Scenario: Crash before an upgrade
- **WHEN** LushText is upgraded after a crash that left an active legacy journal
- **THEN** the relaunched build offers Undo, and Undo restores the original bytes

### Requirement: Recovery removes the crashed run's temp files
The journal manifest SHALL record the run's process id and launch nonce. On startup recovery, Undo and discard, the system SHALL remove the temp files beside each journaled target whose names match the exact pattern, the Replace All tag and the recorded process id and nonce, when that process id is not the current process. Only the journaled targets' own directories SHALL be read. Every other sweep keeps the 24-hour age rule.

#### Scenario: Killed while writing a 512 MiB file
- **WHEN** LushText is killed during pass B of a 512 MiB file and relaunched
- **THEN** the half-written temp beside the target is removed and the target keeps its original bytes
- **AND** the run's Undo, offered for the files it did replace, counts that file as already restored and writes nothing to it
