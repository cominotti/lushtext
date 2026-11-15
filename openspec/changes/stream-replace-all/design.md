# Design: stream-replace-all

## Context

Programme item 2 of `docs/next/long-line-documents.md` (and
`power-of-two-thresholds` design D13, item 2) asks for three things:

- Replace All, chunked;
- an on-disk undo journal, so the per-file limit reaches the open tier
  (512 MiB) in constant memory;
- skipped files listed with their reason, with an "Open and replace in editor"
  action.

### Today (`main` `2c91ccf2`)

Replace All is a single pass on a worker
(`services/content_search/replace.rs::apply_replacements_inner`). For each
file in path order it:

1. acquires the file's `TargetWriteGuard`;
2. checks its facts against `MAX_REPLACE_FILE_BYTES` (16 MiB);
3. reads the whole file (`fs_read::bounded_bytes`) and validates its UTF-8;
4. builds the new text in memory (`build_replaced_text`, a monotonic line
   cursor with stale-line checks);
5. charges before plus after bytes to a 64 MiB `UndoPayloadLedger`, and the
   retained owner to `MAX_REPLACE_UNDO_RETAINED_BYTES`;
6. writes a JSON entry holding both snapshots as strings, then arms the
   incremental manifest;
7. rewrites the target with `atomic_write`.

A 10 MiB first pass (`MAIN_REPLACE_FILE_BYTES`) makes sure no 10–16 MiB file
takes undo budget from a file `main` used to replace (PT stage-4 review).

The result is `ReplaceResult`: exact counts, plus the skipped paths and the
error strings as two bounded samples (32 entries and 32 KiB each). The window
turns it into one status line, "Replaced X of Y matches in Z files (N files
skipped)". The undo backup, with every before and after byte, crosses to GTK
under a plain-disposal reservation of up to 64 MiB. It stays there for the
undo window, and undo compares the current bytes against it byte for byte.

The `cancel` flag passed to the worker is created in `ui/window/search.rs` and
never set: Replace All cannot be cancelled.

### Measured baseline

From PT task 9.5 M9: a release lib test, one process per measured call, VmHWM
reset first, three interleaved repetitions at load below 4
(`.claude/handoffs/power-of-two-thresholds/runs.log` line 382). The replace
code has not changed since, apart from d704a908's search-budget edits.

| Case | `main` peak | Growth over the 27 MiB process baseline | Wall |
|---|---|---|---|
| one 10 MiB file | 49.0–49.3 MiB | 21.6 MiB (2.16×) | 9–11 ms |
| one 16 MiB file | 61.2–61.4 MiB | 33.9 MiB (2.12×) | 15–18 ms |
| six 16 MiB files (64 MiB undo cap binds) | 94.0–100.3 MiB | about 70 MiB | — |

Growth is linear at about 2.1 times the file size. Taking the per-file cap to
512 MiB without streaming would cost about 1.1 GiB of worker memory per file.
The aggregate cap would also have to exceed 1 GiB, and the GTK process would
retain that for the whole undo window. That disposal reservation is the same
plain lane that startup recovery and the editor share. So the change is
**viable as described**: the cap cannot be raised without the structural fix,
and the structural fix also removes up to 64 MiB of undo bytes that GTK
retains today. No new measurement is needed for this decision. The
implementation re-measures `main` interleaved with the branch (tasks 1.1 and
9.2).

## Goals / Non-Goals

**Goals**

- Every file the editor can open (at most 512 MiB) is eligible for Replace
  All.
- Worker memory is independent of file size.
- GTK retains descriptors only.
- Every data-safety property of today's journal still holds:
  - the per-file entry is durable before the target is touched;
  - stable target guards;
  - durable writes with before-rename and after-rename classification;
  - cancellation rollback;
  - exact accounting;
  - restart-safe cleanup;
  - crash exposure as a retryable Undo.
- No file `main` replaced is skipped on the branch.
- Every skipped file is listed with a typed reason, and the user is offered a
  way to finish where one exists.
- Long runs show progress and can be cancelled. Closing the window never
  strands a run.

**Non-Goals**

- Replacing matches on lines longer than the 4 KiB search excerpt
  (`MAX_SEARCH_MATCH_LINE_BYTES`). Those matches stay preview-ineligible.
  They are now listed as skipped with the editor action. The candidate
  follow-up is recorded in "Follow-ups".
