## Why

On this workstation, Rust builds are the largest source of disk writes. The
filesystem is btrfs RAID1 on dm-crypt with `flushoncommit`, and heavy writes
stall the whole desktop. During this stage's eight measured builds, each NVMe
member wrote 14.4 GiB in under nine minutes, the members looked saturated, and
`rustc` waited on btrfs commits and writeback throttling
(`evidence/baseline.md`). `~/Workspace` is snapshotted (48 h / 7 d), so output
deleted there keeps its space until the snapshots expire. `~/.cache` is not
snapshotted.

The plan agreed with the CST session assumed that intermediates still lived in
`target/`. They do not. CST's `~/.cargo/config.toml` already sends every
checkout's intermediates to `~/.cache/cargo-build/{workspace-path-hash}`, one
build dir per checkout path. `target/kani` is 80K, and uplifted final artifacts
are reflinks of build-dir extents, so they cost no data writes. Measured, the
problems that remain are narrower and different:

1. **cargo-mutants copy mode leaks a cold build per copy.** Each scratch copy
   has a new path, so it gets a new build dir in `~/.cache/cargo-build`, and
   nothing deletes it. A probe run with `-j 2` left two. In LushText a cold test
   build of the workspace writes 5.2 GB. A local `make mutants-*` run
   (`MUTANTS_LOCAL_JOBS=3`) therefore writes and leaves behind about 15 GB
   before it tests its first mutant, every time. That figure is derived (one
   measured cold build times three copies); task 9.1 measures it.
2. **Each mutant writes 1.55 GB with incremental compilation**, against
   0.61 GB without. Turning incremental off alone would store every mutated
   workspace crate in the shared 25G sccache (28.5 MB per mutant measured, never
   hit again).
3. **A runaway mutant can take the host's memory.** A `list_budget` mutant once
   reached 15 GB. Today only the whole-run scope cap stops it, and that
   discards the run.
4. **Two repository lanes write their builds into the snapshotted checkout:**
   - the proof-strength driver pins `CARGO_BUILD_BUILD_DIR` under
     `target/proof-strength/wt`;
   - `flatpak-builder`'s state dir (`.flatpak-builder/`, once 97 GB, also once
     copied into a 47 GB tmpfs by cargo-mutants) holds the whole release Cargo
     build.
5. **Docs-only commits run a cold all-features Clippy** in a fresh worktree:
   1.70 GB and 39 s, measured. Every programme change's spec stage makes such a
   commit.

## What Changes

- **One repository cache root, keyed per checkout.**
  - New helper `scripts/build-cache.sh` resolves
    `${LUSHTEXT_BUILD_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/lushtext-build}`.
  - It names each checkout's directory `<basename>-<sha256(realpath)[:16]>` and
    records the checkout's path there, so `prune` can remove directories whose
    checkout is gone.
  - Nothing in the repository hardcodes `~/.cache`.
- **Local mutation runs go in place, in a bounded pool of mirror slots.**
  - `make mutants-smoke|diff|full` copy the checkout's tracked and untracked,
    non-ignored files, uncommitted edits included, into one or more slots under
    the cache root. Each slot is used by one run at a time, under an exclusive
    lock. The run is `cargo mutants --in-place`, with the slot's own fixed
    target and build dirs.
  - Parallelism (`MUTANTS_LOCAL_JOBS`, still 3 by default) becomes that many
    slots running `--shard k/N` side by side. cargo-mutants refuses
    `--in-place` together with `--jobs`, so this is the only way to keep
    parallel runs in place.
  - The slots' reports are merged into the usual `mutants.out`. The pool is
    host-wide, so disk use is bounded by the pool size, not by the number of
    worktrees.
  - Slots stay warm across runs, so a run cold-builds nothing after a slot's
    first use, and nothing is left behind.
  - CI modes (`ci-pr`, `ci-full`) are unchanged.
- **Incremental compilation off, and sccache bypassed, only in lanes that own
  their build dir.**
  - The mutation slots and both proof-strength arms set
    `CARGO_INCREMENTAL=0`. The Kani lane does too if the implementation's
    first Kani compile shows it writing an incremental cache in its own
    target-triple tree (design D3).
  - The mutation slots and the proof-strength tests arm also pass workspace
    crates straight to rustc (`RUSTC_WORKSPACE_WRAPPER` pass-through), so
    mutants never reach sccache. Dependencies still use sccache.
  - Every lane keeps incremental unless a quiet-window measurement shows its
    per-unit wall time regressing more than 25% (design D3).
  - `CARGO_INCREMENTAL=1` is never exported.
  - The widget build is deliberately left alone: it shares the checkout's build
    dir, where toggling incremental recompiles every workspace member.
- **A per-test address-space limit in local mutation runs.**
  - A Cargo target runner sets `RLIMIT_AS` on each test process only (rustc,
    the linker and nextest are untouched).
  - The unmutated baseline must pass under it.
  - A mutant caught by an allocation failure is listed separately in
    `mutants.out/address-space-caught.txt` for triage.
