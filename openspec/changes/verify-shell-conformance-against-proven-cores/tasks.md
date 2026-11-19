## 0. Baseline

- [x] 0.1 Record in the handoff directory:
  - the baseline from `scripts/kani-shards.py list` for `core-journal-and-write`, `core-journal-liveness`, `core-second-writer`, and `core-multi-window`, with each harness's recorded verdict and its `ORACLES` seconds;
  - the `make test-prop` wall time through `heavy.sh`, with `/proc/loadavg` before and after.

  No Kani shard runs before the code is final (design D9)

## 1. Share the environment models (D1). This is its own commit, before any other group

- [x] 1.1 Move the write model into `services/filesystem/write_protocol/disk_model.rs`, gated `cfg(any(kani, test, feature = "test-utils"))`. It covers `Content`, `Entry`, `TempInode`, `Disk`, and the `Shell` mutant. The choices become arguments: `Disk::execute(action, shell, outcome)` and `Disk::after_crash(durable_branch)`. Move the transfer harnesses' inline environment there too, as `MoveDisk` and `RenameNamespace` with `execute(action, succeeded)`. The harnesses pass `kani::any()` and keep every assertion and `kani::cover!`. The write harnesses' bodies change only at the choice call sites. The transfer harnesses' inline field updates become `execute` calls. `RenameNamespace` keeps the `assert!(renamed)` that one rename harness has today, as in design D1
- [x] 1.2 Move the journal model into `services/draft_service/journal_model.rs` under the same gate. Route its in-model choices through `Choices` (`flag` for the previous-session bits, and `restore_ending`), with a `KaniChoices` implementation that is exactly today's `kani::any()`/`kani::assume`. `KaniChoices` and `any_below`, which only harness call sites use, stay in `kani_proofs.rs`. `Action` derives `kani::Arbitrary` through `cfg_attr(kani, ...)`. Keep `Window`/`Process` and `Scope`. Make S1, S3 and S4 model methods that return their verdict, which the harnesses assert. S2 stays inline in `exec_cleanup_one`
- [x] 1.3 Add both modules to `.cargo/mutants.toml` `exclude_globs`, with a verification-code reason, and to `NOT_ORACLE_MODULES` in `scripts/kani-shards.py`. Then check that each of these passes, and that `make mutants-list` shows no other file's count changing:
  - `make check-kani-shards`;
  - `make check-workflow-boundaries`;
  - `make check-filesystem-boundary`;
  - `cargo check -p lushtext --bins --locked`, with default features

## 2. The conformance tap (D3)

- [x] 2.1 Failing first: write the D4 enumeration skeleton for one case against a `BackendTap` API that does not exist yet, and confirm it fails to compile. The case is an atomic replace over an existing destination, with a `CrashAfter` at `Rename`
- [x] 2.2 Add `Protocol::KIND` (`ProtocolKind`: `Write`, `Move`, `Rename`, `ReplaceFile`, `ReplaceUndo`, `Shrink`) to the six implementations. Put the `Copy + Debug + 'static` bounds that `drive` needs on the trait's associated types, unconditionally, with no derive added to `replace/protocol.rs`, whose types already have them. Keep `protocol_drive`'s own tests and the `no_shell_steps_its_core_outside_protocol_advance` source check passing
- [x] 2.3 Implement `services/filesystem/backend_tap.rs`, under `cfg(any(test, feature = "test-utils"))`, with no process-global slot:
  - a thread-local `TapState`, and a `TapGuard` that uninstalls on drop;
  - a trace of `(kind, run, ordinal, action, outcome)`, with nested runs;
  - plans addressed by kind, run, action ordinal, and call index;
  - before-action hooks;
  - `TapCrash` before and after an action;
  - `tap::content_written`;
  - the three address forms of D3: protocol-scoped, destination-scoped, and scope-wide;
  - crashes through `std::panic::resume_unwind`, so the panic hook prints nothing.

  Widen `sys::cross_device_error_for_test` to the tap's gate. Call it from `drive`, around `execute`, and from every fallible `sys.rs` primitive the durable-write shells and the draft service reach. Enumerate that list from the code, and pin it in a test so that a primitive added without a tap call fails. Outside the gate, every call is an `#[inline(always)]` no-op
