## Rules for every task

- Inherit the toolbox cpuset; never pin with `taskset`. Only timing verdicts
  pin, and `scripts/measure.sh` pins itself to its reference set.
- Run every heavy non-measurement command through `scripts/heavy.sh [--mem <cap>] --
  <cmd>`, one command per call:
  - cargo build, check, clippy, test and doc;
  - make test, check, kani and mutants;
  - smoke lanes;
  - `git commit` with its hook.

  `heavy.sh` sets `CARGO_BUILD_JOBS` and the rustc front-end threads; never
  override them.
- **Memory figures** (peak RSS, `VmHWM` reset first, one process per call) do
  not depend on the load: run them through `heavy.sh`, interleaved `main` and
  branch, and label their wall-time columns non-verdict (design R3).
- **Timing verdicts** go through `scripts/measure.sh` only after the 1-minute
  load has been below 4 for 30 s (`for i in 1 2 3 4 5 6; do awk '{exit !($1 < 4)}' /proc/loadavg || exit 1; sleep 5; done`),
  with `--lock-timeout 300`, timing a binary built beforehand with
  `heavy.sh`, never a `cargo` invocation. A round recorded at load 4 or more
  is non-verdict. If the gate fails or `measure.sh` exits 75, do not retry:
  append the remaining rounds to the main checkout's
  `.claude/handoffs/MEASURE-QUEUE.md` under `flock` and carry on. Record the
  load before and after in `.claude/handoffs/stream-replace-all/runs.log`.
- Verdicts compare both sides on the same front end: the `main` side is
  rebuilt at the `origin/main` the branch last merged (at least `c16d6fdc`),
  and a dev-profile build at opt-level 0 sets
  `LUSHTEXT_RUSTC_FRONTEND_THREADS=0` on both sides.
- Headless only:
  - never use `make run` or the live desktop;
  - never touch the real `~/.local/share/lushtext` or `~/.config`;
  - never use `pgrep -f` or `find /`; wait on your own PIDs.
- A failing-first test is run and seen failing on the branch before its fix.
  A test that guards behaviour the branch already has (a successor of a
  `main` test, task 4.10) is instead shown to fail against a logged mutant of
  the code it guards. Log both in `.claude/handoffs/stream-replace-all/failing-first.log`.
