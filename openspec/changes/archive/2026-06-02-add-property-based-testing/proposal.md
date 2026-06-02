## Why

LushText now has a strong mutation-testing lane for deterministic example tests,
but example tests still cover only the cases maintainers remembered to write.
Property-based testing can exercise high-risk pure logic across many generated
inputs while keeping the expensive mutation lane fast and predictable.

## What Changes

- Add a repository-managed property-based testing capability built around
  `proptest`.
- Introduce a dedicated property-test target and local command surface so
  generated-input tests run only when explicitly requested.
- Keep property tests bounded with project-level guidance for case counts,
  generated input sizes, shrinking, timeouts, and regression files.
- Add initial property coverage for pure deterministic logic where invariants
  are clear: inline footnote lowering, search replacement ranges, path rebasing,
  palette ordering, encoding/line-ending helpers, and sidecar identity helpers.
- Keep GTK widget behavior, filesystem watcher timing, and live UI flows outside
  the property-test lane.
- Update the mutation-testing contract so the default mutation lane does not run
  the property-test target unless a future change intentionally opts in a tiny,
  bounded property subset.
- No breaking application behavior changes.

## Capabilities

### New Capabilities

- `property-based-testing`: Defines LushText's property-test targets, bounded
  runtime policy, regression handling, CI/developer commands, and first
  deterministic invariant coverage.

### Modified Capabilities

- `mutation-testing`: Require the default mutation lane to exclude the
  property-test target so mutation runtime does not multiply generated cases by
  mutant count.

## Impact

- Adds `proptest` as a development/test dependency only.
- Affected files likely include root and crate `Cargo.toml` files, `Makefile`,
  `.config/nextest.toml`, CI workflow files, project testing documentation,
  `.agents/rules/build.md`, and a new property-test target under
  `crates/lushtext-core/tests/`.
- The default non-widget nextest, widget harness, benchmark compile, and
  mutation commands should continue to run without property tests unless the
  property lane is explicitly selected.
- CI gains a separate property-test check, initially suitable for pull requests
  with modest case counts and optionally expandable for scheduled/manual deeper
  runs.
