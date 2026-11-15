# Verus evaluation models

**Status: disposable evaluation model, not a maintained property.** It exists
only as evidence for the change `evaluate-verus-for-unbounded-proofs`, whose
report is
[`docs/next/formal-verification-verus.md`](../../../docs/next/formal-verification-verus.md).
Kani stays the single maintained formal tool
(`openspec/specs/formal-verification-kani/spec.md`). No build, test, lint,
policy, audit, or CI gate reads this directory, and no production crate
depends on it.

## Layout

Two sibling standalone Cargo packages, each with its own empty `[workspace]`
table and `Cargo.lock`, both listed in the root `Cargo.toml` `exclude`, so
`cargo deny`, hakari, Clippy, nextest, and `make fmt` never see them. Both
build into the gitignored `build/formal-evaluation/`.

- `proofs/` — the Verus copies and proofs, built with **Verus's** pinned rustc
  (1.99.1 for release `0.2026.11.02.8bf68ee`), which the script installs into
  a private `RUSTUP_HOME` under `build/formal-evaluation/tools/` and selects
  explicitly. `src/preview_width.rs` is V1 (calibration);
  `src/journal_core.rs` is the copy of the draft journal's decisions;
  `src/bin/table.rs` writes the copy's side of the drift table.
- `drift/` — the production side of the drift table, built with the
  **workspace's** own toolchain through a path dependency on `lushtext-core`,
  in a detached scratch worktree at the compared commit.
- `table/emit.rs` — the table's inputs and format, included verbatim by both
  sides.
- `mutants/` — one patch per mutant. `v1-*`, `v2-*`, and `v3-*` patches name
  the module they verify and the obligation that must fail; `drift-*` patches
  are the drift check's failing-first changes.
- `kani-step.patch` — the scratch Kani one-step inductive attempt (task 3.1),
  against the commit below; it is never applied to production.
- `in-place-probe.patch` — the scratch in-place `#[verus_spec]` probe of
  `journal_core` (task 6.1).

## The copy commit

Every copy was taken from `85758eaf8fb8190439719dd6926a5b2b3ed21fb0`
(`origin/main`, 2026-11-15). The proofs hold for that commit; `verus-drift`
compares against it by default and against any other commit with
`VERUS_DRIFT_REV`.

## Running it

Local only, never a gate. Run the targets directly, never inside one
`scripts/heavy.sh` call: the script wraps every check and every cargo build in
its own `heavy.sh` call.

```
make formal-evaluation FORMAL_EVAL_TARGET=verus-install   # Verus, z3, Verus's rustc
make formal-evaluation FORMAL_EVAL_TARGET=verus-versions
make formal-evaluation FORMAL_EVAL_TARGET=verus-drift     # the table exchange
make formal-evaluation FORMAL_EVAL_TARGET=verus-v1        # V1 and its mutant
make formal-evaluation FORMAL_EVAL_TARGET=report-data     # the results table
```

Results append to `build/formal-evaluation/runs/results.tsv` with wall time,
peak tree RSS, and the load at start and end.
