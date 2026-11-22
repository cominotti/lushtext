# Tasks

## Scope

- **Kani: none (no proven module touched).** The `core-replace-protocol`
  harnesses read only `services/content_search/replace/protocol.rs`.
  `replace/admission.rs` has no harness. No harness file names
  `plain_disposal`, `search_panel`, `automation` or `command_palette`
  (design D9). If the implementation touches any `model/` file besides
  `model/automation.rs`, run `scripts/kani-shards.py oracle <module>` and
  `grep -rln '<module>' crates --include=kani_proofs.rs` first, and record the
  result in this list.
- **Timing: none (no timed path touched).** The bodies of the judged spans
  `replace_completion` and `replace_progress_heartbeat` are unchanged
  (design, "Timing"). No verdict round and no lane case are owed.
- **Axioms: none new** (design D9).
- **Tests** come first in each group, and each names where it must fail.
- **Proof lanes and commits.**
  - Every code and test file here is under the UI prefixes, so a commit that
    carries them needs the three smokes current (`testing.md`):
    `make visual-geometry-smoke`, `make accessibility-smoke`,
    `make visual-smoke`. Run them last, just before the commit.
  - **One code commit.** Group 1 does not commit (task 1.7): its seam and red
    tests land with group 2 in the single commit of task 2.11, so the
    UI-prefix files change in one commit and the smokes run once
    (`testing.md`, "Proof Obligations"; `parallel-agent-work.md`,
    "Durability"). The red tree is kept as a patch in the handoff directory
    instead, so the failing-first checkpoint survives.
  - Every heavy command and every commit goes through `scripts/heavy.sh`.
    Commit with `scripts/heavy.sh --mem none -- git commit -F <file>`, signed,
    then push the change branch.
  - Cheap gates come first, in the order of `testing.md`, "Gate Order Before a
    Commit".
- **Data safety.** The data-safety full audit (`$data-safety`) runs at
  review. Task 2.10 runs its scoped checklist on the diff before the
  group-2 commit.

## 1. Test seam and failing-first regressions (design D7, D8)

- [x] 1.1 Add the index-build delay seam.
  - `with_file_index_build_delay(Duration)` and `delay_file_index_build()` in
    `crates/lushtext-core/src/ui/command_palette/test_policy.rs`, behind
    `#[cfg(feature = "test-utils")]`.
  - Call it from the rebuild worker in `ui/window/palette_shell.rs`, after the
    GTK-side `MAX_FILE_INDEX_RETAINED_BYTES` reservation and before the
    worker shrinks it.

  Verify: `cargo check -p lushtext --bins --locked` (default features) and
  `make check-clippy` pass, and a shipped build has no call (the `cfg`
  holds).
- [x] 1.2 Write T1 in `crates/lushtext/tests/widget/search_panel.rs`. It
  covers the single window with a filled lane: preview three files,
  `wait_for_journal_disk_idle`, `settle_file_index_builds`, then
  `fill_disposal_capacity_for_test()` in the same turn, Confirm, then after
  500 ms assert that the files are unchanged. Drop the fill, and within 20 s
  every file reads "thread\n", `last_apply_counts` is `Some` and Undo is
  offered. It uses only surfaces that exist on `main`. Every fill-based test
  below follows design D8, "Filling the lane": settle first, and make every
  preview and open every tab before the fill.
  **It must fail on `main`** at the wait after the drop, because the run never
  starts.
- [x] 1.3 Write T2 (design D8): the two-window variant. A holds an Undo from
  an earlier run and a second preview; B previews, both windows settle, the
  fill is taken and B confirms; A's Confirm and A's Undo must each be
  refused with "Another window is running Replace All"; after the drop B
  completes and A's kept preview confirms. **It must fail on `main`**: A is
  not refused, because B's transaction was released at the refusal.
- [x] 1.4 Write T6 (design D8): a variant of
  `test_an_apply_while_another_window_runs_is_refused`, keeping the original
  test too.
  - Install `with_file_index_build_delay(Duration::from_secs(3))`.
  - Confirm **without** `confirm_with_settled_file_index`: call
    `rebuild_file_index()` on B's window, wait until
    `file_index_builds.snapshot().started` rises, then Confirm, so the build
    provably holds its 64 MiB.

  **It must fail on `main`** (attribution Evidence 2: 2 of 2). Through the
  helper it would pass, and that pass would prove nothing.
- [x] 1.5 Assign any new test to its widget shard in `scripts/widget-shards.py`
  if a new module file was added (`make check-widget-shards`).
