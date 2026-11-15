## Why

Workspace search has no per-file size limit (`services/content_search/search.rs`
streams any file through `grep-searcher`). So Replace Preview offers matches in
a 40 MiB log or a 300 MiB data dump, and then Replace All skips the file. Two
limits cause the skip. The per-file limit is `MAX_REPLACE_FILE_BYTES`, 16 MiB.
The aggregate undo payload is `MAX_REPLACE_UNDO_BYTES`, 64 MiB of before and
after bytes per run. The user learns about the skip only as a count in the
status bar ("(3 files skipped)"). No list says which files were skipped or why,
and nothing offers another way to finish the job. The editor itself opens files
up to 512 MiB (`DocumentScale::REFUSE_TO_OPEN`). Workspace search can find
matches in those files, but Replace All cannot reach them.

Raising the two caps does not fix this. Replace All holds the whole file, the
whole new text and both undo snapshots in memory. `power-of-two-thresholds`
measured this (task 9.5 M9, on code unchanged since, apart from d704a908's
search-budget edits):

| Input | `main` peak growth | Ratio to file size |
|---|---|---|
| one 10 MiB file | 21.6 MiB | 2.16× |
| one 16 MiB file | 33.9 MiB | 2.12× |
| six 16 MiB files (64 MiB undo cap) | about 73 MiB, peak 94–100 MiB | the aggregate cap binds |

At the open tier this scales to roughly 1.1 GiB of worker memory for one file.
The undo backup also stays on the GTK side for the whole undo window: original
and replaced bytes, held in `GuardedReplaceUndoBackup` under a 64 MiB disposal
reservation. A 512 MiB file would need that reservation to grow past 1 GiB. The
on-disk journal costs the same again: each entry stores both snapshots as JSON
strings (`ReplaceBackupDiskEntry`), so escaping can make an entry larger than
twice the file.

`power-of-two-thresholds` design D13 (item 2) and the long-line programme
record (`docs/next/long-line-documents.md`, remaining-programme item 2) queue
the structural fix: chunked replace with an on-disk undo journal, so the
per-file limit reaches the open tier in constant memory, and a list of skipped
files with their reasons and an "Open and replace in editor" action.

## What Changes

- **Streaming per-file replace, in constant memory, up to the open tier.**
  - `MAX_REPLACE_FILE_BYTES` becomes `DocumentScale::REFUSE_TO_OPEN`
    (512 MiB). It is referenced, not copied.
  - Each file streams in fixed chunks of 1 MiB (`REPLACE_STREAM_CHUNK_BYTES`)
    through the same sorted-edit, stale-line-checked construction as today.
    UTF-8 validation carries the incomplete trailing sequence across chunk
    boundaries, and the line cursor carries a split CRLF and any line longer
    than a chunk.
  - Worker memory no longer grows with file size.
- **An on-disk undo body instead of in-memory snapshots.** Each replaced file
  gets one durable raw **body** file holding its original bytes. A new journal
  entry kind (`replace-all-undo-stream-entry`) records the body file name and
  the length and SHA-256 digest of both the original and the replaced bytes.
  - The replaced bytes are never retained, in memory or on disk.
  - Undo's divergence check stays exact: it compares size and SHA-256 digest by
    streaming the current file. A digest collision is the only way to restore
    over a file that has diverged.
  - GTK keeps only compact descriptors: path, body name, lengths and digests.
- **Two passes per file, with no change to the durable-write protocol core.**
  - Pass A streams the target into the journal body through
    `atomic_replace_stream`. It validates and dry-runs the replacement into a
    hashing sink.
  - The entry is then written and the journal armed.
  - Pass B streams the body through the replace engine into the target's
    `atomic_replace_stream`. It verifies the output digest and the target's
    identity facts before the rename.
  - `services/filesystem/write_protocol.rs` is not modified. A new I/O-free
    per-file journal protocol (`ReplaceFileProtocol`) orders body, entry, arm,
    rewrite and cleanup, and is Kani-proved crash-safe.
- **Disk admission instead of the 64 MiB memory aggregate.** A file's body and
  output are admitted against measured free space (`statvfs`, through the
  filesystem boundary) plus a reserve, `REPLACE_DISK_RESERVE_BYTES`, 1 GiB.
  The reserve is waived for exactly the files `main` would have replaced. A
  no-loss ledger replays `main`'s admission (10 MiB first pass, 16 MiB per-file
  limit, 64 MiB before-and-after payload), so no run replaces a file that
  `main` replaced any later than `main` did.
- **External modification is detected.** Pass B refuses to rename when the
  target's size, modification time or inode changed since pass A. The file is
  skipped as "changed while replacing".
