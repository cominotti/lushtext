# Kani shard budgets

How to record a Kani shard's budget and what to do when a shard breaks a
margin. The rules (every harness in exactly one shard and one oracle list, the
margins, never raising `timeout-minutes`, pending records naming an active
change) are in [`.agents/rules/build.md`](../../.agents/rules/build.md),
"Formal Verification (Kani)"; which shards a change runs is decided by
[`.agents/rules/verification-runs.md`](../../.agents/rules/verification-runs.md).

## Measuring

- Measurement mode, `make kani KANI_MEASURE=<json>` (`kani-shards.py run
  --measure`), records each shard's wall time, the peak resident memory of its
  largest descendant (CBMC), and every harness's `Verification Time:`.
- `.github/workflows/kani.yml` always runs in measurement mode and uploads one
  `kani-measure-<shard>` artifact plus a job-summary table.
- Record a new or changed shard's budget (`ci_minutes`, `ci_peak_gib`,
  `measured_in`) from a dispatched run, the larger of two. For a
  pull-request shard the PR's own Kani run is one of them; dispatch
  `kani.yml`, which runs every shard, only for a scheduled shard's figure,
  once the change's final code is pushed.
- Before a branch can push and dispatch, the table must hold some figure: a
  local one is allowed only as `PENDING DISPATCH (owed by <change>)`, naming
  the open OpenSpec change that will replace it.

## Fitting a shard that breaks a margin

In this order:

1. split the shard;
2. try another solver (`#[kani::solver(...)]`), only if the verdict is
   unchanged;
3. only as a last resort reduce a bound, recording it in the programme record
   (`docs/next/formal-verification.md`) and the harness's doc comment together
   with the property statement it weakens.

Never raise `timeout-minutes` instead. A harness that hits the memory cap is
not retried: shrink its bounds first.

## Parallel local runs

Kani cannot run two jobs on one target directory; use a second `--target-dir`
for a parallel local run. Proof strength's Kani arm is serial within one run;
run shards of it in parallel with `PROOF_STRENGTH_ARM=kani
PROOF_STRENGTH_SHARD=k/n` and a distinct `PROOF_STRENGTH_WORKTREE` per shard
(each run is its own 16G admitted scope), and the tests arm once, unsharded,
because its cargo-mutants output directory is shared. It waits for 14 GiB
available memory before each run and caps each Kani run at 12 GiB of address
space (`PROOF_STRENGTH_MIN_AVAILABLE_GIB`, `PROOF_STRENGTH_MEMORY_LIMIT_GIB`).
