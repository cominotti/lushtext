# Property Testing

LushText uses `proptest` for generated-input checks over pure deterministic
logic. Property tests complement example tests and mutation testing: examples
name specific regressions, mutation checks whether assertions catch small code
changes, and properties exercise invariants across many bounded inputs.

## Scope

The property-test target currently covers:

- Markdown inline-footnote lowering
- Replace All text transformation range and ordering rules
- Replace All apply -> immediate undo byte restoration for tiny deterministic files
- Sidecar path rebasing after rename flows
- Command-palette merge ordering and truncation
- Encoding, line-ending, and sidecar hash helpers
- EditorConfig save-time formatting idempotence
- Session and draft JSON round trips

Keep GTK widget construction, compositor behavior, D-Bus or portal state,
filesystem watcher timing, file chooser flows, and live sessions out of this
target. Tiny tempdir-backed service properties are allowed when the workflow is
deterministic, bounded, and does not involve watchers, portals, file choosers, or
live application sessions. If a proposed property needs those runtime surfaces,
cover it in the widget harness or first extract a pure helper with a small
production API.

## Commands

Run the bounded local lane:

```sh
make test-prop
```

That expands to:

```sh
cargo nextest run -p lushtext-core --features property-tests --test properties --profile property
```

Run a deeper opt-in pass:

```sh
make test-prop-deep
make test-prop-deep PROPTEST_DEEP_CASES=1024
```

The property target is guarded by `required-features = ["property-tests"]`, so
default commands such as `cargo nextest run --workspace` and the mutation
wrapper do not compile or execute it unless the feature is explicitly enabled.

## Runtime Policy

The shared property helpers live under
`crates/lushtext-core/tests/properties/support.rs`.

- Default pull-request/local case count: 64 cases per property
- Deep-run knob: `LUSHTEXT_PROPTEST_CASES`
- Deep-run cap: 4096 cases per property
- Shrink limit: 1024 attempts
- Per-case timeout: 10 seconds
- Input bounds: small strings, paths, vectors, byte samples, and tiny temp files

Keep generated domains intentionally small. A useful property should encode a
clear invariant over a compact model, not a broad random end-to-end workflow.

## Regression Files

Minimized failures are persisted at:

```text
crates/lushtext-core/proptest-regressions/properties.txt
```

When `properties.txt` appears or changes, review it like any other regression
artifact. Keep it with the fix that explains the generated case. Future runs
replay persisted cases before trying fresh generated inputs.

If a failure turns out to be a generator mistake rather than a real invariant,
tighten the generator and remove the false regression seed.

## Relationship to Other Gates

- `cargo nextest run --workspace` remains the default non-widget example-test
  lane.
- `scripts/run-widget-tests.sh --headless --retries 1` remains the display and
  GTK behavior lane.
- `cargo bench -p lushtext-core --no-run` still compile-checks benchmark code.
- Mutation testing stays separate by default so generated property cases are
  not multiplied by every mutant.

Future mutation/property overlap must be explicit. If a tiny property is useful
under mutation, add a separate documented mutation mode or narrow opt-in that
passes the feature intentionally; do not add `property-tests` to the default
mutation wrapper or mutation CI baseline.
