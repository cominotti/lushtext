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
(`.claude/handoffs/power-of-two-thresholds/runs.log` line 382). In that run
the 10 MiB rows were measured on the `main` of the day (before PT) and the
16 MiB rows on the PT branch, which is now `main`. Both are paths `main` runs
today: a file of at most 10 MiB still takes the first pass unchanged. The
replace code has not changed since, apart from d704a908's search-budget edits
(comments in `search_backup.rs` and the preview's row rebinding).

| Case | `main` peak | Growth over the 27 MiB process baseline | Wall |
|---|---|---|---|
| one 10 MiB file | 49.0–49.3 MiB | 21.6 MiB (2.16×) | 9–11 ms |
| one 16 MiB file | 61.2–61.4 MiB | 33.9 MiB (2.12×) | 15–18 ms |
| six 10 MiB files (64 MiB undo cap binds: 3 replaced, 3 skipped) | 99.9–100.3 MiB | about 73 MiB | — |
| six 16 MiB files (64 MiB undo cap binds: 1 replaced, 5 skipped) | 94.0–94.4 MiB | about 67 MiB | — |

The last row is a visible loss on `main` today: five of six 16 MiB files are
skipped for undo budget. Growth is linear at about 2.1 times the file size. Taking the per-file cap to
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

**The output is bounded too.** A file whose replacement output would exceed
`MAX_REPLACE_FILE_BYTES` is skipped as `LargerThanOpenTier` ("the replacement
would make it larger than 512 MiB"): pass A's dry run stops at the limit
(`OutputLimitExceeded`, kept). Without this bound Undo could never restore the
file, because its divergence check streams the current file under the same
limit (D6), and LushText could not open the result either.

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
  lines are ineligible. The buffer refuses any byte past the longest
  recorded line of its edits plus one (`\r`), so it holds at most 4 KiB plus
  one byte; a longer line is stale without being buffered. The line is
  compared with `original_line` exactly as today, and an edit applies only
  when the whole line matches.
- The output is the same bytes `build_replaced_text_reference` produces,
  including the final unterminated line, empty lines and CRLF.

The engine reports the source length and digest, the output length and
digest, the edit count and a `ReplacementTextOutcome`. That outcome keeps its
variants, `OutputLimitExceeded` included (D1, D4), and gains only `NotUtf8`.

The engine is generic over its digest sink: production feeds SHA-256, the
tests a sink that records the bytes. Its UTF-8 carry lives in
`replace/stream/utf8.rs`, whose validation has a byte-loop path under
`cfg(kani)`, as `model/paragraph_boundary.rs` has for its SIMD search, because
CBMC cannot run `simdutf8`'s runtime-detected SIMD paths (D11).

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

Bodies live in a subdirectory, `replace-backup-journal/bodies/`, and are
written with a new label, `WriteLabel::REPLACE_UNDO_BODY`.

- **The subdirectory.** Every journal scan is bounded by
  `JOURNAL_SCAN_MAX_ENTRIES` (16,384, asserted at least
  `MAX_REPLACE_PREVIEW_ROWS` and equal to `format_upgrade`'s
  `REPLACE_JOURNAL_SCAN_MAX_ENTRIES`), and the incremental loader counts every
  directory entry, not only `*.json`. A body beside each entry would double a
  full run's directory past that bound and deactivate its Undo. In their own
  directory the bodies get their own scan under the same bound, and every
  existing scan keeps its coverage proof.
- **The label.** It keeps body temps apart from target temps. The
  journal-owned sweep (D9) matches only `WriteLabel::REPLACE`, and a kill point
  narrowed to `@replace` hits the target's write, not the body's.

The per-file order for apply:

- **P0.** Acquire the `TargetWriteGuard` and take the target's facts `F0`
  (size, mtime in ns, dev, ino, canonical path).
- **P1.** Admit the body on the journal's filesystem (D4). Before the run's
  first body, prepare the incremental journal (`begin_incremental_journal`,
  which deletes any previous run's journal), so that no body is ever written
  into a previous run's journal (D3a).
- **P2.** Run **pass A**:
  `atomic_replace_stream(journal/bodies/<hash>.body, REPLACE_UNDO_BODY)`. Its
  producer reads the target in chunks, writes each chunk to the body, hashes
  it as the source, and feeds it to the engine with a **hashing sink** as the
  output: a dry run, so no output is written. At EOF the producer re-reads the
  target's facts. If they differ from `F0` it fails with `ChangedDuringReplace`
  and the body temp is removed (`BeforeRename`). A stale line, invalid UTF-8,
  an invalid preview or cancellation also fail the producer, so the body is
  discarded before any entry exists. If there are no edits (`Unchanged`), the
  body is discarded and the file is not counted. In the first pass the dry
  run's output is limited to `MAIN_REPLACE_FILE_BYTES`, as `main`'s is: a file
  whose output exceeds it fails the producer as deferred, its body temp is
  removed, and it is processed again in the second pass (D4).
- **P3.** Admit the output on the target's filesystem (D4). The output size is
  now known.
- **P4.** Write the entry JSON durably, then arm the manifest once, after the
  run's first entry (as today). The manifest now also records the run's `pid`
  and `launch_nonce` (D9).
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

### D3a. One journal per application

The journal directory is shared by every window, but each window's search
panel owns its own undo (`supersede_prior_undo_for_replace` retires only the
panel that starts a run). On `main` a run in window B deletes window A's
journal on disk, and A's Undo keeps working for the session from the bytes A
holds. With descriptors, A would hold nothing to restore from. So:

- **Supersession is application-wide.** Starting a run retires the undo
  affordance in every window of the application, not only in the panel that
  started it. A window that loses one says so once in its status bar ("Undo
  for the earlier Replace All is no longer available"). The on-disk journal
  already kept only the latest run; this makes the windows agree with it.
- **Refuse, don't queue.** While a Replace All, its rollback or an Undo is in
  flight in any window, an apply or Undo in another window is refused with a
  status message. The panel's transaction gate already refuses a second one in
  the same window. So no worker waits minutes on the journal guard, and Cancel
  never has to reach a worker that has not started.
- **A cancellable wait for the brief holders.** The other holders of the
  journal guard are startup recovery and the journal disk jobs. A run's worker
  waits for them in a `try_lock` loop that polls the cancel flag, so Cancel
  and Stop and Close are seen there too. A journal disk job checks its
  generation before it waits for the guard as well as after, so a superseded
  one returns at once instead of holding a worker slot.
- **Prepare before the first body.** The run prepares the journal, deleting
  any previous run, before its first body. Today it does so only after its
  first file's edits are confirmed. So a run that ends up replacing nothing now
  leaves no previous journal on disk. `main` left that journal in place, no
  window offered it, and a restart could bring it back. The UI had already
  retired that undo when the run started. Test:
  `a_run_that_replaces_nothing_removes_the_superseded_journal`.
- **Shrink only.** A descriptor journal cannot be rebuilt from memory. For
  both kinds, three paths retire: `search_backup::save`'s delete-then-rebuild,
  the non-superset fallback of `shrink_journal_to`, and cancellation's
  `persist_undo_backup`. A partial undo or rollback writes the smaller active
  manifest, then removes the entries and bodies it no longer lists. An entry
  whose JSON or body is already missing on disk is dropped from the undo owner
  and reported, never rebuilt.

*Alternative rejected:* one journal per run (`replace-backup-journal/<run>/`),
so window A keeps its Undo. Recovery, the format-upgrade inventory and the
cleanup would all have to handle several active runs, for a case (two windows
each with a pending Replace All Undo) that `main` already fails to preserve
across a restart.

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

**No-loss waiver.** The reserve is waived for every file `main` would have
admitted. Here `main` is `main` at `2c91ccf2`, whose first pass still uses the
pre-PT 10 MiB limit. A pure `MainAdmissionLedger` replays `main`'s rule
(`apply_replacements_inner`):

- a first pass, in path order, over the files whose source and output are
  both at most `MAIN_REPLACE_FILE_BYTES` (10 MiB); a file whose source or
  output is larger is deferred;
- a second pass, in path order, over the deferred files whose source and
  output are both at most `MAIN_MAX_REPLACE_FILE_BYTES` (16 MiB);
- each admitted file charges its source plus output length against
  `MAIN_REPLACE_UNDO_BYTES` (64 MiB), and its retained entry weight
  (`replace_undo_entry_retained_byte_weight`: both payloads plus the path)
  against `MAX_REPLACE_UNDO_RETAINED_BYTES`; a file that does not fit both is
  not admitted;
- a file that fails before rename reclaims both charges, as `main` does;
- a file whose source or output is above 16 MiB never charges.

The processing order keeps `main`'s two passes, with the second pass extended
to 512 MiB. A first-pass file whose output turns out larger than 10 MiB is
deferred at the end of pass A (D3), exactly where `main` deferred it. So the
ledger admits every file `main` would have admitted. It can admit more only
after a pre-rename failure that `main` would not have had
(`ChangedDuringReplace`), whose charge `main` would have kept, and that never
costs a file `main` replaced. For a waived file only the raw need applies,
which `main` also needed (it wrote both snapshots as JSON).

**Why the reserve protects the waived files.** After any file that is not
waived, at least the reserve stays free on both filesystems: it was admitted
with its write plus the reserve, and a completed rewrite frees the old blocks.
The waived files of a run need at most `MAIN_REPLACE_UNDO_BYTES` of bodies,
plus one output at a time of at most `MAIN_MAX_REPLACE_FILE_BYTES`. So the
1 GiB reserve leaves room for all of them, unless another program fills the
disk, and then a write fails before rename, as on `main`. A compile-time
assertion pins `REPLACE_DISK_RESERVE_BYTES >= MAIN_REPLACE_UNDO_BYTES +
MAIN_MAX_REPLACE_FILE_BYTES`. `MAIN_REPLACE_FILE_BYTES` (10 MiB),
`MAIN_MAX_REPLACE_FILE_BYTES` (16 MiB) and `MAIN_REPLACE_UNDO_BYTES` (64 MiB)
stay as recorded presentation-cap exceptions. They are defined as `main`'s
values.

**When the waiver is decided.** The waiver needs the output length, which
exists only after pass A, but the body is admitted at P1, before pass A. So
the decision is made in two steps.

- **P1, provisional.** A file is provisionally waived when its source is
  within `main`'s limit for the current pass and its source length fits the
  ledger's remaining room. That holds for every file `main` admitted, because
  `main` charged source plus output, which is at least the source. A
  provisionally waived body needs only its raw size free.
- **P3, final.** With the output length known, the ledger charges source
  plus output exactly as `main` did. If the charge fits, the waiver stands.
  If it does not, the file needed the reserve after all: P3 re-checks free
  space with the reserve on both filesystems. When that fails, the body is
  deleted (no entry exists yet) and the file is skipped as
  `NotEnoughDiskSpace`, exactly where `main` would have skipped it for its
  undo payload.

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
- **U2.** Admit the restore output on the target's filesystem with its raw
  need only, never the reserve. `main`'s undo also wrote a temp of the
  original's size beside the target, with no reserve. Requiring 1 GiB free
  would refuse an Undo, or a cancellation's rollback, that `main` performed.
  A file without its raw need free is skipped and stays retryable, and an
  `ENOSPC` during the write is a `BeforeRename` failure, which also stays
  retryable. Then run the same `atomic_replace_workspace_file_stream` wrapper
  as P5. Its producer copies the body while hashing it, and at EOF requires
  the body digest to equal the original's and the target facts to equal U0's.
  Otherwise nothing is renamed.

  Before the first restore write of an Undo that runs in a later launch than
  the one the manifest records, the leftover sweep (D9) runs for the recorded
  pair. The manifest is then durably re-armed with the current launch's pid
  and nonce. A kill during that Undo's 512 MiB restore then leaves a temp that
  the next recovery owns.
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
payload bound (`payload_budget_exceeded`), which becomes
`MAIN_REPLACE_UNDO_BYTES`, referenced; `MAX_REPLACE_UNDO_BYTES` retires as a
name. It turns each entry into a descriptor whose original is
`UndoOriginal::LegacyEntry { entry_file }`, with the lengths and digests
computed at load. The inline strings are dropped on the loading worker. Undo
re-reads that one entry on its worker, under the same bound, and streams the
original from it. So GTK holds only descriptors for a legacy journal too, and
at most one legacy entry's bytes (at most 64 MiB) are in memory, on a worker.
New runs never write the legacy kind. Mixed journals, with legacy and stream entries
in one manifest, are rejected as malformed: one run writes one kind.

**Downgrade.** An older build sees an entry of an unknown kind and treats it
as malformed. It must not expose undo, write a target or read a body. Its
incremental loader reads only `*.json` files, so the new entries are malformed
to it and `bodies/` is never opened. Its ordinary cleanup (`delete`, a
recursive removal of the journal directory) may remove the bodies. That loses
only the undo affordance, which the downgrade loses anyway. The target files
hold the replaced content the user asked for. Task 7.6 verifies this with a
fixture and records it.

*Verified (task 7.6).* `main_loader_exposes_no_undo_for_a_streamed_journal`
(`crates/lushtext-core/tests/persistent_json_format.rs`) vendors `main`'s
entry and manifest shapes and feeds them a journal this branch writes:
`main` parses the manifest (serde ignores `pid` and `launch_nonce`), rejects
every streamed entry with `UnsupportedFormat` (wrong kind), which its loader
quarantines as malformed, so the journal is inactive and no Undo is offered,
and the bodies sit in `bodies/` as non-`.json` files its non-recursive entry
scan skips. Nothing in `main` writes a target from such a journal.

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

**One reason per file.** A file that qualifies for two gets the first in this
order:

1. `UnsavedChanges`, decided on GTK before any worker runs;
2. the preview reasons, `LineTooLong` then `InvalidPreview`, for a file none
   of whose matches was eligible;
3. the worker's reasons, in the order it checks them: `Unreadable`,
   `LargerThanOpenTier`, `NotUtf8`, `ChangedSinceSearch`,
   `NotEnoughDiskSpace`, `UndoJournalFailed`, `ChangedDuringReplace`,
   `WriteFailed`.

`LineTooLong` and `InvalidPreview` come from the preview's ineligible matches,
grouped by file. A file can be **partly replaced**: its eligible matches are
replaced and some matches on long lines are not. It is listed too, with
`partial: true` and the count of matches left. Its row says "Replaced; N
matches on long lines were not", and offers the editor action. The user
still has matches that were not replaced, and a list of files that "were not
replaced" would hide exactly the files that need finishing. The report counts
partly replaced files separately from skipped ones, so the per-reason skip
counts stay exact and a partly replaced file is counted as replaced.

**Every unsaved file.** When every file of a run is open with unsaved
changes, `ui/window/search.rs` today returns early with "No replacements to
apply…" before any worker runs. That path now builds the same skip report on
GTK, with one `UnsavedChanges` row per file, bounded by the preview's file
count. The panel shows Show Skipped Files, where the editor action matters
most.

One Replace All touches at most `MAX_REPLACE_PREVIEW_ROWS`
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
  called presentation surface). Its `GtkListView` reads a lazy `GListModel`
  plus `GtkSectionModel`, built like the notes browser's
  (`ui/window/notes/row_model.rs`, ledger A93–A95). Rows are created only when
  GTK asks for them, and the reason headings come from a header factory, so
  opening the dialog costs the same at 3 rows as at 16,384. A `GListStore` of
  row objects would create one GObject per row on GTK.
- The report crosses to GTK as a plain-disposal-owned payload (at most
  `MAX_REPLACE_SKIP_LIST_BYTES` of paths). It is retired off GTK when the next
  run replaces it.
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

The query, the options and the template come from the run's request. They are
captured when the run starts and kept with its skip report, never read from
the panel's live fields, which the user may have edited since.

In literal mode both fields are prefilled. In regex mode the pattern is
prefilled unchanged, and the template goes through the pure
`rust_replacement_to_gtksourceview(template, capture_names) -> Option<String>`,
which takes the compiled pattern's capture names:

- A reference follows Rust `regex`'s rule (`Captures::expand`): `$` followed
  by the longest run of `[_0-9A-Za-z]`, or `${…}`. A run of digits names a
  group by number and anything else names it by name, so `$1a` names a group
  called `1a`.
- Every reference becomes `\g<N>` or `\g<name>`, never `\N`: `${1}0`
  becomes `\g<1>0`, where `\10` would name group 10.
- `$$` becomes `$`, and a `$` that starts no reference stays a literal `$`.
- A literal backslash becomes `\\`.
- A reference to a group the pattern does not have returns `None`. Rust
  expands it to nothing, and GtkSourceView would fail it.

When it returns `None`, the replacement field is left empty and the bar says
why. Case and whole-word settings carry over. The user reviews and applies in
the editor, and nothing is written until the user saves, so the action is
data-safe by construction. In-tab Undo also covers it, but only below the
editor's undo tier; above that tier the save is still the only write.

Two toolkit facts are pinned as GTK axioms with probes and samples (task 1.3):

- GtkSourceView's regex replacement syntax: `\g<N>`, `\N` and `\g<name>`
  expand, `\g<1>0` is group 1 followed by `0`, and `$1` is literal.
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
  shrinks the journal to the files it could not restore, keeps it active, and
  offers Undo for them (D3a). A worker still waiting for the journal guard
  sees the flag in its cancellable wait (D3a) and ends without touching a
  file.
- **Close.** Today nothing checks for a run, and closing the last window
  can end the process while the worker writes. The journal makes that
  recoverable, but it looks like a crash.

  The save precedent (`close_request` refuses while an editor saves, and
  publishes a warning) fits a write of milliseconds, not a run of minutes. So
  a `close_request` during a run opens an `AdwAlertDialog`, "Replace All Is
  Still Running", with three responses:
  - **Keep Running** (default): nothing closes;
  - **Close When Finished**: the close is deferred until the worker's
    terminal, and the replacements complete;
  - **Stop and Close**: the run is cancelled, its written files are rolled
    back, and the window closes after the terminal.

  Every close waits for the terminal, then re-enters the normal close-safety
  path (drafts, session). Application quit takes the same path. Rollback
  keeps its data-safety ordering. A process kill at any point is covered by
  the journal and D9.

  *Alternative rejected:* refuse and warn, as for a save. The user then has
  to find Cancel or wait with no way to say "close when done".

### D9. Recovery and cleanup know bodies and journal-owned temp files

- **Entries and bodies.** Startup recovery loads descriptors only. It stats
  each entry's body.
  - A missing body, or one whose size differs from `original.len`, makes its
    entry malformed. As for any malformed entry on `main`, the journal then
    offers no Undo (`journal_is_active`): the malformed entry is copied to
    the recovery quarantine as evidence, the loader writes the cleanup marker
    (implementation decision 5, so the journal stays inactive across a crash
    before cleanup), and startup cleanup removes the inactive journal, the
    other entries' bodies included. Those bodies are the only copies of their
    files' originals, so this costs the Undo of the whole run, never a file's
    current bytes: every target keeps the replaced content the user
    confirmed, as on `main`, whose cleanup deletes the inline originals the
    same way. Offering Undo for the undamaged subset is rejected: it would
    restore part of one confirmed run without saying which part is missing.
    Task 3.5b pins this end to end.
  - An orphan body, one with no entry, is benign and is not a diagnostic.
    A crash between P2 and P4 leaves one, and so does a crash between an
    entry's removal and its body's. It must not cost Undo for the files
    written before it.
  - Cleanup, `delete` and `shrink_journal_to` remove orphan bodies, scanning
    `bodies/` under the same bound as the entries.
- **Temp files.** The manifest records the `pid` and `launch_nonce` of the
  apply run, or of a later launch's Undo once that Undo re-arms it (D6).
  Both can be read back from a temp name: `temp_name::parse` returns the pid,
  and `DurableTempName::launch_nonce` returns the high 32 bits of the
  sequence. On recovery, after undo, and after a discard, for each journaled
  target the cleanup removes the leftovers in the target's own directory that
  satisfy the new `leftovers::is_journal_owned_leftover`, scanning each
  distinct parent directory once:
  - the exact builder pattern;
  - the tag `WriteLabel::REPLACE`;
  - the (pid, nonce) pair equal to the manifest's;
  - that pair differing from the current launch's (pid, nonce);
  - a regular file (lstat).

  The predicate keys on the pair, as `is_sweepable_leftover`'s `this_launch`
  test already does, and never on the pid alone. Inside the Flatpak sandbox's
  pid namespace a relaunch can reuse the crashed process's pid, but never its
  nonce. A temp from the current launch is never removed, because only a
  journal from an earlier launch is recovered.

  The 24-hour age rule stays for every other sweep. A recorded nonce proves
  the temp belongs to the crashed run, so no age is needed. A user workspace
  is still never traversed: only the journaled targets' parent directories are
  read, one bounded scan each.
- **Kill windows.** New `KillWindow`s:
  - `replace-body-before-entry`: the body is renamed into `bodies/`, and its
    entry is not written yet;
  - `replace-entry-before-target-rename`: inside pass B's producer at EOF,
    after the last chunk of the target temp is written and before the producer
    returns, so the temp exists and the rename has not happened;
  - `replace-target-renamed-before-finish`: after the target's durable write
    returned, before the next file;
  - `replace-undo-before-target-rename`: the same point as the second one, in
    an Undo's restore.

  `LUSHTEXT_KILL_AT` gains an occurrence ordinal: `name@label#n` fires at the
  n-th reach. So the smoke can kill on the third file of five, after the first
  two were replaced. Kill points terminate the process with `abort()`, as the
  existing ones do. The crash-recovery smoke drives Replace All through
  automation, kills at each window, relaunches, and asserts the outcome: the
  journal state, the undo affordance, the target bytes, and no leftover temp.

### D10. GTK never waits on a guard

The journal guard and target guards are only ever taken on workers
(`spawn_blocking_then`), and the change keeps that true. A run now holds the
journal guard for seconds, so three things wait their turn on a worker, never
on GTK:

- the panel-close journal delete;
- an undo;
- a new Replace All.

An editor save of a file being rewritten waits on its target guard on its
worker, for at most one file's passes A and B (the guard spans P0–P6). This
is recorded. The responsiveness requirement adds the scenario.

A run holds one of the eight `spawn_blocking_then` slots for its whole
duration, as today but for longer. D3a keeps it the only long holder: a second
run or an Undo is refused rather than queued, and a superseded journal disk
job returns before it waits for the guard.

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

  The abstract journal also models the application-wide refusal (D3a): at
  most one run or Undo mutates it at a time.
- **`core-replace-stream`** proves the stream's **UTF-8 seams**
  (`replace/stream/utf8.rs`, its own module so the proof's oracle is exactly
  the code it checks): fed in chunks of 1 to 4 bytes, `Utf8Carry` rejects
  exactly the sources of at most 8 arbitrary bytes that one validation of the
  whole source rejects (`utf8_seams_do_not_depend_on_the_chunk_size`), and the
  `cfg(kani)` byte scan it runs on equals `core::str::from_utf8` on every input
  of at most 4 bytes (`the_byte_scan_matches_core_from_utf8`). `simdutf8`'s
  runtime-detected SIMD is out of CBMC's reach. Unwinding
  `core::str::from_utf8` over 8 symbolic bytes dominated the trace of the
  first one-harness run that ran out of memory (implementation decision 22,
  an attribution from the trace, not an isolated probe). So the harness
  validates with the byte scan, which the unit test
  `the_byte_scan_matches_simdutf8` ties to the production validator on every
  sequence of class-edge bytes up to 5 long. The seam harness's whole-source
  verdict over 8 bytes is therefore the byte scan's: its equality with
  `core::str::from_utf8` is proved up to 4 bytes and tested against
  `simdutf8` up to 5 class-edge bytes, not proved at 8. Every chunk and
  every carried sequence the harness validates is at most 4 bytes, so the
  carry's own calls stay inside the proved domain.
- **The engine's line logic is not a Kani proof** (see "Kani memory" below).
  Its split independence is checked **exhaustively over bounded domains** by
  the unit test
  `the_engine_does_not_depend_on_its_chunk_size_over_a_bounded_domain`: every
  source of at most 4 characters over `a`, `b`, `\r`, `\n` and `é` (781
  sources), no edit and every edit set of at most two edits over the lines
  `""`, `a`, `ab`, `\r` and the replacements `""`, `x\n` (lines 1–3), every
  chunk size from 1 to the source length, and two output limits (none, and
  one byte below the whole output). At each point the chunked feed's outcome
  equals the whole feed's, a replaced file's output, count and digests equal
  the whole feed's, a replaced file equals the reference builder
  (`build_replaced_text_reference`), the output stays within the limit, and
  the digests record exactly the bytes fed and written. It ran in 7.5 s in a
  debug build in stage 1; with task 3.3c's additions (below) it runs in
  17.6 s (debug, at a 1-minute load of 9.4), so it stays in the ordinary
  unit suite. Two logged mutants
  fail it (`failing-first.log`): one that mishandles a lone `\r` line (at the
  reference comparison) and a seam-only one, in which `collect` keeps only
  the last piece of a targeted line split across chunks (at the chunked
  feed's outcome). The property
  test `streaming_engine_matches_the_reference_at_every_chunk_size` samples
  larger inputs, chunk sizes 1 to 16, against the reference.

  **Stage-2 review additions (task 3.3c).** The stage-1 domain has two gaps
  that the retired engine harness `an_invalid_source_is_never_replaced`
  (arbitrary bytes through the whole engine) used to close:
  - every source is valid UTF-8, so no point sends an invalid source
    through the engine; only the one example of
    `the_engine_reports_a_stale_line_and_invalid_utf8_split_across_chunks`
    does. Task 3.3c adds a second domain, every source of at most 3 symbols
    over the five plus `0x80`, a lone `0xC3` and `0xFF`, with the same edit
    sets, chunk sizes and limits. Validity is computed per source, because
    some of these sequences are valid (`0xC3 0x80` is U+00C0). No point is
    `Replaced` unless its source is valid UTF-8, an invalid source ends `NotUtf8`, `StaleLine` or
    `OutputLimitExceeded` at every chunk size, and with no edit and no limit
    it ends `NotUtf8` at every chunk size;
  - no recorded line holds `é`, so a character cut by a seam is replaced
    only on an untargeted line, and the collected-line path never sees a
    cut character inside a line that matches. Task 3.3c adds a recorded line
    with `é`.

  *Done (stage 3).* The line table is `""`, `a`, `ab`, `\r`, `aé` (ranges on
  character boundaries only, as a preview records them) and the replacements
  `""`, `x\n`. The second domain is every source of at most 3 symbols over
  `a`, `b`, `\r`, `\n`, `é`, `0x80`, `0xC3`, `0xFF` (585 sources), with no
  edit and every **single** edit, not the two-edit sets: the single edits
  already reach every way an invalid byte can meet a targeted line, and the
  two-edit sets would multiply the run time. A coverage guard counts each
  case the retired harness covered (two edits applied, the limit stopping a
  file, a stale line, a CRLF seam, an unterminated final line replaced
  across a seam, a cut character inside a replaced targeted line, an invalid
  source split by a seam) and fails at zero. The logged mutant in which
  `finish_outcome` skips `Utf8Carry::finish` fails it (a lone `0xC3` is
  `Replaced`). The whole check runs in 17.6 s in a debug build, above the
  15 s estimate; the tables were not trimmed, because each one feeds a
  coverage case.

  Its coverage guard also counts every case the retired covers named (two
  edits applied, the limit stopping a file, a stale line, an unterminated
  final line replaced across a seam, a cut character inside a targeted
  line, an invalid source split by a seam), so no part of the domain is
  vacuous.

Both shards are scheduled. Their runner budgets come from two `kani.yml`
dispatches, the larger of the two (task 3.3b, `build.md`, "Shard gates and
budgets"), and `make check-kani-shards` checks them against the margins.
`core-replace-protocol` is gated on pull requests if its measured wall time
is within the PR-gate budget of the shard table. The change adds harnesses,
so it also owes `make proof-strength` for both oracle modules (task 9.4).

#### Kani memory (stage-1 revision, 2026-11-17)

The implementation stage's one-harness form (8 arbitrary bytes, chunks 1–4,
two edits from a table, a symbolic limit, `core::str::from_utf8` as the
oracle) ran CBMC out of memory at the 16 GiB cap twice (13.63 GiB peak RSS,
16.8 min, in symbolic execution). The CI margin is stricter than the local cap:
`MAX_SHARD_PEAK_GIB` is 12 GiB and `MAX_SHARD_MINUTES` 25, summed over the
shard. The revision fitted it by the runbook's order (split, then solver,
then a recorded bound reduction). Step 2 did not apply: every engine probe
was still in symbolic execution when it was stopped, before any solver ran,
so a solver choice could not change its memory. The probes ran one at a time through
`scripts/heavy.sh --mem 16G` in a source snapshot, each stopped once its CBMC
passed 12 GiB (or 8 GiB in the last ladder), since a run past the margin is
disqualified whatever it would have returned (handoff `runs.log` and
`kani-probes-summary.txt`):

| Probe | Peak / wall | Result |
|---|---|---|
| UTF-8 seams alone (`Utf8Carry`, 8 bytes, chunks 1–4, byte-scan oracle) | 3.44 GiB, 161.6 s | verified |
| Byte scan vs `core::str::from_utf8` (4 bytes) | 1.75 GiB, 24.0 s | verified |
| Line seams, 4 bytes over `a b \r \n`, symbolic edit text, chunks 1–4, unwind 8 | 12.05 GiB at 7.5 min | stopped |
| the same with a fixed-capacity `cfg(kani)` line buffer | 12.2 GiB at 6.5 min | stopped; no gain, dropped |
| static edit tables, chunks 1–3 | 11.8 GiB at 5.2 min, rising | stopped |
| plus a loop-free `write_all` in the harness sink (the default loop unrolled 426 times), unwind 5 | 12.0 GiB at 9.7 min | stopped |
| ladder: concrete source length and chunk per harness, N = 4, 3, 2 (9 harnesses, down to one engine run over 2 bytes with one fixed edit) | every one past 8 GiB in 3.7–4.0 min | stopped |
| one engine run over 2 symbolic bytes, one fixed edit, **no sinks** (`io::sink`, no digests) | 8.47 GiB at 3.4 min | stopped |

Every probe was stopped while still growing, so none shows where CBMC would
have ended. The four-byte forms were past the 12 GiB margin when stopped.
The ladder and the no-sink run were stopped just past 8 GiB (8.0–8.5 GiB),
under the margin, so that they do not fit is an extrapolation from their growth, not a
measurement. The last row points at the engine itself under CBMC, not the
harness's domain, sinks or comparisons: its trace is dominated by
slice-index and `copy_from_slice` failure paths and `memcmp`, with 18,630
inlined slice-index failure paths for a 2-byte source. That count fits
nested loop unrolling: every ladder harness ran at `unwind(6)`, although a
2-byte source needs about 3. No probe lowered the unwind bound to the
domain's need. So the record does not show that no smaller bound keeps a
property worth stating; it shows that none of the forms tried fitted, and
that the next probe to try is a per-domain unwind (deferral inventory). The decision, step 3 of the fit order:
keep the two UTF-8 harnesses (3.72 min and 3.42 GiB in one local shard run; the
table records 4.0 min / 4.0 GiB pending dispatch), and move the line logic to
the bounded exhaustive unit test above, which checks every point of a larger
domain than any Kani form could hold and compares with the reference builder
as well as the whole feed. What this weakens:
- the line logic's split independence is established for the enumerated
  domains and sampled beyond them, not proved for every byte value;
- the byte classes the line logic distinguishes (`\n`, `\r`, equality with
  the recorded line, a multi-byte character cut by a seam, inside a targeted
  line once task 3.3c lands) are all in the alphabet;
- "an invalid source is never replaced, at any chunk size" was a Kani
  property of the retired engine harness, over 5 arbitrary bytes. It is now
  the carry's proof composed by reading the code (`feed` checks the carry
  before the line logic sees a chunk, `finish_outcome` checks
  `Utf8Carry::finish` before any `Replaced`), plus task 3.3c's bounded
  invalid-byte domain. The `cfg(kani)` `find_newline` byte loop and
the harness-only line buffer were removed with it, so the engine has no
`cfg(kani)` code left. Proving the line logic at another altitude (a
byte-step line cursor whose split independence holds by construction) is a
follow-up in `docs/next/formal-verification.md`, "Deferral inventory".

### D12. Module layout and the readability row

`services/content_search/replace.rs` (3,255 lines) becomes `replace/`:

| File | Holds |
|---|---|
| `mod.rs` | the entry points |
| `stream.rs` | the engine |
| `stream/utf8.rs` and `stream/utf8/kani_proofs.rs` | the incremental UTF-8 carry and its proofs (stage-1 revision, D11) |
| `protocol.rs` and `protocol/kani_proofs.rs` | the per-file protocols and their proofs |
| `apply.rs` | the apply orchestration |
| `undo.rs` | undo and rollback |
| `admission.rs` | the no-loss ledger and disk admission, pure |

The split also moves the `.cargo/mutants.toml` entries keyed on
`content_search/replace\.rs`, and reviews `sonar-project.properties`
(`.agents/rules/build.md`, "SonarQube Cloud Scope").

The UI side adds `ui/search_panel/skipped_files.rs` as a called presentation
surface, and extends `policy.rs` and `evidence.rs`. `mod.rs` is at its
370-line facade budget, so the new stages (progress, cancel, skipped list) are
narrated by turning existing prose into delegation names, and the facade must
not exceed 370. The `WFR-SEARCH-REPLACE` row re-derives its measured cells
(`.agents/rules/workflow-convention.md`).

## Risks / Trade-offs

- **Many small files.** Each file now takes three durable writes (body,
  entry, target) where `main` takes two (entry, target), plus a second read.
  Task 9.3 measures 2,000 small files on an `fsync`-backed directory,
  interleaved with `main`. The acceptance is at most 1.6× `main`'s wall time.
  If that fails, the candidate fallback is to embed originals of at most
  64 KiB in the entry, as the legacy kind did but with digests, so the
  separate body file is not written. It is not part of this change's
  requirements. Applying it is a spec amendment, made before the code: the
  journal requirement, the format fixtures, the Kani protocol and the
  descriptor rule.
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
- **Two windows** no longer keep two Undos for one session (D3a). The status
  message says so when it happens.
- **Test scale.** A cap-boundary test cannot build 512 MiB in memory or on a
  tmpfs. The tests run against a test-only per-file limit at KiB scale, the
  real limit is reached only through sparse files and metadata, and every
  large fixture lives in a disk-backed scratch directory, never `/tmp` on
  tmpfs, whose pages would count against the `heavy.sh` memory cap.

## Follow-ups

- **Long lines.** Record the excerpt's byte offset within the source line in
  `SearchMatch` (the line's byte length is already recorded,
  `original_line_byte_len`), so the engine can verify the excerpt in place,
  and replace matches on lines longer than 4 KiB. That is a weaker
  stale check than a whole-line comparison, so it needs its own data-safety
  review.
- **Reflinks.** Use reflink (`FICLONE`) for the body on btrfs and XFS, with
  validation reading the clone.
- **No limit.** Past the open tier, with disk admission as the only bound.

## Stage-1 revision (2026-11-17)

The implementation stage ended with 26 of 53 boxes open (its record is the
handoff's `decisions.md`, entries 1–22, and `pr-body.md`). No stage-4 review
has run yet, so this revision is a planning revision: it merges `main`, fits
the Kani proof, and re-plans what is left so the change can pass its own
bars. Each decision is user experience first, and data safety above all,
because Replace All writes users' files.

**R1. `main` merged at `c16d6fdc`** (commit `03df8bd8`). The only conflict was
the two new sections at the end of `docs/next/long-line-documents.md`; both
are kept. The merge brought no `services/` change: `main`'s Replace All code is
the same as at `2c91ccf2` apart from one test's call of the reworked
`fail_next_parent_sync_for_test`, and the durable-rename classification of
#105 was already merged in `a1f58d4e`. The new gates of #110 and #112
(`check-session-hygiene`, `check-signal-connections`) pass on the branch as it
is. The three smokes were re-run on the merged tree before the commit.

**R2. Kani fits by a recorded bound reduction** (D11, "Kani memory"). The
stream shard keeps the two UTF-8 harnesses, now in `replace/stream/utf8.rs`;
the line logic is checked by the bounded exhaustive unit test. The shard ran
once on the final harness inputs, from the worktree (`make kani
KANI_SHARD=core-replace-stream KANI_MEASURE`, 2026-11-17): both verified, all
five covers satisfied, 3.72 min and 3.42 GiB, recorded as the pending local
figure (4.0 min, 4.0 GiB). The
protocol shard is unchanged (3/3 verified on `23ef433a`, 0.34 min,
1.59 GiB); its mutation check (task 3.2a) is still owed.

**R3. Memory figures run through `heavy.sh`; only timing is queued.**
`parallel-agent-work.md` sends timing verdicts through `measure.sh`. A peak
RSS (`VmHWM`, reset first, one process per call) does not depend on the load,
so the M9 memory cases (tasks 1.1, 4.8, 9.2) run through `scripts/heavy.sh`
and need no quiet window; only their wall-time columns are non-verdict there.
Decision 9 of the implementation record ran the M9 campaign inside
`measure.sh` with `cargo test` compiling under the exclusive lock, which held
every agent's heavy work for about ten minutes a run; the queued rounds now
time only a pre-built binary. The one `main` repetition that run produced is
valid memory evidence (`mem/evidence.txt`, the `main` of 2026-11-14, whose Replace
All code equals `c16d6fdc`'s):

| Case | `main` peak (2026-11-14) | PT's figure | Difference |
|---|---|---|---|
| one 10 MiB file | 48.39 MiB | 49.0–49.3 MiB | −1.2 % |
| one 16 MiB file | 60.25 MiB (growth 33.2 MiB over 27.08 MiB) | 61.2–61.4 MiB | −1.6 % |
| six 10 MiB files (3 replaced) | 97.89 MiB | 99.9–100.3 MiB | −2.0 % |
| six 16 MiB files (1 replaced) | 92.49 MiB | 94.0–94.4 MiB | −1.6 % |

Every row is within the 5 % bar of task 1.1, so the baseline stands; task 1.1
owes the other two repetitions and the `fsync`-backed wall-time case. The
branch's acceptance in 4.8 keeps `main`'s 33.9 MiB (PT's figure, the larger).

**R4. Test continuity is a task, not an assumption.** 48 test names of
`main`'s `replace.rs` (`main-replace-tests-missing-on-branch.txt`) have no
test of the same name on the branch; decision 19 says the behaviours moved
to the stream-engine, apply/undo and ledger tests, but no record maps them.
Several have no visible successor: the waits on an existing save guard and on
startup journal recovery, the cancel path, an undo journal that cannot be
persisted, the nonexistent file, the skip paths, the exact-cap entry and file,
the partial-restore failure, the rollback refusals of a file changed after
replacement or mid-rollback, and the per-target hook isolation across
interleaved undos. Task 4.10 maps every name to a successor or a new test, and
proves each successor kills a logged mutant of the behaviour it guards.

**R5. A cancelled file whose entry or body removal fails gets a service
test** (decision 21 rests on the U1 reasoning and the Undo harness only).
Task 4.11, failing-first through a removal fault seam.

**R6. Commit plan.** The smokes fingerprint file contents. The source of
truth is two scripts: `scripts/accessibility_source_fingerprint.py`
(`RELEVANT_PREFIXES` and `RELEVANT_EXACT_PATHS`) and
`scripts/check-visual-proof-policy.py` (`VISUAL_SENSITIVE_PREFIXES`,
`VISUAL_SENSITIVE_EXACT`, `VISUAL_SENSITIVE_SUFFIXES`). At `c16d6fdc` they
cover:
- the prefixes `crates/lushtext-core/src/ui/`,
  `crates/lushtext/tests/widget/`, `resources/ui/`, `resources/style/`,
  `crates/cargo-gtk-proof/src/` and `scripts/visual-geometry-scenarios/`;
- every `.blp`, `.css` and `.ui` file;
- `crates/lushtext-core/src/model/automation.rs` and
  `scripts/lushtext-automation.py`;
- the smoke runners and helpers (`scripts/run-accessibility-smoke.sh`,
  `scripts/run-visual-smoke.sh`, `scripts/smoke_warning_classifiers.py`,
  `scripts/accessibility_warning_allowlist.py`, the visual-geometry scripts
  and the gtk-agentic-debugging AT-SPI scripts);
- `docs/accessibility*.md`, `docs/automation*.md` and
  `docs/end-user-coverage.md`.

So the remaining work lands non-visual first, in groups that touch none of
those paths (G1 to G4 in `tasks.md`). Every fingerprinted edit lands in
**one** visual commit (G5), with the three smokes run once on its final tree.
A G1–G4 task that turns out to need a fingerprinted path moves that edit to
G5. For example, if G3's crash smoke needed a new automation-client command
or snapshot field, that edit would move. Measurement verdicts come after G5,
on the final code. A verdict failure that needs a fingerprinted edit repeats
the smokes once more.

**R7. What the waiting formal-verification changes build on.** Recorded in
`docs/next/formal-verification.md`, "Replace All protocol and stream
(`stream-replace-all`)": `durable_write.rs` and `write_protocol.rs` are
unchanged on this branch; the overlap is `filesystem/write.rs`
(`atomic_replace_workspace_file_stream`), `WriteLabel::REPLACE_UNDO_BODY`
(`KNOWN` 10 → 11), `filesystem/leftovers.rs` (`is_journal_owned_leftover`),
`services/kill_point.rs` (four windows and the `#n` ordinal),
`services/search_backup.rs` and the journal's `bodies/` directory, and the
I/O-free `replace/protocol.rs` machines.

**R8. Housekeeping.** The handoff's `wip-20261115-0002.patch` is a
pre-commit snapshot that `2adaf083` superseded; it is not applied. The probe
directories `kani2/`, `kani-src2/` and `kani-target2/` are deleted; their
peaks, wall times and the last probe harness stay in `runs.log` and
`kani-probes-summary.txt`. The task rules lost the stale
`taskset -c 8-23` pin, the retry-until-quiet verdict rule and the PR #99
exclusion.

## Stage-2 review of the revision (2026-11-17)

An independent review of the stage-1 revision checked it against the code,
the rules, the Kani shard table and the handoff's probe logs. It kept every
R1–R8 decision, including the bound reduction (R2). Its corrections:

- **S1. The bound reduction's record states only what was measured** (D11,
  "Kani memory"):
  - only the four-byte forms were measured past 12 GiB; that the ladder and
    the no-sink run would not fit is an extrapolation;
  - step 2 of the fit order did not apply, because every probe was still in
    symbolic execution, before any solver ran;
  - the ladder ran at `unwind(6)` for sources that need about 3, and its
    trace fits nested loop unrolling. So a per-domain unwind is the first
    probe of the deferral entry. The record no longer claims that no smaller
    bound keeps a property worth stating.

  The same wording is now in the delta spec, the programme record and the
  `kani-shards.py` comment.
- **S2. The property that left with the engine harness is restored as a
  bounded check** (task 3.3c, delta scenario "An invalid source is never
  replaced"). The retired harness `an_invalid_source_is_never_replaced` ran
  arbitrary bytes through the whole engine. The stage-1 unit domain is
  valid UTF-8 only, and its recorded lines never hold `é`. Replace All
  writes users' files, so "a non-UTF-8 file is never rewritten, however it is
  chunked" keeps a checked statement. The test also gains a coverage count
  for each retired cover, and `ReplaceStream`'s rustdoc stops promising more
  than the checks cover.
- **S3. The commit plan names every fingerprinted path** (R6). The two gate
  scripts also fingerprint `model/automation.rs`,
  `scripts/lushtext-automation.py`, the smoke runners and helpers,
  `crates/cargo-gtk-proof/src/`, `scripts/visual-geometry-scenarios/` and
  every `.blp`, `.css` and `.ui` file. A G1–G4 edit that needs one of them
  moves to G5.
- **S4. The chunk-size verdict holds up no implementation task** (task 2.4).
  `parallel-agent-work.md` says a queued verdict never holds up
  implementation, and `stream.rs` is not fingerprinted. So G5 does not wait
  for part A. A size other than the shipped 1 MiB lands in its own commit
  whenever part A reports, and parts B and C and the memory rounds run on a
  tree with both.
- **S5. A path-keyed gate names a missing test** (task 4.10).
  `scripts/run-performance-smoke.sh`'s `replace_undo_workflows` filter runs
  `parallel_undo_operations_consume_only_their_own_target_hooks`, which the
  branch no longer has, so that lane fails as stale. The task restores the
  test under that name with its evidence label, or re-keys the script, and
  runs the filter.
- **S6. Proof strength is owed** (task 9.4). The change adds harnesses, so
  `make proof-strength` runs once for each of the two oracle modules.
  `core-replace-protocol`'s final measured run also replaces the placeholder
  oracle seconds (60, 60 and 10).
- **S7. The dispatch step names its exception** (task 3.3b): the stage that
  owns the merge watches its own `kani.yml` runs under
  `parallel-agent-work.md`'s CI-evidence exception, before it takes
  `MERGE-LOCK`.
