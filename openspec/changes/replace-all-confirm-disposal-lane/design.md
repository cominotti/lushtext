# Design

## Context

See `proposal.md` ("Why") for the defect and its baseline.

### Confirm today, step by step

These are the names on `main` at `5b9abbb6`.

1. `begin_confirmed_replacement` (`ui/search_panel/replace_execution.rs`):
   - claims the application's single transaction
     (`journal.rs::begin_replace_transaction`, which reserves the journal
     generation);
   - takes the accepted outcome and the checked ids out of live state;
   - opens a fresh preview ticket;
   - sets `preview_mode = false` and `preview_pending = true`;
   - partitions the checked rows on a worker.
2. The partition completion revalidates the ticket.
   - If the ticket is current, it sets `preview_pending = false` and calls
     the window's guarded Replace All callback with the `GuardedReplacements`.
     That value is a `DisposalOwned` selection that already holds its own lane
     reservation.
   - After the callback returns, it releases the transaction if the window did
     not take its generation (`replace_transaction_generation_reserved`). It
     then drains the preview worker (`finish_preview_worker`).
3. The window callback (`ui/window/search.rs`):
   1. `take_replace_transaction` consumes the reserved generation into a
      `ReplaceJournalFreshness`.
   2. It computes `skip_paths` and `open_canonical_identities` from the open
      tabs. When every file is open with unsaved changes or an active save, it
      finishes with an unsaved-only report.
   3. It reserves the undo descriptors with `try_reserve_undo_replacement`.
      This is a 64 MiB replacement reservation that credits the installed
      undo journal's weight.
   4. **Refusal (exit E8):** a warning, `finish_replace_transaction()`, and
      return. The `GuardedReplacements` drops, and its payload goes to the
      lane. **This is the defect.**
   5. On success:
      - `supersede_prior_undo_for_replace()`;
      - `begin_replace_run()`, which creates the `ReplaceRunControl` (cancel
        flag and progress), copies the run's intent, and turns the button into
        Cancel Replace All;
      - `start_replace_progress_tracking()`;
      - `spawn_blocking_then` the streamed run.

### Facts the design relies on

Each item was read in the code at `5b9abbb6`; the file is named with it.

- **The capacity wakeup** is `plain_disposal::CapacityWakeup`.
  - It is a 16 ms `timeout_add_local` (`DISPOSAL_RETRY_INTERVAL`) that fires
    its callback once the lane's `capacity_epoch` differs from the epoch the
    caller observed before its attempt.
  - The epoch advances on every permit drop, shrink, and worker job finish.
    So the index build's shrink and the test's `DisposalCapacityHold` drop
    both wake it.
  - `arm` is a no-op while armed. The source slot is cleared before the
    callback runs, so re-arming from inside the callback works.
  - Precedents: `arm_preview_capacity_retry`, the persisted-undo startup load
    (`undo_capacity_wakeup`), and `start_file_index_build`
    (`file_index_capacity_wakeup`).
