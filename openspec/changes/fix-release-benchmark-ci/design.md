## Context

The current release benchmark path has repeatedly produced cancelled GitHub Actions runs. The evidence from `v0.5.1` shows the workflow enters `content_search/literal_10k_files` and remains there until cancellation. The benchmark creates a bounded channel, calls the streaming search service synchronously, and drains the receiver only after the search call returns. Because the fixture produces more events than the bounded channel can hold, the benchmark can block on `tx.send(...)` before Criterion completes warmup.

The release workflow also drifted toward longer timeouts as a recovery attempt. That conflicts with the hard CI rule that no GitHub Actions job may run longer than 30 minutes. Several existing workflows currently exceed that ceiling, so the fix needs both a specific benchmark repair and a broader CI budget pass.

## Goals / Non-Goals

**Goals:**

- Restore every GitHub Actions job timeout to 30 minutes or less.
- Make the release benchmark report lane deterministic enough to complete and upload its release asset within that budget.
- Fix streaming benchmark harnesses so they do not deadlock on production-style bounded channels.
- Keep full/deep performance diagnostics available only where their jobs are explicitly bounded, split, or manual/scheduled with per-job limits.
- Make release status reporting distinguish historical failed runs from the current required green surface and any successful replacement run.

**Non-Goals:**

- No application runtime behavior change is required for this proposal unless implementation discovers that production search can still deadlock under normal UI draining.
- No public tag rewrite or release deletion is included.
- No attempt to make a single full Criterion suite fit under 30 minutes by hiding failures or increasing runner time.
- No new external CI service or paid larger runner is required.

## Decisions

### 1. Fix the benchmark harness before changing report scope

The content-search benchmark should either drain events concurrently while `content_search::search()` runs, or use an unbounded channel when the benchmark is measuring search throughput rather than channel backpressure. Production uses a worker thread plus periodic receiver draining, so a production-like concurrent drain is the preferred harness for benchmarks that retain a bounded channel.

Alternative considered: reduce fixture sizes only. That would make the deadlock less likely but would not fix the broken harness shape.

### 2. Treat release benchmark reporting as a bounded release artifact, not the full performance lab

The tag-triggered release benchmark workflow should produce a useful release asset inside 30 minutes. It can use the existing short report mode, a curated release-report benchmark group, or another bounded report scope. Full Criterion reports belong in scheduled/manual diagnostic lanes only after those lanes are split or bounded so each job stays under 30 minutes.

Alternative considered: keep full mode and raise `timeout-minutes`. Rejected because the hard CI ceiling is 30 minutes and because the current full mode contains a harness bug that longer timeouts cannot solve.

### 3. Enforce the 30-minute ceiling mechanically

Implementation should add or update a workflow policy check so `timeout-minutes` values above 30 fail locally and in CI. Existing workflows above the ceiling must be split, shortened, or reclassified. This prevents future recovery attempts from silently reintroducing 45, 60, 90, or 180 minute jobs.

Alternative considered: rely on reviewer discipline. Rejected because this incident came from exactly that kind of drift.

### 4. Define "fully green" as current required surface plus successful replacements

GitHub keeps failed or cancelled workflow history forever. The release process should not claim those failures never happened. It should require every expected release workflow responsibility to be currently satisfied by a successful run for the relevant commit, tag, or explicit recovery dispatch, and it should report any failed/cancelled runs that were superseded.

Alternative considered: require zero failed runs in history. Rejected because it is impossible after a repair run and would make recovery indistinguishable from rewriting history.

## Risks / Trade-offs

- [Risk] A short release report may be less comprehensive than the old intended full report. -> Mitigation: document the release report scope clearly and keep deeper reports as scheduled/manual diagnostics with artifacts.
- [Risk] Splitting long diagnostic workflows could produce more workflow entries. -> Mitigation: name jobs by lane and keep shared policy checks so each job remains bounded and easy to interpret.
- [Risk] Concurrent draining in benchmarks may measure receiver overhead as well as search work. -> Mitigation: choose the harness intentionally: production-like bounded drain when backpressure is part of the workflow, unbounded collection when measuring raw search throughput.
- [Risk] Release monitoring language could hide previous failures by saying they were superseded. -> Mitigation: require final reports to list failed/cancelled run IDs and the successful replacement run ID that satisfied the same responsibility.

## Migration Plan

1. Revert the release benchmark timeout back to 30 minutes.
2. Add or update workflow timeout policy validation and make it cover every `.github/workflows/*.yml` job.
3. Repair or split workflows that currently exceed 30 minutes.
4. Fix content-search benchmark harness draining and add a focused regression check that would fail on the old bounded-channel deadlock.
5. Bound the release benchmark report scope and verify it uploads the release asset within 30 minutes.
6. Update publish-release guidance to require successful replacement evidence instead of unbounded reruns.
7. Rerun the release benchmark workflow for the affected release tag and verify the report asset.

Rollback is a normal revert before relying on the repaired release workflow. Public release tags must remain immutable; any release asset recovery should use a repaired workflow dispatch against the existing tag.

## Open Questions

- Should the release asset use the existing `bench-report --mode short` output, or should the project define a smaller named release-report scope?
- Should scheduled/manual deep diagnostics be split by benchmark family now, or only after the release report lane is repaired?
- Should the workflow timeout policy live in an existing agent-doc or workflow-check script, or as a new dedicated CI policy script?
