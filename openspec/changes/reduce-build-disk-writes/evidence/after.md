# After (stage 3, 2026-11-15, branch `reduce-build-disk-writes`)

Measured on the same host as `baseline.md`, CPUs 8-23. The 1-min load stayed
between 4.8 and 31 for the whole stage (other agents' work and a game), so
**every wall time here is non-verdict**. The byte figures are per process tree
(`/proc/<pid>/io` after the tree is reaped, `io-wrap.py` in the handoff dir)
and do not depend on what else ran.

## Mutation lane (task 9.1)

Fixed focused scope: `MUTANTS_RE='chunked_threshold_with|markdown_chunked_cap'`
over the smoke file `crates/lushtext-core/src/model/document_scale.rs`, 13
mutants: 6 caught, 6 missed, 1 unviable in every round (`mutation-scope.txt`
lists them). Four rounds interleaved main, branch, main, branch.

- **main**: `origin/main`'s `make mutants-smoke` recipe (copy mode,
  `MUTANTS_JOBS=3`, 4 build jobs, 4 test threads, a 36G scope), run from this
  worktree, whose copy path is unchanged.
- **branch**: `make mutants-smoke` (three mirror slots, warmed by one
  unmeasured run, incremental kept per D3).

The coordinator's convoy note (`.claude/handoffs/MEASURE-QUEUE.md`) stopped
this stage from taking exclusive measurement locks, so these rounds ran
through `scripts/heavy.sh`; the wall-time verdict is queued there. The branch
rounds waited 20-30 min for memory admission behind other agents before
starting, so their whole-run wall time is not comparable; the slots' own wall
times are.

| round | tested in | process-tree `write_bytes` | build dirs left in `~/.cache/cargo-build` | per-mutant build / test, median |
| --- | --- | --- | --- | --- |
| main 1 | 6 min | 31.77 GB | 3 (8.04 + 6.86 + 6.74 GB) | 13.9 s / 5.7 s |
| branch 1 | slots 90, 154, 323 s | 21.48 GB | **0** | 8.0 s / 3.0 s |
| main 2 | 6 min | 31.73 GB | 3 (6.86 + 7.91 + 6.87 GB) | 12.8 s / 5.7 s |
| branch 2 | slots 123, 192, 365 s | 21.38 GB | **0** | 9.5 s / 5.2 s |

Against the D9 bars:

- **Zero new `~/.cache/cargo-build` entries on the branch: met.** The
  listing is shared with every agent, so each new directory was classified
  by the source paths its build-script outputs name (`leaked.sh` in the
  handoff dir). Every directory naming
  `/tmp/cargo-mutants-reduce-build-disk-writes-*` was born in a main round,
  three per round; none names a slot or this checkout. The others name other
  worktrees, or are release builds no mutation run makes.
- **Branch `write_bytes` below main's in every round pair: met**, by about a
  third (21.5 against 31.8 GB, 21.4 against 31.7 GB). With incremental kept
  (D3), each warm mutant still writes about 1.5 GB (incremental cache and the
  relinked test binaries); what is gone is the three cold trees per run.
- **sccache growth below 1 MB on the branch: not attributable by size.** The
  cache grew 33 MB and 1.1 GB over the branch rounds' windows, which include
  20-30 min of other agents' builds. Attributed structurally instead: in the
  warm branch round every `Compiling` line is a workspace member
  (`lushtext-core`, `lushtext`; no dependency compiled), and workspace members
  reach rustc through `rustc-passthrough.sh`, so this lane stored nothing in
  sccache. Main stored the three cold copies' misses.
- **Branch median wall time at most 125% of main's: non-verdict, queued.** The
  longest slot finished in 5.4 and 6.1 min against main's 6 min, so the
  branch is not slower here; the quiet-window verdict is in
  `MEASURE-QUEUE.md`.

**Correction to `baseline.md`.** It estimated that a local `make mutants-*`
run "writes about 15 GB and leaves about 15 GB behind". Measured here, one
`make mutants-smoke` on `main` writes 31.7 GB and leaves three build dirs of
6.7-8.0 GB, **21.6 GB per run**. The six left by this stage's two main rounds
were deleted afterwards (`leaked-deleted.txt` in the handoff dir: each one's
build-script outputs name only a vanished `/tmp/cargo-mutants-*` copy).

## Other lanes

| lane | before | after |
| --- | --- | --- |
| proof-strength build state | `target/proof-strength/wt` in the snapshotted checkout | `<checkout-cache>/proof-strength/wt` (`kani` 0.9 GB, `tests-build` 6.6 GB), nothing under `target/` but `config/` and `results/` (lane case 5.3) |
| proof-strength tests arm | copy mode, `--jobs`, one cold tree per job | `--in-place` in the worktree, one warm build dir |
| Flatpak builder state | `<checkout>/.flatpak-builder` (once 97 GB), snapshotted | `<checkout-cache>/flatpak-builder` (48 MB after a build), app dir beside it (lane case 7.2) |
| `make kani` | incremental cache in the triple tree (22 MB per widgets compile, 355 MB per `lushtext-core`) | `CARGO_INCREMENTAL=0`, no cache (D3) |
| docs-only commit | full gate: rustfmt, Clippy, policy audits | rustfmt and policy audits, Clippy skipped (lane case 8.3) |

## Owed

- The quiet-window wall-time verdicts: task 9.1's four rounds and task 9.2's
  three remaining proof-strength Kani rounds (`MEASURE-QUEUE.md`).
- The system-level `btrfs-stall-report` figure for one `make mutants-diff`,
  main against branch (`host-io-request.md` in the handoff dir).
