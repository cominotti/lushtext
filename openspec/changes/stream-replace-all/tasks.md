## Rules for every task

- Pin to CPUs 8–23 (`taskset -c 8-23`).
- Run every heavy non-measurement command through `scripts/heavy.sh [--mem <cap>] --
  <cmd>`, one command per call:
  - cargo build, check, clippy, test and doc;
  - make test, check, kani and mutants;
  - smoke lanes;
  - `git commit` with its hook.
- Run every timing or memory verdict through `scripts/measure.sh`.
  - A verdict counts only if its recorded 1-minute load is below 4. Otherwise
    label it non-verdict and retry.
  - Interleave `main` and the branch.
  - Record the load before and after in
    `.claude/handoffs/stream-replace-all/runs.log`.
- Headless only:
  - never use `make run` or the live desktop;
  - never touch the real `~/.local/share/lushtext` or `~/.config`;
  - never use `pgrep -f` or `find /`.
- A failing-first test is run and seen failing on the branch before its fix.
  Log the failure in `.claude/handoffs/stream-replace-all/`.
- Do not touch the two tests PR #99 is fixing:
  - `window::test_autosave_during_a_local_history_restore_keeps_the_previous_draft`;
  - the sidebar directory-retention test.
- Kani: run each new shard once, detached, after its harness inputs are final.
  Leave the pull-request-gated shards to CI unless their harness inputs
  change.
- Large fixtures (hundreds of MiB) live in a disk-backed scratch directory
  inside this worktree's handoff directory, never on a tmpfs `/tmp`, whose
  pages count against the `heavy.sh` memory cap. Delete them after each
  measurement.

## 1. Baseline and toolkit facts (before any fix)

- [ ] 1.1 Re-measure M9 on `origin/main` with PT's method: a release lib
  test, one process per measured call, VmHWM reset first, three interleaved
  repetitions.
  - Cases: one 10 MiB file, one 16 MiB file, and six 16 MiB files.
  - Also add one 16 MiB file with an `fsync`-backed (non-tmpfs) target
    directory, for wall time.
  - Record the figures against design "Measured baseline". A difference of
    more than 5 % from PT's figures is investigated before going on.
- [ ] 1.2 Measure the plain-disposal reservation that `main`'s undo backup
  takes on GTK after six 10 MiB files. Use evidence, not a timing. Record it
  as the GTK-side baseline.
- [ ] 1.3 GTK axioms, each with a probe and a sample in `gtk-lush-axioms` (use
  the next free ids at implementation time; run `make gtk-axioms` and
  `make check-gtk-axioms`):
  - (a) `GtkSourceSearchContext::replace_all` with regex enabled expands
    `\g<N>`, `\N` and `\g<name>`, expands `\g<1>0` as group 1 followed by
    `0`, and treats `$1` literally;
  - (b) `set_at_word_boundaries(true)` finds the same occurrences as workspace
    search's whole-word mode on an ASCII-identifier fixture, and the probe
    records any difference on non-ASCII text;
  - (c) GRegex compiles the Rust-`regex` constructs that the translator
    passes through: classes, `\b`, `\d`, `\w`, `(?i)`, named groups
    `(?P<n>…)` and `(?<n>…)`. Record those that fail.

  Each result goes to the GTK axiom ledger
  (`docs/next/formal-verification.md`) with "Verified against".

## 2. Streaming engine (service, GTK-free)

- [x] 2.1 Split `services/content_search/replace.rs` into `replace/` (design
  D12), with no behaviour change. The existing tests must pass unchanged.
  Move the `.cargo/mutants.toml` entries keyed on `content_search/replace\.rs`
  and review `sonar-project.properties` in the same commit. This is a commit
  of its own.
