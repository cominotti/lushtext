## Context

This is the repository side of a plan agreed with the sophia-btrfs session,
which owns CST: the host's btrfs tooling, `~/.cargo/config.toml`, the sccache
wrapper and the snapshot policy. The measured baseline is in
`evidence/baseline.md`, and every figure below comes from it. Several programme
changes now run at the same time in separate worktrees, so any build directory
the repository shares across runs must be keyed per checkout or held under an
exclusive lock.

The facts that shaped the design:

- **Intermediates are already outside the snapshots.** CST's
  `build.build-dir = ~/.cache/cargo-build/{workspace-path-hash}` keeps each
  checkout's build dir separate.
- **Final artifacts in `target/` are reflinks**: `filefrag` shows the extents
  shared with the build dir. They cost snapshot retention, not data writes.
- **cargo-mutants copy mode leaks one cold build dir per scratch copy**
  (probe: `-j 2` left two), because each copy's random path hashes to a new
  build dir.
- **cargo-mutants 27 refuses `--in-place` together with `--jobs`** (checked).
- **Per mutant, writes are 1.55 GB with incremental and 0.61 GB without.**
  Without incremental, the CST wrapper sends the mutated crate to sccache
  (+28.5 MB per mutant).
- **An `inc=0` build after an incremental compile recompiles; the reverse is
  `Fresh`.** So toggling incremental in a shared build dir recompiles workspace
  members.
- **A cold docs-only pre-commit costs 1.70 GB / 39 s for Clippy and 1.52 GB /
  105 s for `check-policy`.** A warm Clippy costs nothing.

## Goals / Non-Goals

**Goals:**

- No repository lane leaves build directories behind.
- No repository lane writes build trees into the snapshotted checkout.
- No avoidable cold builds or incremental caches in the mutation lanes.
- A runaway test process is stopped alone.
- Docs-only commits skip the only gate step that cannot be affected by them.
- CI keeps every verdict.

**Non-Goals:**

- Moving `target/`, `target/kani` or `target/responsiveness`.
- Touching CST's config or wrapper.
- The widget build's incremental setting.
- Making the documentation-reading policy checks cheaper (deferred, see the
  end).

## Decisions

### D1. One repository cache root, keyed per checkout

`scripts/build-cache.sh` (bash, `--self-test`) is the only place the location is
computed:

- `root` prints
  `${LUSHTEXT_BUILD_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/lushtext-build}`.
- `checkout [<path>]` prints `<root>/checkouts/<basename>-<sha256(realpath)[:16]>`.
  It creates the directory with a `CACHEDIR.TAG` and a `checkout-path` file
  holding the realpath.
- `prune [--dry-run]` removes checkout directories whose recorded path is no
  longer a directory, and never touches anything else.
- `make build-cache-prune` runs it.

The Makefile, `run-mutants.sh` and `proof-strength.py` all call this helper;
none computes the path itself.

- **Why not reuse cargo's `{workspace-path-hash}` build dir?** It exists only
  on CST hosts, its location is CST's to change, and the repository cannot
  clean it. Lanes that own their state need a location the repository both
  controls and cleans.
- **Why basename plus hash?** It is readable in `du` output and collision-free
  across checkouts with the same name.
- **Rejected: hardcoding `~/.cache`.** It breaks `XDG_CACHE_HOME` users and CI
  containers.

### D2. Local mutation runs use an exclusive pool of in-place mirror slots

The local modes of `scripts/run-mutants.sh` (`smoke`, `diff`, `full`) run in
**mirror slots** when `MUTANTS_MIRROR=1`. The Makefile's `mutants-*` targets
set it. The CI modes (`ci-pr`, `ci-full`) and `list` are unchanged.

**Layout and lock.**

- Slots live at `<root>/mutants/slot-<k>`, for k = 1..`MUTANTS_POOL_SIZE`
  (default 3, the current `MUTANTS_LOCAL_JOBS`).
- Each slot has `lock`, `src/`, `build/` and `target/`, and its own
  `CACHEDIR.TAG`.
- A run takes up to `MUTANTS_LOCAL_JOBS` slots with `flock -n` on their `lock`
  files. It never waits: waiting there would mean waiting for other work while
  holding heavy.sh's shared `LOCK`, which `verification-runs.md` forbids and
  which can starve a pending measurement.
