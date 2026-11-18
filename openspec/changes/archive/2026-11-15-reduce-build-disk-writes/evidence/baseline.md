# Baseline (stage 1, 2026-11-14, worktree `reduce-build-disk-writes` at `2c91ccf2`)

Everything here was measured on the sophia host (btrfs RAID1 on dm-crypt,
`flushoncommit`, `compress=zstd:1`) inside the `hypatia-f45` toolbox. Builds ran
through `scripts/heavy.sh` on CPUs 8-23 with `CARGO_BUILD_JOBS=4`.

## Where build output already goes

`~/.cargo/config.toml` is CST's file, not the repository's. It sets
`build.build-dir = "~/.cache/cargo-build/{workspace-path-hash}"` and
`rustc-wrapper = ~/.local/libexec/cst/rustc-wrapper` (sccache, cache
`~/.cache/sccache`, 25G). `~/.cache` and `~/Workspace` are separate subvolumes
(`stat`: dev 105 and dev 104, both inode 256), and only `~/Workspace` is
snapshotted.

- **Intermediates are already outside the snapshots, one build dir per checkout
  path.** `cargo metadata` gives a distinct `build_directory` for each of the 15
  checkouts on the host. `~/.cache/cargo-build` holds 67G. One worktree's build
  dir is 31G, of which 13G is `debug/incremental`, 9.8G `debug/deps`, 6.7G
  `debug/examples` and 0.9G is Kani's `x86_64-unknown-linux-gnu/` tree.
- **Final artifacts stay in `target/` (snapshotted), but as reflinks.**
  `target/debug` is 1.2-1.7G per worktree. About 1.6G of it is `examples/`, the
  ~110 `gtk-lush-axioms` sample binaries of about 16 MB each, uplifted by
  `cargo test`. `filefrag -v` shows the uplifted copy and the build-dir copy on
  the same physical extents, flagged `shared`, with link count 1. So uplifting
  writes no data. The cost is that a snapshot of `target/` keeps an artifact's
  extents alive after the build dir replaces them. That is snapshot policy
  (CST), not a repository change.
- **`target/kani` is 80K.** Kani's compile output already lives in the build
  dir's triple directory. `target/responsiveness` holds one stripped release
  binary. Neither is worth moving.
- **Two repository lanes defeat the build dir:**
  - `scripts/proof-strength.py` pins `CARGO_BUILD_BUILD_DIR` to
    `target/proof-strength/wt/target/kani/kani`, inside the snapshotted
    checkout.
  - `make flatpak` keeps `flatpak-builder`'s state dir in
    `<checkout>/.flatpak-builder`. That dir holds the whole release Cargo build
    of every Flatpak build, and it once reached 97 GB
    (`docs/next/workflow-readability.md`). It is snapshotted and was copied by
    cargo-mutants smoke runs.

## cargo-mutants copy mode leaks a cold build dir per copy

A probe crate under the scratchpad (`cargo mutants -j 2`, 4 mutants) added two
new `~/.cache/cargo-build/<hash>` directories, one per scratch copy in `/tmp`.
cargo-mutants deleted the copies but not the build dirs, because each copy's
random path hashes to a new build dir. The probe's leftovers were removed by
hand afterwards.

A cold `cargo test --workspace --no-run` in a fresh LushText worktree:

| run | wall | process-tree `write_bytes` | build-dir growth | `target/` growth |
| --- | --- | --- | --- | --- |
| incremental (default) | 48 s | 5.22 GB | 5.17 GB | 1.77 GB (reflinks) |
| `CARGO_INCREMENTAL=0`, own build dir | 105 s | 2.64 GB | 3.11 GB | — |

So a local `make mutants-*` run (`MUTANTS_LOCAL_JOBS=3`) cold-builds about
three such trees, writes about 15 GB and leaves about 15 GB behind in
`~/.cache` every time it runs. Wall times are **non-verdict**: the 1-min load
was 7.6-26 while other agents worked.

## Per-mutant rebuild (one real mutation in `model/document_scale.rs`)

`chunk_highlighted` returns `true`, then `cargo test --workspace --no-run`, then
the file is restored and the build repeated:

| mode | wall (non-verdict) | process-tree `write_bytes` | sccache growth |
| --- | --- | --- | --- |
| incremental, mutant | 8 s (load 7.9) | 1.55 GB | 0 |
| incremental, restore | 8 s (load 7.6) | 1.54 GB | 0 |
| `CARGO_INCREMENTAL=0`, mutant | 27 s (load 19.6) | 0.61 GB | **28.5 MB** |
| `CARGO_INCREMENTAL=0`, restore | 27 s (load 19.9) | 0.61 GB | 0 |

- Without incremental, each mutant writes 2.5x less.
- The CST wrapper skips sccache only for incremental compiles. Once
  incremental is off, every mutated workspace crate is stored in the shared
  25G sccache, where it is never hit again and pushes out useful dependency
  entries. At the scope's 6,378 mutants that is about 180 GB of churn.
- The wall times were taken at different loads and are not comparable. The
  implementation measures them in a quiet window (design D3).

## `CARGO_INCREMENTAL` toggling in a shared build dir

Probe crate under the scratchpad. A `CARGO_INCREMENTAL=0` build after an
incremental compile recompiles the crate, and this happened in both probe runs.
An incremental build after a non-incremental compile was `Fresh`. So a lane that
switches incremental off **inside a build dir other lanes also use** recompiles
every workspace member each time it follows a normal build. This rules out
`CARGO_INCREMENTAL=0` for the widget build, which shares the checkout's build
dir with `make test`, Clippy and the dev builds.

## Docs-only commit in a fresh worktree (the pre-commit hook, cold)

| step | wall (non-verdict) | process-tree `write_bytes` | build-dir growth |
| --- | --- | --- | --- |
| `make check-clippy`, cold | 39 s (load 11.9) | 1.34 GB | 1.70 GB |
| `make check-policy`, cold | 105 s (load 10.4) | 0.86 GB | 1.52 GB |
| `make check-clippy`, warm | < 1 s | 0.5 MB | 0 |

On a docs-only commit, Clippy costs something only when its state is stale.
That is the case in a fresh worktree, which every programme change's spec stage
creates, and after merging `main`. The rest of the cold cost comes from
`check-policy`: `check-terminology` builds `lushtext-core` for one integration
test, and `check-visual-proof-policy` builds `cargo-gtk-proof`. Both read
documentation, so they must still run.

## Host IO during this stage

`btrfs-stall-report --since '2026-11-14 20:55:30' --until '2026-11-14 21:04:10' --no-below`
covers the eight builds above, with other agents also running:

- Each NVMe member wrote 14.4 GiB and `user.slice` was charged 18.4 GiB.
- The NVMe members looked saturated (await 10.7 ms, queue depth 8.0).
- Two stall episodes; in the worst (21:03:06-21:03:24) `rustc` waited in
  `btrfs_start_ordered_extent_nowriteback` and in WBT `rq_qos_wait`, with
  commits up to 474 ms.

The measured builds account for about 15.7 GB of the window's writes.

## Not measured here (owed)

- Quiet-window `measure.sh` before/after figures (design D3 and D9).
- System-level IO for one representative `make mutants-diff` before and after,
  requested from the coordinator in the handoff.
