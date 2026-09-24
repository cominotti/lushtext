# stateright evaluation model (E8)

**Status: disposable evaluation model, not a maintained property.** It exists
only as evidence for exploration idea E8 of the tool decision recorded in
[`docs/next/formal-verification-quint-vs-tlaplus.md`](../../../docs/next/formal-verification-quint-vs-tlaplus.md).
Kani stays the single maintained formal tool
(`openspec/specs/formal-verification-kani/spec.md`). No build, test, lint,
policy, audit, or CI gate reads this directory, and no production crate
depends on it.

It is a standalone Cargo package: it has its own empty `[workspace]` table and
its own `Cargo.lock`, and the root `Cargo.toml` lists it in `exclude`, so
`cargo deny`, hakari, Clippy, and nextest never see it. `stateright` is
0.31.0, the latest stable release on 2026-09-24.

## What it ports

Target T2, the two-process draft journal (the Kani K8 scenario).
`src/env.rs` ports the harness **environment** of
`crates/lushtext-core/src/services/draft_service/kani_proofs.rs`: `Journal`,
`Editor`, `Window`, `DiskEntry`, `Action`, `previous_session`, `step`,
`step_as`, `startup`, `run_deletion_step`, and the invariants S1 to S4. Every
**decision** is a call to the real
`lushtext_core::services::draft_service::journal_core`, which is compiled from
the repository through a path dependency. There is no second specification
language, so there is no bridge to keep in sync.

The mechanical differences from the Kani harness:

- each `kani::any()` is an explicit choice: a model action is the tuple
  (actor, `Action`, id, fault), plus the `RestoreEnding` chosen inside
  `RestoreApply` when that choice is reached (excluding `Stale` and
  `MissingBody`, as the harness's `kani::assume` does);
- the previous-session bits (entry, body, and backing moved, per id) are the
  set of initial states, and so is the startup fault in the one-process model;
- the inline asserts in `WriteBody` ("a body was written without an entry")
  and `ExecCleanup` (S2) set sticky flags, which the properties
  `NoBodyWithoutEntry` and `S2` report;
- an action that leaves the journal unchanged returns `None`. This matches
  the harness's no-op steps, because the journal states reachable within
  `max_steps` steps are the same.

`src/model.rs` defines the models:

- **one process** is `journal_invariants_hold_under_crashes`: startup with
  either fault, then up to `max_steps` actions;
- **two processes** is `a_second_writer_breaks_the_journal_invariants`: A and
  B start up without faults, then up to `max_steps` actions by either actor.

`tests/k8_traces.rs` replays the two K8 counterexamples decoded in
`docs/next/formal-verification.md` (S1 loss, and a body without an entry in
its 8-action and 6-action forms) and a one-process control.

## Commands

Run these from this directory. `.cargo/config.toml` points the target
directory at the gitignored `build/formal-evaluation/target-rust`. Nothing in
the repository's gates formats or lints this package; run `cargo fmt` and
`cargo clippy` here by hand if you change it. Its own `Cargo.lock` resolves
`lushtext-core`'s dependencies separately from the root lockfile, so the two
can drift.

```sh
T=../../../build/formal-evaluation/target-rust
nice -n 19 env CARGO_BUILD_JOBS=6 cargo build --release
# The E1 tests shell out to quint; see the E1 section for their command.
nice -n 19 env CARGO_BUILD_JOBS=6 cargo test --release --test k8_traces --test seeded_from_tlc
$T/release/lushtext-journal-stateright --max-steps 8 --threads 8 --strategy dfs
$T/release/lushtext-journal-stateright --two-processes --max-steps 8 --threads 8 --strategy dfs
```

The binary's flags are:

- `--two-processes`;
- `--max-steps N` (default 6);
- `--threads N` (default 1);
- `--strategy bfs|dfs` (default `bfs`);
- `--depth-bound`, which leaves the step counter out of state identity.
  This is sound only with `--threads 1 --strategy bfs`;
- `--finish-when-k8`, which stops once S1 and `NoBodyWithoutEntry` both have
  counterexamples. The state count it prints is then partial.

The binary prints the unique state count, whether each property held, and
otherwise its counterexample path as (actor, action, id, fault, ending) steps,
followed by the wall time.

BFS keeps its whole frontier in memory. At 4 two-process steps that is
about 1.2 GB, against about 80 MB for DFS, so use DFS for deep runs.

## E1: Quint Connect replay

Also evidence only, for exploration idea E1. `quint-connect` 0.1.2 and its
helpers are **dev-dependencies**; nothing in `src/` depends on them. The
tests shell out to `quint`, so put the pinned Quint first on `PATH`:

```sh
Q=../../../build/formal-evaluation/tools
export PATH=$PWD/$Q/quint/node_modules/.bin:$PATH QUINT_HOME=$PWD/$Q/quint-home
nice -n 19 env CARGO_BUILD_JOBS=6 cargo test --release \
  --test quint_connect_core --test quint_connect_service --test e1_findings -- --nocapture
```

`QUINT_SEED=n` fixes the traces, and `QUINT_VERBOSE=1` prints every step and
the state diff on a divergence (the diff is printed only at that verbosity).

- `tests/quint_connect_core.rs` (part A) replays `quint run --mbt` traces of
  `k3_8` and `k8_6`, and the `k8_traces_connect` runs, through `src/env.rs`,
  comparing the whole Quint `j` record and `n` after every step.
  `k8_traces_connect` in `../quint/journal.qnt` re-expresses the three
  `k8_traces` runs with a `choice` variable, because `quint test` writes no
  `mbt::` variables.
- `tests/quint_connect_service.rs` (part B) replays the `k3_service`
  instance (one process, fault-free, no backing movement) against the real
  `draft_service` over a tempdir; the driver plays the window. The weighted
  run pins a divergence: `tests/e1_findings.rs` reduces it to five public
  service calls. `E1_CONTENT_STAMP=1` swaps in content-keyed set-aside names
  to explore past it.
- `tests/common/mod.rs` holds the Quint shapes as serde types.
