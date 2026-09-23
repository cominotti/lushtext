## 1. Measurement mode in the shard runner

- [ ] 1.1 Add `--measure <json>` to `scripts/kani-shards.py run`. Per shard it records wall time (monotonic), peak RSS (`getrusage(RUSAGE_CHILDREN).ru_maxrss` after the child exits), exit status, and the per-harness `Verification Time:` lines teed from Kani's output. When `GITHUB_STEP_SUMMARY` is set it appends a Markdown table (design D1)
- [ ] 1.2 Extend `--self-test` with a parser case for Kani's `Verification Time:` and harness-name lines, and a case for the JSON shape; `make check-kani-shards` passes
- [ ] 1.3 Verify locally: `make kani KANI_SHARD=widgets-geometry` through the measure path writes a JSON whose shard wall time is within 5 % of an external `time` measurement, and lists all 9 harnesses

## 2. Measure every shard on the real runner

- [ ] 2.1 In `kani.yml`, run `make kani` in measurement mode and upload the JSON with `actions/upload-artifact` (one artifact per shard). Add the Kani install cache (design D5), keyed on `KANI_VERSION`
- [ ] 2.2 With the maintainer's go-ahead, push the change branch. Dispatch `gh workflow run kani.yml --ref <branch>` with a cold cache, wait with `gh run watch`, and fetch the artifacts with `gh run download`
- [ ] 2.3 Dispatch a second run (warm install cache) and fetch its artifacts. For each shard record the larger of the two runs' wall time and peak memory, plus both run ids
- [ ] 2.4 Evaluate the `target/kani` cache (design D5): add it on the branch, dispatch twice more, and adopt it only if the warm `widgets-geometry` job saves at least 3 minutes and the cache stays under 5 GB in total. Otherwise remove it, and record the rejection with its figures
- [ ] 2.5 If any shard exceeds 25 minutes or 12 GiB, apply design D3 in order: split first, re-measure, and only then tune the solver or reduce bounds. Record every step taken, and any bound change, in the programme record and the harness doc comment. For `core-second-writer`, confirm through Kani's failure output that the recorded counterexample is still found
- [ ] 2.6 Every job of the final dispatched run passes, and each one finishes within `timeout-minutes: 30`

## 3. Budgets and gate in the shard table

- [ ] 3.1 Failing first: add self-test cases to `kani-shards.py` for an unmeasured shard, a shard over 25 minutes, a shard over 12 GiB, and a `pull-request` shard over 15 minutes. Confirm they fail against the current table, which has no budget fields
- [ ] 3.2 Convert `SHARDS` to records carrying `gate`, `ci_minutes`, `ci_peak_gib`, and `measured_in` (design D2). Fill them from task 2.3, or from 2.5 after a split. Gate `widgets-geometry` as `pull-request` and every other shard as `scheduled`
- [ ] 3.3 Make `check` enforce the budget rules, and make `github-outputs` emit `pr-shards` besides `shards`. Remove any temporary `--allow-unmeasured` path. `make check-kani-shards` and its self-test pass

## 4. Promote `widgets-geometry` to the pull-request gate

- [ ] 4.1 Add `pull_request:` and `push: branches: [main]` triggers to `kani.yml`, and select `pr-shards` for those events and `shards` for schedule and dispatch (design D4). Rewrite the header comment so it no longer says the lane is outside the pull-request gate
- [ ] 4.2 `make check-workflow-timeouts` passes, and every Kani job keeps `timeout-minutes: 30`
- [ ] 4.3 Open (or update) the change's pull request. Confirm through `gh pr checks` that only `Kani Proof Harnesses (widgets-geometry)` ran, that it passed, and that its measured wall time is at most 15 minutes. Confirm with a dispatch that all shards still run on `workflow_dispatch`
- [ ] 4.4 Failing first: on a scratch commit that is never merged, break `viewport_slice` containment by one pixel. Confirm that the pull-request Kani check fails with a counterexample, then drop the commit

## 5. Gate the fixture writers behind `test-utils`

