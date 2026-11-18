# Compiler front-end threads (2026-11-17)

Why `scripts/heavy.sh` sizes `CARGO_BUILD_JOBS` from the other heavy commands
running, and gives the checkout's own unoptimized crates rustc's parallel
front end only when a command has the machine to itself. The rule is in
[`.agents/rules/build.md`](../../.agents/rules/build.md), "Compiler
parallelism".

## Upstream state

The parallel front end is unstable in Rust 1.98 (`-Zthreads=N`). Nightly
turned it on by default with 2 threads on 2026-11-14
([announcement](https://blog.rust-lang.org/2026/11/16/enabling-parallel-frontend-on-nightly/));
upstream reports no miscompilation, four open ICEs and eleven reproducibility
issues (output order, not code), and says it "will likely not scale very well
above ~8 threads" ([MCP #1005](https://github.com/rust-lang/compiler-team/issues/1005)).
Threads beyond the first take jobserver tokens, so a rustc never runs more
threads in all than cargo's `-j`.

## Why a wrapper, scoped per crate

- A global `RUSTC_BOOTSTRAP=1` with `rustflags` was rejected: the build
  scripts of anyhow 1.0.104, thiserror 2.0.21 and proc-macro2 1.0.107 probe
  for `RUSTC_BOOTSTRAP` and would compile nightly-only code paths into those
  dependencies, unlike the shipped binary.
- `RUSTC_BOOTSTRAP=<crate name>` lets rustc accept unstable options for that
  one crate (`--crate-name` must match). The wrapper sets it only in its own
  process, for crates under the checkout (no `--cap-lints`,
  `CARGO_MANIFEST_DIR` inside the root, `--crate-name` equal to
  `CARGO_CRATE_NAME`), so no dependency and no build-script probe sees it.
  With and without the wrapper, the 36 dependency build-script `output` files
  were byte-identical, and 199 of 212 compiler calls (every dependency) were
  untouched.
- `RUSTC_WORKSPACE_WRAPPER` is taken (clippy-driver, the mutation lanes'
  passthrough) and hashed into every member's units, so the wrapper sits in
  the `RUSTC_WRAPPER` position, which cargo hashes into nothing, and hands
  each call on to the wrapper cargo would otherwise run (CST's sccache
  wrapper here). Switching it on or off rebuilt 0 units either way.
- `~/.cargo/config.toml` and CST's wrapper are host-owned, and a committed
  `.cargo/config.toml` would reach CI, release, Snap and the staged Flatpak
  tree; only `heavy.sh` installs the wrapper.
- rustc discards a crate's incremental cache when unstable options turn on or
  off (switching 8 → 1 threads without the bootstrap cost 8.7 s on
  `lushtext-core`'s check, 1 → 2 cost 6.1 s, 2 → 8 nothing). The thread count
  itself is untracked, so the wrapper keeps the bootstrap at one thread
  (`-Zthreads=1`): 8 ↔ 1 then cost nothing (2.3–2.5 s each), and one thread
  with the bootstrap ran as fast as the plain compiler (19.8 s against
  19.4 s).

## Measurements

Heavy-work wall times through `heavy.sh`, not timing verdicts; load averages
between 6 and 20 from the runs themselves. S1: `cargo clippy --workspace
--all-targets --all-features -- -D warnings` with the 28 members cold and the
dependencies warm (a fresh worktree). S3: `cargo build --workspace
--all-targets --all-features`, same state (link-heavy). Pairs are interleaved
rounds.

| Jobs / threads | S1 alone | S3 alone | 3 × S3 at once (makespan) |
| --- | --- | --- | --- |
| 4 / 1 (before) | 19.2 s | 52.6 s | 72.7–77.2 s |
| 4 / 8 | 20.1 s | — | 83.1–88.7 s |
| 8 / 1 | 19.9 s | 35.8 s | — |
| 8 / 8 | 13.8–14.3 s | 36.9–40.3 s | 79.7–84.0 s |
| 16 / 1 | 19.8–19.9 s | 37.1–41.3 s | — |
| 16 / 8 | 11.6–12.3 s | 28.1–31.4 s | 83.1–91.0 s |
| 16 / 16 | 11.6–11.7 s | — | — |

- Alone, the threads carry the clippy gain (T=1 stays at ~20 s at any `-j`),
  and the build gain needs 16 jobs; 16 threads gain nothing over 8.
- With three builds at once the machine is saturated and the parallel front
  end is slower even at 4 jobs (+14%), so a contended command keeps the
  previous setting.
- An incremental clippy after touching `lushtext-core` took ~6.8 s in every
  configuration (upstream: 3–5%).
- Peaks of the scope: S3 alone 7.5 GiB at 4 jobs, 8.5–8.7 GiB at 16; the whole
  workspace cold with sccache off, 16 jobs and 8 threads, 9.5 GiB (84 s); a
  cold `cargo build --release -p lushtext` at 16 jobs, 2.6 GiB. All fit the
  default 12G cap.

End to end through the committed `heavy.sh` (no hand-built environment):
the fresh-worktree clippy took 13.9 s and 11.8 s alone (16 jobs, 8 threads)
and 19.9 s and 19.2 s with a second `LOCK` holder (4 jobs, 1 thread). sccache
keys do not depend on `CARGO_BUILD_JOBS` (30 dependency hits at 4 and at 16
jobs, in fresh build directories passed with `--config`; `CARGO_*`
variables holding paths do change the keys). The pinned 1.98.0, 1.99.1 and
1.100.0 compile through the wrapper. One Kani shard
(`core-protected-view-navigation`) verified through the new `heavy.sh`.
Ad-hoc stage scripts ran widget tests under `--mem 6G` and `8G`, below the
measured 16-job peaks, so a cap below 12G keeps 4 jobs.

Artifacts: `.claude/handoffs/rustc-frontend-threads/campaign*.txt` in the main
checkout (gitignored).
