## 0. Baseline

- [ ] 0.1 Record in the handoff directory:
  - the baseline from `scripts/kani-shards.py list` for `core-journal-and-write`, `core-journal-liveness`, `core-second-writer`, and `core-multi-window`, with each harness's recorded verdict and its `ORACLES` seconds;
  - the `make test-prop` wall time through `heavy.sh`, with `/proc/loadavg` before and after.

  No Kani shard runs before the code is final (design D9)

## 1. Share the environment models (D1). This is its own commit, before any other group

- [ ] 1.1 Move the write model into `services/filesystem/write_protocol/disk_model.rs`, gated `cfg(any(kani, test, feature = "test-utils"))`. It covers `Content`, `Entry`, `TempInode`, `Disk`, and the `Shell` mutant. The choices become arguments: `Disk::execute(action, shell, outcome)` and `Disk::after_crash(durable_branch)`. Move the transfer harnesses' inline environment there too, as `MoveDisk` and `RenameNamespace` with `execute(action, succeeded)`. The harnesses pass `kani::any()` and keep their assertions. Their bodies change only at the choice call sites
- [ ] 1.2 Move the journal model into `services/draft_service/journal_model.rs` under the same gate. Route its in-step choices through `Choices`, with a `KaniChoices` implementation that is exactly today's `kani::any()`/`kani::assume`. `KaniChoices` stays in `kani_proofs.rs`. Keep `Window`/`Process` and `Scope`. Make S1–S4 model methods that return their verdict, which the harnesses assert
- [ ] 1.3 Add both modules to `.cargo/mutants.toml` `exclude_globs`, with a verification-code reason, and to `NOT_ORACLE_MODULES` in `scripts/kani-shards.py`. Then check that each of these passes, and that `make mutants-list` shows no other file's count changing:
  - `make check-kani-shards`;
  - `make check-workflow-boundaries`;
  - `make check-filesystem-boundary`;
  - `cargo check -p lushtext --bins --locked`, with default features

## 2. The conformance tap (D3)

- [ ] 2.1 Failing first: write the D4 enumeration skeleton for one case against a `BackendTap` API that does not exist yet, and confirm it fails to compile. The case is an atomic replace over an existing destination, with a `CrashAfter` at `Rename`
- [ ] 2.2 Add `Protocol::KIND` (`ProtocolKind`: `Write`, `Move`, `Rename`, `ReplaceFile`, `ReplaceUndo`, `Shrink`) to the six implementations, and the `Copy + Debug + 'static` bounds that `drive` needs under the gate. Keep `protocol_drive`'s own tests and the `no_shell_steps_its_core_outside_protocol_advance` source check passing
- [ ] 2.3 Implement `services/filesystem/backend_tap.rs`, under `cfg(any(test, feature = "test-utils"))`, with no process-global slot:
  - a thread-local `TapState`, and a `TapGuard` that uninstalls on drop;
  - a trace of `(kind, run, ordinal, action, outcome)`, with nested runs;
  - plans addressed by kind, run, action ordinal, and call index;
  - before-action hooks;
  - `TapCrash` before and after an action;
  - `tap::content_written`;
  - the three address forms of D3: protocol-scoped, destination-scoped, and scope-wide.

  Call it from `drive`, around `execute`, and from every fallible `sys.rs` primitive the durable-write shells and the draft service reach. Enumerate that list from the code, and pin it in a test so that a primitive added without a tap call fails. Outside the gate, every call is an `#[inline(always)]` no-op
- [ ] 2.4 Re-express the six durable-write hooks as one-entry plans, and delete their thread-locals and the observer `OnceLock`. The hooks are `FAIL_FINAL_TEMP_SYNC_AFTER_METADATA`, `FAIL_NEXT_PARENT_SYNC`, `FAIL_NEXT_SOURCE_REMOVAL`, `FAIL_NEXT_RENAME_CROSS_DEVICE`, `NEXT_NO_REPLACE_UNSUPPORTED`, and `TEMP_AFTER_CONTENT_OBSERVER`. The cross-device plan fails `sys::rename` inside the protocol. All 40 `durable_write` tests keep their assertions. The four that read a hook cell directly (the parent-sync counter near lines 1251, 1255, and 1284, and the observer near line 1592) read the tap's plan state instead
- [ ] 2.5 Extend `KILL_POINT_MARKERS` in `scripts/crash-recovery-smoke-driver.py` with the `TapCrash` payload text and one unique tap string. Check with `strings` that `cargo build --release -p lushtext` contains neither. Run `make crash-recovery-smoke` once on the final code (task 10.1) to show the guard passes
- [ ] 2.6 Unwind audit over the final code. List every `Drop` impl and every lock on the unwind path of each primitive that D4 crashes inside. Record that none does I/O and that none is poisoned and read afterwards. Record in the handoff why the draft driver crashes only between model steps: the manifest lock `expect`s on poison