- Non-UTF-8 targets. They are skipped with the editor action, because the
  editor's encoding support can open them.
- Changing `services/filesystem/write_protocol.rs`.
- Persisting the skip list across restarts. It is session state; the journal
  is the only persisted state.

## Decisions

### D1. The per-file limit is the open tier, referenced

`MAX_REPLACE_FILE_BYTES` becomes `pub const MAX_REPLACE_FILE_BYTES: u64 =
DocumentScale::REFUSE_TO_OPEN`, with the kind "derived". The cap matches what
the editor can offer as a fallback: past it, no "Open and replace in editor"
exists, so the reason says the file is larger than the largest file LushText
opens.

*Alternative rejected:* no per-file limit at all. Streaming makes memory
constant at any size, but time and disk are not. A file the user cannot open
in LushText would have no fallback and no way to inspect the result, and it
makes D4's disk admission the only bound. Recorded as a follow-up if users ask.

### D2. A chunked streaming engine with the same edit semantics

`replace/stream.rs` holds the engine. It is GTK-free and I/O-trait-based
(`Read` in, `Write` out). It reads 1 MiB chunks (`REPLACE_STREAM_CHUNK_BYTES`,
an internal cap, a power of two; tasks 2.4 and 9.3 measure 256 KiB against
1 MiB for throughput).

- It keeps today's sorted `PendingEdit`s and monotonic line numbering.
- UTF-8 is validated incrementally with `simdutf8::compat`: an incomplete
  trailing sequence of at most 3 bytes is carried into the next chunk, and an
  invalid sequence fails the file as `NotUtf8`.
- The line cursor carries a `\r` that ends one chunk when the next starts with
  `\n`, so CRLF is never split. A line longer than a chunk streams through
  without being buffered.
- A **targeted** line is collected into a line buffer. That line is at most
  `MAX_SEARCH_MATCH_LINE_BYTES` long, because preview rows with truncated
  lines are ineligible, so the buffer is bounded at 4 KiB plus one chunk
  seam. The line is compared with `original_line` exactly as today, and an
  edit applies only when the whole line matches.
- The output is the same bytes `build_replaced_text_reference` produces,
  including the final unterminated line, empty lines and CRLF.

The engine reports the source length and digest, the output length and
digest, the edit count and a `ReplacementTextOutcome`. That outcome gains no
variant except `NotUtf8`.

**Bound.** Retained memory is two chunk buffers, the line buffer, the SHA-256
states and the edit records (proportional to accepted replacements, as today).
The acceptance figure is in task 9.2.

*Alternative rejected:* memory-map the file. `mmap` of a file another program
truncates raises `SIGBUS`, and a page fault on a network filesystem blocks
without a cancellation point.

### D3. An on-disk body plus digests, and two passes per file (no protocol-core change)

A journal entry becomes the new kind
`dev.cominotti.lushtext.replace-all-undo-stream-entry` (v1 envelope):

```json
{ "path": "…", "body_file": "<hash>.body",
  "original": { "len": 123, "sha256": "…" },
  "replaced": { "len": 125, "sha256": "…" },
  "identity": { "dev": 0, "ino": 0, "size": 123, "mtime_ns": 0 } }
```

The body file holds the raw original bytes. The replaced bytes are not stored:
undo needs only their length and digest to recognize them. Per replaced file,
the journal holds the file's size once, where today it holds about twice the
size plus JSON escaping.

The per-file order for apply:

- **P0.** Acquire the `TargetWriteGuard` and take the target's facts `F0`
  (size, mtime in ns, dev, ino, canonical path).
- **P1.** Admit the body on the journal's filesystem (D4).
- **P2.** Run **pass A**: `atomic_replace_stream(journal/<hash>.body)`. Its
  producer reads the target in chunks, writes each chunk to the body, hashes
  it as the source, and feeds it to the engine with a **hashing sink** as the
  output: a dry run, so no output is written. At EOF the producer re-reads the
  target's facts. If they differ from `F0` it fails with `ChangedDuringReplace`
  and the body temp is removed (`BeforeRename`). A stale line, invalid UTF-8,
  an invalid preview or cancellation also fail the producer, so the body is
  discarded before any entry exists. If there are no edits (`Unchanged`), the
  body is discarded and the file is not counted.
- **P3.** Admit the output on the target's filesystem (D4). The output size is
  now known.
