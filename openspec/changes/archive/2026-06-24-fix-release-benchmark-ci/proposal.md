## Why

The release benchmark workflow has been cancelled across multiple releases because it runs an unbounded full Criterion report that currently deadlocks inside the content-search benchmark. CI also contains multiple workflows with timeouts above the hard 30-minute ceiling, so release publication and diagnostics need to be redesigned around bounded jobs instead of stretching timeouts.

## What Changes

- Restore the release benchmark workflow to a hard 30-minute maximum and make the report lane deliberately bounded.
- Audit all GitHub Actions job timeouts and restructure or split any workflow job that exceeds 30 minutes.
- Fix the content-search benchmark harness so streaming search results are drained concurrently, matching production backpressure behavior instead of deadlocking after the bounded channel fills.
- Split release benchmark expectations from deeper performance diagnostics: the release asset must be generated from a CI-safe report, while full Criterion coverage remains scheduled/manual until it is proven bounded.
- Tighten release monitoring guidance so failed, cancelled, timed-out, or missing expected workflows are blockers that require repair or a successful replacement run before the release is called green.
- Require verification evidence that the `v*` release benchmark run uploads the benchmark report asset within the budget.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `performance-regression-coverage`: Define bounded CI job budgets, release benchmark reporting, streaming benchmark harness behavior, and separation between release-safe reports and deeper scheduled/manual diagnostics.
- `flathub-publication`: Define release completion semantics for benchmark-report workflows and require successful or superseding release-related GitHub Actions evidence before publication is reported green.

## Impact

- Affected workflows: `.github/workflows/release-benchmark.yml`, `.github/workflows/end-user-smoke.yml`, and any release-related workflow monitoring guidance.
- Affected CI budget policy: all `.github/workflows/*.yml` jobs with `timeout-minutes` above 30, including builder-diagnostics runtime, end-user smoke, release benchmark, Snap, and mutation testing jobs.
- Affected benchmark/reporting code: `scripts/bench-report.sh`, `crates/lushtext-core/benches/benchmarks.rs`, and Makefile benchmark-report targets if needed.
- Affected agent guidance: `.agents/skills/publish-release/**` should reflect the 30-minute hard limit and superseding-success completion model.
- No user data migration, app runtime behavior, or Flatpak package format changes are expected.
