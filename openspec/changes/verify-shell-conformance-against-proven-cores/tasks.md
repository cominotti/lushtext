## 0. Preconditions

- [ ] 0.1 Confirm that `harden-kani-lane-and-draft-token` has landed: both fixture modules are gated behind `test-utils`, `property-tests` implies `test-utils`, the `kani_proofs.rs` harness-module rule and its mutation exclusion exist, and the shard table records budgets. Confirm that `verify-multi-window-draft-journal` has landed: the model has `Window` and `Process` actors, and the multi-window harness passes. If either change is missing, stop: this change builds on both
- [ ] 0.2 Record the baseline: every journal and write-protocol harness verdict and time from the latest `make kani` run of the affected shards, plus the current `make test-prop` wall time and the time of the CI property job

## 1. Share the environment models (D1)

- [ ] 1.1 Move the write `Disk` model and its `Shell` mutant into `services/filesystem/write_protocol/disk_model.rs`, gated `cfg(any(kani, test, feature = "test-utils"))`, and route every `kani::any()` in it through a `Choices` trait with a `KaniChoices` implementation. `kani_proofs.rs` keeps only the harness functions
- [ ] 1.2 Do the same for the journal model: move it into `services/draft_service/journal_model.rs` and route through `Choices`, keeping `Window`/`Process` from change 3. Make the S1–S4 assertions model methods, and keep the harness bodies in `kani_proofs.rs`
- [ ] 1.3 Run `make kani` for the affected shards. Every harness must give the same verdict. Each wall time must be within 20 % of the task 0.2 baseline, or the shift must be explained in the programme record. `make check-kani-shards` passes
- [ ] 1.4 Add both model modules to `.cargo/mutants.toml` `exclude_globs`, with a verification-code reason. Confirm with `make mutants-list` that no other file's count changes, and that `make check-workflow-boundaries` and `make check-filesystem-boundary` pass
- [ ] 1.5 `cargo build --release -p lushtext` compiles no model: check with `strings` for a model assertion message

## 2. The backend tap (D3)

- [ ] 2.1 Failing first: write the D3 exhaustive test skeleton for one triple (atomic replace, a `CrashAfter` at `Rename`) against a `BackendTap` API that does not yet exist, and confirm that it fails to compile
- [ ] 2.2 Implement `services/filesystem/backend_tap.rs` under `cfg(any(test, feature = "test-utils"))`, with three parts: a thread-local `TapState`, a `TapGuard` that uninstalls on `Drop`, and injection plans keyed by protocol kind and action ordinal. Give it `before`/`after` call sites in `atomic_write_stream_with_metadata`, `run_rename_protocol`, and `move_durable`, or once in `Protocol::drive` if altitude group 1 has landed. The calls are no-ops outside the gate. Add the fallible-call counter to the listed `sys.rs` primitives
- [ ] 2.3 Re-express `fail_next_parent_sync_for_test`, `FAIL_FINAL_TEMP_SYNC_AFTER_METADATA`, `fail_next_rename_cross_device_for_test`, and the temp observer as one-entry tap plans. Every existing `durable_write` unit and fault-injection test passes unchanged
- [ ] 2.4 Confirm that the release binary has no tap: `strings target/release/lushtext` finds none of the tap's messages or the marker payload's text. Confirm that `cargo check -p lushtext --bins` passes with default features

## 3. Write-shell conformance (D3)

- [ ] 3.1 Implement `services/conformance/{mod,ops,write_shell}.rs` (test-utils), which runs the real primitive, the protocol core, and `disk_model` in lockstep through `ScriptChoices` fed by the tap record. Check after every action: the core's next action, one fallible call, the outcome fed back unchanged (with the `RemoveTemp` indifference assertion), destination bytes equal to `visible()`, temp presence, and temp mode no wider than the destination. At `Finish`, check the returned variant against `WriteClass`. After a crash, check the destination and whether `temp_name` parses the leftover
- [ ] 3.2 Add the deterministic exhaustive test over every (primitive, action ordinal, injection) triple, in `make test`, taking at most 5 s
- [ ] 3.3 Add the `tests/properties/shell_conformance_write.rs` property (generated destination state, mode, payloads, primitive, and injection), and register it in `tests/properties.rs`
- [ ] 3.4 For every divergence: commit the shrunk case as a failing deterministic test, classify it (shell, environment, or core, as in the spec), fix it, and record it in the programme record. A core defect follows task 6.4