- If some slots are free, it runs on those and prints how many. If none are
  free, it exits 75 and names each holder: the PID and the checkout recorded in
  the slot's `owner` file.

**Sync (`scripts/mutants-mirror.py sync <checkout> <slot-src>`, with
`--self-test`).**

- The file set is `git ls-files -z --cached --others --exclude-standard`, minus
  tracked files deleted in the working tree. That is the set cargo-mutants'
  `gitignore = true` copy takes, uncommitted edits included.
- A file whose bytes differ is rewritten, which gives it a new mtime so Cargo
  sees it as changed. Identical files are left untouched, so their mtimes and
  Cargo's freshness survive.
- Files not in the set are deleted, as are directories left empty.
- An interrupted in-place run can leave a mutated file in a slot. The next sync
  finds its bytes differ and restores it, so a slot heals itself.
- No `rsync` dependency.
- `.git` is never copied, which matches cargo-mutants' `copy_vcs = false`
  default.

**Run.**

- Each slot runs `cargo mutants --in-place` from its `src/`, with:
  - `CARGO_TARGET_DIR=<slot>/target` and `CARGO_BUILD_BUILD_DIR=<slot>/build`,
    explicit and independent of CST;
  - the D3, D4 and D5 environment;
  - the existing jobs, test-thread and timeout knobs.
- With N slots, the i-th slot taken (i = 0..N-1) also gets `--shard i/N`.
  cargo-mutants shards are 0-based. A user `MUTANTS_SHARD` combined with N > 1 is refused, because the
  two would compose ambiguously.
- `diff` mode writes `git.diff` in the checkout, as today, and passes it by
  absolute path.

**Output.**

- Each slot writes `--output <checkout>/mutants.out.slots/slot-<k>`.
- `mutants-mirror.py merge` then rotates the checkout's `mutants.out` to
  `mutants.out.old`, as cargo-mutants does. It writes a `mutants.out` with the
  same files a single run produces: the `*.txt` outcome lists concatenated, the
  `log/` and `diff/` entries unioned (names are unique per mutant), and
  `outcomes.json` with its outcome list concatenated and its counters summed.
  The implementation pins this format against cargo-mutants 27.0.0's real
  output.
- With N = 1 the slot writes `<checkout>/mutants.out` directly.

**Exit status.** A slot exit outside {0, 2, 3} wins (the first such). Otherwise
the merged status is 2 if any slot missed mutants, else 3 if any timed out, else
0. The implementation confirms cargo-mutants' own precedence and matches it.

**Rationale.**

- A warm slot has nothing cold to build, so the copy mode's roughly 15 GB of
  per-run cold builds, and the leak, both go away. Only a slot's first use is
  cold.
- Shards keep the parallelism today's `--jobs 3` gives.
- A host-wide pool bounds disk by pool size, not worktree count: about
  3 × 3-5 GB. Per-checkout slots, given the 15 checkouts already on the host,
  would not be bounded.
- Two runs from different checkouts in one slot simply resync. Only the
  workspace members that differ rebuild, and the dependencies stay warm.

**Rejected alternatives.**

- **Copy mode with a per-run build root deleted at exit.** It fixes the leak
  but keeps three cold 5.2 GB builds per run.
- **One fixed build dir shared by every copy.** Cargo's build-dir lock
  serialises the jobs' builds, and members are keyed by the copies' random
  paths, so they pile up.
- **A blocking wait for a slot.** It violates the lock protocol, as above.
- **Serial `--in-place` in a single mirror.** It loses parallelism. At the
  measured 27 s rebuild plus the test phase, a 50-mutant `mutants-diff` takes
  roughly 3x longer.

### D3. `CARGO_INCREMENTAL=0` only in lanes that own their build dir, behind a pass bar

The mutation slots (D2), both proof-strength arms (D6) and the Kani lane set
`CARGO_INCREMENTAL=0`.

- Kani builds in the build dir's `x86_64-unknown-linux-gnu/` tree, which
  ordinary builds never use. Its 320 MB `incremental/` there is pure write cost
  for a lane that runs once per final code.
- Nothing exports `CARGO_INCREMENTAL=1`. A lane that keeps incremental leaves
  the variable unset.