- **P4.** Write the entry JSON durably. Before the first file, prepare the
  incremental journal, and arm the manifest once (as today). The manifest now
  also records the run's `pid` and `launch_nonce` (D9).
- **P5.** Run **pass B**: `atomic_replace_workspace_file_stream(resolved target,
  WriteLabel::REPLACE)`. This is a new shell-level wrapper in
  `filesystem/write.rs`. Like today's `atomic_write` it writes the resolved
  target identity, so a symlink keeps writing its target, and it sweeps the
  target's own leftovers afterwards. Its producer reads
  the **body** in chunks, hashes it, and feeds it to the engine with the
  temp-file writer as the output, hashing that output. At EOF the producer
  requires three things: body digest equal to the original digest, output
  digest equal to pass A's, and target facts equal to `F0`. Otherwise it fails
  and nothing is renamed. On `BeforeRename`, the entry is deleted and then the
  body (entry first, so no entry ever points at a missing body). On
  `AfterRename`, the entry and body are kept, and the file counts as replaced
  with a durability warning, as today.
- **P6.** Release the guard.

Pass B reads the body, not the target, so the bytes that are rewritten are
exactly the bytes the undo body holds. Comparing the digests proves that the
two passes agreed.

`services/filesystem/write_protocol.rs`, `durable_write.rs`'s protocol
execution and the Kani shards that prove them are **unchanged**. Every target
and body write is one existing `atomic_replace_stream` call, and every new
decision (the P-order, the failure classes, the cleanup order) lives in a new
I/O-free `replace/protocol.rs` (`ReplaceFileProtocol`, `ReplaceUndoProtocol`)
in the same `step(outcome) -> action` style. Kani proves it (D11).

*Alternatives rejected:*

- (a) Store the replaced bytes too: twice the disk for no gain over a digest.
- (b) Regenerate and compare at undo, from the body plus a persisted edit
  list: the edit list is document text (up to the 64 MiB preview budget) and
  would have to be persisted.
- (c) One pass that tees into both the body and the target temp: the target's
  rename happens when the producer returns, so the entry would have to be
  armed from inside the target producer, before its metadata step. That
  either changes the protocol core or nests two durable writes in an order
  that is hard to prove.
- (d) BLAKE3: faster, but a new dependency. `sha2` 0.10 detects SHA-NI at run
  time and is already vendored.

### D4. Disk admission with a reserve, and a no-loss ledger instead of the memory aggregate

The 64 MiB `UndoPayloadLedger` and the two-pass memory rationale retire.
`MAX_REPLACE_UNDO_RETAINED_BYTES` (64 MiB, an internal cap) stays. It now
bounds the GTK descriptor owner, which is about 200 bytes plus the path per
file, so in practice it never binds.

**Free-space probe.** The probe is `fs_metadata::free_space(path) ->
io::Result<u64>`, using `rustix::fs::statvfs` with `f_bavail * f_frsize`,
through the filesystem boundary. A probe error counts as "unknown". Unknown
free space admits the file and relies on a write's ENOSPC turning into a
`BeforeRename`, which is safe because nothing is renamed.

**Admission.** A body (P1) needs at least the source length plus the reserve
free on the journal's filesystem. An output (P3) needs at least the output
length plus the reserve free on the target's filesystem. The reserve is
`REPLACE_DISK_RESERVE_BYTES`, 1 GiB, a policy tier and a power of two. Without
it, an undo journal could fill a user's disk, and other programs would start
to fail.

**No-loss waiver.** The reserve is waived for a file exactly when `main` would
have admitted it. A pure `MainAdmissionLedger` replays `main`'s rule:

- files of at most 10 MiB in a first pass, then files of 10–16 MiB;
- each charged its source plus output length against 64 MiB, in path order;
- files above 16 MiB never charge it.

The processing order keeps `main`'s two passes, with the second pass extended
to 512 MiB, so the ledger's decisions are `main`'s decisions exactly. For a
waived file only the raw need applies, which `main` also needed (it wrote both
snapshots as JSON). `MAIN_REPLACE_FILE_BYTES` (10 MiB),
`MAIN_MAX_REPLACE_FILE_BYTES` (16 MiB) and `MAIN_REPLACE_UNDO_BYTES` (64 MiB)
stay as recorded presentation-cap exceptions. They are defined as `main`'s
values.

The failing-first test is
`no_loss_every_file_main_replaced_is_replaced_with_little_free_space`. It uses
a fake free-space probe with less than the reserve free.