- [ ] 2.1a Add a test-only per-file limit override
  (`set_max_replace_file_bytes_for_test`, gated like
  `set_max_replace_undo_bytes_for_test`). Migrate every test and bench that
  builds a `MAX_REPLACE_FILE_BYTES`-sized buffer or file to it, keeping its
  name and assertions:
  - `replacement_output_accepts_exact_file_limit_and_rejects_one_byte_over`
    (`crates/lushtext/tests/integration/search_replace.rs`);
  - the `replace.rs` unit tests that size fixtures by the limit;
  - the benches' dense-lines and over-cap fixtures
    (`crates/lushtext-core/benches/benchmarks.rs`), pinned to a named size.

  The real 512 MiB limit is reached only through sparse files and metadata
  (`LargerThanOpenTier`), never by allocating or writing it. No test may
  allocate a cap-sized buffer.
- [ ] 2.2 Failing-first property test in
  `crates/lushtext-core/tests/properties/`. For generated inputs (LF, CRLF,
  a lone CR, multi-byte UTF-8, a final unterminated line, empty lines) and
  every split offset, with chunk sizes 1 to 16, the streaming engine's output,
  `ReplacementTextOutcome` and digests equal `build_replaced_text_reference`
  and a SHA-256 of the reference output. It fails because the engine does not
  exist.
- [ ] 2.3 Implement `replace/stream.rs` (D2):
  - chunked reads with incremental UTF-8 that carries its incomplete tail;
  - the CRLF carry;
  - a bounded targeted-line buffer;
  - a hashing sink;
  - cancellation checks per chunk.

  Add `sha2` to `lushtext-core` and run `cargo hakari generate`,
  `cargo deny check advisories bans sources licenses`, and
  `make cargo-sources`, verifying that `cargo-sources.json` gains no crate.
- [ ] 2.4 Choose `REPLACE_STREAM_CHUNK_BYTES`: measure the branch's
  throughput at 256 KiB, 1 MiB and 4 MiB on a 512 MiB fixture, then take the
  smallest power of two within 5 % of the best. Record it, with its kind
  rustdoc.
- [ ] 2.5 Failing-first unit tests for `NotUtf8` at a chunk seam, for a
  targeted line split across three chunks, and for a non-targeted 64 MiB line
  that is never buffered (allocation evidence through
  `ReplaceConstructionMetrics`). Also: output past the per-file limit (test
  override) stops at the limit with `OutputLimitExceeded` and the file is
  skipped as `LargerThanOpenTier` (design D1).

## 3. Per-file protocol, journal format and Kani

- [ ] 3.1 Write `replace/protocol.rs`: `ReplaceFileProtocol` (P0–P6) and
  `ReplaceUndoProtocol` (U0–U3) as I/O-free `step(outcome) -> action`
  machines (D3, D6).
- [ ] 3.2 Write `replace/protocol/kani_proofs.rs` and add shard
  `core-replace-protocol` to `scripts/kani-shards.py` (D11, properties 1–5).
  Then run a mutation check, logged: reorder cleanup so the body goes before
  the entry, and show a counterexample.
- [ ] 3.3 Add shard `core-replace-stream` (D11): split independence on at
  most 8 bytes, chunk sizes 1 to 4, at most 2 edits. The harness uses a
  recording digest sink and the `cfg(kani)` UTF-8 byte path (design D2), never
  SHA-256 or `simdutf8`. Measure both shards'
  wall time and peak memory with `KANI_MEASURE`, and record the budgets
  (`make check-kani-shards`). Gate `core-replace-protocol` on pull requests
  only if it fits the PR budget in the shard table.