- **The per-file checks happen on the worker, after admission, inside the
  service.** `services/content_search/replace/apply.rs` checks each file's
  recorded line content on the worker (`ReplacementTextOutcome::StaleLine`,
  "validates stale search results per file") and its
  identity facts at pass A and pass B ("refuses a target changed during the
  run"). `apply_request_if_current` checks the journal generation
  (`ReplaceJournalFreshness`) under the journal coordinator before any write.
  - The brief's "file digests" recheck is therefore already done on the
    worker, after the wait. No GTK-side digest is added.
  - That check reads the file on disk. **An edit held only in an open tab
    never reaches disk**, so for unsaved tabs the recomputed skip set (D3) is
    the only guard, and it is tested (D8, T7 and T8).
- **The disposal-lane contract** (`ui/AGENTS.md`): a document-sized result
  keeps its reservation until accepted transfer or final worker destruction,
  and capacity pressure retains at most one compact request and one retry
  source. The parked run is an already-reserved `DisposalOwned` selection,
  not a new admission, and its one retry source is the admission wakeup.
  GTK callbacks never block on admission: the wait is a timeout source, not
  a blocking call.
- **A run in flight already has the reactions the wait needs:**
  - `close_request` checks `replace_run_in_flight()` and asks the
    close-during-run question;
  - the button click handler routes to `request_replace_cancel()`;
  - `begin_confirmed_replacement` refuses while the transaction is claimed;
  - `replace_preview_pending()` includes `replace_transaction_claimed()`, so
    the `replace-preview` readiness stays pending;
  - `search-complete` stays pending through `replace_in_flight`;
  - other windows' `begin_replace_transaction` refuses while this panel's
    transaction is claimed.
- **A new search, Escape and panel hide do not touch a run in flight.**
  `invalidate_replace_preview_request` returns early once `preview_mode` and
  `preview_pending` are both false. That is the state after the partition
  completion, so the parked selection is unaffected.
- **Progress notifications expire, and a search resolves them.**
  `NotificationBus::update_progress` records expire unless they are updated
  or heart-beaten, and a heartbeat only refreshes a record that still
  exists. `prepare_search_progress_tracking` runs on every search-entry
  change and search start, and begins with `finish_search_progress_tracking`,
  which resolves the Search owner's status record. The run's poll survives
  this because it re-publishes with `update_search_progress_message` on
  every tick. A one-off `publish_status_message` expires after 10 s,
  which is why the attribution's failures showed an empty status line.

### Baseline

These figures are from the attribution at `ba1f8154`, with an instrumented
build (`flaky-apply/attr-instrumentation.patch`), running
`scripts/heavy.sh --envelope 16G -- scripts/run-widget-tests.sh --headless -- --exact search_panel::test_an_apply_while_another_window_runs_is_refused`:

| Case | Attempts | Refused at the undo reservation (E8) |
| --- | --- | --- |
| Batch A: 6 rounds × 12 concurrent runs + 8 CPU hogs, load 5.5 → 11.9 | 74 | 2 |
| Batch B: same, no hogs, load 5.7 → 5.2 | 72 | 0 |
| Index worker delayed 3 s (`ATTR_INDEX_SLEEP_MS=3000`) | 2 | 2 |

- In 66 of the 72 batch-B runs, an index build started within 1 ms of
  Confirm.
- Passing margins were 0–22 ms.
- H2 (a stale preview ticket) was never observed in 148 attempts.
- No new measurement is needed: every decision below follows from the code
  path and these figures.

## Goals / Non-Goals

**Goals:**
- **G1.** A Confirm refused only for disposal capacity keeps its selection and
  transaction, and starts by itself once capacity allows.
- **G2.** While it waits, every user-facing and cross-window rule of a run in
  flight applies. Cancel ends it with nothing written.
- **G3.** The wait is visible, announced once, and readable from evidence and
  automation.
- **G4.** Tests prove the race without the #119 workaround.

**Non-Goals:** as `proposal.md` ("Non-goals") lists, with Undo's capacity
refusal and the reservation's size first. Two more design-level boundaries:
- the wait has no timeout;
- the run's worker, journal and service are byte-for-byte unchanged.

## Decisions

### D1. The wait is the run's first phase, not a separate pre-run state

On the first refusal, the window calls `begin_replace_run()` before
parking. One part of `begin_replace_run` moves to admission:
- clearing `last_skip_report` and `last_run_intent`;
- hiding Show Skipped Files.

So a cancelled wait keeps the earlier run's skip report and its Open and
Replace in Editor rows, and `main` loses nothing. So the `ReplaceRunControl` (cancel flag, progress), the run's intent
copy and the Cancel Replace All button all exist from that moment.
`replace_in_flight` is true from then on. `supersede_prior_undo_for_replace()`
stays where it is: only after the reservation succeeds, immediately before
the worker starts.

Why:
- **Escape keeps the run.** Confirm followed by Escape is the ordinary way
  back to the editor. A wait cancelled by Escape would silently drop a replace
  the user confirmed, which is exactly the loss this change removes. A run
  that has started keeps going after Escape, and the user cannot tell when
  admission happened.
- **Close, Cancel and readiness need no new rules.** They reuse the
  run-in-flight rules listed in "Facts". There is no second state machine for
  them.
- **New searches are safe.** The run validates each file on the worker after
  the wait, so a search or edit during the wait cannot make it write stale
  replacements. A file edited in an open tab is skipped by the recomputed skip
  set (D3).
- **Undo is kept on cancel.** Keeping the supersession after admission means
  a cancelled wait leaves the earlier Undo installed and usable. The
  replacement reservation still credits that journal's weight.

Alternatives rejected:
- **Cancel the wait on Escape, close or a new search** (the brief's sketch,
  R1 of `bound-widget-log-capture`). This drops confirmed work on the most
  common next keystroke. It also needs freshness rules that the run's own
  checks already provide. It needs a separate close and readiness path too.
  "Close" is kept in a form that still cancels: Stop and Close from the
  existing close-during-run question.
- **Recheck the preview generation before starting.** The preview was consumed
  at Confirm, and the selection was validated current at the partition
  completion. A later generation bump (a new search) says nothing about the
  files. The worker's per-file checks are what decide.
- **Park before `take_replace_transaction` and re-invoke the panel's
  callback.** That re-reserves a generation the window already consumed. It
  also collides with the panel's post-callback
  `replace_transaction_generation_reserved()` release.

### D2. Where the parked run lives: a new panel coordination module `replace_admission.rs`

`ui/search_panel/replace_admission.rs` is coordination for Replace stage 4's
admission. It owns:
- `SearchPreviewState::parked_replace_run: RefCell<Option<ParkedReplaceRun>>`;
- `replace_admission_wakeup: DisposalCapacityWakeup`.

`ParkedReplaceRun` holds the `GuardedReplacements` and the
`ReplaceJournalFreshness`. It is a seam value object, built once by the
window at the refusal and validated as a unit at resumption.

Operations:
- `defer_replace_run(parked, observed_epoch)`: parks the run and arms the
  wakeup.
- On wakeup, `resume_parked_replace_run()` takes the parked run. It then asks
  the pure policy (D5) whether to resume, and invokes the window's new
  `replace_run_resumed` callback (`mod.rs`, `connect_replace_run_resumed`)
  with `ReplaceAdmissionResume::Resume(parked)`.
- `cancel_parked_replace_run()`: used by `request_replace_cancel` only.
  Dispose does not call it (D4), because it must not call back into a
  window that is being destroyed.
  - It cancels the wakeup.
  - It hands the parked selection to bounded retirement (the existing
    `spawn_guarded_preview_retirement` path).
  - It invokes the same callback with `Cancelled`.
- `replace_admission_deferred()`: the evidence read.

Why the panel:
- every capacity wakeup, and each one's dispose cancel, lives with the
  workflow that owns the parked value;
- `ui/search_panel/AGENTS.md` keeps timers and admission bookkeeping out of
  `mod.rs` and out of the window shell;
- `journal.rs` (772 lines) already carries the startup-recovery retry, and
  this is a different stage's admission;
- the convention names coordination modules for their job (`admission`),
  with the stage-order qualifier because the panel holds two stage orders.

The window keeps the narrative, because the skip set needs the window's tabs
(D3). `ui/window/search.rs` splits its callback into three steps:
- `claim`: `take_replace_transaction`;
- `admit_or_wait`: skip set, then reservation;
- `start`: supersede, progress, spawn.

Both the first call and every resumption enter `admit_or_wait`.

The parking happens inside the callback, after `take_replace_transaction`
consumed the generation. So when the callback returns,
`replace_transaction_generation_reserved()` is false, the panel does not
release the transaction, and `finish_preview_worker()` only clears
`preview_worker_running`: no preview request is queued while a transaction
is claimed.

Alternative rejected: keeping the wakeup and the parked selection in the
window. A second owner of panel workflow state would sit outside the panel's
dispose, and would duplicate the transaction ownership that `journal.rs`
holds.

### D3. Every attempt recomputes the skip set, then reserves; the epoch is read first

`admit_or_wait` does the same steps on the first attempt and on every
resumption:

1. Compute `skip_paths` and `open_canonical_identities` from the current tabs.
2. If every file is skipped, finish with the unsaved-only report, as today.
   On a resumed attempt this also ends the wait:
   - stop the wait's keepalive (D6) and clear the button's waiting
     description;
   - `finish_replace_run(Some(report))`, then `finish_replace_transaction()`;
   - `close_if_requested_after_replace_run()`, so a Close When Finished
     chosen during the wait still closes.
3. `observed_epoch = disposal_capacity_epoch()`.
4. `try_reserve_undo_replacement(weight)`.
   - If refused, `defer_replace_run(parked, observed_epoch)`. On the first
     refusal, call `begin_replace_run()` first, and announce and show the
     wait (D6).
   - If admitted, start (step 3 of the split).

Notes:
- The epoch is read before the attempt, so a release between the attempt and
  the arming still wakes the wait.
- A wakeup whose epoch moved but whose capacity is still short refuses again
  and re-arms with a fresh epoch. Because of the 16 ms poll and the
  epoch-gated callback, it never spins.
- The "every file unsaved" report comes **before** any wait: the user learns
  at once that nothing would run, rather than after the lane frees up.

### D4. Cancel, close and dispose during the wait

- **Cancel Replace All** (`request_replace_cancel`): while
  `replace_admission_deferred()` holds, it calls
  `cancel_parked_replace_run()` instead of setting the flag and waiting for a
  worker that does not exist. The window's `Cancelled` handler then:
  - stops the wait's status keepalive (D6);
  - calls `finish_replace_run(None)` and `finish_replace_transaction()`;
  - publishes and announces "Replace All cancelled; no file was changed";
  - calls `close_if_requested_after_replace_run()`.

  The earlier Undo is still installed, because it was never superseded, and
  `finish_replace_transaction` makes its button sensitive again.
- **Close.** `close_request` sees `replace_run_in_flight()` and asks the
  existing question:
  - "Stop and Close" routes through `request_replace_cancel` (above) and then
    closes;
  - "Close When Finished" waits for the run as it does today.
- **Dispose.** `imp::dispose` cancels `replace_admission_wakeup`, drops the
  parked run (its `DisposalOwned` drop hands the payload to the lane, never
  destroying it on GTK), and clears the transaction flags. It invokes no
  window callback and publishes no status. A disposed panel
  that a weak-ref upgrade can still reach can then no longer refuse another
  window. No file has been written, so nothing needs recovery.

### D5. One pure decision for a resumption (`policy.rs`)

`policy::replace_admission_resume(facts) -> ReplaceAdmissionResume` decides,
from GTK-free scalar facts:
- whether the transaction is still claimed by this panel;
- whether the parked freshness generation is still current;
- whether cancel was requested;
- whether the run control still exists.

It returns `Resume`, or `Abandon(reason)`. An abandoned resumption follows the
Cancelled path with its reason logged (counts only, no content).

It is pure, so the mutation scope reaches it, and unit tests cover each
input combination. The window never starts a run the policy did not clear.

### D6. How the wait is shown

- **Announcement.** On entering the wait, the window announces once on
  `AnnouncementLane::StatusUpdate`: id `replace-all-waiting`, text "Replace
  All will start when memory frees up". The text is also published as the
  Search owner's progress status message, through
  `update_search_progress_message`.
- **Keepalive.** A 1 s keepalive timer in `search_progress`
  (`replace_wait_keepalive_source_id`) **re-publishes** the waiting text with
  `update_search_progress_message` on every tick while
  `replace_admission_deferred()` holds, as the run's poll does. A heartbeat
  would not do: a search started during the wait resolves the record. So
  the waiting text comes back within a second of a search's own progress
  message (T4b).
  - `start_replace_progress_tracking` stops it and takes over the slot when
    the run starts.
  - The `Cancelled` handler stops it and resolves the message.
- **Button.** The label is Cancel Replace All (from `begin_replace_run`). The
  accessible description and tooltip read "Waiting for memory before Replace
  All starts" while deferred. They are set when the wait is entered and
  cleared when it is left: at admission, on cancel and on the all-unsaved end.
  The setter lives in `accessibility.rs`, and `refresh_accessibility_state`
  does not compute it, because `finish_replace_run` calls that refresh inside
  the judged completion span ("Timing").
- **Evidence.** `SearchPanelEvidence::replace_admission_deferred: bool`. It
  is ungated, because automation reads it.
- **Automation.** A snapshot field, `content_search.replace_admission_deferred`
  (`model/automation.rs`, `ui/automation.rs`), documented in both automation
  docs.
  - `replace_in_flight` keeps its name. Its documented meaning gains "or
    waiting for capacity to start".
  - No readiness predicate or blocker changes: `search-complete` and
    `replace-preview` already stay pending through a claimed transaction and
    a run in flight.

Alternative rejected: putting the waiting text in `count_label`. A new search
during the wait rewrites that label (`restore_search_summary`), so it would
not stay up. The status line plus the button stays correct, whatever the
search does.

### D7. Test seam: an index-build delay in the palette's `test_policy.rs`

- `ui/command_palette/test_policy.rs` gains `with_file_index_build_delay`.
  `window/palette_shell.rs`'s rebuild worker calls `delay_file_index_build()`
  after the GTK-side 64 MiB reservation and before the worker shrinks it.
  This is the attribution's `ATTR_INDEX_SLEEP_MS`, behind
  `#[cfg(feature = "test-utils")]`.
- It belongs to the palette workflow's single test-only policy, not the
  search panel's: the search panel's `AGENTS.md` allows one `test_policy.rs`
  per workflow and no statics elsewhere.

### D8. Tests, failing first

All live in `crates/lushtext/tests/widget/search_panel.rs`.

- **T1, single window, filled lane.**
  1. Search and preview three files, then `wait_for_journal_disk_idle`.
  2. Take `fill_disposal_capacity_for_test()` and Confirm.
  3. After 500 ms, assert the deferred state:
     - no file changed;
     - `replace_admission_deferred` and `replace_in_flight` are true;
     - the transaction is claimed;
     - the button reads Cancel Replace All.
  4. Drop the fill. Within 10 s the run completes, every file reads
     "thread\n", and Undo is offered.

  **Fails today:**
  - at step 3, because `replace_admission_deferred` does not exist, so the
    test does not compile; once that field is added to evidence alone, the
    test fails on `replace_in_flight` false;
  - at step 4, because the run never starts (the probe printed "run started
    after release false").
- **T2, two windows.** B is deferred under a fill, and A's Confirm is refused
  with "Another window is running…" while B waits. Dropping the fill completes
  B, and A then confirms successfully. Fails today: B's transaction is
  released at the refusal, so A's apply is not refused.
- **T3, cancel while waiting.**
  1. Make an earlier Replace All, so Undo is offered.
  2. Under a fill, preview and confirm a second one, then press Cancel.
  3. Assert:
     - nothing changed;
     - the status says so;
     - the transaction is released;
     - Undo is offered again and restores the first run's files once the
       fill drops.

  Fails today: the Cancel button never appears.
- **T4, Escape while waiting.** The run still completes after the panel is
  hidden.
- **T4b, a new search while waiting.** Start a workspace search while the run
  waits. After the search is done, the status bar shows the waiting text
  again within 2 s, and dropping the fill completes the run.
- **T5, close while waiting.** Stop and Close closes with files unchanged.
- **T6, the original regression, with a build provably in flight.** The
  test runs with `with_file_index_build_delay(3 s)` installed. On `main`,
  `replace_all_held_at_second_file` confirms through
  `confirm_with_settled_file_index`, which waits out every build, so the
  delay alone would pass.
  - The variant confirms **without** that helper. First it calls
    `rebuild_file_index()` on B's window and waits until
    `file_index_builds.snapshot().started` rises. With the 3 s delay, the
    build then holds its 64 MiB at Confirm.
  - Fails today: 2 of 2 in the attribution's forced overlap (Evidence 2).
- **T7, a file becomes unsaved while waiting.** Under a fill, open one of the
  three files in a tab and type into it. Drop the fill. That file keeps its
  disk bytes and is reported as open with unsaved changes, and the other two
  are replaced. This guards D3's recomputation: the worker's stale check
  cannot see a tab-only edit.
- **T8, every file becomes unsaved while waiting.** As T7, with all three
  files edited in tabs. Nothing is written, the unsaved-only report is
  offered, the transaction is released, and a Close When Finished chosen
  during the wait closes the window.
- **T9, capacity still short.** In one GTK turn, drop the fill and take a
  second fill, so the epoch moves while the lane stays full. The run is
  still deferred after 500 ms. Dropping the second fill completes it.
- **T10, dispose while waiting.** Destroy window B while its run waits (no
  close dialog). No file changes. Window A can then preview and confirm, and
  its run completes.
- **Policy unit tests** for D5.

**Retire #119's workaround.** `confirm_with_settled_file_index` and
`settle_file_index_builds` go, and their 8 call sites go back to
`panel.activate_confirm_replacements()`. Every call site waits on outcomes
with budgets of 20 s or more:
- the files' text;
- `last_apply_counts` with `!replace_preview_pending`;
- `files_done == 2`;
- `skipped_files_offered && !replace_in_flight`.

A deferral satisfies each of these once the build shrinks, so the 8 tests
become coverage of this fix rather than a mask over it. The helper's doc
comment names this change as the reason it can go.

### D9. Proof scope

- **Kani: none.** No `services/` or `model/` module with a harness is touched.
  - The `core-replace-protocol` harnesses read only `replace/protocol.rs`
    (`super::`).
  - `replace/admission.rs` has no harness.
  - `find crates -name kani_proofs.rs | xargs grep -l 'plain_disposal\|search_panel\|automation\|command_palette'`
    finds nothing.
  - The model change (`model/automation.rs`) is one snapshot field.
  - Recheck with `scripts/kani-shards.py oracle` if the implementation
    touches any other `model/` file.
- **Axioms: none new.** Nothing relies on toolkit behaviour beyond the
  `timeout_add_local` sources the existing wakeups use. No ledger entry
  changes.
- **Smokes and audit.** The UI-prefix files need the three smokes
  (`testing.md`). The data-safety audit applies: the Replace All transaction
  and undo journal.

## Timing

Timing: none (no timed path touched).
- The two judged spans of the Replace All lane case are `replace_completion`
  (the completion closure in `window/search.rs`, `B_SETTLE_US`) and
  `replace_progress_heartbeat` (`poll_replace_progress`, `B_INTERACT_US`).
  Their bodies are unchanged.
- `finish_replace_run` and `finish_replace_transaction`, which the
  completion span calls, are unchanged. The skip-report clearing that D1
  moves is three field writes taken out of `begin_replace_run` and done at
  admission, before the worker and outside any judged span. The waiting
  description is set and cleared outside the span too (D6).
- The new code runs only on the refusal path, outside those spans:
  - the wait's keepalive;
  - the `Cancelled` handler;
  - the resumption, which runs the same tab walk the callback runs today.
- No verdict round is owed.

## Risks / Trade-offs

- **[A wait that never ends]** An installed palette index near its 64 MiB
  bound, plus other long-lived owners, can keep the lane short with no epoch
  change. → It is the same case `main` refuses on every Confirm. Here it is a
  visible, announced, cancellable wait, and other windows are refused with
  the run-in-progress message. The fix for that edge case is a smaller
  reservation (proposal "Non-goals"), left for a follow-up.
- **[A waiting run blocks other windows' Replace All and Undo]** → This is
  already true for a run in flight, and the wait ends with the next capacity
  change. Cancel is always offered.
- **[Close When Finished during a long wait]** → The window stays open until
  capacity allows the run and the run ends, as for a long run. Stop and Close
  is offered beside it.
- **[Parked selection memory]** → It keeps exactly the lane reservation it
  already holds today between partition and refusal. Nothing new is charged.
- **[The retired helper hid other flakes]** → The 8 tests now exercise the
  deferral whenever an index build overlaps. A failure there is a real defect
  of this change, and the flake rule applies.

## Migration Plan

There is no data or format change: the journal, settings and on-disk state
are untouched. Rollback is a revert. Pre-change behaviour only discarded a
selection, so a revert loses nothing persisted.

## Design revisions

- Revision 0: 2026-11-19, the original design (stage 1 of
  `replace-all-confirm-disposal-lane`). Its first commit was corrected
  before the push by a second stage-1 commit (the waiting text's home and
  keepalive, T6's in-flight build, T7–T10, the skip report, dispose), not by
  a design revision.