*Alternative rejected:* a fixed aggregate journal cap, for example 4 GiB.
It is arbitrary on large disks and unsafe on small ones. Free space is the
real bound.

### D5. External modification is refused, not raced

Today the window between the read and the rename is small. With streaming it
can last seconds, so P2 and P5 both re-check `F0` (size, mtime in ns, dev,
ino). The `TargetWriteGuard` already orders in-app writers (editor save,
another Replace All, undo, sidebar rename). The facts check covers other
programs. A mismatch skips the file with `ChangedDuringReplace` and the editor
action. The remaining window, between the producer's final check and the
rename, is the same as today's, and is recorded.

### D6. Undo, rollback and legacy entries stream from bodies

The undo order for each entry:

- **U0.** Acquire the guard and take facts.
- **U1.** Stream-hash the current file. It is already restored if its length
  and digest equal the original's. It needs restoring if they equal the
  replaced length and digest. Anything else is `Diverged`: the file is skipped
  and stays retryable.
- **U2.** Admit the restore output on the target's filesystem (D4; an undo is
  never waived, because `main`'s undo held both snapshots in memory and had no
  disk need, so the reserve applies). Then run the same
  `atomic_replace_workspace_file_stream` wrapper as P5. Its producer copies the body while hashing
  it, and at EOF requires the body digest to equal the original's and the
  target facts to equal U0's. Otherwise nothing is renamed.
- **U3.** Shrink the journal: delete the entry, then the body.

Cancellation rollback (D8) runs the same steps over the files this run wrote.

