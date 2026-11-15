## 0. Verification scope (stage 1, 2026-11-15)

- **Kani shards:** none. No proven module or harness is touched; the scratch Kani attempts (tasks 3.1, 4.4) run in a scratch worktree, outside every shard, and change no shard table.
- **GTK axioms and samples:** none. No toolkit behaviour is relied on; nothing GTK-facing is verified.
- **Responsiveness lane cases and timing verdicts:** none. Verification times are cost figures recorded through `scripts/heavy.sh` with the load (design D6), not `scripts/measure.sh` verdicts.
- **Proof strength:** not run. The N9 results are read, not regenerated.
- **Failing-first:** the drift check (task 1.5), each V1 counterexample (task 2.2), each V2 mutant (task 4.3), and each V3 dropped precondition (task 5.1) must fail before the corresponding proof counts.
- **Failing means the named obligation:** a known-false claim is refuted by a proved witness, and a mutant or dropped precondition counts only when the named postcondition, assertion, or invariant conjunct fails, never on a timeout, an `rlimit` hit, or a build error (design D2, D3).
- **Machine:** every heavy command in its own `scripts/heavy.sh --mem 16G -- <one command>` call under `taskset -c 8-23` (commits: `--mem none`), long runs detached; a multi-check `verus-*` target wraps each check itself and is run under `taskset` only, never inside one `heavy.sh` (design D6).
- **Production comment:** task 7.4 corrects one module rustdoc line in `journal_core.rs`; it changes no harness input, so it owes no shard run.

## 1. Gate audit, home, and tooling

