## 1. Model refactor and baseline (no production change)

- [x] 1.1 Refactor `crates/lushtext-core/src/services/draft_service/kani_proofs.rs` into `Window` (per-window state), `Process` (windows plus coordinator state), and `Journal` (disk plus processes), following design D1. `Crash` becomes a process action. Verify: `cargo kani -p lushtext-core --harness journal_invariants_hold_under_crashes` still PROVED, `a_dirty_editor_becomes_clean_without_faults` PROVED at k = 7, `a_dirty_editor_may_need_seven_steps` still `should_panic`, and `a_second_writer_breaks_the_journal_invariants` still `should_panic`, with times and memory recorded.
- [x] 1.2 Add the modelled mutation lane, missing-body entry removal in `ExecCleanup`, and one untitled id to the single-window model (design D2). Run the single-window harness. If it fails, decode the trace by concrete playback and go to 1.3 before anything else; if it proves, record the result.
- [x] 1.3 (Not needed: the extended single-window harness proved with no counterexample.) (Only if 1.2 found a counterexample) For each single-window counterexample, write a failing-first widget or integration test, fix the production cause, and re-run 1.2 until PROVED. This is a pre-existing blocker and must be closed in this change.
- [x] 1.4 Add `journal_invariants_hold_across_two_windows` (one process, two windows, baseline per-window lane and per-window startup restore, bounds per design D5) and a `CloseWindow` action. Run it. Record every counterexample as a decoded action trace in a scratch note for 2.x.
- [x] 1.5 Add the harness to a new `core-multi-window` shard in `scripts/kani-shards.py`. Verify `make check-kani-shards` passes, and that removing the entry makes it fail.

## 2. Failing-first multi-window tests

- [x] 2.1 Create `crates/lushtext/tests/widget/multi_window_drafts.rs`, registered in the widget harness, with a shared helper that builds two `LushtextWindow`s over one `GApplication` and one isolated `TestContext` data directory. Wait only on async-completion predicates with adequate budgets; no copy-pasted wait helpers.
- [x] 2.2 For each counterexample from 1.4, write one widget test that reproduces its trace through production actions (for example: A registers a new file-backed id with a delayed body write while B's orphan cleanup runs; A writes an untitled body with a delayed commit while B's cleanup inspects; B is created after A's restore; the same path is opened in both windows). Run each test and record that it FAILS on the unfixed tree, with the failure message, in the task note.
- [x] 2.3 Add one test per ADDED scenario in `specs/draft-session-recovery/spec.md` not already covered by 2.2, including "Single-window behaviour is unchanged" (the existing draft suite passes unchanged).

## 3. Fixes

- [x] 3.1 Add the pure lane-admission, draft-id-claim, and startup-restore-claim decisions the counterexamples require to `services/draft_service/journal_core.rs`, with characterization unit tests. Verify: `make test-unit`, and GTK-free and I/O-free as the module doc requires.
- [x] 3.2 Implement the process-wide journal coordinator in the draft workflow's role home (design D3). Choose its module name and role with the `lushtext-workflow` skill. Route `autosave_execution`, `journal::drive_pending_draft_mutations`, `journal::flush_dirty_drafts`, and `cleanup_journal::run_orphan_cleanup_pass` through it. Keep per-window flags as evidence projections. Verify: the 2.2 cleanup tests pass.
- [x] 3.3 Make startup session/draft restore and orphan-cleanup scheduling process-once, and have cleanup inspect against the latest persisted manifest under the lock. Verify: the "second window does not restore again" test passes, and existing widget tests that build two windows are audited and still pass (any updated test records why).
- [x] 3.4 Make duplicate-path detection process-wide, redirecting the open to the owning window's tab, and add the claim backstop hold with its status message (design D4). Verify: the same-path test passes.
- [x] 3.5 Switch the model's coordinator to the fixed rules (it calls the new `journal_core` functions), and re-run `journal_invariants_hold_across_two_windows` until PROVED at the D5 bounds. Re-run every other journal harness and confirm its result is unchanged. Any counterexample kept instead of fixed gets a `should_panic` harness, reachability evidence, and a recorded maintainer decision in design.md.
- [x] 3.6 Run the full lane: `make kani` (all shards), then `make check`, `make test`, and `make test-widget`, all green. Re-run each new multi-window test 5 times in isolation to rule out flakes.

## 4. Data-safety audit

- [x] 4.1 Run the explicit `data-safety` skill audit over the change's diff and the multi-window draft paths: autosave vs. cleanup vs. deletion across windows, close of one window while the other autosaves, window dispose releasing claims and lane ownership exactly once, the Data page opening a preserved draft into whichever window owns the dialog, and the session file written by one window while another has untitled drafts (design D6). Fix every confirmed finding failing-first, or record it in the deferral inventory with its reasoning.
- [x] 4.2 Confirm by test or trace that no path added by this change deletes a draft body or manifest entry without the journal core's ownership check, and that a failure at any coordinator step leaves the pre-step state recoverable.

## 5. Documentation sync

- [x] 5.1 Update `docs/next/formal-verification.md`: phase 4 gets a multi-window subsection with the harness table (bounds, result, time, memory), each counterexample and its fix or decision, and the new shard. Remove "and one window" from the unmodelled assumption, and update the deferral inventory A6 entry ("cleanup gating is per window" no longer holds).
- [x] 5.2 Update `docs/next/formal-verification-next.md` N4: multi-window done, two-bin simultaneous requests still open.
- [x] 5.3 Update the `WFR-DRAFT-RECOVERY` row in `docs/workflow-readability-matrix.md`: re-derive measured cells, the new or extended coordination module, seams, and evidence fields. Run `make check-workflow-boundaries`.
- [x] 5.4 Update the `ui/window/drafts/mod.rs` narrative facade (the coordinator, its cross-window wakeup resumption point, and the shared-state table), AGENTS.md / `.claude/CLAUDE.md` "Draft persistence" and "Tab duplicate detection" design decisions, and `crates/lushtext-core/src/ui/window/AGENTS.md`.
- [x] 5.5 (No automation surface changed: no action, D-Bus member, snapshot field, readiness predicate, or blocker meaning; the per-window flags the `draft-autosave` blocker reads keep their meaning as projections of the lane.) If any automation snapshot field, readiness predicate, or blocker meaning changed, update `docs/automation.md` and `docs/automation-reference.md` and run `make check-automation-docs`; otherwise record "no automation surface changed" in the task note.
- [x] 5.6 Run `openspec validate verify-multi-window-draft-journal --strict` and confirm it is valid.