- [x] 2.4 Re-express the six durable-write hooks as one-entry plans, and delete their thread-locals and the observer `OnceLock`. The hooks are `FAIL_FINAL_TEMP_SYNC_AFTER_METADATA`, `FAIL_NEXT_PARENT_SYNC`, `FAIL_NEXT_SOURCE_REMOVAL`, `FAIL_NEXT_RENAME_CROSS_DEVICE`, `NEXT_NO_REPLACE_UNSUPPORTED`, and `TEMP_AFTER_CONTENT_OBSERVER`. The cross-device plan fails `sys::rename` inside the protocol. Then:
  - every `*_for_test` setter, including the `filesystem::write` re-exports, keeps its name, visibility, signature and semantics: armed on this thread until consumed, with no guard (design D3);
  - each plan counts exactly as its hook did; the parent-sync plan counts `sync_parent_dir` calls, not every directory sync;
  - every caller keeps its assertions unchanged: the 40 `durable_write` tests and the tests in `filesystem/write.rs`, `draft_service/set_aside.rs`, `local_history_service.rs`, `content_search/replace/tests.rs` and `ui/sidebar/workspace_section/file_execution.rs`. The last is under a UI prefix and must not be edited;
  - the three tests that read a hook cell directly read the tap's plan state instead. They are `the_parent_sync_fault_lets_the_given_syncs_succeed_first` (lines 1251 and 1255), `a_cross_directory_rename_whose_destination_sync_fails_is_after_rename` (line 1284), and `atomic_write_temp_with_new_bytes_is_not_wider_than_private_destination` (lines 1592 and 1604)
- [ ] 2.5 Extend `KILL_POINT_MARKERS` in `scripts/crash-recovery-smoke-driver.py` with the `TapCrash` payload text and one unique tap string. Check with `strings` that `cargo build --release -p lushtext` contains neither. Run `make crash-recovery-smoke` once on the final code (task 10.1) to show the guard passes
- [x] 2.6 Unwind audit over the final code. List every `Drop` impl and every lock on the unwind path of each primitive that D4 crashes inside. Record that none does I/O and that none is poisoned and read afterwards. Record in the handoff why the draft driver crashes only between model steps: the manifest lock `expect`s on poison. List every process-wide static the draft service reaches that a between-steps crash keeps, where a real restart would not: the launch nonce and pid, `set_aside::PLACEMENTS`, `obsolete_lineage_cleanup_failure`, and any others found. Show that none changes the abstracted state, or reset it in the driver's `Crash`

## 3. Write-shell conformance (D4)

- [x] 3.1 Implement `services/conformance/{mod,ops,write_shell}.rs` (test-utils). It steps the shared `Disk` from the tap trace and checks, after every action, the D4 list:
  - outcome against backend results, with the `RemoveTemp` indifference, the absent-destination probe, and the content stream as the named exceptions. The model's `RemoveTemp` is stepped with the removal call's own result, not the fed-back `Done`;
  - the pinned call table;
  - destination bytes against `visible()`;
  - temp presence against `linked`;
  - temp mode no wider, when the destination existed.

  The driver takes a caller-owned root directory and names neither `proptest` nor `tempfile`, which are dev-dependencies. At the terminal action it checks the result variant against the core's class. For moves it checks the nested copy against its own `Disk` and the source against `MoveDisk`. For renames it checks the entry's name against `RenameNamespace`. After a crash it checks the destination against `after_crash(false)`, the `temp_name` parse, and `is_sweepable_leftover`. That predicate is given the real file's facts and a `LeftoverClock` naming another launch, with `now` past `STALE_LEFTOVER_AGE`
- [x] 3.2 Add the deterministic exhaustive test over (primitive × destination state × action ordinal × call index × injection). It includes `MAX_TEMP_NAME_ATTEMPTS` consecutive collisions, runs in `make test`, and takes at most 5 s. Pin the per-action call table as its own test
- [x] 3.3 Add `tests/properties/shell_conformance_write.rs` (64 cases: modes, payload sizes, two injections per run), and register it in `tests/properties.rs`
- [x] 3.4 Triage every divergence under the classification requirement. Commit the shrunk case as a failing deterministic test, fix it, and record it in the programme record. A core defect follows task 7.4