- The widget build is excluded: it shares the checkout's build dir with
  `make test` and Clippy, and the toggle probe shows that each switch
  recompiles every member.

**Pass bar, fixed before measuring.** A lane keeps `CARGO_INCREMENTAL=0` unless
its median per-unit wall time regresses by more than 25% against incremental in
the D9 protocol. The unit is the build phase plus test phase of one mutant,
from `outcomes.json`, for mutation runs. It is one package compile under
`cargo kani` for Kani.

- On a regression, that lane keeps incremental, still in its own dir. It may
  then still write less than `main`, because no cold builds are left.
- The decision is recorded in this file with the figures.
- The 25% bound reflects the host priority: write-heavy builds stall the
  desktop for everyone (baseline: saturated NVMe members, WBT throttling), and
  the 2.5x write reduction is worth a bounded slowdown of a background lane.

### D4. Workspace crates bypass sccache in the mutation lanes

With incremental off, CST's wrapper starts handing workspace crates to
sccache. A mutated crate is a guaranteed miss and is then stored (28.5 MB per
mutant), pushing useful dependency entries out of the shared 25G cache.

The mutation slots and the proof-strength tests arm therefore set
`RUSTC_WORKSPACE_WRAPPER=scripts/rustc-passthrough.sh` (a one-line
`exec "$@"`). Cargo then calls `rustc-wrapper rustc-passthrough.sh rustc ...`
for workspace members. CST's wrapper documents that it executes directly
whenever its first argument is not `rustc`. Dependencies keep sccache, which
matters for a slot's first, cold build.

- On hosts without a `RUSTC_WRAPPER` (CI, hosts without CST) the passthrough
  is a no-op.
- An explicit CST knob is asked for in `cst-asks.md`. When CST provides it, the
  repository switches to the knob in the change that adopts it.
- **Rejected: `CST_SCCACHE=0` for the whole lane.** A slot's first build would
  compile every dependency from scratch.

### D5. A per-test address-space limit through a Cargo target runner

The local mutation slots and the proof-strength tests arm set
`CARGO_TARGET_<HOST>_RUNNER=scripts/test-address-space-runner.sh`, with the
host triple taken from `rustc -vV`. The runner sets
`ulimit -v <LUSHTEXT_TEST_ADDRESS_SPACE>` and `exec`s the test binary.

- nextest runs one test per process through the target runner, so every test
  process gets its own `RLIMIT_AS`, and its listing pass does too.
- rustc, the linker, build scripts, nextest itself and cargo-mutants are not
  limited.

**The value.** The implementation finds the smallest power-of-two GiB at which
the unmutated baseline of the full mutation scope passes. The default is twice
that, and at least 4 GiB. It is recorded in the Makefile with the measurement
and checked against the `property-tests` baseline of the proof-strength tests
arm, which may need more. A baseline failure under the limit aborts the run
visibly; cargo-mutants already stops on a failing baseline.

**Reporting.**

- After a run, `mutants-mirror.py merge` (or a one-slot pass) lists every
  mutant whose log contains Rust's allocation-failure message,
  `memory allocation of N bytes failed`, in
  `mutants.out/address-space-caught.txt`, and prints the count.
- Such mutants are still caught. The unmutated code passes under the same
  limit, so unbounded memory growth is a behaviour change the tests detected.
- They are listed separately so triage can confirm each one, the way
  `docs/mutation-testing.md` asks for other non-assertion kills.
- This replaces "the run hit its scope cap, discard it" for runaway tests. The
  whole-run scope cap stays, for builds.

**Rejected alternatives.**

- **`ulimit -v` around the whole run.** It limits rustc and lld, which reserve
  far more address space than they use, so it produces false failures.
- **nextest wrapper scripts.** Their support depends on the nextest version;
  the target runner works everywhere.
- **cgroup `memory.max` per test.** That needs a scope per process, which is
  too costly per test.

### D6. Proof-strength: worktree in the cache root, tests arm in place

- **The default `WORKTREE` becomes `<checkout-cache>/proof-strength/wt`.** The
  Kani arm's pinned `CARGO_BUILD_BUILD_DIR` (`wt/target/kani/kani`, read by
  `reach_tokens`) moves with it.
