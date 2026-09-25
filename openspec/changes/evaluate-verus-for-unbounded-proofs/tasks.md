## 1. Gate audit, home, and tooling

- [ ] 1.1 Resolve the latest non-rolling Verus release from its release page. Record its bundled Z3, its pinned rustc (read from the archive's `version.json`), and the SHA-256 of the archive. Confirm the current `#[verus_spec]`, `cargo verus`, `nonlinear_arith`, and thread-count options from the guide or Context7, never from memory. Record the AutoVerus requirements (API key, Verus commit). Start the report's log with these, and with the budgets from design D6.
- [ ] 1.2 Re-run the gate audit of the Quint change (`docs/next/formal-verification-quint-vs-tlaplus.md` §10) for the new paths. Cover `scripts/check-filesystem-boundary.sh`, the terminology guard, `sonar-project.properties`, `scripts/check-workflow-boundaries.py`, `scripts/check-agent-docs.sh`, `scripts/check-workflow-timeouts.py`, `cargo deny`, hakari, Clippy, nextest, and `make mutants-*`. Record which of them see `formal/evaluation/verus/`, the new script targets, or the report. Adjust the design if any would.
- [ ] 1.3 Create `formal/evaluation/verus/` as a standalone package:
  - an empty `[workspace]` table and its own `Cargo.lock`;
  - a `rust-toolchain.toml` with Verus's pinned rustc;
  - a `.cargo/config.toml` whose `target-dir` is under `build/formal-evaluation/`;
  - a README with the "disposable evaluation model, not a maintained property" status and a pointer to the report.

  Add it to the root `Cargo.toml` `exclude`. Confirm that `cargo metadata`, `cargo deny check advisories bans sources licenses`, `cargo hakari verify`, `make check`, and `make check-policy` pass unchanged, and that `git status` shows no build output inside the directory.
- [ ] 1.4 Extend `scripts/formal-evaluation.sh` with `verus-install`, `verus-versions`, `verus-v1`, `verus-v2`, `verus-v3`, `verus-drift`, and `verus-all`:
  - install into `build/formal-evaluation/tools/`, with Verus's rustc under a private `RUSTUP_HOME`/`CARGO_HOME` there;
  - verify the archive checksum, and fail if `z3` is missing (verus-lang/verus#2914);
  - run every check through the existing `measure` wrapper, under `taskset -c 8-23`, with at most 16 threads and `FORMAL_EVAL_MEM_MB` defaulting to 16384 for these targets;
  - print a clear missing-tool message.

  Reach them through the existing local-only `make formal-evaluation FORMAL_EVAL_TARGET=…`, and update its help text. Confirm that no workflow references them and that no prerequisite of `check`, `check-policy`, `test`, `kani`, or `end-user-smoke` does. Record the install time, the download size, and the prerequisites.
- [ ] 1.5 Write the drift check (design D1). Write it failing first: it must fail on a deliberate one-line change to a copied function before it is used.
  - Finite-domain `journal_core` decisions: a behavioural comparison over every input, against the real functions through a path dependency on `lushtext-core`.
  - Integer functions: a normalised text comparison.

  Record how the helper builds with the pinned toolchain.

## 2. V1 calibration

- [ ] 2.1 Declare V1's budget in the log. Copy `clamped_preview_width` and prove each property that `crates/lushtext-core/src/ui/markdown_preview/policy/kani_proofs.rs` proves. Record spec, proof, and code line counts, the rewrite footprint, the trusted base, and verification time and memory.
- [ ] 2.2 Confirm that Verus fails on `preview_width_is_not_always_a_third`, and that the proof fails on a mutant of the copied function. Choose the mutant by first confirming that a Kani harness rejects it; an equivalent mutant does not count. If V1 cannot reproduce the known answers within budget, record that, and decide with evidence whether to continue. Add `band_floor` only if budget remains.

## 3. Kani-first attempts (design D5)

- [ ] 3.1 Declare the budget. In a scratch worktree under `build/formal-evaluation/`, never committed, draft the inductive-step Kani attempt for S1–S4 over the multi-window model:
  - either a loop-contract harness under `-Z loop-contracts`,
  - or a one-step harness over an arbitrary state assumed to satisfy the invariant.

  Record its text, the result, time, and memory in the report, as phase 3 did.
- [ ] 3.2 Record the compositional attempt for S1–S4 (per-id and per-window independence), or the reason none applies. Cite the existing attempts for L1 and for the slice loop, and record that the slice claim is not targeted.

## 4. V2 unbounded journal safety

- [ ] 4.1 Declare V2's budget. Copy the `journal_core` decisions under the drift check. Port the multi-window model of `crates/lushtext-core/src/services/draft_service/kani_proofs.rs`: windows, the process journal, disk, faults, crash and restart, one process (A6). Use `vstd` collections for unbounded ids and windows.
- [ ] 4.2 Prove `Init ⟹ Inv` and `Inv ∧ step ⟹ Inv'` for every action, with S1–S4 and "one owning window per id" as the invariant's backbone. Record each strengthening conjunct.
- [ ] 4.3 Reintroduce mutants M1 (`Unavailable` preserves nothing) and M2 (`TwoWindowBaselineScope`) one at a time. Confirm that the proof fails on each, and that the recorded Kani harness or baseline run rejects the same mutant. Mark any extra mutant with the Kani harness that kills it.
- [ ] 4.4 Re-run task 3.1's Kani inductive step with V2's strengthened invariant, and record the result (design D5).
- [ ] 4.5 Record the metrics (the spec's deductive-verifier list). Walk through one typical production change to `journal_core`, for example a new `RestoreEnding` variant, and describe what it would require in the proof and in the copy.

## 5. V3 L1 from every reachable state

- [ ] 5.1 Declare the budget. Prove: `Inv(s)`, a drained lane, and a dirty editor imply that the fault-free pass is clean within 7 steps (design D4). Show that the proof fails without the fault-free assumption, and without the lane-drain precondition.
- [ ] 5.2 Only if budget remains: attempt the E9 form, "eventually clean under weak fairness with a bounded environment", with a ranking function or `verus-tla`. Record the route taken, or mark it "not attempted".

## 6. In-place probe and AutoVerus

- [ ] 6.1 Declare the budget. In the scratch worktree, annotate `journal_core` with `#[verus_spec]` and run `cargo verus` on `lushtext-core`. Record what works, what fails (dependencies, edition 2024, `const fn`, crate imports), and the diff. Store the diff as a patch file under `formal/evaluation/verus/`.
- [ ] 6.2 Record AutoVerus as "not attempted: the maintainer declined sending project code to an external LLM API (2026-09-25)", with its requirements from task 1.1.

## 7. Report and decision

- [ ] 7.1 Write `docs/next/formal-verification-verus.md`, following the folder-set terminology guard. It contains:
  - versions, checksums, and the machine;
  - per-target metrics against Kani and against the Quint/TLA+ E6 and E9 results;
  - the Kani-first attempts;
  - the exploration log, failures included;
  - a reproduction section;
  - the gate audit;
  - a decision section choosing among design D7's options, with the losing options' evidence and the fate of `formal/evaluation/verus/`.
- [ ] 7.2 Update `docs/next/formal-verification.md`: §2's tool facts and measured results, the Kani-first paragraph, and the deferral inventory. Update `docs/next/formal-verification-next.md`: "Tool criteria", the change table, and "Dormant: Lean", which must state the amended gate.
- [ ] 7.3 Update the `make formal-evaluation` entries in AGENTS.md and `.agents/rules/build.md`, still marked local-only, with the `verus-*` targets.

## 8. Verification

- [ ] 8.1 On a checkout without the Verus tools, run `make check`, `make check-policy`, and `make test`. They pass unchanged, and `make formal-evaluation FORMAL_EVAL_TARGET=verus-v1` fails with the install message.
- [ ] 8.2 From the recorded versions, run `make formal-evaluation FORMAL_EVAL_TARGET=verus-drift` and `FORMAL_EVAL_TARGET=verus-v1`. They pass, and V1 reproduces its known answers.
- [ ] 8.3 Confirm that `git diff` touches no production crate, no Kani harness or shard table, and no CI workflow. Beyond `formal/evaluation/verus/`, the script, the docs, and the Makefile help text, the only file it touches is the root `Cargo.toml` `exclude`.
- [ ] 8.4 Run `openspec validate evaluate-verus-for-unbounded-proofs --strict`.