- Kani: run each shard once, detached, after its harness inputs are final.
  A harness whose CBMC passes 12 GiB (the shard table's runner margin) is
  stopped and its bounds shrink before any rerun.
- Large fixtures (hundreds of MiB) live in a disk-backed scratch directory
  inside this worktree's handoff directory, never on a tmpfs `/tmp`, whose
  pages count against the `heavy.sh` memory cap. Delete them after each
  measurement.
- GTK axiom ids for this change: **A127–A131** only (task 1.3 takes A127,
  A128 and A129).

## Commit plan (stage-1 revision, design R6)

The three smokes fingerprint every file under `crates/lushtext-core/src/ui/`,
`crates/lushtext/tests/widget/`, `resources/ui/` and `resources/style/`, and
`docs/accessibility*.md`, `docs/automation*.md` and
`docs/end-user-coverage.md`. The remaining work lands in this order; G1 to G4
touch none of those paths, and G5 is the one commit that does.

- **G0 (done in stage 1):** merge of `origin/main` (`03df8bd8`), the Kani
  redesign and the bounded exhaustive engine test (3.3, 3.3a), and this
  re-plan.
- **G1, service data safety:** 3.2a, 3.5a, 3.6, 4.3, 4.6, 4.9 (service tests),
  4.10, 4.11, then 2.4's chunk constant once its rounds report.
- **G2, GTK axioms:** 1.3 (`crates/gtk-lush/axioms/`, the ledger in
  `docs/next/formal-verification.md` and the skill's ledger reference).
- **G3, crash smoke:** 7.3's driver and lane (`scripts/crash-recovery-smoke-driver.py`);
  its `docs/end-user-coverage.md` line waits for G5.
- **G4, measurement harnesses:** the branch-side M9 test and the 2.4/9.3
  timing binaries (services and benches only), so the queue entries can turn
  `READY`.
- **G5, the visual commit:** 4.9 (widget tests), 5.2, 5.4, 5.5a, 5.6, 6.2,
  6.3, 8.2 and every fingerprinted doc of 8.1/8.5. Run `make
  blueprint-generate`, `make check-blueprint`, `make lint-blueprint`, the
  widget shards, then `make visual-geometry-smoke`, `make
  accessibility-smoke` and `make visual-smoke` once, and commit right after.
- **After G5:** the memory verdict (4.8, 9.2) through `heavy.sh`, the queued
  timing verdicts (9.1, 9.3), then 9.4 and 8.6.

## 1. Baseline and toolkit facts (before any fix)

- [ ] 1.1 Re-measure M9 on `origin/main` with PT's method: a release lib
  test, one process per measured call, VmHWM reset first, three
  repetitions, through `heavy.sh` (memory, design R3).
  - Cases: one 10 MiB file, one 16 MiB file, six 10 MiB and six 16 MiB files.
  - Repetition 1 exists (2026-11-14, `mem/evidence.txt`, design R3: every
    case within 2 % of PT's). Run repetitions 2 and 3 on a `main` tree
    rebuilt at the merged `origin/main` with `measure/main_m9.rs.add`, from a
    pre-built test binary (`cargo test --release --no-run` through
    `heavy.sh`, then the binary with `--exact … --ignored`).
  - The 16 MiB `fsync`-backed (non-tmpfs) wall-time case is a timing
    verdict: it is one of 9.3's queued rounds.
  - Record the figures against design "Measured baseline". A difference of
    more than 5 % from PT's figures is investigated before going on.
- [ ] 1.2 Measure the plain-disposal reservation that `main`'s undo backup
  takes on GTK after six 10 MiB files. Use evidence, not a timing. Record it
  as the GTK-side baseline.
- [ ] 1.3 GTK axioms **A127, A128, A129** (a, b, c), each with a probe and a
  sample in `gtk-lush-axioms` (group G2; if `main` has taken one of them by
  then, use the next of A130–A131 and say so); run the three samples with
  `make gtk-axiom-sample AXIOM=<id> CHECK=1` on the shipped toolkit and the
  GNOME 50 floor (`scripts/run-gtk-axioms.sh runtimes --only A127,A128,A129`),
  and `make check-gtk-axioms`:
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
- [x] 2.1a Add a test-only per-file limit override
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
- [x] 2.2 Failing-first property test in
  `crates/lushtext-core/tests/properties/`. For generated inputs (LF, CRLF,
  a lone CR, multi-byte UTF-8, a final unterminated line, empty lines) and
  every split offset, with chunk sizes 1 to 16, the streaming engine's output,
  `ReplacementTextOutcome` and digests equal `build_replaced_text_reference`
  and a SHA-256 of the reference output. It fails because the engine does not
  exist.
- [x] 2.3 Implement `replace/stream.rs` (D2):
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
  rustdoc. A timing verdict: G4 builds one release binary with a test-only
  chunk-size override (never a shipped setting), turns part A of the queued entry
  "stream-replace-all: timing verdicts" to `READY <sha>`, and the
  coordinator runs it; the constant lands in G1's last commit.
- [x] 2.5 Failing-first unit tests for `NotUtf8` at a chunk seam, for a
  targeted line split across three chunks, and for a non-targeted 64 MiB line
  that is never buffered (allocation evidence through
  `ReplaceConstructionMetrics`). Also: output past the per-file limit (test
  override) stops at the limit with `OutputLimitExceeded` and the file is
  skipped as `LargerThanOpenTier` (design D1).

## 3. Per-file protocol, journal format and Kani

- [x] 3.1 Write `replace/protocol.rs`: `ReplaceFileProtocol` (P0–P6) and
  `ReplaceUndoProtocol` (U0–U3) as I/O-free `step(outcome) -> action`
  machines (D3, D6).
- [x] 3.2 Write `replace/protocol/kani_proofs.rs` and add shard
  `core-replace-protocol` to `scripts/kani-shards.py` (D11, properties 1–5).
  Verified 3/3 on `23ef433a` (0.34 min, 1.59 GiB peak).
- [ ] 3.2a Mutation check of `core-replace-protocol`, logged: reorder cleanup
  so the body goes before the entry, and show the counterexample in which an
  entry references a missing body (the delta's "A reordered cleanup is
  caught"). Run it once through `make kani KANI_SHARD=core-replace-protocol`
  on a scratch copy of the tree with the mutant, detached; restore nothing in
  the worktree.
- [x] 3.3 Shard `core-replace-stream`, redesigned in stage 1 (design D11,
  "Kani memory"): the UTF-8 carry moved to `replace/stream/utf8.rs`, whose
  `kani_proofs.rs` proves the UTF-8 seams (8 arbitrary bytes, chunks 1–4) and
  the byte scan against `core::str::from_utf8` (4 bytes); the unit test
  `the_byte_scan_matches_simdutf8` ties the byte scan to the production
  validator. Every engine-level harness form passed the 12 GiB margin and is
  recorded as a bound reduction. The shard ran once on the final harness
  inputs (`runs.log`, `kani-stream-final.json`); its local figure is in the
  table as `PENDING DISPATCH (owed by stream-replace-all)`.
- [x] 3.3a The bounded exhaustive check of the engine's line logic,
  `the_engine_does_not_depend_on_its_chunk_size_over_a_bounded_domain`
  (`replace/tests.rs`, design D11), with a logged mutant it kills (a lone
  `\r` line's carriage return).
- [ ] 3.3b Budgets: two `kani.yml` dispatches after the final code is pushed
  (`build.md`, "Shard gates and budgets"), replacing both shards' pending
  local figures, larger of the two. Gate `core-replace-protocol` on pull
  requests only if it fits the PR budget in the shard table. This is the
  change's one CI-evidence step and is done by the stage that owns the
  merge.
- [x] 3.4 `services/search_backup.rs`:
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
- [x] 3.5 Failing-first: a legacy v1 active journal loads and undoes after
  the change (D6). Add it to `tests/properties/replace_journal_recovery.rs`,
  extending the generated states with bodies: missing, truncated, orphaned
  and mixed kinds. Undo is never offered for a journal without validated
  originals.
- [ ] 3.5a Failing-first: a crash between the fourth file's body and its
  entry leaves an active journal whose Undo restores the first three files,
  and cleanup removes the orphan body (D9). The property
  `an_orphan_body_never_costs_undo` covers the loader's half; this task is
  the end-to-end service test through the `replace-body-before-entry` kill
  seam (or its test-only equivalent).
- [ ] 3.6 Format upgrade: update `format_upgrade/{inventory,plan,diagnostics}.rs`
  for the new kind and the body files (`inventory.rs` and `plan.rs` are done;
  `diagnostics.rs` is not). Verify the newer-version diagnostics with
  `make run-format-upgrade-newer-manual-test`'s data, headless, through its
  fixture only, never by launching the manual test.

## 4. Apply, admission, undo and rollback

- [x] 4.1 `fs_metadata::free_space` (`rustix::fs::statvfs`) through the
  filesystem boundary, with a test-only fake probe under `test-utils`. Keep
  `make check-filesystem-boundary` green.
- [x] 4.2 `replace/admission.rs`: the pure `MainAdmissionLedger` and the disk
  admission with `REPLACE_DISK_RESERVE_BYTES` (D4). The ledger replays the
  output-based deferral, the retained-weight charge and the reclaim on a
  pre-rename failure. Add the compile-time assertion
  `REPLACE_DISK_RESERVE_BYTES >= MAIN_REPLACE_UNDO_BYTES +
  MAIN_MAX_REPLACE_FILE_BYTES`. Unit-test it against a transcription of
  `main`'s `apply_replacements_inner` admission, including a file whose
  output, not its source, crosses 10 MiB and one stopped by the retained
  weight.
- [ ] 4.3 Failing-first no-loss tests through `apply_replacements` with the
  fake free-space probe and the test-only per-file limit. They cover the
  provisional P1 waiver and its final decision at P3 (design D4):
  - `no_loss_every_file_main_replaced_is_replaced_with_little_free_space`;
  - `no_loss_a_large_file_early_never_displaces_a_file_main_replaced`;
  - `no_loss_a_small_file_whose_output_grows_past_10_mib_is_deferred_as_on_main`;
  - `no_loss_a_larger_file_never_displaces_a_file_main_replaced`, restored
    under its name as an end-to-end test (the ledger tests that replaced it,
    `ledger_matches_main_on_six_files_at_the_old_caps` and
    `ledger_matches_main_over_generated_runs`, stay).

  "Failing-first" here means each test fails against the branch with the
  waiver removed (a logged mutant), since the waiver already exists.
- [x] 4.4 `replace/apply.rs`: the two passes (P0–P6) with the
  `atomic_replace_workspace_file_stream` wrapper in `filesystem/write.rs`
  (resolved target identity, `WriteLabel::REPLACE`, the leftover sweep). The
  write-protocol core is unchanged; `git diff` of
  `services/filesystem/write_protocol*` is empty, and this is checked in the
  review.
- [x] 4.5 Failing-first: another writer appends to the target during pass B,
  using a test hook after pass A. No rename happens, the file is skipped as
  `ChangedDuringReplace`, and its body and entry are removed (D5).
- [ ] 4.6 `replace/undo.rs`: streaming undo and rollback (D6). Done: a
  diverged file, a body corrupted after the run, Undo and rollback with less
  free space than the reserve
  (`a_rollback_with_less_free_space_than_the_reserve_restores_every_file`).
  Owed:
  - the three spec scenarios of "Undo reads enforce the file cap during
    ingestion" under their `main` names:
    `undo_growth_after_metadata_is_skipped_without_unbounded_allocation`
    (today `undo_growth_after_metadata_is_skipped_without_reading_it_whole`;
    rename it back), `test_undo_replacements_allows_current_file_at_exact_size_cap`
    (at the test-only limit) and a bounded-read I/O failure during a
    partial restore, which keeps the failed entries
    (`test_undo_replacements_keeps_failed_entries_after_partial_restore`);
  - each one failing-first, or against a logged mutant when the behaviour
    already holds.
- [x] 4.7 Exact accounting: the existing tests in "Replace All undo payload
  accounting is exact across failures" pass against the disk and descriptor
  counters.
- [ ] 4.8 Memory acceptance (M9 method, interleaved with `main`, through
  `heavy.sh`: design R3), with the same replacements at every size, on a
  disk-backed fixture. G4 adds the branch-side measurement test (the shape
  of `measure/main_m9.rs.add` over `apply_replacements`'s new signature). The branch's peak
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

  Failing-first service tests (group G1, before the widget half): a worker
  waiting for the journal guard sees Cancel; a superseded journal disk job
  returns before it waits; and
  `a_run_that_replaces_nothing_removes_the_superseded_journal`. The widget
  half lands in G5.
- [ ] 4.10 Test continuity (design R4). For each of the 48 names in
  `.claude/handoffs/stream-replace-all/main-replace-tests-missing-on-branch.txt`,
  record in `decisions.md` the branch test that now guards the same
  behaviour, or write one. Each successor is shown to fail against a logged
  mutant of the code it guards. These have no visible successor and get a
  test each, with `main`'s name where the behaviour is unchanged:
  - `test_apply_replacements_waits_for_existing_save_guard_on_same_target`;
  - `test_apply_replacements_waits_for_startup_journal_recovery_guard`
    (now the cancellable wait of D3a: it must also see Cancel, 4.9);
  - `test_apply_replacements_cancel`;
  - `test_apply_replacements_aborts_when_undo_journal_cannot_be_persisted`
    (no target is written when the entry or the body cannot be made
    durable);
  - `test_apply_replacements_nonexistent_file` and
    `test_apply_replacements_skip_paths` (with their skip reasons);
  - `test_apply_replacements_allows_file_at_exact_size_cap` and
    `test_apply_replacements_skips_over_file_size_cap_before_reading` (test
    limit; the second must read no byte);
  - `rollback_disposition_refuses_a_file_changed_after_replacement` and
    `rollback_keeps_the_journal_when_a_target_changed_mid_rollback`;
  - `undo_hooks_fire_only_for_their_own_target_across_interleaved_undos` and
    `parallel_undo_operations_consume_only_their_own_target_hooks`;
  - `test_apply_replacements_persists_undo_journal_before_success` (the
    entry and its body are durable before the target's rename).

  Names whose function no longer exists (`build_replaced_text`,
  `replaced_capacity`, `line_spans`, the inline `replace_undo_entry`) map to
  the stream-engine tests, the bounded check of 3.3a and the reference
  builder's own tests; the record says which.
- [ ] 4.11 Failing-first (decision 21): a cancelled file whose entry removal,
  then whose body removal, fails through a per-target fault seam. The target
  keeps its original bytes, the journal stays active with that entry and its
  body (never an entry without its body), and the next Undo finds the target
  already original (U1) and retires the entry and the body.

## 5. Skip report and the Skipped Files dialog

- [x] 5.1 `model/content_search.rs`: `ReplaceSkipReason`, `ReplaceSkipReport`
  and `MAX_REPLACE_SKIP_LIST_BYTES` (4 MiB, presentation cap). Fold the
  preview-ineligible matches (`LineTooLong`, `InvalidPreview`) in by file.
  Test the 16,384-file failure-heavy case for exact counts and the bounded
  list.
- [ ] 5.2 `ui/search_panel/policy.rs`: `replace_in_editor_availability` and
  `rust_replacement_to_gtksourceview`, with unit and property tests and in
  the mutation scope (group G5: `policy.rs` is under the fingerprinted `ui/`
  prefix). Done: both functions and their unit tests. Owed: the property
  tests, and the translation table checked against axiom A127 (1.3 a), which
  decides it.
- [x] 5.3 `ui/search_panel/skipped_files.rs` (a called presentation surface):
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
- [x] 5.5b Failing-first: a file with matches on short lines and on a 10 KiB
  line is replaced, and is listed as partly replaced (`LineTooLong`, with the
  count left).
- [x] 5.5 Widget test: completion with skips shows "· N skipped" and Show
  Skipped Files. Keyboard path: Tab, Enter, row action, then Escape returns
  focus. The dialog's model is lazy: opening it at 16,384 rows creates only
  the realized rows (evidence through the model's item-call count, as the
  notes browser's A93 probe counts them).
- [ ] 5.6 Land the visual edits together, as group G5. The first visual
  commit (`2adaf083`) carried 5.3, 5.5 and the Cancel Replace All button
  state; what is left (5.2, 5.4, 5.5a, 6.2, 6.3, 8.2 and the fingerprinted
  docs) lands in **one** commit, with its widget tests green. The commit
  carries every template change:
  - `resources/ui/search-panel.blp`: the Show Skipped Files control in the
    completion row, and the confirm button's Cancel Replace All state;
  - `resources/ui/search-bar.blp`, only if `prefill_replace` needs a template
    child;
  - the Skipped Files dialog's template, if it has one.

  Run `make blueprint-generate`, `make check-blueprint`, `make lint-blueprint`,
  then `make visual-geometry-smoke`, `make visual-smoke` (with a Replace All
  skipped-files capture) and `make accessibility-smoke` once on that commit's
  final tree.

## 6. Progress, Cancel and close

- [x] 6.1 Add `ReplaceProgress` atomics, and a progress poll on the
  notification bus with the search-progress delay and heartbeat (D8). The
  progress is announced once.
- [ ] 6.2 Failing-first: cancelling in the middle of the third of five files
  leaves the third untouched, restores the first two and clears the journal.
  The test drives `win.cancel-search-panel-replacements` and also the button,
  which becomes "Cancel Replace All". Done: the held third file and the
  action. Owed: the button is **clicked** (`gtk_widget_activate` on it, or
  its `clicked` emission through `ui/signal_connect.rs`'s helpers), not only
  read by label, and the same three assertions hold.
- [ ] 6.3 Failing-first: `close_request` during a run opens "Replace All Is
  Still Running" (D8). The dialog exists
  (`ui/window/search_progress_execution.rs`, decision 10); its tests do not.
  Test three paths:
  - Keep Running closes nothing;
  - Close When Finished completes the run, keeps its journal active, then
    runs the normal close safety;
  - Stop and Close rolls back and then closes.

  In every case each file is original, or replaced with an active entry.
  Check the application-quit path too.
- [x] 6.4 The GTK side owns descriptors only. The undo disposal reservation
  shrinks to the descriptor weight. A widget evidence test checks that no
  `GuardedReplaceUndoBackup` holds file bytes, for a stream journal and for a
  loaded legacy journal (`UndoOriginal::LegacyEntry`). It runs at the test
  limit's scale; task 9.2 is the lane for the 1 GiB scenario.

## 7. Recovery, temp files and crash smoke

- [x] 7.1 `leftovers::is_journal_owned_leftover` plus a bounded per-target
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
- [x] 7.2 Add the `KillWindow`s `replace-body-before-entry`,
  `replace-entry-before-target-rename`,
  `replace-target-renamed-before-finish` and
  `replace-undo-before-target-rename`, as empty inline functions in shipped
  builds. Place the two `…-before-target-rename` points inside pass B's (and
  the restore's) producer at EOF, after the temp holds its bytes (design D9).
  Extend `LUSHTEXT_KILL_AT` with an occurrence ordinal (`name@label#n`),
  with unit tests, and document it in `services/kill_point.rs` and
  `docs/end-user-coverage.md`.
- [ ] 7.3 (group G3) Add a crash-recovery smoke case (`scripts/crash-recovery-smoke-driver.py`)
  that drives Replace All through the automation actions on a multi-file
  fixture, kills at each window on the third of five files (and at
  `durable-renamed-before-dirsync@replace`), plus at
  `replace-undo-before-target-rename` during an Undo in a later launch,
  relaunches and asserts the outcome (the crash-restart-recovery-coverage
  delta). Run `make crash-recovery-smoke`.
- [x] 7.4 Integration test (`crates/lushtext/tests/integration/search_replace.rs`):
  an interrupted cleanup with bodies never resurrects undo.
- [x] 7.5 The startup app-data leftover sweep (`app_data_leftovers.rs`) still
  covers the journal directory's temp files of entries and bodies.
- [x] 7.6 Downgrade fixture: a journal written by the branch, loaded by
  `main`'s loader (vendored as a test fixture of the shape, or by checking
  out `main` in a scratch worktree and running its unit test against the
  directory). `main` exposes no undo, writes no target and reads no body; its
  cleanup may remove the journal directory with the bodies, which loses only
  the undo affordance. Record the result in design D6.

## 8. Automation, accessibility and docs

- [x] 8.1 Snapshot fields `content_search.replace_in_flight`,
  `replace_progress_*`, `replace_cancel_requested`, `replace_skip_counts`
  (per reason) and `replace_skip_not_listed`. `search-complete` stays pending
  through rollback. Update `docs/automation.md` and
  `docs/automation-reference.md`, then run `make check-automation-docs` and
  `make automation-client-self-test` if the client changed.
- [ ] 8.2 (group G5) `make accessibility-smoke` gets a skipped-files case,
  and `win.show-search-panel-skipped-files` then lists `AccessibilitySmoke`
  in its catalog lanes (decision 11). Update `docs/accessibility.md`, the
  rows in `docs/accessibility-matrix.md` and the Orca checklist row for the
  Skipped Files dialog.
- [x] 8.3 `docs/thresholds.md`:
  - `MAX_REPLACE_FILE_BYTES` derived;
  - `MAIN_*` recorded exceptions;
  - `REPLACE_DISK_RESERVE_BYTES`, `REPLACE_STREAM_CHUNK_BYTES` and
    `MAX_REPLACE_SKIP_LIST_BYTES`;
  - `MAX_REPLACE_UNDO_BYTES` retired as a name; the legacy loader's bound
    references `MAIN_REPLACE_UNDO_BYTES`.

  Run `make check-thresholds`.
- [x] 8.4 `docs/workflow-readability-matrix.md` `WFR-SEARCH-REPLACE`:
  - the new files;
  - `skipped_files.rs` as a called surface;
  - policy consumers;
  - evidence fields;
  - re-derived measured cells.

  The facade stays at most 370 lines. Run `make check-workflow-boundaries`.
- [ ] 8.5 Update `README.md` (Replace All up to 512 MiB, Skipped Files,
  Cancel), `AGENTS.md` (the module layout, including `replace/stream/utf8.rs`,
  the "Multi-file Replace All" and "Replace/Undo bounded evidence"
  decisions), `ui/search_panel/AGENTS.md`, `docs/end-user-coverage.md` (the
  crash smoke case) and the gtk-testing skill references, if a new testing
  pitfall appears. `ui/search_panel/AGENTS.md` and
  `docs/end-user-coverage.md` are fingerprinted and land in G5; the rest may
  land earlier.
- [ ] 8.6 Programme record (`docs/next/long-line-documents.md`):
  - item 2's status;
  - the prepared AppStream release notes ("Replace All rewrites files up to
    512 MiB … lists skipped files and offers to finish them in the editor");
  - an "SR" implementation section with the measurements.

## 9. Lanes and verdicts

- [ ] 9.1 Responsiveness lane cases (`make responsiveness-smoke`, measurement
  build). The cases do not exist in the lane yet: G5 adds them (the lane's
  scenario list and its self-test, `make check-responsiveness-lane`), then
  part C of the queued entry "stream-replace-all: timing verdicts"
  turns `READY <sha>`. Interleave with `main` where `main` has the case; the
  dialog and the progress poll are new, so those are branch-only rounds
  judged against the budget alone:
  - (a) the Replace All completion terminal with 16,384 skips, within
    `B_settle`;
  - (b) opening the Skipped Files dialog with 16,384 rows, plus its first
    frame, within `B_settle`;
  - (c) the progress heartbeat during a 512 MiB run, within `B_interact`
    (the lane for "Replace All reports progress for long runs");
  - (d) typing in another tab during a 512 MiB run, within `B_interact`.
- [ ] 9.2 Memory verdict, the M9 method, as in 4.8: four rounds
  interleaved `main` → branch → `main` → branch through `heavy.sh` (design
  R3), on the final code.
- [ ] 9.3 Wall-time record (timing verdicts, part B of the queued entry
  "stream-replace-all: timing verdicts", four rounds interleaved `main` → branch → `main` →
  branch per case, after G5):
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
  - `make kani KANI_SHARD=core-replace-protocol` once and detached on the
    final code (`core-replace-stream`'s inputs were final in stage 1 and it
    ran then; rerun it only if `replace/stream/utf8.rs` changes);
  - `make crash-recovery-smoke`, `make accessibility-smoke`,
    `make gtk-axioms` and `make check-gtk-axioms`.

  All must be green on local evidence.