## 4. Draft-service conformance (D4)

- [ ] 4.1 Check the D4 action→service-call table against the real call sites in `ui/window/drafts/` and `ui/window/session_restore/`, and correct the table in `design.md` where it differs
- [ ] 4.2 Implement the abstraction function in `services/conformance/draft_service.rs`: manifest entries and backing versions, bodies through the injective content↔text table, backing mtimes through explicit whole-second `fixture::set_modified` times, set-aside and local-history preservation, and the held authority. Unknown files in the drafts directory fail the abstraction
- [ ] 4.3 Implement the single-window driver (one `Op` per model action, window half owned by the model, crash drops every held value, startup through `load_restore_state_cancellable`), with `BeforeEffect` faults only. It must pass on the real service before the fault domain is widened. If it does not, each divergence goes through task 3.4
- [ ] 4.4 Add `tests/properties/shell_conformance_drafts.rs` (≤ 16 ops, 64 cases) and the two-window property (≤ 16 ops, 32 cases), using change 3's `Window`/`Process` model over one service instance

## 5. Widen the fault domain (D4)

- [ ] 5.1 Replace `fault: bool` with `Fault { None, BeforeEffect, AfterEffect }` in `journal_model.rs`. `AfterEffect` persists the step's effect and leaves the window believing it failed, and only for the steps D4 lists
- [ ] 5.2 Run the journal shards under `make kani`. Record verdicts, times, and peak memory against change 1's margins. If a margin is broken, split the shard first, then re-measure. Reduce a bound only as a recorded last resort
- [ ] 5.3 If Kani finds a counterexample over `AfterEffect`, it is a core or environment finding. Handle it as in task 6.4, including reachability evidence and a maintainer decision if it is kept as a `should_panic` residual
- [ ] 5.4 Enable the `AfterEffect` and `At(k)` fault positions in the drafts driver. An intermediate multi-write state that matches neither model outcome is an environment finding (step atomicity). Resolve it by extending the model and re-running task 5.2, or by reordering the shell behind a failing-first test. Record which resolution was chosen

## 6. Divergence discipline and budgets

- [ ] 6.1 Run `make test-prop` locally and on CI. Record the wall time of each conformance property. Each must stay within the D8 budget and within the property job's timeout. If a budget is exceeded, shorten the script bound first, then lower that property's case count. Never raise the job timeout
- [ ] 6.2 Run `make test-prop-deep PROPTEST_DEEP_CASES=512` with scripts of up to 32 ops once, and triage every failure through task 3.4
- [ ] 6.3 Set the conformance shrink config (`max_shrink_iters = 256`, `max_shrink_time = 45_000`). Show on a scratch, never-merged injected divergence that shrinking reaches a minimal script within the nextest `property` profile ceiling and persists it to `proptest-regressions/properties.txt`
- [ ] 6.4 Core-defect procedure, used whenever one is found:
  - mark the property as invalidated in the programme record, from the finding's commit;
  - add or adjust a Kani harness so that it exhibits the counterexample, and show that it fails;
  - fix the core;
  - re-prove, and mark the property as proved again;
  - add a service or widget regression test that failed before the fix

## 7. Fuzz reuse (D7)

- [ ] 7.1 Add `ops::decode` and `fuzzing::exercise_conformance_script_for_fuzzing`, selected by the first input byte's top bit in `operation_script`. Make the `fuzzing` feature imply `test-utils`, and keep the existing operation-count and input-length caps
- [ ] 7.2 Add reviewable conformance seeds under `fuzz/corpus/operation_script/`: one per primitive, plus one draft script per model action. `make fuzz-corpus-replay` passes on stable, and `make fuzz-operation-smoke` runs within its existing time bound
- [ ] 7.3 Promote every fuzz-found divergence to a committed seed plus a deterministic test, through task 3.4