- [ ] 3.4 `services/search_backup.rs`:
  - the `replace-all-undo-stream-entry` kind;
  - raw `*.body` files in `replace-backup-journal/bodies/`, written with the
    new `WriteLabel::REPLACE_UNDO_BODY` (design D3; add it to
    `WriteLabel::KNOWN` and to the temp-name and leftover tests);
  - the manifest's `pid` and `launch_nonce`, and its re-arming by a later
    launch's Undo (D6);
  - descriptor loading that stats bodies but does not read them;
  - an orphan body that is not a diagnostic, and a missing or size-mismatched
    body that is (D9);
  - entry-before-body deletion;
  - shrink only: retire the rebuild paths of `save`, of `shrink_journal_to`'s
    fallback and of cancellation's `persist_undo_backup` (D3a);
  - orphan bodies in cleanup, `delete` and `shrink_journal_to`, through a
    bounded scan of `bodies/`;
  - `REPLACE_JOURNAL_SCAN_MAX_ENTRIES` in `format_upgrade` stays equal to
    `JOURNAL_SCAN_MAX_ENTRIES`, and the bodies scan uses the same bound.

  The persistent JSON fixtures (`crates/lushtext-core/tests/persistent_json_format.rs`)
  cover a valid entry, a missing optional field, an unknown field, a wrong
  kind, a body size mismatch and a mixed-kind manifest.
- [ ] 3.5 Failing-first: a legacy v1 active journal loads and undoes after
  the change (D6). Add it to `tests/properties/replace_journal_recovery.rs`,
  extending the generated states with bodies: missing, truncated, orphaned
  and mixed kinds. Undo is never offered for a journal without validated
  originals.
- [ ] 3.5a Failing-first: a crash between the fourth file's body and its
  entry leaves an active journal whose Undo restores the first three files,
  and cleanup removes the orphan body (D9).
- [ ] 3.6 Format upgrade: update `format_upgrade/{inventory,plan,diagnostics}.rs`
  for the new kind and the body files. Verify the newer-version diagnostics
  with `make run-format-upgrade-newer-manual-test`'s data, headless, through
  its fixture only.

## 4. Apply, admission, undo and rollback

- [ ] 4.1 `fs_metadata::free_space` (`rustix::fs::statvfs`) through the
  filesystem boundary, with a test-only fake probe under `test-utils`. Keep
  `make check-filesystem-boundary` green.
- [ ] 4.2 `replace/admission.rs`: the pure `MainAdmissionLedger` and the disk
  admission with `REPLACE_DISK_RESERVE_BYTES` (D4). The ledger replays the
  output-based deferral, the retained-weight charge and the reclaim on a
  pre-rename failure. Add the compile-time assertion
  `REPLACE_DISK_RESERVE_BYTES >= MAIN_REPLACE_UNDO_BYTES +
  MAIN_MAX_REPLACE_FILE_BYTES`. Unit-test it against a transcription of
  `main`'s `apply_replacements_inner` admission, including a file whose
  output, not its source, crosses 10 MiB and one stopped by the retained
  weight.
- [ ] 4.3 Failing-first no-loss tests. They cover the provisional P1 waiver
  and its final decision at P3 (design D4):
  - `no_loss_every_file_main_replaced_is_replaced_with_little_free_space`;
  - `no_loss_a_large_file_early_never_displaces_a_file_main_replaced`;
  - `no_loss_a_small_file_whose_output_grows_past_10_mib_is_deferred_as_on_main`;
  - keep `no_loss_a_larger_file_never_displaces_a_file_main_replaced` passing.
- [ ] 4.4 `replace/apply.rs`: the two passes (P0–P6) with the
  `atomic_replace_workspace_file_stream` wrapper in `filesystem/write.rs`
  (resolved target identity, `WriteLabel::REPLACE`, the leftover sweep). The
  write-protocol core is unchanged; `git diff` of
  `services/filesystem/write_protocol*` is empty, and this is checked in the
  review.
- [ ] 4.5 Failing-first: another writer appends to the target during pass B,
  using a test hook after pass A. No rename happens, the file is skipped as
  `ChangedDuringReplace`, and its body and entry are removed (D5).
- [ ] 4.6 `replace/undo.rs`: streaming undo and rollback (D6). The existing
  undo tests keep their names: the growth race, exactly at the limit, and the
  I/O failure. Add failing-first tests for a diverged file (not overwritten
  and retryable), for a body corrupted after the run (its digest mismatches,
  so no write happens and it is retryable), and for Undo and rollback with
  less free space than the reserve but enough for their writes (every file is
  restored; D6).