## 4. Draft-service conformance (D5), before-effect faults first

- [x] 4.1 Check the D5 action→call table against the real call sites in `ui/window/drafts/` and `ui/window/session_restore/`. Correct the design's table where it differs, and record each difference in the handoff
- [ ] 4.2 Implement the abstraction in `services/conformance/draft_service.rs`:
  - manifest entries and backing versions;
  - bodies, through the injective content↔text table;
  - explicit whole-second mtimes;
  - preservation, from the set-aside area only (design D5: production does not count a local-history snapshot as preservation);
  - the held authority.

  Unknown files in the drafts directory fail the abstraction. Durable-write leftovers that `parse_durable_temp_name` recognises are counted and reported, not compared
- [ ] 4.3 Implement the single-window driver with enabled-indexed ops (D2). The window half is model-owned, and a crash drops every held value between model steps. `ScriptChoices` follows design D1:
  - the previous-session layout comes from the script, and the driver seeds the real data directory to it before the first `Startup`;
  - restore endings are read off the real resolution, and the driver asserts the model admits each one;
  - `Oversized` is never chosen, a restriction recorded with the property.

  Each step's tap plan is installed only around that step's service calls, never around seeding, mtime setting or the abstraction. Enable only `BeforeEffect` (scope-wide call 0), `Crash`, and `Startup`. It must pass on the real service before task 5 widens anything. Each divergence goes through task 3.4
- [ ] 4.4 Add `tests/properties/shell_conformance_drafts.rs` (≤ 16 ops, 64 cases), holding the strategies and the case `TempDir`. Use the D2 shrink config: `property_config()` with `max_shrink_iters = 256` and `max_shrink_time = 45_000` ms
- [ ] 4.5 Add the reach test to the `properties` target, so it runs in `make test-prop`, where the strategy is. It uses a fixed seed and the default case count, interprets each script over the journal model alone, and has recorded floors for `WriteBody`, `Commit`, `DeletionStep`, `RestoreApply`, `ExecCleanup`, and `Startup`. Then run the two-mutant calibration on scratch, never-merged commits:
  - a set-aside copy that `preserve_stale_draft_body` skips;
  - `remove_manifest_entry` returning `Ok` while it leaves the entry on disk.

  Record the number of cases each needed in the design and in `docs/property-testing.md`
- [ ] 4.6 On a scratch, never-merged injected divergence, show that shrinking reaches a minimal script within the nextest `property` profile ceiling and persists it to the regression file

## 5. Widen the journal fault domain (D6) and step atomicity (D7)

