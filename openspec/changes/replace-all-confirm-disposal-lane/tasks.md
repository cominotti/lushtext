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
  - Every file here is under the UI prefixes, so each commit needs the three
    smokes current (`testing.md`): `make visual-geometry-smoke`,
    `make accessibility-smoke`, `make visual-smoke`. Run them last, just
    before the commit.
  - Every heavy command and every commit goes through `scripts/heavy.sh`.
    Commit with `scripts/heavy.sh --mem none -- git commit -F <file>`, signed,
    then push the change branch.
  - Cheap gates come first, in the order of `testing.md`, "Gate Order Before a
    Commit".
- **Data safety.** The data-safety full audit (`$data-safety`) runs at
  review. Task 2.10 runs its scoped checklist on the diff before the
  group-2 commit.

## 1. Test seam and failing-first regressions (design D7, D8)

- [ ] 1.1 Add the index-build delay seam.
  - `with_file_index_build_delay(Duration)` and `delay_file_index_build()` in
    `crates/lushtext-core/src/ui/command_palette/test_policy.rs`, behind
    `#[cfg(feature = "test-utils")]`.
  - Call it from the rebuild worker in `ui/window/palette_shell.rs`, after the
    GTK-side `MAX_FILE_INDEX_RETAINED_BYTES` reservation and before the
    worker shrinks it.

  Verify: `cargo check -p lushtext --bins --locked` (default features) and
  `make check-clippy` pass, and a shipped build has no call (the `cfg`
  holds).
- [ ] 1.2 Write T1 in `crates/lushtext/tests/widget/search_panel.rs`. It
  covers the single window with a filled lane: preview three files,
  `wait_for_journal_disk_idle`, `fill_disposal_capacity_for_test()`, Confirm,
  then after 500 ms assert that the files are unchanged. Drop the fill, and
  within 10 s every file reads "thread\n", `last_apply_counts` is `Some` and
  Undo is offered. It uses only surfaces that exist on `main`.
  **It must fail on `main`** at the wait after the drop, because the run never
  starts.
- [ ] 1.3 Write T2: the two-window variant. B is filled and confirmed; A then
  confirms and must be refused with "Another window is running Replace All";
  after the drop B completes. **It must fail on `main`**: A is not refused,
  because B's transaction was released at the refusal.
- [ ] 1.4 Write T6 (design D8): a variant of
  `test_an_apply_while_another_window_runs_is_refused`, keeping the original
  test too.
  - Install `with_file_index_build_delay(Duration::from_secs(3))`.
  - Confirm **without** `confirm_with_settled_file_index`: call
    `rebuild_file_index()` on B's window, wait until
    `file_index_builds.snapshot().started` rises, then Confirm, so the build
    provably holds its 64 MiB.

  **It must fail on `main`** (attribution Evidence 2: 2 of 2). Through the
  helper it would pass, and that pass would prove nothing.
- [ ] 1.5 Assign any new test to its widget shard in `scripts/widget-shards.py`
  if a new module file was added (`make check-widget-shards`).
- [ ] 1.6 Run T1, T2 and T6 red with
  `scripts/heavy.sh --envelope 16G -- scripts/run-widget-tests.sh --headless --retries 0 -- --exact <test>`.
  Save each log and its exit status to `$H/replace-all-confirm-disposal-lane/red/`.
  Verify: each fails at the named point and at no earlier one.
- [ ] 1.7 Run the cheap gates and the three smokes, then commit
  `test(search-panel): reproduce the Confirm refused by a file-index build`
  and push.

## 2. Wait for capacity instead of giving up (spec: all four ADDED requirements; design D1–D6, D8)

- [ ] 2.1 Write the unit tests for the pure resumption decision first,
  covering every input combination of design D5: the transaction claimed or
  not, the freshness current or stale, cancel requested or not, the run
  control present or absent. **They must fail** (they do not compile) before
  2.2.
- [ ] 2.2 Add `policy::replace_admission_resume` and `ReplaceAdmissionResume`
  in `ui/search_panel/policy.rs`. They stay GTK-free. Verify: the 2.1 tests
  pass and `make check-workflow-boundaries` passes.
- [ ] 2.3 Extend T1 and T2 with the deferred-state assertions of design D8,
  step 3:
  - `replace_admission_deferred` and `replace_in_flight` are true;
  - the transaction is claimed;
  - the button reads Cancel Replace All.

  Add the tests of design D8:
  - T3: cancel while waiting keeps the earlier Undo and the earlier Show
    Skipped Files report;
  - T4: Escape while waiting still replaces;
  - T4b: after a new search during the wait, the waiting text comes back;
  - T5: Stop and Close while waiting writes nothing;
  - T7: a file edited in a tab during the wait is skipped as unsaved;
  - T8: every file edited during the wait means nothing is written, and Close
    When Finished closes;
  - T9: an epoch change with the lane still full keeps the wait;
  - T10: dispose while waiting writes nothing and frees the transaction for
    another window.

  **Each must fail** before 2.4–2.7:
  - the extended T1 and T2 because the field is missing;
  - T3–T5, T4b and T7–T10 because no wait exists, so no run starts after the
    refusal.
