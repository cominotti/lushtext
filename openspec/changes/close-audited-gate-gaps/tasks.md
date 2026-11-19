# Tasks

Tests: behaviour changes in group 2 (the subscription, the print hook) and group 4 (the durable-write error chain); their failing-first tests come before the code. Groups 1 and 3 are tooling and docs.
Kani: `core-protected-fold` (comment-only edit; a pull-request shard, left to CI), plus any shard task 0.1 finds reads `services/durable_write.rs`.
Timing: none (no timed path touched).
Start: only after every edit, spec and queued change of the programme coordinator's session has finished (maintainer, 2026-11-19), and after the maintainer places this change in `PARALLEL-LEDGER.md`. Task 0.1 re-measures the baseline first.

## 0. Re-measure the baseline

- [ ] 0.1 On the then-current `origin/main`, re-derive every figure in `proposal.md` "Why" and design D1's table:
  - `check-policy`'s prerequisites and their layout;
  - CI's `make` steps;
  - the hakari baseline;
  - the leak's line, the `theme.rs` site count and the `DurableWriteError` callers;
  - whether `durable_write.rs` is now a Kani harness input.
  
  Record the result in the change's handoff and update the artifacts in place. Verify with `openspec validate close-audited-gate-gaps --strict`.

## 1. CI and local gate alignment

- [ ] 1.1 Grep `scripts/`, `.githooks/` and `.github/` for parsers of the `check-policy:` line. If the coordinator assigned "one per line" to this change, rewrite `check-policy`'s prerequisites one per line (backslash continuations). Verify that `make -n check-policy` lists the same 33 prerequisites as before (diff the two outputs).
- [ ] 1.2 Add `check-policy-ci`, the covered-elsewhere table (target and job) and the local-only table (target and reason) to the `Makefile`, seeded from design D1's table. Add `scripts/check-policy-ci.py --self-test`, planting each failure (missing, duplicated, stale), and wire the check into `check-policy`. Verify that `make check-policy` passes and that the self-test fails on each planted case.
- [ ] 1.3 Remove the second `proof-strength.py --self-test`: keep it in `check-kani-shards`, drop it from `check-build-cache-helpers`. Verify that `make -n check-policy | grep -c 'proof-strength.py --self-test'` prints 1.
- [ ] 1.4 In `ci.yml`'s `Lint` job:
  - replace `cargo fmt --all -- --check` with `make check-fmt`;
  - add a `make check-policy-ci` step;
  - add the guarded hakari step with all three commands (design D3, `CARGO_HAKARI_VERSION`);
  - remove the individual `make check-*` steps the aggregate now covers.
  
  Verify with `make check-workflow-timeouts` and with `actionlint` if installed.
- [ ] 1.5 Update `.agents/rules/build.md` ("Validation gates", "CI"), `AGENTS.md` ("Rust Linting Policy", "Packaging and CI") and `README.md` (if it lists CI gates) to name `check-policy-ci`, the local-only table and the hakari step. Verify with `make check-agent-docs`. Commit through `scripts/heavy.sh --mem none -- git commit -F ...`, signed, and push the branch.
- [ ] 1.6 CI runs only on `pull_request` and on push to `main` (no `workflow_dispatch`), so open a draft PR for the branch and read its runs (CI-evidence exception, `parallel-agent-work.md`; read them before taking `MERGE-LOCK`).
  - Reclassify, with recorded reasons, every `check-policy-ci` prerequisite that fails for an environmental reason; fix every real finding. Commit and push each round.
  - Record the `Lint` wall time of two runs before and after in the change's handoff.
  
  Verify: `Lint` green on two runs and within its `timeout-minutes`.
- [ ] 1.7 Prove the new gates red on a separate scratch draft PR, closed afterwards and never merged:
  - an unformatted widget-test file fails `Lint` at `make check-fmt`;
  - a hand edit that leaves `workspace-hack/Cargo.toml` different from `cargo hakari generate`'s output fails the hakari step;
  - a planted `make check-signal-connections` violation fails `check-policy-ci`.
  
  Record the run ids.

## 1b. workspace-hack

- [ ] 1b.1 Add the `[final-excludes]` block (design D3) to `.config/hakari.toml`, then run `cargo hakari manage-deps` (which adds the dependency to `cargo-gtk-proof`) and `cargo hakari generate`. Verify that `generate --diff`, `verify` and `manage-deps --dry-run` exit 0, that `workspace-hack/Cargo.toml` is unchanged, and that `make gtk-lush-stock-fixtures` and `cargo deny check advisories bans sources licenses` pass. Run `make cargo-sources` and confirm `build-aux/cargo-sources.json` is byte-identical.
- [ ] 1b.2 Extend `scripts/check-gtk-lush-policy.py` with the two rules in design D3 and their `--self-test` cases. Verify with `make check-gtk-lush-policy` and `make gtk-lush-adoption-matrix`.
- [ ] 1b.3 Fix `.agents/rules/rust.md:12` and `.agents/rules/build.md` ("Adding Dependencies": a new crate runs `cargo hakari manage-deps`; a new family crate joins `final-excludes`), and say in `crates/gtk-lush/GOVERNANCE.md` that the family counts for unification without depending on `workspace-hack`. Verify with `make check-agent-docs`. Commit through `heavy.sh`, signed, and push.