## 3. Write-shell conformance (D4)

- [ ] 3.1 Implement `services/conformance/{mod,ops,write_shell}.rs` (test-utils). It steps the shared `Disk` from the tap trace and checks, after every action, the D4 list:
  - outcome against backend results, with the `RemoveTemp` indifference, the absent-destination probe, and the content stream as the named exceptions;
  - the pinned call table;
  - destination bytes against `visible()`;
  - temp presence against `linked`;
  - temp mode no wider.

  At the terminal action it checks the result variant against the core's class. For moves and renames it checks the real source and destination against `MoveDisk` and `RenameNamespace`. After a crash it checks the destination against `after_crash(false)`, the `temp_name` parse, and `is_sweepable_leftover`. That predicate is given the real file's facts and a `LeftoverClock` naming another launch, with `now` past `STALE_LEFTOVER_AGE`
- [ ] 3.2 Add the deterministic exhaustive test over (primitive × destination state × action ordinal × call index × injection). It includes `MAX_TEMP_NAME_ATTEMPTS` consecutive collisions, runs in `make test`, and takes at most 5 s. Pin the per-action call table as its own test
- [ ] 3.3 Add `tests/properties/shell_conformance_write.rs` (64 cases: modes, payload sizes, two injections per run), and register it in `tests/properties.rs`
- [ ] 3.4 Triage every divergence under the classification requirement. Commit the shrunk case as a failing deterministic test, fix it, and record it in the programme record. A core defect follows task 7.4

## 4. Draft-service conformance (D5), before-effect faults first

- [ ] 4.1 Check the D5 action→call table against the real call sites in `ui/window/drafts/` and `ui/window/session_restore/`. Correct the design's table where it differs, and record each difference in the handoff
- [ ] 4.2 Implement the abstraction in `services/conformance/draft_service.rs`:
  - manifest entries and backing versions;
  - bodies, through the injective content↔text table;
  - explicit whole-second mtimes;
  - set-aside and local-history preservation;
  - the held authority.

  Unknown files in the drafts directory fail the abstraction. If local history is too costly to read at every step, read it at preservation steps and at startup, and record the restriction
- [ ] 4.3 Implement the single-window driver with enabled-indexed ops (D2): the window half is model-owned, and a crash drops every held value between model steps. Enable only `BeforeEffect` (scope-wide call 0), `Crash`, and `Startup`. It must pass on the real service before task 5 widens anything. Each divergence goes through task 3.4
- [ ] 4.4 Add `tests/properties/shell_conformance_drafts.rs` (≤ 16 ops, 64 cases) with the D2 shrink config
- [ ] 4.5 Add the reach test: a fixed seed and the default case count, with recorded floors for `WriteBody`, `Commit`, `DeletionStep`, `RestoreApply`, `ExecCleanup`, and `Startup`. Then run the two-mutant calibration on scratch, never-merged commits:
  - a set-aside copy that `preserve_stale_draft_body` skips;
  - `remove_manifest_entry` returning `Ok` while it leaves the entry on disk.

  Record the number of cases each needed in the design and in `docs/property-testing.md`
- [ ] 4.6 On a scratch, never-merged injected divergence, show that shrinking reaches a minimal script within the nextest `property` profile ceiling and persists it to the regression file

## 5. Widen the journal fault domain (D6) and step atomicity (D7)

- [ ] 5.1 Replace `fault: bool` with `Fault { None, BeforeEffect, AfterEffect }` in `journal_model.rs`:
  - `AfterEffect` applies only to Register's commit, WriteBody, Commit, and `DeletionStep(RemoveEntry)`;
  - it adds per-directory unconfirmed bits, which `DeletionStep(DeleteBody)`'s unsynced unlink also sets;
  - `Crash` keeps or reverts an unconfirmed effect through `Choices::flag`;
  - a later successful durable write into the same directory confirms it;
  - the set-aside copy's after-rename stays a success, as its shell decides.

  The harnesses choose the fault with `kani::any()`
- [ ] 5.2 Enable `AfterEffect` and `At(k)` in the drafts driver. `AfterEffect` is the destination-scoped `SyncDir` of the step's committing write, and only its kept branch is testable, because in-process tests cannot revert. `At(k)` is scope-wide call k. An intermediate multi-write state that matches neither model outcome is a step-atomicity environment finding. Resolve it by extending the model or by reordering the shell behind a failing-first test, and record which
- [ ] 5.3 If Kani or conformance finds a counterexample over `AfterEffect`, handle it as task 7.4, with reachability evidence from real errors

## 6. Replace shells (D8)

