# Per-test address-space limit (task 4.2, design D5)

Measured 2026-11-15 in mutation slot `slot-1` of the real build cache root
(`~/.cache/lushtext-build/mutants/slot-1`), synced from this branch at
`f19f7878` plus uncommitted edits to scripts only. The slot environment was
the mirror mode's: `CARGO_TARGET_DIR` and `CARGO_BUILD_BUILD_DIR` in the slot,
`CARGO_INCREMENTAL=0`, the slot-stable `RUSTC_WORKSPACE_WRAPPER`, and
`CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER=scripts/test-address-space-runner.sh`.
Every command ran through `scripts/heavy.sh` on CPUs 8-23. These are pass/fail
results, not timing verdicts.

## The runner is applied (negative control)

`LUSHTEXT_TEST_ADDRESS_SPACE=64M cargo nextest run --workspace` fails in
nextest's listing pass, which also goes through the target runner:

```
error: creating test list failed
  .../slot-1/build/debug/deps/gtk_lush_axioms-...: error while loading shared libraries:
  libgnutls.so.30: failed to map segment from shared object
```

So a passing run below is a run under the limit, not one where the limit was
never set.

## Baselines

| scope | command | 2 GiB |
| --- | --- | --- |
| default mutation scope (`.cargo/mutants.toml` tests: the workspace, nextest's default filter) | `cargo nextest run --workspace --no-fail-fast`, 4 test threads | **pass**: 2462 passed, 12 skipped, 65.5 s |
| proof-strength tests arm | `cargo nextest run --workspace --features lushtext-core/property-tests --no-fail-fast`, 2 test threads | **pass**: 2557 passed, 12 skipped, 74.4 s |

2 GiB is the first size of the 2, 4, 8, 16 GiB search, so the smallest passing
power of two is 2 GiB, and the default is twice that, **4 GiB**
(`LUSHTEXT_TEST_ADDRESS_SPACE ?= 4G` in the Makefile). It is at most 16 GiB, so
it stops the recorded 15 GB `list_budget` runaway alone: no D5 exception is
needed.

The baseline cargo-mutants runs in each slot before the first mutant is run
under the same limit, so if a future test needs more address space the run
stops before testing any mutant: cargo-mutants exits 4 and `run-mutants.sh` names the limit it ran under.