## 2. Owned visibility subscriptions

- [ ] 2.1 Add a failing-first widget test under `crates/lushtext/tests/widget/`. Assign the module to a shard (`make check-widget-shards`). Verify that it **fails on `main`**, with one extra handler per key.
  - **Count.** Count the handlers matching `changed::workspace-show-hidden-files` and `changed::workspace-excluded-names` on `settings_writes::shared()` with `gobject_ffi::g_signal_handlers_block_matched` (`G_SIGNAL_MATCH_ID | G_SIGNAL_MATCH_DETAIL`) and unblock them in a `Drop` guard.
  - **Steps.** Open and close a window, wait for finalization, and assert the counts are back where they started.
- [ ] 2.2 Change `workspace_visibility::connect_visibility_changed` to take its owner (design D4). Migrate `ui/window/imp.rs` and `ui/sidebar/imp.rs`, and delete the stale lifetime comment. Verify that 2.1 passes and that the sidebar's existing visibility widget tests still pass.
- [ ] 2.3 Extend `scripts/check-settings-writes.py` with the discarded-id rule and the `ui/theme.rs` allowlist (expected count 3, with its reason), plus the self-test cases in design D4. Verify that `make check-settings-writes` passes on the tree and that the self-test fails on each planted case.
- [ ] 2.3b Add a test that `with_print_runner_for_test` restores the previous runner when `f()` panics (`std::panic::catch_unwind`); verify that it fails on `main`. Then add the `Drop` guard (design D7) and verify that it passes.
- [ ] 2.4 Update `.agents/rules/ui.md` ("GSettings Bindings") and `.agents/rules/widget-wiring.md` ("Signal And Binding Lifetimes") with the owner form and the gate rule. Add the three `theme.rs` subscriptions to the `gtk-lush-signals` retained-site audit. Verify with `make check-agent-docs`.
- [ ] 2.5 Run the gate order in `.agents/rules/testing.md`, then the three smokes (`ui/` prefix): `make visual-geometry-smoke`, `make accessibility-smoke`, `make visual-smoke`. Commit right after through `heavy.sh`, signed, and push.

## 3. Committed evidence for code comments

- [ ] 3.1 Copy `.claude/handoffs/c2-measure/sweep-table.md` to `docs/benchmarks/c2-head-sweep.md` with a provenance header (design D5). Repoint `model/protected_fold.rs`'s `N_HEAD` doc and `docs/next/long-line-documents.md:1737` at it. Verify with `git grep -n 'c2-measure' -- crates docs`, which shows no gitignored path in those two places.
- [ ] 3.2 Commit `main_m9.rs.add` next to the stream-replace-all record and repoint `services/content_search/replace/sr_measure.rs:8` (design D5). Verify with `git grep -n 'claude/handoffs' -- crates`, which prints nothing.
- [ ] 3.3 Run `make check-thresholds` (the `N_HEAD` `Kind:` line must stay intact), `make check-fmt` and `make check-clippy`. Commit through `heavy.sh`, signed, and push.

## 4. Durable-write error chain

- [ ] 4.1 Add a unit test in `services/durable_write.rs` that forces a before-rename and an after-rename failure through the existing fault seams and asserts each error's message names its phase and its `source()` is the I/O error. Verify that it fails on `main` (no `source()`).
- [ ] 4.2 Implement design D6. Verify with 4.1, the `durable-file-write-contract` tests, `cargo nextest run -p lushtext-core` and `make check-clippy`. Commit through `heavy.sh`, signed, and push.

## 5. Disposition record

- [ ] 5.1 Write `docs/next/refactor-1-disposition.md` (design D8), covering every audit item and the six changes that take items. Link it from `docs/next/`'s index if one exists. Verify with `make check-terminology` and `make check-agent-docs`. Commit through `heavy.sh`, signed, and push.

## Workflow follow-up

- The coordinator decides whether "check-policy one per line" moves here from `smoke-gate-coherence`, and records it in `PARALLEL-LEDGER.md`.
- Deferred: the `docs/` audit of `.claude/handoffs/` references (its own change).
- Archive after review and merge, per the programme's merge rules.