- **Proof-strength's worktree moves to the cache root.** Results stay in
  `target/proof-strength/results`. Its tests arm runs cargo-mutants in place in
  that clean disposable worktree, with pinned build dirs, so it no longer leaks
  a build dir per run.
- **The Flatpak state and app dirs move to the cache root** (`--state-dir`).
  This covers `make flatpak`, `flatpak-deps`, `flatpak-install` and the release
  validation build. The Cominotti repository generator gets an explicit state
  dir beside its app dir, under its caller's output dir, because in CI that
  dir is on a different mount from `$HOME` (design D7).
- **The pre-commit hook skips Clippy on docs-only commits, failing closed.**
  - A commit counts as docs-only only when every staged path is on a fixed
    allowlist of documentation locations and the commit is not a merge.
  - A `check-policy` audit fails if Rust sources `include_*!` any allowlisted
    path.
  - Rustfmt and `make check-policy` still run. `make pre-commit`, `make check`
    and CI keep the identical Clippy command.

## Non-goals

- **Moving `target/` itself, `target/kani` or `target/responsiveness`.** Their
  data writes are already reflinks (measured). Snapshot retention of `target/`
  is CST's snapshot policy, and the ask is in
  `.claude/handoffs/reduce-build-disk-writes/cst-asks.md`.
- **The widget build's incremental setting.** Toggling it in the shared build
  dir costs more writes than it saves (`evidence/baseline.md`).
- **Changing CST's build-dir config, the sccache wrapper, or its cache size.**
  These are CST's, and the asks are in the same file.
- **Making `check-terminology` and `check-visual-proof-policy` cheaper on cold
  docs-only commits** (1.52 GB, 105 s together). They read documentation and
  must run. The cost is recorded as a deferred candidate in design.md.
- **CI behaviour.** CI's mutation modes, Clippy command and Kani shards keep
  their semantics. If adopted, Kani's `CARGO_INCREMENTAL=0` applies in CI too. It changes
  no verdict, and the shard budgets are re-read at the programme's final CI
  sweep.

## User-visible impact

None for LushText users: no application code, persisted format, setting,
action or automation surface changes.

For the maintainer:

- **Writes:** about 15 GB less written and left behind per local mutation run,
  and 2.5x fewer bytes written per mutant (if D3 keeps incremental off).
- **Leftovers:** no orphan build dirs, and no Flatpak or proof-strength build
  trees in snapshots.
- **Commits:** cold docs-only commits about 40 s faster.
- **Losses against `main`:**
  - A local mutation run now needs free pool slots. A run that finds none exits
    with status 75 and names the holders, instead of waiting while holding the
    shared lock.
  - Each slot's first use is a cold build.
  - A mutant rebuild without incremental may be slower. D3's pass bar bounds
    that at 25%, or incremental stays on for that lane.

## Capabilities

### New Capabilities

- `build-disk-footprint`: Local build lanes keep their build state in a
  repository cache root outside the snapshotted checkout, keyed per checkout
  or bounded by an exclusive slot pool, write no avoidable intermediates, and
  leave no orphaned build directories behind.

### Modified Capabilities

- `mutation-testing`:
  - local developer mutation commands run in place in disposable mirror slots;
  - a per-test address-space limit is applied, and the mutants it caught are
    reported separately.
- `rust-linting-policy`: the pre-commit hook may skip the Clippy step for a
  commit whose staged paths are all on the fail-closed documentation allowlist.
  The Clippy command stays identical in the Makefile, hook and CI.

## Impact

- **Scripts:**
  - new: `scripts/build-cache.sh`, `scripts/mutants-mirror.py`,
    `scripts/test-address-space-runner.sh`, `scripts/rustc-passthrough.sh`,
    `scripts/precommit-scope.py`;
  - changed: `scripts/run-mutants.sh`, `scripts/proof-strength.py` (and its
    self-test assertions), `scripts/kani-shards.py` (only if its self-test pins
    paths), `scripts/release.sh`, `scripts/generate-cominotti-flatpak-repo.sh`,
    `scripts/test-cominotti-flatpak-repo.sh`, `.githooks/pre-commit`.
- **Makefile:** the mutants, kani, proof-strength and flatpak targets, a new
  `build-cache-prune` target, and a new `check-precommit-scope` target in
  `check-policy`.
- **Docs:**
  - `.agents/rules/build.md` (Mutation Testing, Formal Verification, Flatpak,
    Validation gates);
  - `.agents/rules/verification-runs.md` (Shared-machine agent work);
  - `docs/mutation-testing.md`;
  - `AGENTS.md` (Build Commands, Testing);
  - `README.md` if it names the mutation commands' behaviour.
- **Kani:** none (no proven module touched). **GTK axioms:** none (no toolkit
  behaviour relied on). **Timing:** build-lane wall time only, no application
  timed path.