- **A typed skip report and a Skipped Files list.**
  - Every skipped file has a typed reason (`ReplaceSkipReason`).
  - Counts per reason are exact.
  - The list of skipped files is bounded by retained path bytes
    (`MAX_REPLACE_SKIP_LIST_BYTES`, 4 MiB), with a "N more" row.
  - A **Skipped Files** dialog lists the files from the panel's completion row.
    Each row has the file name, the folder, the reason and, where the pure
    policy allows it, **Open and replace in editor**.
  - That action opens the file (or selects its tab) at the first skipped match
    and reveals the in-tab replace bar, prefilled with the query, the options
    and the replacement. In regex mode the replacement is translated to
    GtkSourceView's syntax. The action never applies anything by itself.
- **Long runs are visible, cancellable and close-safe.**
  - Replace All publishes progress through the notification bus (files and MiB
    done), with the same 500 ms delay and 1 s heartbeat as search progress.
  - The confirm button becomes **Cancel Replace All** while a run is in flight.
    Cancelling rolls back the files already written, by streaming from their
    bodies.
  - Closing the window during a run cancels it and defers the close to the
    worker's terminal.
- **Crash recovery covers the new windows.** There are new kill points:
  - body durable, entry not yet written;
  - entry armed, target not yet renamed;
  - target renamed, entry not yet finalized.

  A crash-recovery smoke case exercises them. Recovery removes the crashed
  run's target-side temp files, which it identifies by the pid and launch
  nonce recorded in the manifest, so a 512 MiB temp is never left in the
  workspace.
- **Legacy journals stay undoable.** A v1 `replace-all-undo-entry`, with its
  inline JSON strings bounded by the old 64 MiB cap, still loads and undoes
  after an upgrade. New runs write only body entries.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-replace-safety`:
  - streaming replace up to the open tier;
  - on-disk bodies and digest undo;
  - disk admission with the no-loss ledger;
  - external-modification refusal;
  - the typed skip report and the Skipped Files list;
  - the editor action;
  - cancellation and close;
  - legacy journal compatibility;
  - recovery of temp files.
- `threshold-constants`: the Replace All per-file limit is the open tier; the
  no-loss ledger keeps `main`'s three values as recorded exceptions; the
  skip-reason messages name sizes in IEC units.
- `main-thread-responsiveness`:
  - Replace All completion, progress and the Skipped Files dialog stay within
    `B_settle` and `B_interact`;
  - no GTK dispatch waits on the journal or a target guard.
- `dbus-automation-spine`: Replace All progress, cancellation, skip counts per
  reason and the editor action can be driven and observed without exposing
  document text.
- `gtk-accessibility-spine`: the Skipped Files dialog, the cancel button and
  the progress announcements are accessible.
- `crash-restart-recovery-coverage`: Replace All kill windows are covered by
  the crash smoke.
- `formal-verification-kani`: the per-file journal protocol and the chunked
  engine's split independence are proved.

## Impact

- **Code.** The service side changes:
  - `crates/lushtext-core/src/services/content_search/replace.rs` is split into
    a `replace/` role home: the streaming engine, the per-file protocol plus
    its `kani_proofs.rs`, undo, and admission.
  - `services/search_backup.rs` gets the body entries and the legacy reader.
  - `services/filesystem/metadata.rs` gets the free-space probe.
  - `services/filesystem/leftovers.rs` gets the journal-owned temp predicate.
  - `model/content_search.rs` gets `ReplaceSkipReason` and the skip report.

  The UI side changes:
  - `ui/search_panel/{journal,replace_execution,evidence,policy}.rs`;
  - a new called presentation surface, `ui/search_panel/skipped_files.rs`;
  - `ui/window/search.rs`, for progress, cancel, close deferral and the editor
    action;
  - `ui/search_bar`, for the prefill entry point.
- **Dependencies.** `sha2` (already a workspace dependency, used by
  `cargo-gtk-proof`) is added to `lushtext-core`. Hakari and `cargo deny` are
  rerun. `cargo-sources.json` needs no new crate, which is verified.
- **App data.** There is a new journal entry kind and raw body files under
  `replace-backup-journal/`. The format-upgrade inventory, the orphan
  detection and the cleanup learn about the body files. An older build reading
  the new journal must never expose undo and never delete a body; this is
  verified with a fixture.
- **Docs.**
  - `README.md`, `AGENTS.md`, `ui/search_panel/AGENTS.md`;
  - `docs/thresholds.md`;
  - `docs/workflow-readability-matrix.md` (`WFR-SEARCH-REPLACE`);
  - `docs/automation.md` and `docs/automation-reference.md`;
  - `docs/accessibility.md` and `docs/accessibility-matrix.md`;
  - `docs/end-user-coverage.md`;
  - the programme record.
- **Kani.** Two new shards, `core-replace-protocol` and `core-replace-stream`.
- **GTK axioms.** GtkSourceView's regex replacement syntax and its whole-word
  boundaries, each with a probe and a sample.