- [ ] 6.1 Failing first, against the mutants: add the three `process_file` survivor tests to `content_search/replace/tests.rs`, using the tap's before-action hook at `ReplaceFile`/`WriteBody`:
  - a same-size, new-mtime target modification, skipped under the pass-A reason;
  - a raised cancel plus a bodies directory replaced by a file, ending `Cancelled`, under each of the two cancel mutants.

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

- [ ] 8.1 Confirm that no dependency was added:
  - `Cargo.lock` is unchanged apart from workspace crates;
  - `cargo deny check advisories bans sources licenses` passes;
  - `cargo hakari generate --diff` is clean;
  - `cargo-sources.json` needs no regeneration
- [ ] 8.2 Review `sonar-project.properties` for `services/conformance/`, the model modules, and `backend_tap.rs`, under the SonarQube scope rule

## 9. Kani and proof strength, on the final code (D9)

- [ ] 9.1 Once the journal and write code is final, run detached, once each: `setsid nohup scripts/heavy.sh --envelope 16G -- make kani KANI_SHARD=<shard>` for `core-journal-and-write`, `core-journal-liveness`, `core-second-writer`, and `core-multi-window`. Compare against task 0.1:
  - every unchanged property keeps its verdict;
  - every widened one passes, or is a finding (task 5.3).

  Any harness time more than 20 % off its record is explained in the programme record. A broken margin follows the D6 fallback order. Update the `ORACLES` seconds
- [ ] 9.2 Set the four shard records to `PENDING DISPATCH (owed by verify-shell-conformance-against-proven-cores)`, with the local figures, and pass `make check-kani-shards`
- [ ] 9.3 Run `make proof-strength` for `journal_core` only (D9: only its harnesses widen their domain), detached, through `heavy.sh --envelope 16G`, with the Kani arm sharded (`PROOF_STRENGTH_SHARD=k/n`). Triage each survivor in the programme record
- [ ] 9.4 At the end of implementation, after the final push, dispatch `kani.yml` twice on the branch and record both run ids in the handoff. Do not watch them. The archive records the larger figure of the two runs in each of the four shard records, because the archive commit's hook fails on a pending record

## 10. Final verification

- [ ] 10.1 Run each of these through `heavy.sh`, with the stated envelopes, without warnings:
  - `make check` and `make check-policy`;
  - `make test` (`--envelope 16G`);
  - `make test-prop`;
  - `make fuzz-corpus-replay`;
  - `make crash-recovery-smoke`.

  No UI prefix is touched, so the three visual and accessibility smokes are not owed. Confirm with `git diff --name-only origin/main` that no path under the UI prefixes changed
- [ ] 10.2 Run `openspec validate verify-shell-conformance-against-proven-cores --strict`. At archive time, merge the deltas into `formal-verification-kani`, `durable-file-write-contract`, `draft-session-recovery`, `property-based-testing`, and `crash-restart-recovery-coverage`

## 11. Documentation sync

- [ ] 11.1 `docs/next/formal-verification.md`:
  - Add "Phase 6 — Shell conformance against the proven cores", covering the shared models, the tap and its two sites, the write enumeration and its call table, the drafts driver with its reach and calibration figures, the `AfterEffect` widening and its Kani results, and every finding with its class.
  - Update the deferral inventory: mark the three Replace survivors closed. Add entries for the GTK drafts coordination scenarios, the fuzz conformance mode, the two-window service property, Replace lockstep, mid-call draft crashes (K5), and power-loss semantics (Kani and K5 only), each with its reason and reopening trigger
- [ ] 11.2 `docs/next/formal-verification-next.md`: mark the N11 row and section done, pointing to this change. Make the N11 text claim only what was checked: the GTK drafts coordination is deferred, not covered. Update §4's suggested order
- [ ] 11.3 AGENTS.md, Testing section: add the conformance lane (what it drives, that the oracle is the shared proven model, where it runs). Name the conformance evidence in the durable-write and draft-journal design bullets. `.claude/CLAUDE.md` is a symlink to it
- [ ] 11.4 `.agents/rules/build.md`:
  - Property Testing: conformance properties, enabled-indexed generation, reach floors, the shrink budget, and regression promotion;
  - Formal Verification: shared model modules and where their choices come from, the fault-domain completeness rule, the divergence classes, and the `NOT_ORACLE_MODULES` and mutation entries;
  - fault seams: the backend tap as the durable-write injection seam.

  `.agents/rules/testing.md`, fault seams: the tap is per-invocation and thread-local, and panic crashes are used only where no lock or I/O `Drop` is on the unwind path
- [ ] 11.5 `docs/end-user-coverage.md`: map "the shell obeys its proven core" to the write enumeration and the two properties. `docs/property-testing.md`: the new properties, budgets, reach floors and calibration. `docs/mutation-testing.md`: the three survivors killed. Add a `gtk-testing` skill reference only if a new test pattern appears
- [ ] 11.6 `README.md`: update the testing and formal-verification paragraph if it lists the lanes