- [ ] 1.1 Resolve the latest non-rolling Verus release from its release page (stage 1 found `0.2026.11.02.8bf68ee`, rustc 1.99.1, 36,022,359 bytes, SHA-256 `f3a0119292df8855e4435da8130bee1873b0e6a60c306accf81a08ea687bba3c`; use a newer one if it exists). Record its bundled Z3 version, its pinned rustc (read from the archive's `version.json`), the SHA-256 of the archive, and why the archive shrank from about 487 MB (up to `0.2026.10.26`) to 36 MB. Confirm the current `#[verus_spec]`, `cargo verus verify`/`build`, `nonlinear_arith`, `rlimit`, and thread-count options from the guide or Context7, never from memory. Record the AutoVerus requirements (API key, Verus commit). Start the report's log with these, and with the budgets from design D6.
- [ ] 1.2 Re-run the gate audit of the Quint change (`docs/next/formal-verification-quint-vs-tlaplus.md` §10) for the new paths. Cover every prerequisite of `make check-policy` (including the gates added since that audit: `check-temp-scope`, `check-agent-helpers`, `check-thresholds`, `check-kani-shards`, `check-widget-shards`, `check-gtk-axioms`, `check-explicit-simd`, `check-workflow-timeouts`, `check-workflow-boundaries`, and the terminology guard), `scripts/check-filesystem-boundary.sh`, `sonar-project.properties`, `scripts/check-agent-docs.sh`, `cargo deny`, hakari, Clippy, nextest, `make fmt`/`check-fmt`, and `make mutants-*`. Record which of them see `formal/evaluation/verus/`, the new script targets, or the report (for example, `check-agent-helpers` scans `scripts/` for `pgrep -f`). Adjust the design if any would.
- [ ] 1.3 Create `formal/evaluation/verus/` with a README ("disposable evaluation model, not a maintained property", a pointer to the report, and the copy-pinning commit) and two sibling standalone packages:
  - `proofs/`: an empty `[workspace]` table, its own `Cargo.lock`, a `rust-toolchain.toml` with Verus's pinned rustc, `vstd` matching the release, and `[package.metadata.verus] verify = true`;
  - `drift/`: an empty `[workspace]` table, its own `Cargo.lock`, no `rust-toolchain.toml`, and a path dependency on `lushtext-core`.

  Both set a `.cargo/config.toml` `target-dir` under `build/formal-evaluation/`. Add both to the root `Cargo.toml` `exclude`. Confirm that `cargo metadata`, `cargo deny check advisories bans sources licenses`, `cargo hakari verify`, `make check`, and `make check-policy` pass unchanged, and that `git status` shows no build output inside the directory.
- [ ] 1.4 Extend `scripts/formal-evaluation.sh` with `verus-install`, `verus-versions`, `verus-v1`, `verus-v2`, `verus-v3`, `verus-drift`, and `verus-all`:
  - install into `build/formal-evaluation/tools/`, with Verus's rustc under a private `RUSTUP_HOME`/`CARGO_HOME` there;
  - verify the archive checksum, and fail if `z3` is missing (verus-lang/verus#2914);
  - select each package's toolchain explicitly (`RUSTUP_TOOLCHAIN` plus the private `RUSTUP_HOME` for `proofs/`, the repository pin for `drift/`), never by directory lookup;
  - run every check through the script's existing `measure` subcommand, with at most 16 Verus threads and `FORMAL_EVAL_MEM_MB` defaulting to 14336 for these targets, and wrap each check, and each `cargo` build a target needs, in its own `scripts/heavy.sh --mem 16G --` call inside the script, so a multi-check target (`verus-v2` with its mutants, `verus-all`) never runs under one lock (design D6);
  - extend `classify` for Verus output (`verification results:: N verified, M errors`, the failed `ensures`/`assert` name, `rlimit` exceeded, build errors), so a proof, a refuted claim's witness, a caught mutant, and a resource stop are told apart, and test it on one captured output of each kind;
  - make `measure` append `/proc/loadavg` at start and end to each result row (it records neither today);
  - print a clear missing-tool message;
  - use no `pgrep -f`/`pkill -f`.

  Reach them through the existing local-only `make formal-evaluation FORMAL_EVAL_TARGET=…`, and update its help text. Confirm that no workflow references them and that no prerequisite of `check`, `check-policy`, `test`, `kani`, or `end-user-smoke` does. Record the install time, the download size, and the prerequisites.
- [ ] 1.5 Write the drift check (design D1) as a table exchange: `drift/` (workspace toolchain) writes the production side's table, `proofs/` (Verus toolchain, `cargo verus build`) writes the copy's, and `verus-drift` diffs them. Write it failing first: it must fail on a deliberate one-line change to a copied function before it is used.
  - Finite-domain `journal_core` decisions (`pub`, called directly): every input; for `run_deletion`, every start step with every outcome sequence up to three steps, recording the terminal and the steps performed.
  - Integer functions (`pub(crate)`, so not callable from `drift/`): the script extracts each function and the constants it reads from the pinned commit with `git show`, failing if the signature is not found exactly once, and `drift/` compiles the extracted text and writes the boundary and seeded-sample table. Where the copy keeps the production body (annotations in `#[verus_spec]` attribute form, removed mechanically), also compare the rustfmt-normalised bodies; where it was rewritten, record that the table is the only check. No production item is made more visible.

  Record the `origin/main` commit each copy was taken from. Optionally record whether the annotated copy also builds with a plain `cargo build` under the workspace toolchain (the guide says it does); the check does not depend on it.

## 2. V1 calibration

- [ ] 2.1 Declare V1's budget in the log. Copy `clamped_preview_width` and prove each property that `crates/lushtext-core/src/ui/markdown_preview/policy/kani_proofs.rs` proves (shard `core-geometry-policies`). Record spec, proof, and code line counts, the rewrite footprint, the trusted base, and verification time and memory with the load.
- [ ] 2.2 Refute `preview_width_is_not_always_a_third` in Verus by proving a concrete witness against it (for example `clamped_preview_width(500, 2) == 1`, and `3 * 1 > 2`), and confirm that the proof fails, on the named postcondition, on a mutant of the copied function. Choose the mutant by first confirming that a Kani harness rejects it (the N9 population, or a scratch run); an equivalent mutant does not count. If V1 cannot reproduce the known answers within budget, record that and stop the evaluation (design D2). Add `band_floor` only if budget remains.

## 3. Kani-first attempts (design D5)

- [ ] 3.1 Declare the budget. In a scratch worktree under `build/formal-evaluation/`, created from a recorded commit and never committed, draft the inductive-step Kani attempt for S1–S4 plus one owning window per id, over the multi-window model at the lane's scope:
  - a one-step harness over an arbitrary state assumed to satisfy the invariant (preferred, cheaper),
  - or a loop-contract harness under `-Z loop-contracts`.

  Create the worktree with `git worktree add --detach` (no branch), keep its diff as a patch under `formal/evaluation/verus/`, and remove it with `git worktree remove` when section 6 is done. Pair the step with a base-case harness (`Init ⟹ Inv` after the first startup) and a `kani::cover!` that a state satisfying the assumed invariant exists; the step counts only with both. Run each harness with its own `KANI_TARGET_DIR` under `build/formal-evaluation/`, detached, through `taskset -c 8-23 scripts/heavy.sh --mem 16G --` around the script's `measure` subcommand, one harness per call, kissat before any cap raise. Record its text, the result, time, memory, and load in the report, as phase 3 did. If it proves, record the action bound as removed in Kani at that scope, and narrow V2 to any number of ids and windows (design D5).
- [ ] 3.2 Record the compositional attempt for S1–S4 (per-id and per-window independence), or the reason none applies. If it carries id and window generality together with 3.1, record V2 and V3 as "not targeted: verified in Kani" and go to section 6. Cite the existing attempts for L1 and for the slice loop, and record that the slice claim is not targeted.

## 4. V2 unbounded journal safety

- [ ] 4.1 Declare V2's budget and its target as narrowed by section 3. Copy the `journal_core` decisions under the drift check. Port the multi-window model of `crates/lushtext-core/src/services/draft_service/kani_proofs.rs`: windows, the process journal and its lane, disk, faults, crash and restart, one process (A6). Use `vstd` collections for unbounded ids and windows.
- [ ] 4.2 Prove `Init ⟹ Inv` and `Inv ∧ step ⟹ Inv'` for every action, with S1–S4 and "one owning window per id" as the invariant's backbone. Record each strengthening conjunct. Run the vacuity probe first (design D3): an `assert(false)` after the invariant is assumed in the step must fail, and a concrete initial state must be shown to satisfy the invariant.
- [ ] 4.3 Reintroduce mutants M1 (`Unavailable` preserves nothing) and M2 (`TwoWindowBaselineScope`) one at a time, and confirm that the proof fails on the named obligation for each. Then list the N9 population with `make proof-strength-list PROOF_STRENGTH_MODULE=services/draft_service/journal_core.rs` (no verification; only aggregates are committed), and sample it. Confirm each sampled mutant's kill with a scratch Kani run under task 3.1's run rules, inside the V2 budget: first `journal_decisions_follow_their_documented_rules` (0.35 s), then the invariant harness (`journal_invariants_hold_under_crashes` or `journal_invariants_hold_across_two_windows`). Count for V2 only mutants an invariant harness kills, and confirm the proof fails on each; report decision-table-only kills as a comparison (does V2's invariant see them?), counted neither for nor against V2 (design D3). A mutant no Kani harness kills is listed but does not count.
- [ ] 4.4 Re-run task 3.1's Kani inductive step with V2's strengthened invariant, under the same run rules, and record the result (design D5).
- [ ] 4.5 Record the metrics (the spec's deductive-verifier list). Walk through one typical production change to `journal_core`, for example a new `RestoreEnding` variant, and describe what it would require in the proof, the copy, and the drift tables. Re-run `verus-drift` against current `origin/main` and record whether queued changes have moved the copied code since its pinned commit.

## 5. V3 L1 from every reachable state

- [ ] 5.1 Only if V2 closed (otherwise record "not attempted: depends on V2"). Declare the budget. Prove, over V2's multi-window model: `Inv(s)`, a drained lane, a dirty editor, and its window holding the id's claim imply that the fault-free pass is clean within 7 steps (design D4); record that this is larger than Kani's single-window L1. Show k = 7 tight by a proved witness state from which six fault-free steps leave the editor dirty. Run the vacuity probe on the precondition. Show that the proof fails, on the named obligation, without the fault-free assumption, and without the lane-drain precondition.
- [ ] 5.2 Only if budget remains: attempt the E9 form, "eventually clean under weak fairness with a bounded environment", with a ranking function or `verus-tla`. Record the route taken, or mark it "not attempted".

## 6. In-place probe and AutoVerus

- [ ] 6.1 Declare the budget. In the scratch worktree, annotate `journal_core` with `#[verus_spec]` and run `cargo verus verify` on `lushtext-core` with Verus's toolchain. Record what works and what fails (the workspace `rust-version` against Verus's rustc, GTK `-sys` dependencies, edition 2024, `const fn`, the `impl FnMut` parameter of `run_deletion`, the `cfg_attr(kani, derive(kani::Arbitrary))` attributes, workspace lints, crate imports), and the diff. Record what the `vstd` dependency the annotations need would cost an adoption: `cargo deny check`, `cargo hakari`, the Flatpak's offline `cargo-sources.json` vendoring, and the Snap build. "Blocked by the toolchain gap" is a valid result. Store the diff as a patch file under `formal/evaluation/verus/`.
- [ ] 6.2 Record AutoVerus as "not attempted: the maintainer declined sending project code to an external LLM API (2026-09-25)", with its requirements from task 1.1.

## 7. Report and recommendation

- [ ] 7.1 Write `docs/next/formal-verification-verus.md`, following the folder-set terminology guard. It contains:
  - versions, checksums, and the machine;
  - install and CI cost against the repository's 30-minute job cap (download, unpack, the private rustc install, and verification time per target), and the bridge options back to Rust (copy plus drift tables, in-place annotation, `external_body`), as the evidence rule requires;
  - the re-verification of V1 and V2 (and V3 if it closed) on the newest release available before the report (the next non-rolling one, otherwise the newest rolling one; `0.2026.11.05.31e44e6` already exists), with the result;
  - per-target metrics against Kani and against the Quint/TLA+ E6 and E9 results;
  - the Kani-first attempts, and how they narrowed the targets;
  - the exploration log, failures included;
  - the copy commits and the final drift-check result;
  - a reproduction section;
  - the gate audit;
  - a decision section recommending one of design D7's options to the maintainer, with the losing options' evidence and the fate of `formal/evaluation/verus/`.
- [ ] 7.2 Update `docs/next/formal-verification.md`: §2's tool facts and measured results, the Kani-first paragraph, the recommendation (marked as awaiting the maintainer's decision), and the deferral inventory. Update `docs/next/formal-verification-next.md`: "Tool criteria", the change table (add this change), and "Dormant: Lean", which must state the amended gate.
- [ ] 7.3 Update the `make formal-evaluation` entries, still marked local-only, with the `verus-*` targets: the Makefile's header comment and `help` text, `.agents/rules/build.md` ("Formal Verification (Kani)", the "Tool" bullet), and README.md's formal-verification paragraph (which names only the Quint vs TLA+ comparison). AGENTS.md no longer lists the target (its catalogue defers to `make help`), so none is added there. Check `docs/next/long-line-evolution.md` §4 (which lists this change as queued) and the other files in `.agents/rules/documentation.md`'s list; none other is expected to change.
- [ ] 7.4 Correct `crates/lushtext-core/src/services/draft_service/journal_core.rs`'s module rustdoc: L1 holds "within seven journal steps" (`LIVENESS_STEPS = 7` in `kani_proofs.rs`; `a_dirty_editor_may_need_seven_steps` shows six is too few), not six, wrong since `67aef874`. Comment only: confirm with `git diff` that only `//!` lines changed, so no harness input and no shard run is owed.

## 8. Verification

- [ ] 8.1 On a checkout without the Verus tools, run `make check`, `make check-policy`, and `make test` (each through `scripts/heavy.sh`; `make test` as `--mem none`). They pass unchanged, and `make formal-evaluation FORMAL_EVAL_TARGET=verus-v1` fails with the install message. `make kani` is not run: the diff changes no code a harness compiles (task 7.4 is comment only), and `cargo metadata --format-version 1 --no-deps` lists the same workspace members as `origin/main`; `make check-kani-shards` runs inside `make check-policy`.
- [ ] 8.2 From the recorded versions, run `make formal-evaluation FORMAL_EVAL_TARGET=verus-drift` and `FORMAL_EVAL_TARGET=verus-v1`. They pass, and V1 reproduces its known answers.
- [ ] 8.3 Confirm that `git diff origin/main` touches no production code (beyond task 7.4's rustdoc lines), no Kani harness or shard table, no GTK axiom, and no CI workflow. Beyond `formal/evaluation/verus/`, the script, the docs, the Makefile help text, and that rustdoc, the only file it touches is the root `Cargo.toml` `exclude`. Confirm `git worktree list` shows no scratch worktree left.
- [ ] 8.4 Run `openspec validate evaluate-verus-for-unbounded-proofs --strict`.
