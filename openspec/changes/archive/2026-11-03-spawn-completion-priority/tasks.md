## 1. Attribution and axiom

- [x] 1.1 Profile `eviction/reselect` on `main` cdfff201 under `perf record -e cycles:u --call-graph dwarf --user-callchains -F 999`, launched as a child under `scripts/measure.sh`. Record the mechanism in the design ("Attribution"): validation owns 69.5 % of the CPU, and the permit is held through a validation-paced install. Completions are not the cause.
- [x] 1.2 Report the corrected premise to the coordinator and record the decision: completions and admission hand-offs are in scope, `eviction/reselect`'s cause is queued after C3.
- [x] 1.3 Probe and sample for A80 in `gtk-lush-axioms` (`probes/a80.rs`, `examples/a80_validation_holds_back_lower_priority_idles.rs`, catalogue, fixtures): presented and hidden-page fixtures, with a settled control. `make gtk-axiom-sample AXIOM=A80 CHECK=1` holds on GTK 4.22.5.
- [x] 1.4 A80 ledger row, with its verified-against version and sample. `make check-gtk-axioms` passes.

## 2. gtk-lush-tasks

- [x] 2.1 Failing-first unit tests: `completion_is_delivered_while_validation_is_pending` (a stand-in validation idle at 125 for 256 batches; the old delivery waited for all 256) and `one_completion_is_delivered_per_dispatch` (two ready completions; the old delivery ran both in one iteration).
- [x] 2.2 One completion source at `completion_priority()` (`GDK_PRIORITY_REDRAW + 4`), FIFO, one completion per dispatch, armed exactly once under the queue lock. Both tests pass.
- [x] 2.3 Crate rustdoc, README, CHANGELOG, public-API snapshot (`make gtk-lush-api-advisory`), and the adoption matrix row.
- [x] 2.4 Adoption widget test `gtk_lush_adoption::test_adoption_task_completion_is_not_held_behind_text_validation`, failing-first against the pre-change delivery (`completion_priority()` stubbed to `G_PRIORITY_DEFAULT_IDLE`), passing after.

## 3. LushText hand-offs

- [x] 3.1 The load and save admission drains and their permit releases run at `completion_priority()`. Their facades' resumption-point narration is updated.
- [x] 3.2 Update `AGENTS.md` (Async I/O Pattern), `.agents/rules/widget-wiring.md`, `.agents/rules/build.md` (the CI widget-helper wording), the `plain_disposal` widget-test comment and `docs/next/gtk-lush.md`, wherever they describe the completion as a low-priority or `idle_add_once` idle. `tests/widget/common.rs` was C3's file and is corrected after C3 merged.

- [x] 3.3 Fix what the first `make test` exposed (design, "Found while verifying"): `after_content_revealed` reads the content stack's own `visible` property, and the restore-priority widget test reads second's state inside third's completion. The focus tests pass 5/5 and the priority test 3/3 in isolation. `make gtk-axioms` then found A74's growth check sensitive to load; the probe now alternates its small and large rounds.

- [x] 3.4 Run the composition stages one step above `G_PRIORITY_DEFAULT_IDLE`, so the first session-restore turn, now ready earlier, never shares a stage's dispatch (design, "Found while verifying"). The language catalog's one-off build runs at `G_PRIORITY_DEFAULT_IDLE - 2` for the same reason. Re-measured against `main` ebdd6aa1.

## 4. Lane case

- [x] 4.1 `save/during-validation` in `scripts/responsiveness-smoke-driver.py`. It uses a scratch document in the case's state directory and two 8 MiB short-line fixtures opened by a second activation, then the scratch tab is selected and one character typed, and `win.save` activated (the Ctrl+S action). It records `save_latency_us` from the save activation until the scratch bytes change on disk. The report gets a "Save during validation" table, and the driver self-test is extended.
- [x] 4.2 Narrow the lane's "never saves" wording in `.agents/rules/build.md` and `docs/` to "never writes a fixture".

## 5. Verdict

- [x] 5.1 Build `main` and the branch measurement binaries. Interleave them through `scripts/measure.sh`, one run per hold, with the load recorded, on `eviction/reselect`, `save/during-validation`, `session/restore-3`, `session/restore-3-small`, `recovery/small-drafts`, and the C6 and C2 cases this touches (`markdown/preview`, `editorconfig/tab-width`, dense and `total-size` post-open cases, `paragraph/1900000` wrap on and off).
- [x] 5.2 Check that no completion dispatch exceeds `B_settle` and no input or frame is delayed against `main`. Record the verdict in `docs/next/long-line-documents.md`, and queue `eviction/reselect`'s cause (permit held through a validation-paced install; hidden tabs validating fully) as the next change after C3.

## 6. Verification and ship

- [x] 6.1 `make fmt`, `make check-fmt`, `make check-clippy`, `make check-workflow-boundaries`, `make check-automation-docs`, `cargo check -p lushtext --bins --locked`, the rustdoc gate, `make test` (0 FLAKY), `make gtk-axioms`, and the GTK Lush policy, doctests, examples, MSRV and API-advisory checks.
- [x] 6.2 The smoke lanes the commit hooks require, last, before each commit. Then signed conventional commits, push, PR (not merged). (done: smokes passed before each commit; PR #79 landed by fast-forward at 92d77176)

## 7. Review fixes (independent review of PR #79)

- [x] 7.1 Aging in the delivery source: after eight completions or 16 ms, yield through a `G_PRIORITY_DEFAULT_IDLE` hop with a 16 ms deadline at `completion_priority()`. Failing-first unit tests from the reviewer's `starve-experiment.rs` (an always-ready 125, and separately an always-ready 200, during a 64-task chain). Correct D1, D2, the risks, the README, the rustdoc and `api-review.md`; record the drop of "low-priority" in `docs/next/gtk-lush.md` as a behaviour change.
- [x] 7.2 Per-completion `task_completion` and per-hand-off `task_handoff` spans in the measurement build (`completion-observer` feature), the combined-dispatch measure (A69), and the report's "Task completions against B_settle" verdict, with driver self-tests.
- [x] 7.3 `ViewportSliceBin::outer_request_pending()` true until the bin's next allocation consumes the idle's outer move (a later-frame confirmation in the reveal was tried and dropped: an already-visible reveal produces no frame). Failing-first adoption-lab widget test; Kani model and two harnesses (one `should_panic`); every `gtk-lush-widgets` shard rerun; CHANGELOG and delta spec.
- [x] 7.4 Ten full `make visual-geometry-smoke` runs on the branch, half under extra CPU load, 0 failures; the affected verdict rows re-run interleaved with load recorded.
