## 1. Shard table and gate

- [x] 1.1 Add `scripts/widget-shards.py` with the shard table (`window`, `editor-page`, `surfaces`), static discovery mirroring `crates/lushtext/build.rs`, the ownership/precedence check, budget check (≤ 20 min, measured, named runs), `list`, `check [--self-test]`, `github-outputs`, and `run <shard>|all [--measure JSON]`
- [ ] 1.2 Implement `run`: compile and `--list --format terse` cross-check against static discovery, run `scripts/run-widget-tests.sh --headless --retries 1 -- --exact <names>`, assert every `running N tests` line equals the expected count, and in measurement mode write per-test durations, wall time, status, and count to JSON plus `GITHUB_STEP_SUMMARY`
- [x] 1.3 Self-test: extraction rule, precedence (explicit beats module), unassigned/double/stale/redundant failures, budget failures, log parsing of `running N tests` and `test X ... ok` lines
- [x] 1.4 Add `make check-widget-shards` to `check-policy` and `make test-widget-shard WIDGET_SHARD=<name>`; keep `make test`, `make test-widget`, `make test-widget-headless` unchanged
- [x] 1.5 Failing-first proof (scratch, not committed): add an unassigned widget test module and a doubly assigned entry and show `make check-widget-shards` fails; revert

## 2. CI

- [x] 2.1 Replace `widget-tests` in `.github/workflows/ci.yml` with a `widget-shards` table job and a `Widget Tests (<shard>)` matrix job running `widget-shards.py run <shard> --measure` and uploading `widget-measure-<shard>`
- [x] 2.2 `scripts/check-workflow-timeouts.py` passes for the new jobs

## 3. Measurement

- [ ] 3.1 Push the branch, open a draft PR, and collect at least two CI runs of every shard
- [ ] 3.2 Verify from CI logs that the per-shard selected counts sum to the binary's total
- [ ] 3.3 Record the per-shard maxima and run ids in the table; `make check-widget-shards` passes with every shard ≤ 20 min

## 4. Documentation and verification

- [ ] 4.1 Update `.agents/rules/build.md` (CI job list, widget budget paragraph, rules index text if needed), `AGENTS.md`/`.claude/CLAUDE.md` testing and CI text, and the `gtk-testing` skill
- [ ] 4.2 `make check` and `make test` pass locally
- [ ] 4.3 `openspec validate shard-widget-test-ci-job` passes; record deviations in design.md
