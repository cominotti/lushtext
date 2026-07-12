# Benchmark Coverage and Comparison

Use Criterion benchmarks to answer a concrete performance question. The checkout, not this document, owns versions and group names.

## Contents

1. [Discover current coverage](#discover-current-coverage)
2. [Add representative coverage](#add-representative-coverage)
3. [Compare responsibly](#compare-responsibly)
4. [CI guidance](#ci-guidance)

## Discover current coverage

Read:

- workspace `Cargo.toml` for the current Criterion version;
- `crates/lushtext-core/Cargo.toml` for bench target configuration;
- `crates/lushtext-core/benches/benchmarks.rs` for registered groups and fixtures.

Useful discovery commands:

```bash
rg -n 'fn bench_|criterion_group!|criterion_main!' crates/lushtext-core/benches/benchmarks.rs
cargo bench --package lushtext-core --no-run
```

The benchmark file evolves. Do not paste a static “current coverage” table into a finding.

## Add representative coverage

Benchmark the smallest GTK-free boundary that captures the changed cost. Use `services::filesystem::fixture` for filesystem setup and keep setup outside the measured iteration where appropriate. Cover realistic maximum policy inputs as well as common inputs.

For search/index changes, include matches, misses, representative path/name distributions, Unicode where supported, and maximum bounded index size. For file workflows, include relevant byte sizes, encoding branches, durable-write behavior, cancellation checkpoints, and cleanup outside the timed path. Use `BatchSize` when each iteration consumes its input.

Do not benchmark a simplified helper if the optimization changes orchestration, batching, or result installation outside that helper.

## Compare responsibly

Run baseline and candidate with the same toolchain, build profile, machine load, storage class, and fixture. Criterion supports saved baselines; verify the exact CLI accepted by the checked-in Criterion version before scripting it.

Treat absolute times copied from another machine as anecdotes, not gates. Report distributions and effect size, check for noise/outliers, and connect the result to user-visible latency or a policy bound. A statistically visible nanosecond change is not automatically material.

GTK rendering, allocation, and main-loop behavior usually need the widget/proof harness or live runtime tracing rather than a pure Criterion benchmark.

## CI guidance

Do not add a benchmark action, permissions, historical-storage branch, or failure threshold from a template without reviewing repository CI policy and pinning requirements. Benchmark CI can be noisy and supply-chain-sensitive.

Prefer deterministic compile/smoke coverage in ordinary CI unless the repository already owns a calibrated regression system. If proposing a gate, define runner stability, baseline storage, rerun policy, statistical threshold, artifact retention, action SHA pinning, and who can update the baseline.