- [x] 1.6 Run T1, T2 and T6 red with
  `scripts/heavy.sh --envelope 16G -- scripts/run-widget-tests.sh --headless --retries 0 -- --exact <test>`.
  Save each log and its exit status to `$H/replace-all-confirm-disposal-lane/red/`.
  Verify: each fails at the named point and at no earlier one.
- [x] 1.7 Do not commit. Run the cheap gates on the red tree, then save it
  as a patch: `git -C <wt> add -N` the new files, then
  `git -C <wt> diff HEAD --binary > $H/replace-all-confirm-disposal-lane/red/red-tree.patch`,
  and record `git -C <wt> rev-parse HEAD` beside it. The tree is committed
  with group 2 (task 2.11).

## 2. Wait for capacity instead of giving up (spec: all four ADDED requirements; design D1–D6, D8)

- [x] 2.1 Write the unit tests for the pure resumption decision first,
  covering every input combination of design D5: the transaction claimed or
  not, the freshness current or stale, cancel requested or not, the run
  control present or absent. **They must fail** (they do not compile) before
  2.2.
- [x] 2.2 Add `policy::replace_admission_resume`, `ReplaceAdmissionDecision`
  and `ReplaceWaitEnd` in `ui/search_panel/policy.rs`. They stay GTK-free.
  Verify: the 2.1 tests pass and `make check-workflow-boundaries` passes.
- [x] 2.2a Add the evidence the group-2 tests read, so they compile before
  the behaviour exists (widget tests are one binary):
  - the slot `imp::SearchPreviewState::parked_replace_run`, holding a
    `ParkedReplaceRun` (the `GuardedReplacements` plus the
    `ReplaceJournalFreshness`), empty until 2.5 parks a run;
  - the ungated `SearchPanelEvidence::replace_admission_deferred`, read
    from that slot.

  Verify: `make check-workflow-boundaries`, and every existing
  `search_panel` widget test still compiles.
