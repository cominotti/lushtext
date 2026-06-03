## Recommendation

Adopt the internal filesystem abstraction. The best fit for LushText is not raw
`rustix` at application call sites, but a readable `services::filesystem`
boundary with `rustix` kept private inside the backend. That gives the project
the safety benefits that motivated the change — descriptor-oriented directory
operations, explicit metadata handling, parent-directory sync, and low-level
Unix interop isolation — without making UI, service, test, or benchmark code
read like syscall plumbing.

The implementation follows that recommendation:

- `services::filesystem` is now the public filesystem boundary for reads,
  metadata, traversal, mutation, sidecar helpers, durable writes, and fixtures.
- `services::filesystem::sys` owns direct `rustix`, raw `std::fs`, and Unix
  extension usage for the private backend.
- `lushtext-build-support::filesystem` owns build-script filesystem access so
  Cargo build scripts do not preserve raw `std::fs` leftovers.
- `services::durable_write` remains private to the services module and is
  surfaced through `services::filesystem::write`.
- Production services, UI workflows, integration tests, widget tests,
  properties, fuzz replay, and benchmarks now use the boundary or fixture
  helpers instead of direct raw filesystem calls.
- Repository guidance, rules, skills, and the new
  `scripts/check-filesystem-boundary.sh` audit encode the no-leftovers rule.

## Widget Harness Safety Follow-Up

While validating this change, a plain workspace test invocation could run the
widget harness against the developer's live desktop session. That is now blocked
as a hard safety property:

- `scripts/run-widget-tests.sh` has no native/live-display mode.
- `make test-widget` and `make test-widget-headless` both use the private
  headless runner.
- Plain `cargo test --test widget` remains Cargo-visible by default, but
  non-list executions self-supervise into a private `mutter --headless` session
  before GTK initializes.
- `.config/nextest.toml` excludes the `widget` binary from nextest's default
  non-widget lane so `make test` does not duplicate widget coverage or shard it
  into many compositor sessions; the explicit widget lane owns the suite.

## Validation Evidence

Completed after the current implementation state:

- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed on the current tree. The default Cargo widget
  target self-supervised into private `mutter --headless`; the run completed the
  52 integration tests, 604 widget tests, 496 core tests, and workspace doctests.
- `make test-prop` passed 19 property tests.
- `make fuzz-corpus-replay` passed 3 committed corpus replay tests.
- `make test-widget-headless` passed 604 widget tests through the explicit
  headless runner with no flaky summary.
- The previously flaky
  `window::test_local_history_browser_collapses_and_restore_can_be_undone` was
  fixed by waiting for the first modified-transition baseline before selecting
  the browser row, then rerun successfully 5 times in isolation.
- `./scripts/check-filesystem-boundary.sh` passed.
- `cargo hakari generate` completed after the build-support crate addition.
- `make cargo-sources` completed and regenerated Flatpak cargo sources from the
  current lockfile.
- `make check-agent-docs` passed, including the filesystem-boundary audit.
- `openspec validate adopt-internal-filesystem-abstractions --strict` passed.
- `openspec validate --changes --strict` passed.