- [ ] 5.1 Replace `fault: bool` with `Fault { None, BeforeEffect, AfterEffect }` in `journal_model.rs`, implementing the whole effect table of design D6:
  - `AfterEffect` applies to every manifest commit (Register, Commit, `DeletionStep(RemoveEntry)`, ExecCleanup's missing-body write, Startup's reconciliation) and to WriteBody, and is assumed away elsewhere;
  - unconfirmed bits: one for the manifest file, with the last confirmed entry array it reverts to; one per body file, set by an after-effect write and by every unsynced unlink (`DeleteBody`, ExecCleanup's orphan delete); one per set-aside copy, set by a placement whose sync failed, which the window still counts as placed;
  - `Crash` keeps or reverts each unconfirmed effect through `Choices::flag`. A reverted set-aside move returns the body to `drafts/`, and a reverted copy is gone;
  - a later successful sync of the same directory confirms its effects;
  - the joint revert of several unconfirmed manifest commits is stated as a domain restriction in the harness rustdoc.

  The harnesses choose the fault with `kani::any()`. If L1 fails at k = 7 with a longer fault-free path to clean, re-derive the bound and its `k - 1` sibling (design D6). A stuck editor is a core finding
- [ ] 5.2 Enable `AfterEffect` and `At(k)` in the drafts driver. `AfterEffect` is the destination-scoped `SyncDir` of the step's committing write, and a set-aside placement's unconfirmed effect is the `SyncDir` of its `set-aside/` write or move. Only their kept branch is testable, because in-process tests cannot revert, so `ScriptChoices` answers every crash keep-or-revert flag with "keep". `At(k)` is scope-wide call k. An intermediate multi-write state that matches neither model outcome is a step-atomicity environment finding. Resolve it by extending the model or by reordering the shell behind a failing-first test, and record which
- [ ] 5.3 If Kani or conformance finds a counterexample over `AfterEffect`, handle it as task 7.4, with reachability evidence from real errors

## 6. Replace shells (D8)

- [x] 6.1 Failing first, against the mutants: add the three `process_file` survivor tests to `content_search/replace/tests.rs`, using the tap's before-action hook at `ReplaceFile`/`WriteBody`:
  - mutant 649: the target rewritten with the same bytes and a new mtime, skipped under pass A's `ChangedDuringReplace`. The bytes must not change, or pass A's `StaleLine` pre-empts the facts check;
  - mutants 689 and 691, with one test: a raised cancel, plus a body write that fails before its producer runs (the bodies directory replaced by a file, or a tap `Fail` at the body write's `ProbeMetadata`), ending `Cancelled`.

  Show that the three are now caught with `scripts/heavy.sh --envelope 36G -- env MUTANTS_SMOKE_FILE=crates/lushtext-core/src/services/content_search/replace/apply.rs MUTANTS_RE='<the three mutants>' make mutants-smoke`, which takes a mirror slot or exits 75 when none is free (retry later, never in a loop), and record that in `docs/mutation-testing.md` and the programme record's deferral entry

## 7. Divergence discipline and budgets

- [ ] 7.1 Run `make test-prop` locally through `heavy.sh`, recording `/proc/loadavg`, and record each conformance property's wall time against the D12 budget. If a budget is exceeded, cut the script bound first, then that property's case count. Never raise a job timeout. The CI figure is read once, at the programme's CI check
- [ ] 7.2 Run `make test-prop-deep PROPTEST_DEEP_CASES=512` once, with scripts of up to 32 ops, and triage every failure through task 3.4
- [ ] 7.3 Record every finding, or the absence of findings, with its class in the programme record
- [ ] 7.4 Core-defect procedure, used whenever one is found:
  - mark the property invalidated from the finding's commit;
  - add or adjust a harness that exhibits the counterexample, and show it fails;
  - fix the core;
  - re-prove it and mark the property proved;
  - add a service regression test that failed before the fix

## 8. Dependency and scope checks

- [x] 8.1 Confirm that no dependency was added:
  - `Cargo.lock` is unchanged apart from workspace crates;
  - `cargo deny check advisories bans sources licenses` passes;
  - `cargo hakari generate --diff` is clean;
  - `cargo-sources.json` needs no regeneration
- [x] 8.2 Review `sonar-project.properties` for `services/conformance/`, the model modules, and `backend_tap.rs`, under the SonarQube scope rule

## 9. Kani and proof strength, on the final code (D9)

- [ ] 9.1 Once the journal and write code is final, run detached, once each: `setsid nohup scripts/heavy.sh --envelope 16G -- make kani KANI_SHARD=<shard>` for `core-journal-and-write`, `core-journal-liveness`, `core-second-writer`, and `core-multi-window`. Compare against task 0.1:
  - every unchanged property keeps its verdict;
  - every widened one passes, or is a finding (task 5.3);
  - every `kani::cover!` reports SATISFIED. Kani does not fail on an UNSATISFIABLE cover by itself, so each one is investigated as a possibly weakened model (design D1).

  Any harness time more than 20 % off its record is explained in the programme record. A broken margin follows the D6 fallback order. Update the `ORACLES` seconds
- [x] 9.2 Set the four shard records to `PENDING DISPATCH (owed by verify-shell-conformance-against-proven-cores)`, with the local figures, and pass `make check-kani-shards`
- [ ] 9.3 Run `make proof-strength` once on the final code, detached, through `heavy.sh --envelope 16G`, for both modules design D9 owes:
  - `PROOF_STRENGTH_MODULE=crates/lushtext-core/src/services/filesystem/write_protocol.rs`, whole;
  - `PROOF_STRENGTH_MODULE=crates/lushtext-core/src/services/draft_service/journal_core.rs`, with the Kani arm sharded (`PROOF_STRENGTH_SHARD=k/n`).

  Compare each kill count with the programme record's last figure. A count below it means a weakened model, a finding under task 3.4. Triage each survivor in the programme record
- [ ] 9.4 At the end of implementation, after the final push, dispatch `kani.yml` twice on the branch and record both run ids in the handoff. Do not watch them. The archive records the larger figure of the two runs in each of the four shard records, because the archive commit's hook fails on a pending record

## 10. Final verification

- [ ] 10.1 Run each of these through `heavy.sh`, with the stated envelopes, without warnings:
  - `make check` and `make check-policy`;
  - `make test` (`--envelope 16G`);
  - `make test-prop`;
  - `make fuzz-corpus-replay`;
  - `make crash-recovery-smoke`;
  - the rustdoc lint gate of `testing.md`, "Proof Obligations of a Change", because the change edits doc comments in `durable_write.rs` and `protocol_drive.rs`.

  No UI prefix is touched, so the three visual and accessibility smokes are not owed. Confirm with `git diff --name-only origin/main` that no path under the UI prefixes changed
- [ ] 10.2 Run `openspec validate verify-shell-conformance-against-proven-cores --strict`. At archive time, merge the deltas into `formal-verification-kani`, `durable-file-write-contract`, `draft-session-recovery`, `property-based-testing`, and `crash-restart-recovery-coverage`

## 11. Documentation sync

- [ ] 11.1 `docs/next/formal-verification.md`:
  - Add "Phase 6 — Shell conformance against the proven cores", covering the shared models, the tap and its two sites, the write enumeration and its call table, the drafts driver with its reach and calibration figures, the `AfterEffect` widening and its Kani results, and every finding with its class.
  - Record the named domain restrictions: several unconfirmed manifest commits revert together, the drafts property never chooses `Oversized`, and any D6 fallback exclusion.
  - Update the deferral inventory: mark the three Replace survivors closed. Add entries for the GTK drafts coordination scenarios, the fuzz conformance mode, the two-window service property, Replace lockstep, mid-call draft crashes (K5), and power-loss semantics (Kani and K5 only), each with its reason and reopening trigger
- [ ] 11.2 `docs/next/formal-verification-next.md`: mark the N11 row and section done, pointing to this change. Make the N11 text claim only what was checked: the GTK drafts coordination is deferred, not covered. Update §4's suggested order
- [ ] 11.3 AGENTS.md:
  - Testing section: add the conformance lane (what it drives, that the oracle is the shared proven model, where it runs).
  - Module Layout: add `services/conformance/` and `services/filesystem/backend_tap.rs`, both test builds only.
  - Name the conformance evidence in the durable-write and draft-journal design bullets. `.claude/CLAUDE.md` is a symlink to it
- [ ] 11.4 `.agents/rules/build.md`:
  - Property Testing: conformance properties, enabled-indexed generation, reach floors, the shrink budget, and regression promotion;
  - Formal Verification: shared model modules and where their choices come from, the fault-domain completeness rule, the divergence classes, and the `NOT_ORACLE_MODULES` and mutation entries;
  - fault seams: the backend tap as the durable-write injection seam.

  `.agents/rules/testing.md`, fault seams: the tap is per-invocation and thread-local, and panic crashes are used only where no lock or I/O `Drop` is on the unwind path
- [ ] 11.5 `docs/end-user-coverage.md`: map "the shell obeys its proven core" to the write enumeration and the two properties. `docs/property-testing.md`: the new properties, budgets, reach floors and calibration. `docs/mutation-testing.md`: the three survivors killed. Add a `gtk-testing` skill reference only if a new test pattern appears
- [ ] 11.6 `README.md`: update the testing and formal-verification paragraph if it lists the lanes
