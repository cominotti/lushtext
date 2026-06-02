## Context

LushText currently has deterministic unit, integration, widget, benchmark, and
mutation-testing coverage, but no property-based tests. The newly archived
mutation-testing lane intentionally exercises model, service, and pure-helper
logic through fast non-widget tests. Property tests would add a different kind
of confidence: generated input coverage for invariants that are difficult to
capture with hand-written examples alone.

The key constraint is runtime multiplication. Mutation testing already runs the
same test surface once per mutant. Property tests run many generated cases per
test. Running full property suites inside the default mutation lane would turn
the runtime into `mutants * generated cases`, which is not acceptable for normal
pull-request feedback.

## Goals / Non-Goals

**Goals:**

- Add property-based testing for pure deterministic logic with strong
  invariants.
- Keep generated-input tests bounded, deterministic enough for CI, and easy to
  reproduce after failures.
- Expose a clear local and CI command surface for property tests.
- Preserve the fast default mutation-testing lane by excluding property tests
  unless explicitly opted in later.
- Start with a small high-value property suite rather than trying to property
  test every service.

**Non-Goals:**

- Do not property test GTK widget construction, compositor behavior, live file
  chooser flows, watcher timing, or other display/session-sensitive behavior.
- Do not replace example tests or mutation tests with property tests.
- Do not run large generated-input suites in every default developer command.
- Do not add runtime application dependencies.

## Decisions

### Use `proptest` as a dev dependency

`proptest` provides generated input strategies, shrinking, per-test
configuration, and file-backed regression persistence. It fits Rust unit and
integration test workflows without requiring a custom harness.

Alternatives considered:

- `quickcheck`: smaller surface, but weaker shrinking and less expressive input
  strategy composition for nested Markdown/path/search cases.
- Hand-rolled fuzz loops: easy to start, but failures are harder to shrink,
  persist, and replay consistently.
- Full fuzzing first: valuable later, but higher setup and runtime cost than the
  invariants this change targets.

### Put property tests behind an explicit Cargo feature and test target

Create a `property-tests` feature on `lushtext-core` and a dedicated
`[[test]]` target such as `crates/lushtext-core/tests/properties.rs` with
`required-features = ["property-tests"]`. Local and CI property commands enable
that feature explicitly.

This is stronger than marking tests `#[ignore]`: the property target is not part
of default compile/enumeration unless the property lane asks for it. That keeps
`cargo nextest run --workspace` and cargo-mutants' default nextest runner from
accidentally paying the generated-input cost.

Alternatives considered:

- `#[ignore]` property tests in existing modules: easier, but easier to include
  accidentally and still mixes generated tests into ordinary module layout.
- Test-name filters only: useful as a backstop, but less robust than feature
  gating because command drift can bypass the naming convention.
- A separate workspace crate: strong isolation, but heavier than needed for the
  first pure-helper property suite.

### Keep property runtime bounded by policy and configuration

Property tests should define small generated domains and explicit
`ProptestConfig` values. Pull-request/property CI should use modest case counts
such as 64 or 128. Scheduled/manual deep runs may raise that count after the
base lane proves stable.

Use file-backed regression persistence for minimized failing cases so a property
failure becomes a durable example test input. Generated strings, paths, vectors,
Markdown documents, and replacement lists must have fixed size bounds.

Alternatives considered:

- Default `proptest` case counts everywhere: acceptable for small crates, but
  too easy to overrun feedback time once Markdown/path/search strategies grow.
- Environment-only case counts: flexible, but the repo still needs visible
  defaults so CI behavior is reviewable.

### Keep mutation and property lanes separate by default

The default `scripts/run-mutants.sh` path should continue to run the standard
non-widget nextest surface without enabling `property-tests`. If a future change
adds tiny property tests that are intentionally useful under mutation, it should
do so explicitly with a separate mutation mode or documented opt-in.

This keeps the current mutation contract intact: mutation asks whether ordinary
deterministic assertions catch small code changes, while property testing asks
whether invariants hold across broader generated input space.

Alternatives considered:

- Run all property tests under mutation: too slow and likely to make the
  mutation lane unusable.
- Disable mutation on all modules covered by property tests: loses mutation's
  assertion-quality signal and is unnecessary when test target separation is
  available.

### Start with pure logic where invariants are obvious

The first property suite should focus on deterministic helpers already covered
by mutation testing and example tests:

- Inline footnote lowering: protected ranges remain unchanged, generated labels
  do not collide, and documents without eligible markers return unchanged.
- Search replacement helpers: replacement ordering and range clipping preserve
  intended lines without panicking on bounded random inputs.
- Workspace/path rebasing: paths outside the old root are never rewritten; paths
  inside are rebased predictably.
- Palette result merging: scores remain descending, max limits are respected,
  and left-side tie precedence is stable.
- Encoding/line-ending helpers: identifiers, separators, and basic round-trip
  policies remain stable across generated inputs.
- Sidecar identity helpers: hash generation is deterministic for the same input
  and changes when input bytes change in generated samples.

## Risks / Trade-offs

- Slow or flaky property tests -> Keep generated domains small, case counts
  explicit, and the property lane separate from default nextest/mutation runs.
- Hard-to-debug failures -> Enable regression persistence and document how to
  replay minimized cases.
- Over-broad generated Markdown or path strategies -> Start from small domain
  generators that intentionally model only the syntax each property needs.
- Property tests duplicate example tests -> Treat each property as an invariant;
  keep example tests for named regressions and user-facing scenarios.
- Feature-gated tests can be forgotten -> Add `make test-prop` and a CI job so
  the lane is visible and regularly exercised.

## Migration Plan

1. Add `proptest` as a workspace dev dependency and wire the
   `property-tests` feature/test target.
2. Add a `make test-prop` command and a nextest profile or explicit command that
   enables the property-test feature.
3. Add the first bounded property tests for a small set of pure helpers.
4. Add documentation for property-test scope, case-count policy, regression
   files, and the mutation-lane boundary.
5. Add CI coverage for the property lane with modest case counts.
6. Run the existing validation stack plus the new property command.

Rollback is straightforward: remove the property-test feature/target, dependency,
command surface, CI job, and documentation. Runtime application behavior is not
affected because the dependency is test-only.
