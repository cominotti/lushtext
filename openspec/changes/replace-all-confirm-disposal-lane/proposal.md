# Proposal: Replace All waits for disposal capacity after Confirm

## Why

Confirm Replace gives up for good when the shared plain-data disposal lane is
briefly full. The confirmed selection is thrown away, and the user has to
preview and check the rows again.

- **Mechanism.** Confirm reserves `MAX_REPLACE_UNDO_RETAINED_BYTES` (64 MiB)
  of the 128 MiB ordinary lane before the run starts. A palette file-index
  build holds 64 MiB of the same lane until its worker shrinks the
  reservation. When the two overlap, `ui/window/search.rs` refuses the run
  with "Replace All deferred while memory pressure clears; try again
  shortly". It then releases the transaction, but the preview and its
  checked rows are already gone, and nothing retries.
- **How often an index build starts.** About 300 ms after a window opens,
  on every window activation with workspace folders, and on scope and
  visibility changes. For a real workspace the build holds its 64 MiB for
  the whole traversal.
- **Baseline.** Attribution in
  `.claude/handoffs/flaky-apply/attribution.md`, at `ba1f8154`, before #119
  hid the race in tests with `confirm_with_settled_file_index`. The test was
  `search_panel::test_an_apply_while_another_window_runs_is_refused`, run
  with an instrumented build:
  - **Natural:** refused 2 times in 74 attempts under load (8 CPU hogs, load
    5.5 → 11.9); 0 of 72 without hogs.
  - **Forced:** with the index worker delayed 3 s, 2 of 2 attempts failed.
  - **Window:** in 66 of 72 unloaded runs an index build started within
    1 ms of Confirm. Passing runs survived by margins of 0–22 ms.
  - In every failure the files were untouched and `last_apply_counts` was
    `None`: the run never started (exit E8).
- **No data is lost**, but the message's "try again" really means "preview
  again and re-check every row".
- **Everything else retries.** Replace Preview
  (`arm_preview_capacity_retry`), the persisted-undo startup load and the
  file-index build itself all wait on the lane's capacity wakeup. Confirm is
  the only consumer that throws away what the user built.

The change is worth doing as described: the defect is user-reachable, the
fix reuses an existing pattern, and #119's test workaround can then go.

## What Changes

- A Confirm whose undo reservation is refused no longer gives up. The
  claimed transaction, the confirmed selection and the run's request stay
  held, and the run waits for the lane's capacity wakeup.
  - Each time capacity changes, it recomputes which files are open with
    unsaved edits or an active save, and tries the reservation again.
  - Once admitted, it starts the same run as today.
- While it waits, the run already counts as started:
  - the Replace All button reads **Cancel Replace All**;
  - `replace_in_flight` is true and `search-complete` stays pending;
  - another window's apply or Undo is refused, as during a run;
  - a window close asks the existing close-during-run question.

  Cancel, or Stop and Close, ends the wait at once, with nothing written and
  the earlier Undo still available. Hiding the panel (Escape) or starting a
  new search does not cancel it, just as neither cancels a run in progress.
- The wait is visible and accessible:
  - one announced status message;
  - a persistent "waiting for memory" line in the panel summary;
  - the button's accessible description;
  - a new evidence field and a new automation snapshot field,
    `content_search.replace_admission_deferred`.
- Panel dispose cancels the wakeup and releases the transaction, and the
  parked selection goes to the disposal lane.
- Tests:
  - #119's `confirm_with_settled_file_index` and `settle_file_index_builds`
    workaround is retired, so the tests that used it now cover the fix;
  - a palette test-policy hook delays the file-index build;
  - failing-first tests for one window, two windows and the delayed index
    build.

## Non-goals

- **Undo's capacity refusal.** "Undo deferred while memory pressure clears;
  try again shortly" (`window/search.rs`) also gives up when the lane is
  full. It restores the Undo affordance and keeps the journal, so a retry is
  one click and nothing the user built is lost. Making Undo wait would also
  need a cancel affordance that Undo does not have. It stays as it is; a
  follow-up can take it up if wanted.
- **Sizing the undo reservation from the selection.** The 64 MiB worst case
  for a three-file run is what makes the collision likely. Changing it would
  touch the service's descriptor sizing and `main`'s no-loss admission ledger
  (`services/content_search/replace/`). Waiting fixes the race for every run
  size without widening that surface. Recorded as a possible follow-up in
  `design.md`.
- **No change** to the disposal lane, its limits, or how the file-index build
  reserves.
- **No change** to the run, its journal protocol, per-file stale and identity
  checks, rollback, or skip reasons.

## User-visible impact

- A Confirm made while an index build runs now waits briefly and then
  completes. It is no longer refused, so the user does not have to preview
  again.
- **New state.** A run can show **Cancel Replace All** before its first
  file is touched. Cancelling then reports that nothing changed.
- **Edge case, existing today.** If the lane stays full with no wakeup (an
  installed index near its 64 MiB bound plus other long-lived owners), the
  wait is indefinite. Today the same case is refused every time. Now it shows
  a cancellable waiting state.
- **Loss compared with `main`: none.** The "deferred … try again shortly"
  warning for Replace All goes away. A refusal by another window's
  transaction is unchanged.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-replace-safety`: ADDED requirements:
  - a confirmed Replace All waits for undo-reservation capacity, holding its
    selection and transaction;
  - a run waiting for capacity can be cancelled with nothing written, and the
    earlier Undo is kept;
  - the waiting state is reported accessibly and through automation.

  The cross-window refusal of "One Replace All undo journal per application"
  is extended to a waiting run, as an ADDED requirement that leaves the
  existing text intact.

## Impact

- **Code.** All under `crates/lushtext-core/src/`:
  - `ui/window/search.rs`: the guarded Replace All callback is split into
    claim, then admit-or-wait, then start.
  - `ui/search_panel/`: a new coordination module, `replace_admission.rs`,
    with the parked run and its capacity wakeup, plus `replace_execution.rs`
    (cancel during the wait), `evidence.rs`, `imp.rs` (dispose),
    `accessibility.rs`, `mod.rs` (narrative and the resume callback) and
    `policy.rs` (the pure resume decision).
  - `ui/command_palette/test_policy.rs` and `ui/window/palette_shell.rs`: a
    `test-utils` delay hook for the index build.
  - `ui/automation.rs` and `model/automation.rs`: one snapshot field.
- **Tests:**
  - `crates/lushtext/tests/widget/search_panel.rs`: new tests, and the
    helper retired;
  - the unit tests of the pure resume decision.
- **Docs:**
  - `docs/automation.md`, `docs/automation-reference.md`;
  - `docs/accessibility.md`, `docs/accessibility-matrix.md`
    (`A11Y-WORKSPACE-SEARCH-REPLACE-SKIPPED`);
  - `docs/workflow-readability-matrix.md` (`WFR-SEARCH-REPLACE`);
  - `crates/lushtext-core/src/ui/search_panel/AGENTS.md`;
  - the root `AGENTS.md` Replace All entry.
- **Proofs and data safety:**
  - Kani: none (`services/` is not touched).
  - Timing: none. The judged span bodies (`replace_completion`,
    `replace_progress_heartbeat`) are unchanged.
  - The three smokes, for the UI-prefix files.
  - The data-safety audit, for the Replace All transaction and undo journal.