- [x] 2.3 Extend T1 and T2 with the deferred-state assertions of design D8,
  step 3:
  - `replace_admission_deferred` and `replace_in_flight` are true;
  - the transaction is claimed;
  - the button reads Cancel Replace All.

  Also assert, in T1, that `search-complete` readiness is pending while
  deferred, and that the status line still shows the waiting text after
  11 s with the fill held.

  Add the tests of design D8:
  - T3: cancel while waiting keeps the earlier Undo and the earlier Show
    Skipped Files report, and Undo is unavailable during the wait;
  - T4: Escape while waiting still replaces;
  - T4b: during a new search the search's progress shows, and after it ends
    the waiting text comes back within 2 s;
  - T5: Stop and Close while waiting writes nothing;
  - T7: a file open before the fill and edited during the wait is skipped as
    unsaved;
  - T8: every file edited during the wait means nothing is written, and a
    Close When Finished chosen during the wait re-enters the close path,
    which presents Save Changes? for the edited tabs;
  - T9: an epoch change with the lane still full keeps the wait;
  - T10: dispose while waiting (window B destroyed, the test's references
    dropped, and B's panel waited out until finalized) writes none of B's
    files after the fill drops and frees the transaction for A;

  Add T10b (design D8): a standalone panel's claimed transaction is released
  by its dispose.

  **Each must fail** before 2.4–2.7. With 2.2a in place they compile, and:
  - the extended T1 and T2, and T10, fail at the deferred-state assertion,
    because `replace_admission_deferred` stays false;
  - T3–T5, T4b and T7–T9 fail because no wait exists, so no run starts after
    the refusal;
  - T10b fails because dispose releases nothing.
- [x] 2.3a Run the group-2 tests red, as 1.6 does, and save each log and
  exit status to `$H/replace-all-confirm-disposal-lane/red/`. Verify: each
  fails at the named point and at no earlier one.
- [x] 2.4 Add `ui/search_panel/replace_admission.rs` (design D2).
  - `replace_admission_wakeup` in `imp::SearchPreviewState` (the slot and
    `ParkedReplaceRun` exist since 2.2a).
  - `defer_replace_run`, `resume_parked_replace_run`,
    `cancel_parked_replace_run`, `end_deferred_replace_run` and
    `replace_admission_deferred`; the `ReplaceAdmissionResumption` payload
    (`Resume(parked)` or `Ended(ReplaceWaitEnd)`).
  - `release_replace_transaction_on_dispose()` in `journal.rs`, which resets
    only the two transaction fields it owns and touches no widget (design
    D4).
  - The `replace_run_resumed` callback in `mod.rs`
    (`connect_replace_run_resumed`). Add its control-inversion entry to the
    module docs of `mod.rs` and `replace_admission.rs`.

  The parked selection is retired only through
  `spawn_guarded_preview_retirement`. Verify: `make check-workflow-boundaries`.
- [x] 2.5 Split the guarded Replace All callback in `ui/window/search.rs` into
  claim, then `admit_or_wait`, then start (design D2, D3):
  - the epoch is read before each attempt;
  - the skip set is recomputed on every attempt;
  - the unsaved-only report comes before any wait;
  - `begin_replace_run` is called on the first refusal, or in `start` when
    the first attempt is admitted at once; a resumed attempt reuses the
    existing control. Its skip-report clearing (`last_skip_report`,
    `last_run_intent`, Show Skipped Files) moves to admission, as
    `clear_previous_skip_report()` called by `start` (design D1);
  - `supersede_prior_undo_for_replace` is called only after admission;
  - a resumed attempt that ends all-unsaved stops the keepalive, clears the
    waiting description and calls `close_if_requested_after_replace_run`.

  Remove the "Replace All deferred while memory pressure clears; try again
  shortly" message. Verify: T2, T6, T7, T8 and T9 pass, and T1 passes except
  its status-line assertion, which needs 2.7.
- [x] 2.6 Handle cancel, close and dispose during the wait (design D4):
  - `request_replace_cancel` takes the deferred branch;
  - the window's `Ended` handler calls `end_deferred_replace_run` and
    `finish_replace_transaction`, never `finish_replace_run(None)`, which
    would erase the earlier skip report. For `Cancelled` it publishes and
    announces "Replace All cancelled; no file was changed"; for `Superseded`
    (design D5) it publishes `main`'s "Replace All was superseded before any
    files changed". Either way it calls
    `close_if_requested_after_replace_run`;
  - `imp::dispose` cancels the wakeup, drops the parked run and calls
    `release_replace_transaction_on_dispose()`. It invokes no window
    callback.

  Verify: T3, T4, T5, T10 and T10b pass.
- [x] 2.7 Show the wait (design D6):
  - the `replace-all-waiting` announcement;
  - the progress status message, with a 1 s keepalive that **re-publishes**
    it through `update_search_progress_message`, not a heartbeat, and skips
    its tick while a workspace search is in flight (design D6). It is
    stopped by `start_replace_progress_tracking`, by the `Ended` handler
    and by the all-unsaved end;
  - the button's accessible description and tooltip, set by a setter in
    `ui/search_panel/accessibility.rs` on entering and leaving the wait.
    `refresh_accessibility_state` does not compute them, so the judged
    completion span's body stays unchanged;
  - the snapshot field `content_search.replace_admission_deferred` in
    `model/automation.rs` and `ui/automation.rs`.

  Verify: the extended T1 passes, including that the status line still shows
  the waiting text after 11 s with the fill held, and T4b passes. Run
  `make check-automation-docs` after 2.9.
- [x] 2.8 Retire `confirm_with_settled_file_index` (design D8). Restore its
  8 call sites to `panel.activate_confirm_replacements()`. Keep
  `settle_file_index_builds` for the step before a fill and rewrite its doc
  comment. Make `replace_all_held_at_second_file` wait for
  `replace_in_flight && !replace_admission_deferred`. Verify:
  - `scripts/heavy.sh --envelope 16G -- make test-widget-shard WIDGET_SHARD=<the search_panel shard>`
    passes with no `FLAKY` line;
  - the 14 tests (the 8 call sites and the 6 callers of
    `replace_all_held_at_second_file`) pass in isolation 10 times each with
    `--retries 0` through `heavy.sh`;
  - **forced overlap:** with a temporary, uncommitted edit that installs
    `with_file_index_build_delay(Duration::from_secs(3))` and calls
    `rebuild_file_index()` before each of those Confirms, as T6 does, the
    same 14 tests pass once each with `--retries 0`. Revert the edit and
    check `git -C <wt> diff` shows none of it.

  Save every log to `$H/replace-all-confirm-disposal-lane/retire/`.
- [x] 2.9 Update the documentation `documentation.md` requires:
  - `docs/automation.md` and `docs/automation-reference.md`: the new snapshot
    field row and anchor `snapshot-field-replace-admission-deferred`, plus
    `replace_in_flight`'s widened meaning;
  - `docs/accessibility.md` (Replace All run paragraph) and the
    `A11Y-WORKSPACE-SEARCH-REPLACE-SKIPPED` row of
    `docs/accessibility-matrix.md`;
  - `docs/workflow-readability-matrix.md` `WFR-SEARCH-REPLACE`: the new
    coordination module, seam value object, evidence field and resume
    callback. Re-derive the row's measured cells under
    `workflow-convention.md`, "Re-deriving a row's measured cells";
  - `crates/lushtext-core/src/ui/search_panel/AGENTS.md`: the coordination
    list and the Replace All contracts, including that a run waiting for
    capacity holds the transaction and is cancelled only by Cancel, Stop and
    Close or dispose;
  - the root `AGENTS.md` "Multi-file Replace All" entry, and `README.md` if
    its Replace All feature text names the refusal.

  Verify: `make check-agent-docs` and `make check-automation-docs`.
- [x] 2.9a Extend
  `test_evidence_reads_stay_side_effect_free_across_journal_mutation`
  across `defer_replace_run`, `resume_parked_replace_run` and
  `cancel_parked_replace_run` (design D8, "Evidence proof";
  `workflow-convention.md`, "Evidence surfaces").
- [x] 2.10 Run the data-safety scoped checklist on the diff.
  - Check that the earlier Undo survives every way out of the wait.
  - Check that no path writes a file without admission.
  - Check that the parked payload reaches the lane on every exit.

  Record the findings in `$H/replace-all-confirm-disposal-lane/data-safety-impl.md`.
- [x] 2.11 Run the cheap gates, `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" cargo doc --workspace --no-deps`
  (a new module doc) and the three smokes, then commit groups 1 and 2
  together as
  `fix(search-panel): wait for disposal capacity after Confirm instead of discarding the selection`
  (the body names the failing-first tests), and push.

## 3. Integration gates

- [x] 3.1 Run `scripts/heavy.sh -- make check` and
  `scripts/heavy.sh --envelope 16G -- make test` on the final tree. Both must
  pass, with no `FLAKY`, `FLAKY-SHARD:`, `OOM-KILL:` or `SESSION-HYGIENE:`
  line. Record `/proc/loadavg` before and after.
- [x] 3.2 Run `openspec validate replace-all-confirm-disposal-lane --strict`
  (must pass) and `openspec validate --all --strict`, and check every box
  above against its evidence. At the spec review, `--all --strict` exited 1
  with 86 failures, the same set in this worktree and in the main checkout
  at `5b9abbb6`, and this change among the passes. Expect the same: the
  check is that this change passes and adds no failure. The 86 are a
  coordinator item, not a blocker of this change.

  Done on the tree after the `origin/main` merge (`2bd42bdd`, the archive of
  `fix-headless-session-media-wait`): this change is valid, and `--all
  --strict` exits 1 with the same 86 failures (`r2/openspec-all-3.log`).
  Task 3.1's gates are recorded in `gate-trees.log` on the tree of the commit
  that carries these ticks.

## 4. Apply-stage review fixes

Findings of the review lenses confirmed and applied by the apply stage
(`$H/replace-all-confirm-disposal-lane/review-findings-applied.md` lists each
finding, applied or rejected with its reason). Each behaviour change has a
widget test that failed first (`apply-red/`) and passes (`apply-green/`).

- [x] 4.1 The undo reservation of a Replace All credits every window's
  installed journal, which its admission retires
  (`policy::combined_undo_weight`); test
  `test_a_run_credits_every_windows_undo_journal_it_retires`.
- [x] 4.2 The startup journal load stands down while the panel's own
  transaction is claimed and resumes when it ends; test
  `test_persisted_undo_load_waits_for_a_claimed_transaction`.
- [x] 4.3 A run's admission retires every panel's pending or in-flight
  journal load (wakeup cancelled, other panels' generations advanced, journal
  or not); test `test_a_run_retires_another_windows_journal_load_in_flight`,
  with the test policy's journal disk delay now also holding a startup
  recovery after its read.
- [x] 4.4 The close-during-run question says, while the run still waits,
  that no file has changed (`test_stop_and_close_while_waiting_writes_nothing`),
  and a preview asked for during a run says why nothing happens
  (`test_escape_while_waiting_still_replaces`).
- [x] 4.5 Structure: the admission attempt (epoch, then reservation) moved
  into `replace_admission::try_admit_replace_run`, whose `RefusedReplaceRun`
  alone can park a run; the attempt kind is explicit
  (`AdmissionAttempt`); the waiting description is projected once per wait;
  `end_deferred_replace_run` also releases the transaction; the window reads
  the search worker through `search_worker_running`; the window module's
  header names its stage-4 block.

## Workflow follow-up

- Review stages: `/simplify`, then `code-review` plus the `data-safety` full
  audit, which is owed because this changes the Replace All transaction and
  undo journal.
- Archive syncs the four ADDED requirements into
  `openspec/specs/search-replace-safety/spec.md`.
- Follow-up candidates to record in the ledger, not in this change:
  - Undo's capacity refusal;
  - sizing the undo reservation from the selection.