- [ ] 4.7 Exact accounting: the existing tests in "Replace All undo payload
  accounting is exact across failures" pass against the disk and descriptor
  counters.
- [ ] 4.8 Memory acceptance (M9 method, interleaved with `main`), with the
  same replacements at every size, on a disk-backed fixture. The branch's peak
  growth at 512 MiB is within its growth at 16 MiB times 1.05 plus 1 MiB, and
  below 8 MiB in absolute terms; at 16 MiB it is below `main`'s 33.9 MiB. This
  measurement is the lane for the "A 400 MiB file is replaced" and "Memory is
  flat across file sizes" scenarios (a release test also runs the 400 MiB
  case once). If this fails, the cause is found and fixed. It is never
  waived.
- [ ] 4.9 One journal per application (design D3a). Failing-first widget
  tests with two windows:
  - a run in window B retires window A's Undo, with one status message;
  - an apply in window A while window B's run is in flight is refused, and
    touches no file;
  - an Undo in window A while window B's run is in flight is refused.

  Failing-first service tests: a worker waiting for the journal guard sees
  Cancel; a superseded journal disk job returns before it waits; and
  `a_run_that_replaces_nothing_removes_the_superseded_journal`.

## 5. Skip report and the Skipped Files dialog

- [ ] 5.1 `model/content_search.rs`: `ReplaceSkipReason`, `ReplaceSkipReport`
  and `MAX_REPLACE_SKIP_LIST_BYTES` (4 MiB, presentation cap). Fold the
  preview-ineligible matches (`LineTooLong`, `InvalidPreview`) in by file.
  Test the 16,384-file failure-heavy case for exact counts and the bounded
  list.
- [ ] 5.2 `ui/search_panel/policy.rs`: `replace_in_editor_availability` and
  `rust_replacement_to_gtksourceview`, with unit and property tests and in
  the mutation scope. The axiom 1.3 (a) results decide the translation table.