- **Results stay in `target/proof-strength/results`.** They are durable,
  small, and documented there.
- **`worktree_owned` accepts the cache location.** Its owner-marker check is
  unchanged.
- **A relative `PROOF_STRENGTH_WORKTREE` still resolves against the current
  directory.**
- **The tests arm runs `cargo mutants --in-place` inside the clean worktree.**
  The worktree is disposable and at `--rev`, so the existing "Developer runs a
  local in-place mutation command" scenario still holds.
  - It uses `CARGO_BUILD_BUILD_DIR=wt/target/tests-build` and
    `CARGO_TARGET_DIR=wt/target/tests`.
  - It gets D3's `CARGO_INCREMENTAL=0`, D4's passthrough and D5's runner.
  - It ends the per-run build-dir leak, and its copy of the worktree.
- **The self-test assertions** in `proof-strength.py` (for example
  `pinned == "/wt/target/kani/kani"`) are updated to the new layout.

### D7. Flatpak builder state and app dirs in the cache root

- **The Makefile:**
  - `FLATPAK_STATE_DIR ?= <checkout-cache>/flatpak-builder`, passed as
    `--state-dir`;
  - `FLATPAK_BUILD_DIR ?= <checkout-cache>/flatpak-app`.
- **`scripts/release.sh`** defaults its build dir to
  `<checkout-cache>/flatpak-release-app` and passes the same state dir.
- **`scripts/generate-cominotti-flatpak-repo.sh`** passes the state dir. Its
  output directory stays the caller's.
- **The two dirs are on one subvolume.** `flatpak-builder` checks its cache out
  into the app dir with hardlinks, and a cross-subvolume hardlink fails with
  `EXDEV`.
- **Nothing deletes the existing `.flatpak-builder/`.** It is user state.
  `build.md` names it as safe to remove once the new location is in use.
- **Rationale.** The state dir holds the whole release Cargo build. It once
  reached 97 GB, and it broke cargo-mutants smoke runs by being copied.

### D8. The pre-commit hook skips Clippy only for staged paths on a documentation allowlist

`scripts/precommit-scope.py` (with `--self-test`) has two commands.

**`docs-only`** exits 0 only when all of the following hold:

1. `git rev-parse -q --verify MERGE_HEAD` fails, so the commit is not a merge.
2. `git diff --cached --name-only --no-renames -z` lists at least one path. It
   lists both sides of a rename, and deletions too.
3. Every path is on the allowlist:
   - `docs/**`, `openspec/**`, `.claude/**`, `.agents/**`;
   - `*.md` at the repository root.

   Not allowlisted, by design: `*.md` under `crates/`, `fuzz/` or
   `workspace-hack/`, `.github/**`, `resources/**`, `data/**`, `build-aux/**`
   and `po/**`.

In every other case it exits 1, and so does any git error. An empty commit runs
the full gate.

**`check-includes`** fails when a Rust source or `build.rs` under `crates/`,
`fuzz/` or `workspace-hack/` names an allowlisted path. That means an
`include_str!`, `include_bytes!` or `include!` argument, or a `build.rs` string
naming `docs/`, `openspec/`, `.claude/`, `.agents/` or a root `*.md`. Without
this audit, a future `#![doc = include_str!("../../../README.md")]` would
silently make a docs edit a Clippy input; doc lints such as
`doc_lazy_continuation` warn by default. It runs as
`make check-precommit-scope` inside `make check-policy`.

**The hook:**

```sh
if ./scripts/precommit-scope.py docs-only; then
    echo "Docs-only commit: rustfmt + policy audits (Clippy skipped: no Rust build input staged)"
    exec make pre-commit-docs
fi
exec make pre-commit
```

- `pre-commit-docs: check-fmt check-policy` is the existing gate minus
  `check-clippy`.
- `make pre-commit`, `make check` and CI's Clippy command are unchanged, so the
  hook enforces exactly what it enforces today for any commit outside the
  allowlist.

**Rejected alternatives.**

- **Putting the skip in `make pre-commit`.** `make check` would silently skip
  Clippy too.
- **A denylist of Rust extensions.** It fails open on new build inputs.
- **Skipping `check-policy` as well.** Its terminology, agent-docs and
  automation-docs checks read exactly these files.

### D9. Measurement protocol