The size gate in today's undo (`current_facts.byte_size >
MAX_REPLACE_FILE_BYTES`) stays, and the bounded read becomes a bounded
stream. A file that grows past the cap during the hash stops at the cap and is
skipped. The scenarios of "Undo reads enforce the file cap during ingestion"
keep their names.

**Legacy.** A v1 `replace-all-undo-entry`, with inline strings, can be the
active journal across an upgrade: a crash before the upgrade, then a relaunch
on the new build. The loader keeps reading it under its existing 64 MiB
payload bound (`payload_budget_exceeded`). It turns each entry into a
descriptor whose original is `UndoOriginal::Inline(Arc<[u8]>)` and whose
digests are computed at load. Undo streams from the inline bytes. New runs
never write the legacy kind. Mixed journals, with legacy and stream entries
in one manifest, are rejected as malformed: one run writes one kind.

**Downgrade.** An older build sees an entry of an unknown kind and treats it
as malformed. It must not expose undo, and it must not delete the bodies its
orphan scan does not know. The target files hold the replaced content the user
asked for. Task 7.6 verifies this behaviour with a fixture and records it.

### D7. A typed skip report, the Skipped Files dialog and "Open and replace in editor"

**The model.** `model/content_search.rs` holds the model:

- `ReplaceSkipReason`, one of: `UnsavedChanges`, `ChangedSinceSearch`,
  `ChangedDuringReplace`, `NotUtf8`, `LineTooLong`, `InvalidPreview`,
  `LargerThanOpenTier`, `NotEnoughDiskSpace`, `Unreadable`, `WriteFailed`,
  `UndoJournalFailed`;
- `ReplaceSkipReport`: exact counts per reason, plus a list of
  `ReplaceSkippedFile { path, reason, first_line }` bounded by
  `MAX_REPLACE_SKIP_LIST_BYTES` (4 MiB of charged path bytes, a presentation
  cap and a power of two), and a `not_listed` count.

`LineTooLong` and `InvalidPreview` come from the preview's ineligible matches,
grouped by file. One Replace All touches at most `MAX_REPLACE_PREVIEW_ROWS`
(16,384) files, and typical paths are about 100 bytes, so typical runs list
every skipped file. A path-heavy worst case is cut at 4 MiB and says
"N more not listed". The existing 32-entry, 32 KiB diagnostic sample stays for
status text and logs. The skip list contains paths and reason classes only,
never document text.

**Pure policy (`ui/search_panel/policy.rs`).**
`replace_in_editor_availability(reason, facts)` returns `Offered`,
`NotOpenable` or `ProtectedView`:

- `LargerThanOpenTier` and `Unreadable` are not offered.
- `LineTooLong` is offered unless the source line's byte length exceeds
  `4 × L_PROTECT_EDIT`. Such a line certainly has more than `L_PROTECT_EDIT`
  characters, so the document opens view-only and in-tab Replace is
  unavailable there (existing requirement).
- Every other reason is offered.

A document that turns out to be protected once opened gets the existing
Protected View notice.

**The surface.**

- When a run has skips, the panel's completion row shows **Show Skipped
  Files** ("Replaced 40 of 52 matches in 9 files · 3 skipped").
- The button opens an `AdwDialog` (`ui/search_panel/skipped_files.rs`, a
  called presentation surface) with a `GtkListView` over a plain list model,
  so it is virtualized for 16,384 rows.
- Each row shows the file name, the folder (dim), the reason sentence and an
  **Open and Replace in Editor** button when the action is offered.
- Rows are grouped by reason with exact counts.

**The action.** `win.replace-in-editor(s)` takes the path. It is exported for
automation parity, and its catalog entry is checked.

1. It opens the file through `open_document` (the single duplicate-tab and
   focus authority), or selects its tab.
2. It moves to the first skipped line.
3. It calls `win.begin-replace`.
4. It prefills the in-tab bar through a new
   `LushtextSearchBar::prefill_replace(query, replacement, options)`.

In literal mode both fields are prefilled. In regex mode the pattern is
prefilled unchanged, and the template goes through the pure
`rust_replacement_to_gtksourceview(template) -> Option<String>`: `$N` and
`${N}` become `\N`, `${name}` becomes `\g<name>`, `$$` becomes `$`, and a
literal backslash is escaped. When it returns `None` the replacement field is
left empty and the bar says why. Case and whole-word settings carry over. The
user reviews and applies in the editor, which is undoable and nothing is
written until save. So the action is data-safe by construction.

Two toolkit facts are pinned as GTK axioms with probes and samples (task 1.3):

- GtkSourceView's regex replacement syntax: `\N` and `\g<name>` expand, and
  `$1` is literal.
- `set_at_word_boundaries` matches the same ASCII-identifier occurrences as
  workspace search's whole-word mode.

*Alternative rejected:* list the skipped files inside the results list. The
results belong to the search generation that Replace All invalidates, and
mixing the two kinds of row breaks the results list's identity and retirement
contracts.

### D8. Progress, Cancel and closing during a run

- **Progress.** The worker updates an `Arc<ReplaceProgress>` (atomics: files
  done and total, bytes done and total, phase). A GTK poll publishes "Replacing
  files… 3 of 12 (140 of 900 MiB)" through the notification bus. It uses the
  search-progress shape: a 500 ms delay before first render, a 1 s heartbeat
  lease, and a silent render with one announcement when progress starts.
- **Cancel.** While a run is in flight, the Replace Preview confirm button
  becomes **Cancel Replace All**. `win.cancel-search-panel-replacements` is
  exported for automation. Cancel sets the worker's flag. The worker stops at
  the next chunk boundary (a cancelled producer means `BeforeRename`, so the
  current file is untouched) and rolls back the files it already wrote (D6
  rollback). Clean rollback clears the journal, as today. Failed rollback
  keeps the journal active and offers Undo for the remaining files.
- **Close.** A `close_request` during a run sets the cancel flag. It defers
  the close, with a status message "Cancelling Replace All…", until the
  worker's terminal, then closes. Application quit takes the same path.
  Rollback keeps its data-safety ordering. A process kill at any point is
  covered by the journal and D9.

### D9. Recovery and cleanup know bodies and journal-owned temp files

- **Entries and bodies.** Startup recovery loads descriptors only. It stats
  each body, and a body whose size differs from its `original.len` is a
  malformed entry (preserved and excluded, as today). Orphan detection and
  `delete` include `*.body`. An orphan body (one with no entry, from a crash
  between P2 and P4) is removed by cleanup.
- **Temp files.** The manifest records the run's `pid` and `launch_nonce`.
  On recovery, after undo, and after a discard, for each journaled target the
  cleanup removes the leftovers in the target's own directory that satisfy
  the new `leftovers::is_journal_owned_leftover`:
  - the exact builder pattern;
  - the tag `WriteLabel::REPLACE`;
  - the pid and nonce equal to the manifest's;
  - a pid that is not the current process;
  - a regular file (lstat).

  The 24-hour age rule stays for every other sweep. A recorded nonce proves
  the temp belongs to the crashed run, so no age is needed. A user workspace
  is still never traversed: only the journaled targets' parent directories are
  read, one bounded scan each.
- **Kill windows.** New `KillWindow`s:
  - `replace-body-before-entry`;
  - `replace-entry-before-target-rename`;
  - `replace-target-renamed-before-finish`.

  The crash-recovery smoke drives Replace All through automation, kills at
  each window, relaunches, and asserts the outcome: the journal state, the
  undo affordance, the target bytes, and no leftover temp.

### D10. GTK never waits on a guard

The journal guard and target guards are only ever taken on workers
(`spawn_blocking_then`), and the change keeps that true. A run now holds the
journal guard for seconds, so three things wait their turn on a worker, never
on GTK:

- the panel-close journal delete;
- an undo;
- a new Replace All.

An editor save of a file being rewritten waits on its target guard on its
worker, for at most one file's pass B. This is recorded. The responsiveness
requirement adds the scenario.

### D11. Kani shards

- **`core-replace-protocol`** proves the per-file apply protocol and the undo
  protocol of `replace/protocol.rs` over every outcome sequence and every
  crash point, composed with an abstract durable write (the existing
  write-protocol theorems are its assumptions: a failed write is
  `BeforeRename` and leaves the previous bytes). Properties:
  1. after any crash the target holds the original or the replaced bytes;
  2. replaced implies an active entry whose body holds the original;
  3. no entry references a missing body;
  4. undo writes only when the current file equals the replaced digest;
  5. cleanup never removes the body of an active entry before its entry.
- **`core-replace-stream`** proves the engine's chunk-split independence on
  inputs of at most 8 bytes, every chunk size from 1 to 4, and at most 2 edits:
  the streamed output, outcome and digests' input equal the whole-buffer
  reference. That covers UTF-8 and CRLF seams.

Both shards are scheduled, and their runner budgets are measured once
(`make check-kani-shards`). `core-replace-protocol` is gated on pull requests
if its measured wall time is within the PR-gate budget of the shard table.
Property tests carry the engine at scale (D2), at every split offset of
generated inputs.

### D12. Module layout and the readability row

`services/content_search/replace.rs` (3,255 lines) becomes `replace/`:

| File | Holds |
|---|---|
| `mod.rs` | the entry points |
| `stream.rs` | the engine |
| `protocol.rs` and `protocol/kani_proofs.rs` | the per-file protocols and their proofs |
| `apply.rs` | the apply orchestration |
| `undo.rs` | undo and rollback |
| `admission.rs` | the no-loss ledger and disk admission, pure |

The UI side adds `ui/search_panel/skipped_files.rs` as a called presentation
surface, and extends `policy.rs` and `evidence.rs`. `mod.rs` is at its
370-line facade budget, so the new stages (progress, cancel, skipped list) are
narrated by turning existing prose into delegation names, and the facade must
not exceed 370. The `WFR-SEARCH-REPLACE` row re-derives its measured cells
(`.agents/rules/workflow-convention.md`).

## Risks / Trade-offs

- **Time per byte.** Two passes and four SHA-256 streams cost more per byte
  than `main`'s in-memory pass. Task 9.3 records branch against `main` at
  16 MiB. The acceptance is at most 2× `main`'s wall time plus 50 ms. If it
  fails, the follow-up is to drop pass B's body re-hash or to use reflinks.
  At 512 MiB, task 9.3 records the throughput, and progress (D8) makes it
  visible.
- **Disk.** One body per replaced file stays in app data until undo, discard,
  panel close or the next run, as today's journal does. The reserve keeps the
  last GiB free.
- **The external-writer window** between the final facts check and the rename
  is not closable without OS support, and is the same as today's.
- **A regex dialect mismatch** between Rust `regex` and GRegex/PCRE2 can
  prefill a pattern the in-tab bar rejects or matches differently. The user
  sees the bar's invalid-query state or the matches before applying anything.
- **Downgrade** loses the undo affordance of an in-flight journal (D6). The
  files keep the content the user asked for.

## Follow-ups

- **Long lines.** Record the excerpt's byte offset within the source line in
  `SearchMatch`, so the engine can verify the excerpt in place, plus the line
  length, and replace matches on lines longer than 4 KiB. That is a weaker
  stale check than a whole-line comparison, so it needs its own data-safety
  review.
- **Reflinks.** Use reflink (`FICLONE`) for the body on btrfs and XFS, with
  validation reading the clone.
- **No limit.** Past the open tier, with disk admission as the only bound.
