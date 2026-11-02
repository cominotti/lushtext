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
- [x] 3.2 Update `AGENTS.md` (Async I/O Pattern), `.agents/rules/widget-wiring.md`, `.agents/rules/build.md` (the CI widget-helper wording), the `plain_disposal` widget-test comment and `docs/next/gtk-lush.md`, wherever they describe the completion as a low-priority or `idle_add_once` idle. `tests/widget/common.rs` is C3's file and is corrected after C3 merges (design, "Out of scope").

- [x] 3.3 Fix what the first `make test` exposed (design, "Found while verifying"): `after_content_revealed` reads the content stack's own `visible` property, and the restore-priority widget test reads second's state inside third's completion. The focus tests pass 5/5 and the priority test 3/3 in isolation. `make gtk-axioms` then found A74's growth check sensitive to load; the probe now alternates its small and large rounds.

## 4. Lane case

- [x] 4.1 `save/during-validation` in `scripts/responsiveness-smoke-driver.py`. It uses a scratch document in the case's state directory and two 8 MiB short-line fixtures opened by a second activation, then the scratch tab is selected and one character typed, and `win.save` activated (the Ctrl+S action). It records `save_latency_us` from the save activation until the scratch bytes change on disk. The report gets a "Save during validation" table, and the driver self-test is extended.
- [x] 4.2 Narrow the lane's "never saves" wording in `.agents/rules/build.md` and `docs/` to "never writes a fixture".

## 5. Verdict

- [x] 5.1 Build `main` and the branch measurement binaries. Interleave them through `scripts/measure.sh`, one run per hold, with the load recorded, on `eviction/reselect`, `save/during-validation`, `session/restore-3`, `session/restore-3-small`, `recovery/small-drafts`, and the C6 and C2 cases this touches (`markdown/preview`, `editorconfig/tab-width`, dense and `total-size` post-open cases, `paragraph/1900000` wrap on and off).
- [x] 5.2 Check that no completion dispatch exceeds `B_settle` and no input or frame is delayed against `main`. Record the verdict in `docs/next/long-line-documents.md`, and queue `eviction/reselect`'s cause (permit held through a validation-paced install; hidden tabs validating fully) as the next change after C3.

## 6. Verification and ship

- [x] 6.1 `make fmt`, `make check-fmt`, `make check-clippy`, `make check-workflow-boundaries`, `make check-automation-docs`, `cargo check -p lushtext --bins --locked`, the rustdoc gate, `make test` (0 FLAKY), `make gtk-axioms`, and the GTK Lush policy, doctests, examples, MSRV and API-advisory checks.
- [ ] 6.2 The smoke lanes the commit hooks require, last, before each commit. Then signed conventional commits, push, PR (not merged).
