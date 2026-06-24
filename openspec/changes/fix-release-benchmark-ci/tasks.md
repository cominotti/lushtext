## 1. Restore And Enforce CI Time Budgets

- [x] 1.1 Audit every `.github/workflows/*.yml` job and record all `timeout-minutes` values above 30.
- [x] 1.2 Add or update a deterministic workflow timeout policy check that fails when any job timeout exceeds 30 minutes.
- [x] 1.3 Wire the timeout policy check into the repo's local validation and CI path.
- [x] 1.4 Restore `.github/workflows/release-benchmark.yml` to a 30-minute job timeout.
- [x] 1.5 Restructure or split builder diagnostics runtime, end-user smoke, Snap, mutation testing, and any other over-budget workflow jobs so each job stays at or below 30 minutes.

## 2. Fix Streaming Benchmark Deadlocks

- [x] 2.1 Add a small benchmark helper for running `content_search::search(...)` while draining emitted `SearchEvent` values concurrently.
- [x] 2.2 Update all content-search Criterion benchmarks that can emit more events than their channel capacity to use the helper or an explicitly non-blocking collection channel.
- [x] 2.3 Add focused regression coverage that would fail or time out with the old synchronous-producer/post-return-drain harness.
- [x] 2.4 Verify the content-search benchmark family can complete warmup and sample collection locally without deadlocking.

## 3. Bound Release Benchmark Reporting

- [x] 3.1 Choose and document the release-safe benchmark report scope: existing short report mode or a new named release-report scope.
- [x] 3.2 Update `scripts/bench-report.sh`, Makefile targets, or workflow commands so the release benchmark report uses the bounded release-safe scope.
- [x] 3.3 Keep deeper/full Criterion diagnostics available only through scheduled or manual lanes whose individual jobs stay within 30 minutes.
- [x] 3.4 Ensure benchmark report metadata clearly records mode, fixture sizes, commit, runner/environment details, and whether the report is release-safe or deep diagnostic.

## 4. Repair Release Monitoring Guidance

- [x] 4.1 Update `$publish-release` guidance to state that release repair must not raise job timeouts above 30 minutes.
- [x] 4.2 Update release guidance to define green status as successful current workflow responsibilities plus any successful replacement runs, not absence of historical failed runs.
- [x] 4.3 Require final release reports to list failed/cancelled run IDs and the successful replacement run ID when a recovery dispatch supersedes a failed workflow responsibility.

## 5. Validate And Recover The Release Surface

- [x] 5.1 Run local validation for workflow policy, agent docs, benchmark script behavior, and the repo pre-commit lane.
- [x] 5.2 Run the bounded release benchmark report locally or in CI and confirm it completes within 30 minutes.
- [x] 5.3 Dispatch the repaired release benchmark workflow for `v0.5.1` without rewriting the public tag.
- [ ] 5.4 Verify the `lushtext_0.5.1_bench-report.md` release asset exists after the successful replacement run.
- [ ] 5.5 Verify all GitHub Actions workflows for the repair commit and release recovery surface complete successfully or are explicitly superseded according to the new guidance.