## 8. GTK coordination scenarios (D5)

- [ ] 8.1 Find out whether change 3's process-once startup offers a `test-utils` reset for an in-process restart. If it does not, move the restart halves of scenarios 5 and 6 to service-level deterministic tests, and record that in the design
- [ ] 8.2 Add `crates/lushtext/tests/widget/drafts_conformance.rs` with the eight D5 scenarios. Each replays a recorded model trace through real window actions and compares `DraftEvidence` and the disk abstraction with the model after every settled step. Use the existing readiness and settle helpers. Async completions need adequate budgets, and no wait helper may be copy-pasted
- [ ] 8.3 If the comparison needs a per-id fact that `DraftEvidence` lacks, add a bounded evidence field under the evidence invariants and declare it in the `WFR-DRAFT-RECOVERY` matrix row. Run `make check-workflow-boundaries`, and add no `*_for_test` seam
- [ ] 8.4 Run the new module through `make test-widget` three times in isolation with zero `FLAKY:` lines. It must add at most 60 s to the widget lane

## 9. Dependency and scope checks

- [ ] 9.1 Confirm that no dependency was added: `Cargo.lock` is unchanged except for workspace crates, `cargo deny check advisories bans sources licenses` passes, `cargo hakari generate --diff` reports nothing, and `cargo-sources.json` needs no regeneration
- [ ] 9.2 Review `sonar-project.properties` for the new source files (`services/conformance/`, the model modules, and `backend_tap.rs`), and apply the SonarQube scope rule

## 10. Documentation sync

- [ ] 10.1 `docs/next/formal-verification.md`: add "Phase 6 — Shell conformance against the proven cores", covering the shared models, the tap, the three drivers, measured budgets, the fault-domain widening and its Kani results, and every finding with its classification. Add a deferral-inventory entry for power-loss semantics (Kani and K5 only), and one for the `ViewportSliceBin` shell, with the reasoning
- [ ] 10.2 `docs/next/formal-verification-next.md`: mark the N11 row and section done, pointing to this change, and update the suggested order in §4
- [ ] 10.3 AGENTS.md Testing section: add the conformance lane (what it drives, that the oracle is the proven model, and where it runs), and update the `durable_write` shell and draft-journal design bullets to name the conformance evidence. `.claude/CLAUDE.md` is a symlink to AGENTS.md
- [ ] 10.4 `.agents/rules/build.md`:
  - Property Testing: conformance properties, the total-model shrinking rule, the shrink budget, and regression promotion;
  - Fuzzing: the conformance mode, and `fuzzing` implying `test-utils`;
  - Formal Verification: shared model modules, the choice-source rule, the fault-domain completeness rule, the divergence classification, and the mutation exclusion;
  - the fault-seam guidance: the backend tap as the one durable-write injection seam
- [ ] 10.5 `docs/end-user-coverage.md`: map "shell obeys proven core" to the property (write and drafts), fuzz, and widget scenario lanes. `docs/property-testing.md` and `docs/fuzzing.md`: add the new properties, their budgets, and the seeds. Update the `gtk-testing` skill reference if the widget scenarios introduce a new pattern
- [ ] 10.6 `README.md`: update the testing and formal-verification paragraph if it lists the lanes

## 11. Final verification

- [ ] 11.1 `make check`, `make check-policy`, `make test`, `make test-prop`, `make fuzz-corpus-replay`, and `make test-widget` pass locally without warnings
- [ ] 11.2 `make kani` passes every shard within change 1's recorded margins
- [ ] 11.3 CI is green on the change's pull request, including the property job within its timeout
- [ ] 11.4 Run `openspec validate verify-shell-conformance-against-proven-cores --strict`. At archive time, merge the delta specs into `formal-verification-kani`, `durable-file-write-contract`, `draft-session-recovery`, `property-based-testing`, and `structured-operation-fuzzing`
