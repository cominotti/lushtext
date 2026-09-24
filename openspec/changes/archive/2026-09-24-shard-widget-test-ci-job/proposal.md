## Why

The CI `Widget Tests` job runs all 1,292 headless widget tests serially in one
job and now takes 15–30 minutes on `ubuntu-latest`; runs on `main` for
`b2298918` and `14bf322a` were cancelled at the 30-minute `timeout-minutes`
cap. It is not a hang: three tests whose document sizes are the policy
thresholds under test take about 13 minutes on the runner by themselves, and
they must not be shrunk. The cap is a repository policy
(`scripts/check-workflow-timeouts.py`), and `.agents/rules/build.md` already
names sharding the widget binary, not a longer timeout, as the next step.

## What Changes

- Add `scripts/widget-shards.py`, the single source of truth for widget-test
  shards (mirroring `scripts/kani-shards.py`): a table assigning every widget
  test to exactly one shard by module, with explicit per-test overrides used to
  balance the three threshold-sized tests, and each shard's measured CI runner
  duration.
- Add `make check-widget-shards` (part of `make check-policy`, so `make check`
  and the pre-commit gate) that statically discovers every widget test the same
  way `crates/lushtext/build.rs` registers it and fails when a test is
  unassigned, doubly assigned, a table entry is stale, or a shard's recorded
  runner measurement is missing or over its 20-minute margin.
- Replace the single `Widget Tests` CI job with a `Widget Shard Table` job plus
  a matrix `Widget Tests (<shard>)` job per shard, built from the table. Each
  shard job verifies the compiled binary's `--list` matches the static
  discovery, runs exactly its shard through `scripts/run-widget-tests.sh
  --headless --retries 1 -- --exact …`, asserts the harness selected the
  expected count, and uploads a `widget-measure-<shard>` JSON artifact plus a
  job-summary table with wall time and per-test durations.
- Add `make test-widget-shard WIDGET_SHARD=<name>` for local reproduction of a
  shard. `make test`, `make test-widget`, and `make test-widget-headless` keep
  running the whole suite unchanged.
- The headless mutter harness, per-test retry, whole-run `--retries 1`, FLAKY
  reporting, and warning classification are unchanged.
- Documentation sync: `.agents/rules/build.md`, `AGENTS.md`, and the
  `gtk-testing` skill describe the sharded lane.

## Capabilities

### New Capabilities
- `widget-test-ci-sharding`: how the CI widget lane is split into shards from
  one checked table, how coverage of the whole suite is proven, and the
  measured per-shard runner budget.

### Modified Capabilities
<!-- None: the proof harness's selection and retry behaviour is unchanged. -->

## Impact

- New: `scripts/widget-shards.py`.
- Changed: `.github/workflows/ci.yml` (widget job → table job + matrix),
  `Makefile` (`check-widget-shards`, `test-widget-shard`, `check-policy`).
- Docs: `.agents/rules/build.md`, `AGENTS.md`, `.claude/CLAUDE.md` mirror if
  applicable, `.agents/skills/gtk-testing/`.
- CI check names change from `Widget Tests` to `Widget Tests (<shard>)`. The
  only repository ruleset (23911547) requires `Kani Proof Harnesses
  (widgets-geometry)`, so no required check is broken.
- No production code, dependency, or widget-test change.