- [ ] 2.4 Add `ui/search_panel/replace_admission.rs` (design D2).
  - `ParkedReplaceRun`, `parked_replace_run` and `replace_admission_wakeup`
    in `imp::SearchPreviewState`.
  - `defer_replace_run`, `resume_parked_replace_run`,
    `cancel_parked_replace_run` and `replace_admission_deferred`.
  - The `replace_run_resumed` callback in `mod.rs`
    (`connect_replace_run_resumed`). Add its control-inversion entry to the
    module docs of `mod.rs` and `replace_admission.rs`.

  The parked selection is retired only through
  `spawn_guarded_preview_retirement`. Verify: `make check-workflow-boundaries`.
- [ ] 2.5 Split the guarded Replace All callback in `ui/window/search.rs` into
  claim, then `admit_or_wait`, then start (design D2, D3):
  - the epoch is read before each attempt;
  - the skip set is recomputed on every attempt;
  - the unsaved-only report comes before any wait;
  - `begin_replace_run` is called on the first refusal, with its
    skip-report clearing (`last_skip_report`, `last_run_intent`, Show
    Skipped Files) moved to admission (design D1);
  - `supersede_prior_undo_for_replace` is called only after admission;
  - a resumed attempt that ends all-unsaved stops the keepalive, clears the
    waiting description and calls `close_if_requested_after_replace_run`.

  Remove the "Replace All deferred while memory pressure clears; try again
  shortly" message. Verify: T1, T2, T6, T7, T8 and T9 pass.
- [ ] 2.6 Handle cancel, close and dispose during the wait (design D4):
  - `request_replace_cancel` takes the deferred branch;
  - the window's `Cancelled` handler publishes and announces "Replace All
    cancelled; no file was changed", and calls
    `close_if_requested_after_replace_run`;
  - `imp::dispose` cancels the wakeup, drops the parked run and clears the
    transaction flags. It invokes no window callback.

  Verify: T3, T4, T5 and T10 pass.
- [ ] 2.7 Show the wait (design D6):
  - the `replace-all-waiting` announcement;
  - the progress status message, with a 1 s keepalive that **re-publishes**
    it through `update_search_progress_message`, not a heartbeat. It is
    stopped by `start_replace_progress_tracking`, by the `Cancelled` handler
    and by the all-unsaved end;
  - the button's accessible description and tooltip, set by a setter in
    `ui/search_panel/accessibility.rs` on entering and leaving the wait.
    `refresh_accessibility_state` does not compute them, so the judged
    completion span's body stays unchanged;
  - the ungated `SearchPanelEvidence::replace_admission_deferred`;
  - the snapshot field `content_search.replace_admission_deferred` in
    `model/automation.rs` and `ui/automation.rs`.

  Verify: the extended T1 passes, including that the status line still shows
  the waiting text after 11 s with the fill held, and T4b passes. Run
  `make check-automation-docs` after 2.9.
- [ ] 2.8 Retire `confirm_with_settled_file_index` and
  `settle_file_index_builds` (design D8). Restore the 8 call sites to
  `panel.activate_confirm_replacements()`. Verify:
  `scripts/heavy.sh --envelope 16G -- make test-widget-shard WIDGET_SHARD=<the search_panel shard>`
  passes with no `FLAKY` line. Then run the 8 tests in isolation 10 times
  each with `--retries 0` through `heavy.sh`, and save the logs to
  `$H/replace-all-confirm-disposal-lane/retire/`.
- [ ] 2.9 Update the documentation `documentation.md` requires:
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
- [ ] 2.10 Run the data-safety scoped checklist on the diff.
  - Check that the earlier Undo survives every way out of the wait.
  - Check that no path writes a file without admission.
  - Check that the parked payload reaches the lane on every exit.

  Record the findings in `$H/replace-all-confirm-disposal-lane/data-safety-impl.md`.
- [ ] 2.11 Run the cheap gates, `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" cargo doc --workspace --no-deps`
  (a new module doc) and the three smokes, then commit
  `fix(search-panel): wait for disposal capacity after Confirm instead of discarding the selection`
  and push.

## 3. Integration gates

- [ ] 3.1 Run `scripts/heavy.sh -- make check` and
  `scripts/heavy.sh --envelope 16G -- make test` on the final tree. Both must
  pass, with no `FLAKY`, `FLAKY-SHARD:`, `OOM-KILL:` or `SESSION-HYGIENE:`
  line. Record `/proc/loadavg` before and after.
- [ ] 3.2 Run `openspec validate replace-all-confirm-disposal-lane --strict`
  and `openspec validate --all --strict`, and check every box above against
  its evidence.

## Workflow follow-up

- Review stages: `/simplify`, then `code-review` plus the `data-safety` full
  audit, which is owed because this changes the Replace All transaction and
  undo journal.
- Archive syncs the four ADDED requirements into
  `openspec/specs/search-replace-safety/spec.md`.
- Follow-up candidates to record in the ledger, not in this change:
  - Undo's capacity refusal;
  - sizing the undo reservation from the selection.