- [ ] 5.3 `ui/search_panel/skipped_files.rs` (a called presentation surface):
  an `AdwDialog` with a `GtkListView` over a lazy `GListModel` plus
  `GtkSectionModel` (design D7, the notes browser's pattern), the reason
  headings and the rows (D7). Follow `.agents/rules/ui.md` (grouped-row
  dialog readability, adaptive dialog sizing) and `widget-wiring.md`.
- [ ] 5.4 Add `win.replace-in-editor(s)` to the action catalog, and
  `LushtextSearchBar::prefill_replace`. Widget tests:
  - an unsaved tab is selected with the bar prefilled, and the buffer is
    unchanged;
  - a closed file opens at the first skipped line;
  - regex mode with `$1`, `${1}0` and a reference to a missing group (the
    replacement stays empty and the bar says why);
  - the prefill comes from the run's request after the panel's query was
    edited;
  - no row action for `LargerThanOpenTier`;
  - a Protected View open shows the notice.
- [ ] 5.5a Failing-first widget test: every file of a run is open with
  unsaved changes. The early return in `ui/window/search.rs` ("No
  replacements to apply…") now produces the skip report, and Show Skipped
  Files lists each file with `UnsavedChanges` and the editor action.
- [ ] 5.5b Failing-first: a file with matches on short lines and on a 10 KiB
  line is replaced, and is listed as partly replaced (`LineTooLong`, with the
  count left).
- [ ] 5.5 Widget test: completion with skips shows "· N skipped" and Show
  Skipped Files. Keyboard path: Tab, Enter, row action, then Escape returns
  focus. The dialog's model is lazy: opening it at 16,384 rows creates only
  the realized rows (evidence through the model's item-call count, as the
  notes browser's A93 probe counts them).
- [ ] 5.6 Land the visual edits together. The visible parts of 5.3, 5.5 and
  6.2 (the Cancel Replace All button state) land in one commit, with their
  widget tests green; 6.2's worker-side cancellation may land earlier. The
  commit carries every template change:
  - `resources/ui/search-panel.blp`: the Show Skipped Files control in the
    completion row, and the confirm button's Cancel Replace All state;
  - `resources/ui/search-bar.blp`, only if `prefill_replace` needs a template
    child;
  - the Skipped Files dialog's template, if it has one.

  Run `make blueprint-generate`, `make check-blueprint`, `make lint-blueprint`,
  `make visual-smoke` (with a Replace All skipped-files capture) and
  `make accessibility-smoke` on that commit.

## 6. Progress, Cancel and close

- [ ] 6.1 Add `ReplaceProgress` atomics, and a progress poll on the
  notification bus with the search-progress delay and heartbeat (D8). The
  progress is announced once.
- [ ] 6.2 Failing-first: cancelling in the middle of the third of five files
  leaves the third untouched, restores the first two and clears the journal.
  The test drives `win.cancel-search-panel-replacements` and also the button,
  which becomes "Cancel Replace All".
- [ ] 6.3 Failing-first: `close_request` during a run opens "Replace All Is
  Still Running" (D8). Test three paths:
  - Keep Running closes nothing;
  - Close When Finished completes the run, keeps its journal active, then
    runs the normal close safety;
  - Stop and Close rolls back and then closes.

  In every case each file is original, or replaced with an active entry.
  Check the application-quit path too.
- [ ] 6.4 The GTK side owns descriptors only. The undo disposal reservation
  shrinks to the descriptor weight. A widget evidence test checks that no
  `GuardedReplaceUndoBackup` holds file bytes, for a stream journal and for a
  loaded legacy journal (`UndoOriginal::LegacyEntry`). It runs at the test
  limit's scale; task 9.2 is the lane for the 1 GiB scenario.

## 7. Recovery, temp files and crash smoke

- [ ] 7.1 `leftovers::is_journal_owned_leftover` plus a bounded per-target
  sweep on recovery, undo and discard (D9). The predicate keys on the
  recorded (pid, nonce) pair and on that pair differing from the current
  launch's. Unit tests:
  - a matching temp is removed;
  - a temp with the same pid but another nonce (the Flatpak pid reuse case)
    is kept;
  - a temp of the current launch, a non-regular file and a symlink are kept;
  - a temp of a later launch's Undo is removed after that Undo re-armed the
    manifest (D6);
  - each distinct parent directory is scanned once;
  - no workspace traversal happens.
- [ ] 7.2 Add the `KillWindow`s `replace-body-before-entry`,
  `replace-entry-before-target-rename`,
  `replace-target-renamed-before-finish` and
  `replace-undo-before-target-rename`, as empty inline functions in shipped
  builds. Place the two `…-before-target-rename` points inside pass B's (and
  the restore's) producer at EOF, after the temp holds its bytes (design D9).
  Extend `LUSHTEXT_KILL_AT` with an occurrence ordinal (`name@label#n`),
  with unit tests, and document it in `services/kill_point.rs` and
  `docs/end-user-coverage.md`.
- [ ] 7.3 Add a crash-recovery smoke case (`scripts/crash-recovery-smoke-driver.py`)
  that drives Replace All through the automation actions on a multi-file
  fixture, kills at each window on the third of five files (and at
  `durable-renamed-before-dirsync@replace`), plus at
  `replace-undo-before-target-rename` during an Undo in a later launch,
  relaunches and asserts the outcome (the crash-restart-recovery-coverage
  delta). Run `make crash-recovery-smoke`.
- [ ] 7.4 Integration test (`crates/lushtext/tests/integration/search_replace.rs`):
  an interrupted cleanup with bodies never resurrects undo.
- [ ] 7.5 The startup app-data leftover sweep (`app_data_leftovers.rs`) still
  covers the journal directory's temp files of entries and bodies.
- [ ] 7.6 Downgrade fixture: a journal written by the branch, loaded by
  `main`'s loader (vendored as a test fixture of the shape, or by checking
  out `main` in a scratch worktree and running its unit test against the
  directory). `main` exposes no undo, writes no target and reads no body; its
  cleanup may remove the journal directory with the bodies, which loses only
  the undo affordance. Record the result in design D6.

## 8. Automation, accessibility and docs

- [ ] 8.1 Snapshot fields `content_search.replace_in_flight`,
  `replace_progress_*`, `replace_cancel_requested`, `replace_skip_counts`
  (per reason) and `replace_skip_not_listed`. `search-complete` stays pending
  through rollback. Update `docs/automation.md` and
  `docs/automation-reference.md`, then run `make check-automation-docs` and
  `make automation-client-self-test` if the client changed.
- [ ] 8.2 `make accessibility-smoke` gets a skipped-files case. Update
  `docs/accessibility.md`, the rows in `docs/accessibility-matrix.md` and the
  Orca checklist row for the Skipped Files dialog.
- [ ] 8.3 `docs/thresholds.md`:
  - `MAX_REPLACE_FILE_BYTES` derived;
  - `MAIN_*` recorded exceptions;
  - `REPLACE_DISK_RESERVE_BYTES`, `REPLACE_STREAM_CHUNK_BYTES` and
    `MAX_REPLACE_SKIP_LIST_BYTES`;
  - `MAX_REPLACE_UNDO_BYTES` retired as a name; the legacy loader's bound
    references `MAIN_REPLACE_UNDO_BYTES`.

  Run `make check-thresholds`.
- [ ] 8.4 `docs/workflow-readability-matrix.md` `WFR-SEARCH-REPLACE`:
  - the new files;
  - `skipped_files.rs` as a called surface;
  - policy consumers;
  - evidence fields;
  - re-derived measured cells.

  The facade stays at most 370 lines. Run `make check-workflow-boundaries`.
- [ ] 8.5 Update `README.md` (Replace All up to 512 MiB, Skipped Files,
  Cancel), `AGENTS.md` (the module layout, the "Multi-file Replace All" and
  "Replace/Undo bounded evidence" decisions), `ui/search_panel/AGENTS.md`,
  `docs/end-user-coverage.md` (the crash smoke case) and the gtk-testing
  skill references, if a new testing pitfall appears.
- [ ] 8.6 Programme record (`docs/next/long-line-documents.md`):
  - item 2's status;
  - the prepared AppStream release notes ("Replace All rewrites files up to
    512 MiB … lists skipped files and offers to finish them in the editor");
  - an "SR" implementation section with the measurements.

## 9. Lanes and verdicts

- [ ] 9.1 Responsiveness lane cases (`make responsiveness-smoke`, measurement
  build). Interleave with `main` where `main` has the case; the dialog and
  the progress poll are new:
  - (a) the Replace All completion terminal with 16,384 skips, within
    `B_settle`;
  - (b) opening the Skipped Files dialog with 16,384 rows, plus its first
    frame, within `B_settle`;
  - (c) the progress heartbeat during a 512 MiB run, within `B_interact`
    (the lane for "Replace All reports progress for long runs");
  - (d) typing in another tab during a 512 MiB run, within `B_interact`.
- [ ] 9.2 Memory verdict, the M9 method, as in 4.8.
- [ ] 9.3 Wall-time record:
  - at 16 MiB, the branch is within 2× `main` plus 50 ms (design Risks);
  - at 512 MiB, record the throughput and its split between pass A, pass B
    and the `fsync`s;
  - 2,000 small files (1–8 KiB) on an `fsync`-backed directory, interleaved
    with `main`: the branch is within 1.6× `main`'s wall time. If it is
    not, stop and amend the artifacts first (design Risks: the inline-original
    fallback is a spec amendment, with its format fixtures, Kani protocol and
    descriptor rule), then implement it and re-measure.
- [ ] 9.4 Gates:
  - `make check`;
  - `make test`, with the widget shards (CI parity);
  - property tests and fuzz corpus replay;
  - `make kani KANI_SHARD=core-replace-protocol` and `core-replace-stream`,
    once and detached;
  - `make crash-recovery-smoke`, `make accessibility-smoke`,
    `make gtk-axioms` and `make check-gtk-axioms`.

  All must be green on local evidence.