- [ ] 5.1 Failing first: add the fixture-gating rule and its self-test to `scripts/check-filesystem-boundary.sh` (design D7), and run it against the unchanged tree. It must fail on both ungated `pub mod fixture;` declarations. Also add a scratch production call to `draft_service::fixture::write_body` in `crates/lushtext-core/src/ui/window/drafts/journal.rs` and confirm that `cargo check -p lushtext --bins` compiles it
- [ ] 5.2 Gate `pub mod fixture;` in `services/draft_service.rs` and `services/filesystem/mod.rs` with `#[cfg(any(test, feature = "test-utils"))]`. Update the `RegisteredDraft` rustdoc to say the fixture exists only in test and `test-utils` builds
- [ ] 5.3 Confirm that `cargo check -p lushtext --bins` now fails on the scratch call with an unresolved `fixture`. Record the error, then remove the scratch call. The policy rule now passes
- [ ] 5.4 In `crates/lushtext-core/Cargo.toml` set `property-tests = ["test-utils"]`. Pass `--features test-utils` in the Makefile `bench`, `bench-baseline`, and `bench-compare` targets, in `scripts/bench-report.sh`, in `scripts/run-performance-smoke.sh`, and in the `ci.yml` bench-compile job
- [ ] 5.5 Build every consumer through its documented command, without warnings: `make test`, `make test-prop`, `make fuzz-corpus-replay`, `cargo bench -p lushtext-core --features test-utils --no-run`, `make performance-smoke` (or its bench compile step when the host skips the smoke), and `make check-terminology`
- [ ] 5.6 Add `cargo check -p lushtext --bins --locked` (default features) to the `ci.yml` lint job. Confirm the job still fits its `timeout-minutes`
- [ ] 5.7 Confirm the release binary carries no fixture code: `cargo build --release`, then check that `strings target/release/lushtext` finds no `write fixture text` panic message

## 6. Harness and fixture code out of production accounting

- [ ] 6.1 Record the baseline: `make mutants-list` lists 171 mutants in `**/kani_proofs.rs` and 1 in `services/draft_service/fixture.rs`. Record the total (6,001 mutant lines at proposal time)
- [ ] 6.2 Add `crates/**/kani_proofs.rs` and `crates/lushtext-core/src/services/draft_service/fixture.rs` to `.cargo/mutants.toml` `exclude_globs`, with a comment giving the reason (design D8). Re-run `make mutants-list`: the total drops by exactly 172, no `kani_proofs.rs` or `draft_service/fixture.rs` line remains, and no other file's count changes
- [ ] 6.3 Failing first: add self-test fixtures to `scripts/check-workflow-boundaries.py` for a gated `ui/example/policy/kani_proofs.rs` (must pass), an ungated one (must fail), and one with no parent (must fail). Confirm that the gated fixture fails before the rule exists
- [ ] 6.4 Implement `KANI_HARNESS_MODULE_NAME` recognition in both the unclassified-decision-logic rule and the role-home rule (design D9). `make check-workflow-boundaries` passes on the real tree and its self-test passes

## 7. Documentation sync

- [ ] 7.1 `docs/next/formal-verification.md`: in phase 2, replace "CI shard times on GitHub runners are not yet measured" with the measured per-shard table (wall time, peak memory, run ids, cache verdicts) and the pull-request gate. Record any split or bound change from task 2.5. Remove the `draft_service::fixture::write_body` deferral-inventory entry and note in phase 4 that it was closed
- [ ] 7.2 `docs/next/formal-verification-next.md`: mark N1 and N5 done, with pointers to this change, and update the suggested order in §3
- [ ] 7.3 `.agents/rules/build.md` (Formal Verification section): record the `kani_proofs.rs` naming rule and its mutation and boundaries exclusion, the gate and budget fields of the shard table, the pull-request selection, measurement mode, the margins, and the D3 fit order. Replace the sentence saying the lane stays outside the pull-request gate. In the fixture and test-utils guidance, record that both fixture modules are test-only and that benchmark commands pass `--features test-utils`
- [ ] 7.4 `AGENTS.md`: update the CI bullet (`kani.yml` also runs on pull requests, limited to pull-request-gated shards), the `make kani` line in Build Commands, and the `RegisteredDraft` sentence in Draft persistence ("unrepresentable in production builds; fixtures are test-only"). `.claude/CLAUDE.md` is a symlink to `AGENTS.md`, so it needs no separate edit
- [ ] 7.5 `README.md`: update the formal-verification paragraph (pull-request gate) and any benchmark command that now needs `--features test-utils`
- [ ] 7.6 Merge this change's delta specs into `openspec/specs/formal-verification-kani/spec.md` and `openspec/specs/draft-session-recovery/spec.md` at archive time, and run `openspec validate harden-kani-lane-and-draft-token --strict`

## 8. Final verification

- [ ] 8.1 `make check`, `make check-policy`, and `make test` pass locally
- [ ] 8.2 `make kani` passes locally, all shards, with the pinned Kani
- [ ] 8.3 The change's final pull-request CI is green, including the Kani pull-request job, and one final `workflow_dispatch` of `kani.yml` passes every shard within budget