The pass bars are fixed above, before any measurement. Every timed run goes
through `scripts/measure.sh` and counts only when its recorded 1-min load is
below 4; otherwise it is labelled non-verdict and retried. Write volume is
attributed per process tree (`/proc/<pid>/io` `write_bytes`, which includes
reaped children), plus the growth of the lane's dirs and of sccache. It is
therefore independent of other agents, and does not need a quiet window to be
valid.

1. **Mutation lane.** A fixed focused scope: `MUTANTS_RE` over the
   `document_scale.rs` smoke file, about 10 mutants, written to an evidence
   diff. Four rounds interleaved: main (copy mode, `--jobs 3`), branch (slots,
   3), main, branch. Branch slots warmed by one unmeasured run.
   - Recorded: wall time, `write_bytes`, new `~/.cache/cargo-build` entries
     (must be 0 on the branch), `<root>` growth, sccache growth (must be about
     0 on the branch), and per-mutant build and test durations from
     `outcomes.json`.
2. **D3 per lane.** Branch slots, with and without `CARGO_INCREMENTAL=0`, each
   under its own `LUSHTEXT_BUILD_CACHE` root so neither toggles the other's
   build dir. Two interleaved rounds per mode.
   - Kani: one package compile of the cheapest shard
     (`scripts/kani-shards.py list`), each mode with its own
     `CARGO_BUILD_BUILD_DIR`. Two interleaved rounds.
3. **Pre-commit.** A docs-only commit in a fresh worktree, hook timing and
   build-dir growth. Non-verdict by nature, since it is cold once.
4. **Host IO.** If a quiet window is not available, the system-level
   `btrfs-stall-report` "after" figure for one `make mutants-diff` is left as a
   request in `.claude/handoffs/reduce-build-disk-writes/`, for the
   coordinator.

Timing: build-lane wall time only. No application timed path is touched, so
`B_interact` and `B_settle` are not involved.

## Ledger axioms, Kani, GTK

- GTK axioms relied on: none. No new toolkit behaviour.
- Kani: none (no proven module touched). The Kani lane changes only its
  environment; D9 compiles one package under it.
- GTK axiom samples: none.

## Risks / Trade-offs

- **A slot's first use is a cold build.** It costs about one copy-mode copy,
  once per slot, not once per run.
- **A slot shared across checkouts rebuilds the members that differ.** That is
  bounded by the workspace members; dependencies stay warm.
- **Interrupted in-place runs.** The mutated file sits in a slot, never in the
  checkout, and the next sync restores it. A test asserts this.
- **Every pool slot busy.** The run exits 75 with the holders named, and
  `MUTANTS_LOCAL_JOBS=1` uses whatever single slot frees up first. This is a
  deliberate UX trade against starving measurements.
- **The CST wrapper's "first argument is not `rustc`" behaviour (D4)** is CST
  API. It is asked to be kept stable, or replaced by a knob.
- **`RLIMIT_AS` false positives.** A test that reserves large address space
  without using it (thread arenas, mmap) would fail its baseline. The baseline
  must pass, so this is caught before any mutant runs, and the D5 limit has a
  2x margin.
- **CI.** Kani's `CARGO_INCREMENTAL=0` applies in CI too and changes no
  verdict. Shard budgets are re-read at the programme's final CI sweep. The CI
  mutation modes do not change.

## Deferred (candidates, not this change)

On a cold docs-only commit, `check-policy` still builds `lushtext-core` for
`check-terminology` (an integration test using the filesystem boundary) and
builds `cargo-gtk-proof` for `check-visual-proof-policy`. Together that is
1.52 GB and 105 s. Making those checks independent of a full crate build is a
separate change: it touches the terminology guard's design and the
visual-proof tool, not build placement.

## Migration Plan

1. Land the helpers with failing-first self-tests.
2. Switch the lanes over.
3. Measure (D9).
4. Update the docs.

There is no data migration. The old `.flatpak-builder/` and
`target/proof-strength/wt` are left for the maintainer to delete, and `build.md`
says so. Rollback is a revert: the lanes return to copy mode and the checkout
dirs.

## Open Questions

None blocking. CST-side items are in
`.claude/handoffs/reduce-build-disk-writes/cst-asks.md`.
