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
- Shell conformance of the durable-write primitives
  (`shell_conformance_write.rs`, `verify-shell-conformance-against-proven-cores`):
  generated modes, payload sizes (up to 8 KiB) and up to two injected backend
  faults per run, chosen enabled-indexed among the actions a fault-free run of
  the same case makes. Each case runs the real primitive under the conformance
  tap and replays it through the proven core and its shared Kani disk model;
  the exhaustive single-fault and crash enumeration of the same driver runs in
  the unit tests (`services::instrumentation::conformance::write_shell`). Budget: <= 10 s (0.06 s
  measured).
- Shell conformance of the draft journal
  (`shell_conformance_drafts.rs`, `verify-shell-conformance-against-proven-cores`):
  generated enabled-indexed scripts (up to 16 ops in default 64-case runs, up to
  32 ops in `PROPTEST_DEEP_CASES=512` runs) over `INITIAL_LAYOUT_BITS = 8`
  previous-session layouts with `BeforeEffect`, `AfterEffect`, `At(k)` faults,
  and crashes (`FaultDomain::Full`), driving real `draft_service` lockstep
  against `Journal<SingleWindowScope, 3, 1, 1>` (`max_shrink_iters = 256`,
  `max_shrink_time = 45_000` ms). Budgets: <= 30 s for `drafts` (0.10 s
  measured) and <= 2 s for `draft_conformance_strategy_reaches_every_journal_stage`
  (0.00 s measured; 64 fixed-seed model scripts asserting floors
  `startup >= 64`, `write_body >= 12`, `commit >= 6`, `deletion_step >= 50`,
  `restore_apply >= 35`, `exec_cleanup >= 10`). Two-mutant calibration caught
  `preserve_stale_draft_body` skipping `set_aside::keep_copy` at case 2 (0.151 s)
  and `remove_manifest_entry` leaving the entry on disk at case 14 (0.241 s).

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
cargo nextest run -p lushtext-model -p lushtext-services -p lushtext-ui --features property-tests --test properties --profile property
cargo nextest run -p gtk-lush-settle --features property-tests --lib --profile property
```

Run a deeper opt-in pass:

```sh
make test-prop-deep
make test-prop-deep PROPTEST_DEEP_CASES=1024
```

The property targets are guarded by `required-features = ["property-tests"]` (or
`#[cfg(all(test, feature = "property-tests"))]` in `gtk-lush-settle`), so
default commands such as `cargo nextest run --workspace` and the mutation
wrapper do not compile or execute them unless the feature is explicitly enabled.

## Runtime Policy

The shared property helpers live under:

- `crates/lushtext-model/tests/properties/support.rs`
- `crates/lushtext-services/tests/properties/support.rs`
- `crates/lushtext-ui/tests/properties/support.rs`

- Default pull-request/local case count: 64 cases per property
- Deep-run knob: `LUSHTEXT_PROPTEST_CASES`
- Deep-run cap: 4096 cases per property
- Shrink limit: 1024 attempts
- Per-case timeout: 10 seconds
- Input bounds: small strings, paths, vectors, byte samples, and tiny temp files

Keep generated domains intentionally small. A useful property should encode a
clear invariant over a compact model, not a broad random end-to-end workflow.

## Regression Files

Minimized failures are persisted per crate under `proptest-regressions/` (for
example `crates/lushtext-services/proptest-regressions/properties.txt`, plus
module-scoped regression files under `crates/lushtext-model/proptest-regressions/`
and `crates/lushtext-ui/proptest-regressions/`):

```text
crates/lushtext-services/proptest-regressions/properties.txt
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
- `cargo bench -p lushtext-model -p lushtext-services -p lushtext-ui --features test-utils --no-run` still compile-checks benchmark code.
- Mutation testing stays separate by default so generated property cases are
  not multiplied by every mutant.

Future mutation/property overlap must be explicit. If a tiny property is useful
under mutation, add a separate documented mutation mode or narrow opt-in that
passes the feature intentionally; do not add `property-tests` to the default
mutation wrapper or mutation CI baseline.
