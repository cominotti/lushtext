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
- `checkout-path [<path>]` prints the same directory without creating
  anything. Callers that resolve the location at load time (the Makefile's
  variables, `proof-strength.py`'s module-level `WORKTREE`) use it, so a
  target or import that never writes there creates no directory. The writer
  calls `checkout` once before its first write.
- `prune [--dry-run]` removes checkout directories whose recorded path is no
  longer a directory, and never touches anything else. A proof-strength
  worktree inside a pruned directory leaves a stale entry in the repository's
  `git worktree list`; `prune` prints the `git worktree prune` hint, and the
  next proof-strength run already runs it (`remove_worktree`).
- `make build-cache-prune` runs it.

The Makefile, `run-mutants.sh` and `proof-strength.py` all call this helper;
none computes the path itself. The Cominotti repository generator is the one
exception for Flatpak state, because its app dir follows its caller's output
dir (D7).

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
  the slot's `owner` file. A holder is named only while its lock is really
  held; an `owner` file left by a killed run is reported as stale, not as a
  holder.
- The slot lock's file descriptor is close-on-exec, or held by a wrapper
  process that does not pass it on, so no child keeps a slot. `heavy.sh`'s
  header gives the reason: a daemon a build leaves behind (an sccache server)
  would otherwise hold the slot after the run exits.

**Sync (`scripts/mutants-mirror.py sync <checkout> <slot-src>`, with
`--self-test`).**

- The file set is `git ls-files -z --cached --others --exclude-standard`, minus
  tracked files deleted in the working tree. That is the set cargo-mutants'
  `gitignore = true` copy takes, uncommitted edits included.
- A file whose bytes differ is rewritten, which gives it a new mtime so Cargo
  sees it as changed. Identical files are left untouched, so their mtimes and
  Cargo's freshness survive.
- The executable bit is mirrored (98 tracked files carry it), and a mode-only
  difference is applied without rewriting the bytes.
- A tracked symlink is mirrored as a symlink with the same target, never
  followed: the tree tracks 10, including the directory links `.claude/rules`
  and `.claude/skills`. A symlink whose target resolves outside the slot is
  refused, so an in-place mutant can never write through a link into the
  checkout.
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
  - the existing test-thread, build-jobs and timeout knobs.
- Mirror mode never passes `--jobs`, and it unsets `CARGO_MUTANTS_JOBS`:
  cargo-mutants 27 refuses `--in-place` with either, even `--jobs 1` (probed).
  The Makefile still exports `MUTANTS_JOBS=$(MUTANTS_LOCAL_JOBS)`, which
  `mutants_args` appends today, so mirror mode reads `MUTANTS_LOCAL_JOBS` as
  the slot count and drops `MUTANTS_JOBS` from the cargo-mutants arguments.
- `RUSTC_WORKSPACE_WRAPPER` (D4) names a slot-stable path, a copy of
  `rustc-passthrough.sh` at `<root>/mutants/rustc-passthrough.sh`, not the
  checkout's copy. Cargo takes the workspace wrapper into workspace members'
  unit hash (it is what keeps Clippy's and `cargo check`'s artifacts apart),
  so a path that changed with each checkout could rebuild every member when
  runs from different checkouts alternate in a slot. Task 3.3 confirmed this
  with one probe (stage 3, probe crate, `CARGO_INCREMENTAL=0`, pinned target
  and build dirs): with the wrapper at path A the crate compiled, a rebuild was
  `Fresh`, the identical wrapper at path B recompiled the crate, and path A
  again was `Fresh` (Cargo keeps both variants). So the stable path matters.
- With N slots, the i-th slot taken (i = 0..N-1) also gets `--shard i/N`.
  cargo-mutants shards are 0-based. A user `MUTANTS_SHARD` combined with N > 1 is refused, because the
  two would compose ambiguously.
- `diff` mode writes `git.diff` in the checkout, as today, and passes it by
  absolute path.

**Output.** `--output D` makes cargo-mutants create `D/mutants.out` (probed).

- Each slot runs with `--output <checkout>/mutants.out.slots/slot-<k>`, so its
  report is `<checkout>/mutants.out.slots/slot-<k>/mutants.out`. The
  `/mutants.out*/` `.gitignore` entry already covers that directory.
- `mutants-mirror.py merge` then rotates the checkout's `mutants.out` to
  `mutants.out.old`, as cargo-mutants does. It writes a `mutants.out` with the
  files a single run produces. These facts come from a cargo-mutants 27.0.0
  probe with two shards:
  - **Names collide across shards.** Every shard has its own `Baseline`
    outcome and `log/baseline.log`. The `_001`, `_002` suffix that separates
    mutants at one source location is a per-run counter, so two shards can
    write the same `log/` or `diff/` name for different mutants. Merge
    therefore namespaces every entry per slot (`log/slot-<k>/…`,
    `diff/slot-<k>/…`) and rewrites the `log_path` and diff references in
    `outcomes.json` to match.
  - **`outcomes.json`:** the outcome lists are concatenated, keeping one
    `Baseline` entry per slot. The counters `total_mutants`, `missed`,
    `caught`, `timeout`, `unviable` and `success` are summed. `start_time` is
    the earliest and `end_time` the latest; `cargo_mutants_version` must be the
    same in every slot.
  - **`mutants.json`** (the mutant list) is concatenated. `*.txt` lists are
    concatenated. `debug.log` is concatenated with a header per slot, and
    `lock.json` is dropped, since it describes a finished run.
  - **An empty shard** exits 0 and writes no `outcomes.json` and no baseline.
    Merge accepts that and counts nothing for it.
- With N = 1 the slot runs with `--output <checkout>`, so cargo-mutants writes
  and rotates `<checkout>/mutants.out` itself.

**Exit status.** A slot exit outside {0, 2, 3} wins (the first such). Otherwise
the merged status is 3 if any slot timed out, else 2 if any missed mutants,
else 0. That is cargo-mutants' own precedence, confirmed by a stage-3 probe (a
run with two missed mutants and one timeout exits 3), and the reverse of what
this design first assumed.

**Rationale.**

- A warm slot has nothing cold to build, so the copy mode's roughly 15 GB of
  per-run cold builds, and the leak, both go away. Only a slot's first use is
  cold.
- Shards keep the parallelism today's `--jobs 3` gives.
- A host-wide pool bounds disk by pool size, not worktree count: at most
  3 × `MUTANTS_SLOT_MAX_GIB` (24 GiB). Measured: 10.4-11.1 GiB a slot warm
  with one dependency set, 19.4-20.2 GiB after one `Cargo.lock` refresh (two
  sets, 67 GB for the whole cache root at the stage-4 review, 2026-11-15), so
  the next refresh will empty the slots and build them cold once. Per-checkout
  slots, given the 15 checkouts already on the host, would not be bounded,
  and copy mode left about 22 GB behind per run, without bound.
- Two runs from different checkouts in one slot simply resync. Only the
  workspace members that differ rebuild, and the dependencies stay warm,
  unless `Cargo.lock` or the feature set differs.
- **Growth bound.** Cargo never deletes stale units from a build dir, so a slot
  used by branches with different dependency sets keeps every one of them. At
  acquisition, a slot whose `build/` plus `target/` exceeds
  `MUTANTS_SLOT_MAX_GIB` (default 24, about twice a warm slot: once D3 kept
  incremental, a slot held 10.4-11.1 GiB after a focused run, 4.9 GiB of it
  incremental cache, so the first default of 12, twice a cold
  `CARGO_INCREMENTAL=0` build, would have emptied warm slots) is emptied
  before the sync, and the run says so. That costs one cold build and keeps
  the pool's disk bounded.
- **Static shards can finish unevenly**, unlike `--jobs`, which hands out
  mutants as jobs free up. Task 9.1 records each slot's wall time.

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

### D3. `CARGO_INCREMENTAL=0` only in lanes with a build tree of their own, behind a pass bar

The mutation slots (D2), both proof-strength arms (D6) and the Kani lane were
to set `CARGO_INCREMENTAL=0`, each subject to the pass bar below; the measured
decisions close this section (the slots and the tests arm kept incremental).
Each builds in a tree that no ordinary build uses:

- the slots and the proof-strength worktree in their own pinned build dirs;
- Kani in the target-triple tree (`x86_64-unknown-linux-gnu/`) of the
  checkout's build dir. Kani compiles with `kani-compiler` and
  `--target`, so its units never coincide with those of `make test`, Clippy or
  the dev builds, and toggling incremental there cannot recompile theirs.

Kani's part is **conditional**. The stage-1 baseline recorded a 0.9G Kani
triple tree in one worktree's build dir, but no build dir on the host holds a
Kani tree now, so the size of its `incremental/` is unverified. Task 9.2 first
runs one package compile under `cargo kani` and records where `incremental/`
lands and how large it is. If none is written, or it lands in a tree ordinary
builds use, `make kani` is left unchanged and the decision is recorded here.

- Nothing exports `CARGO_INCREMENTAL=1`. A lane that keeps incremental leaves
  the variable unset.
- The widget build is excluded: it shares the checkout's build dir with
  `make test` and Clippy, and the toggle probe shows that each switch
  recompiles every member.

**Pass bar, fixed before measuring.** A lane keeps `CARGO_INCREMENTAL=0` unless
its median per-unit wall time regresses by more than 25% against incremental in
the D9 protocol. The units:

- **mutation slots:** the build phase plus test phase of one mutant, from
  `outcomes.json`;
- **proof-strength tests arm:** the same unit and the same in-place
  build-then-nextest shape, so it follows the slots' decision, which is
  recorded for both;
- **proof-strength Kani arm:** one mutant's rebuild under `cargo kani` in the
  worktree's pinned build dir, after one applied mutant. This is where
  incremental could help most, since every mutant edits the package;
- **Kani lane:** one package compile under `cargo kani`.

- On a regression, that lane keeps incremental, still in its own dir. It may
  then still write less than `main`, because no cold builds are left.
- The decision is recorded in this file with the figures.
- The 25% bound reflects the host priority: write-heavy builds stall the
  desktop for everyone (baseline: saturated NVMe members, WBT throttling), and
  the 2.5x write reduction is worth a bounded slowdown of a background lane.

**Measured decisions (task 9.2, 2026-11-15).** Every round went through
`scripts/measure.sh` (`runs.log` in the handoff dir). The 1-min load at the
start of every round was 6.7-8.6 (other agents' unmanaged work, a game), so
by D9 every wall time below is **non-verdict**; the decisions rest on effects
far larger than the load spread, and are recorded as such. The optional
confirming pair for the slots queued in `.claude/handoffs/MEASURE-QUEUE.md`
was not run. The Kani rounds were rerun in a quiet window, under "Measured
verdicts" below. The slot decision therefore still rests on non-verdict rounds,
where the effect was 2.2x.

- **Mutation slots: keep incremental** (`MUTANTS_SLOT_INCREMENTAL=""`). One
  slot, the six mutants of `MUTANTS_RE=chunked_threshold_with` on the
  `document_scale.rs` smoke file, each mode in its own `LUSHTEXT_BUILD_CACHE`
  root, interleaved inc0, inc1, inc0, inc1, inc0 (the first inc0 round's
  baseline hit D11's flaky test and tested nothing, so a third inc0 round was
  run):

  | round | per-mutant build+test, median | round wall | `write_bytes` |
  | --- | --- | --- | --- |
  | inc1 r1 | 11.6 s (build 8.9) | 125 s | 10.20 GB |
  | inc1 r2 | 11.2 s (build 8.7) | 118 s | 10.12 GB |
  | inc0 r2 | 24.1 s (build 21.8) | 219 s | 6.03 GB |
  | inc0 r3 | 26.9 s (build 24.1) | 223 s | 6.03 GB |

  Without incremental the median unit is 25.5 s against 11.4 s, +124%, five
  times the 25% bar, in both pairs. It writes 40% less per run. These mutants'
  tests take 3 s; a missed mutant's full test run (about 50 s) dilutes the
  build share, which the warm-up's missed mutants put at +42% (81 s against
  57 s), still past the bar. The slots keep incremental in their own build
  dirs, where it touches no other build; the cold builds and leaked dirs,
  the larger write, are gone either way (task 9.1).
- **Proof-strength tests arm: follows the slots** (the same in-place
  build-then-nextest unit, not measured separately): incremental kept,
  `CARGO_INCREMENTAL` left as the caller set it.
- **Kani lane: `CARGO_INCREMENTAL=0`** (`make kani`, task 6.1; confirmed by
  quiet rounds under "Measured verdicts", +0.9%). The first
  `cargo kani -p gtk-lush-widgets --only-codegen` with incremental on wrote
  its cache into the build dir's own triple tree,
  `<build-dir>/x86_64-unknown-linux-gnu/debug/incremental` (11 MB cold, 22 MB
  after one edit), and nothing into the host tree's `debug/incremental`; for
  `lushtext-core` it was 355 MB. So Kani does keep an incremental cache, in a
  tree no other build uses. Unit: one rebuild after a one-line edit of
  `crates/gtk-lush/widgets/src/lib.rs` (a comment, incremental's best case),
  each mode in its own `CARGO_BUILD_BUILD_DIR`, in the proof-strength
  worktree, through `measure.sh` (`kani-round.sh` in the handoff dir):

  | round | inc0 | inc1 | start load |
  | --- | --- | --- | --- |
  | r1 | 18.5 s | 17.1 s | 6.2 / **3.4** |
  | r2 | 20.9 s | 18.0 s | 7.1 / 11.5 |

  Median 19.7 s against 17.6 s, +12%: inside the bar, so the lane drops the
  cache. Only inc1 r1 started below load 4; the rest are non-verdict, and the
  difference is small in either direction.
- **Proof-strength Kani arm: `CARGO_INCREMENTAL=0`, final** (the verdict is
  under "Measured verdicts" below; `KANI_ARM_INCREMENTAL = "0"`). The record
  that follows is the stage-3 state. One round ran (`lushtext-core` rebuild after the
  `MAX_MARKDOWN_EVENTS / 4` → `* 4` mutant of `ui/markdown_preview/policy.rs`:
  35.7 s without incremental, start load 4.9) before the coordinator stopped
  the chain, whose exclusive locks were blocking other agents at load 14, and
  queued the remaining three rounds in `.claude/handoffs/MEASURE-QUEUE.md`. If
  the queued rounds show incremental more than 25% faster, the arm switches
  `KANI_ARM_INCREMENTAL` to `""` in a follow-up.

### D4. Workspace crates bypass sccache in the mutation lanes

With incremental off, CST's wrapper starts handing workspace crates to
sccache. A mutated crate is a guaranteed miss and is then stored (28.5 MB per
mutant), pushing useful dependency entries out of the shared 25G cache.

The mutation slots and the proof-strength tests arm therefore set
`RUSTC_WORKSPACE_WRAPPER` to `scripts/rustc-passthrough.sh` (a one-line
`exec "$@"`; the slots use the slot-stable copy of D2). Cargo then calls `rustc-wrapper rustc-passthrough.sh rustc ...`
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

- The search stops at 16 GiB. If the baseline does not pass there, no limit is
  adopted and that is recorded here as an explicit exception.
- The limit exists to stop the recorded 15 GB `list_budget` runaway alone. If
  the derived default exceeds 16 GiB (a baseline needing more than 8 GiB), it
  would not stop that runaway. The implementation then records an explicit
  exception here with the figures, instead of presenting a limit that protects
  nothing as a fix.

**Reporting.**

- After a run, `mutants-mirror.py merge` (or a one-slot pass) lists in
  `mutants.out/address-space-caught.txt` every caught mutant whose log shows an
  allocation failure, and prints the count. The patterns are pinned in the
  self-test:
  - Rust's `memory allocation of N bytes failed`;
  - GLib's `failed to allocate N bytes`;
  - `failed to spawn thread` and `Cannot allocate memory` / `ENOMEM`
    (`os error 12`), which an address-space limit produces when a thread stack
    or an `mmap` is refused.
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
- **A parallel Kani shard's `PROOF_STRENGTH_WORKTREE`** should be under the
  cache too (`$(scripts/build-cache.sh checkout-path)/proof-strength/wt1`);
  `worktree_owned` accepts anything under `<checkout-cache>/proof-strength`.
- **The tests arm's wrapper and runner** are the checkout's own
  `scripts/rustc-passthrough.sh` and `scripts/test-address-space-runner.sh`,
  not the worktree's: a `--rev` older than this change has neither, and the
  checkout's path is stable across runs of that checkout, which is all D4
  needs (the worktree's build dir is per checkout too).
- **The default location is resolved without creating it.** `WORKTREE` is a
  module-level constant computed at import, and the self-test imports the
  module, so the driver uses `build-cache.sh checkout-path` (D1) and creates
  the directory only in `ensure_worktree`. The self-test runs under a temporary
  `LUSHTEXT_BUILD_CACHE`, so it never touches the real cache.
- **The tests arm runs `cargo mutants --in-place` inside the clean worktree.**
  The worktree is disposable and at `--rev`, so the existing "Developer runs a
  local in-place mutation command" scenario still holds.
  - It drops `--jobs` (today `--jobs 1`, `TESTS_JOBS`): cargo-mutants 27
    refuses `--in-place` with any `--jobs`, `1` included, and with
    `CARGO_MUTANTS_JOBS` (probed). The arm is already serial, so nothing is
    lost. The recorded `meta.command` changes accordingly; it is metadata and
    does not invalidate results.
  - An interrupted in-place mutant is discarded by `ensure_worktree`'s
    `git checkout --force` at the next run, as for the Kani arm today.
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
- **`scripts/generate-cominotti-flatpak-repo.sh`** passes
  `--state-dir="$out_dir/flatpak-builder"`, beside its existing app dir
  `$out_dir/build-dir`. Its output directory stays the caller's.
  - It does not use the cache root, because its app dir follows the caller's
    output dir. `release.yml` runs it in a container with that output dir under
    `$RUNNER_TEMP` (`/__w/_temp`), while `$HOME` (`/github/home`) is a
    separate bind mount. A state dir in the cache root would put the state and
    the app dir on different mounts.
  - Its default output dir, `build-aux/cominotti-flatpak`, stays in the
    checkout, as its state does today (`.flatpak-builder` in the working
    directory). This is a recorded exception: the deploy and verify scripts
    and `docs/next/flatpak-packaging.md` name that path, the generator runs
    once per release, and moving it is left to a separate change.
  - `scripts/test-cominotti-flatpak-repo.sh` runs with a temporary
    `LUSHTEXT_BUILD_CACHE` and asserts that the stub `flatpak-builder` received
    `--state-dir` under its temporary output dir.
- **Each state dir is on the same mount as its app dir.** `flatpak-builder`
  checks its cache out into the app dir, using hardlinks where it can, and a
  hardlink across mounts or subvolumes fails with `EXDEV`. The Makefile and
  `release.sh` keep both under the checkout's cache dir; the generator keeps
  both under its output dir.
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

**`check-includes`** fails when a compile-time input of a Rust source under
`crates/`, `fuzz/` or `workspace-hack/` resolves to an allowlisted path. The
compile-time inputs are:

- the argument of `include_str!`, `include_bytes!` and `include!`, resolved
  relative to the source file, as rustc resolves it;
- a `#[path = "..."]` module attribute, resolved the same way;
- in a `build.rs`, any string literal naming `docs/`, `openspec/`, `.claude/`,
  `.agents/` or a root `*.md`, which covers `cargo:rerun-if-changed` and
  files the build script reads.

An include argument that cannot be resolved statically (built with `concat!`
or `env!`) fails closed when any literal in it names one of those locations.

Runtime reads and plain string literals are not compile-time inputs and stay
allowed. Four sources contain `docs/` literals today: `cargo-gtk-proof`'s
`policy.rs` and the terminology test read documentation at run time, and two
`lushtext-core` unit tests use a temporary `docs/` fixture directory. A docs
edit cannot change what Clippy reports for any of them. Without this audit, a future
`#![doc = include_str!("../../../README.md")]` would silently make a docs edit
a Clippy input; doc lints such as `doc_lazy_continuation` warn by default. It
runs as `make check-precommit-scope` inside `make check-policy`.

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
   - Recorded: wall time, `write_bytes`, new `~/.cache/cargo-build` entries,
     `<root>` growth, sccache growth, per-slot wall time, and per-mutant build
     and test durations from `outcomes.json`.
   - **Pass bars, fixed before measuring:**
     - zero new `~/.cache/cargo-build` entries on the branch;
     - sccache growth on the branch below 1 MB per measured run (mutants never
       reach it; dependencies are warm);
     - branch process-tree `write_bytes` per run below main's in every round
       pair;
     - branch median wall time no more than 25% above main's. A larger
       regression is recorded here as an explicit exception with the figures
       and the reason, and the maintainer is told; it does not silently pass.
2. **D3 per lane.** Branch slots, with and without `CARGO_INCREMENTAL=0`, each
   under its own `LUSHTEXT_BUILD_CACHE` root so neither toggles the other's
   build dir. Two interleaved rounds per mode.
   - Kani: one package compile of the cheapest shard
     (`scripts/kani-shards.py list`), each mode with its own
     `CARGO_BUILD_BUILD_DIR`. Two interleaved rounds. The first round also
     records where `incremental/` lands and its size, which decides whether
     D3 applies to Kani at all.
   - Proof-strength Kani arm: in the relocated worktree, one applied mutant of
     the cheapest oracle module, then its `cargo kani` rebuild, with and
     without `CARGO_INCREMENTAL=0`, each mode in its own pinned build dir. Two
     interleaved rounds.
   - Proof-strength tests arm: no separate run; it follows the slots'
     decision (D3), and the record says so.
3. **Pre-commit.** A docs-only commit in a fresh worktree, hook timing and
   build-dir growth. Non-verdict by nature, since it is cold once.
4. **Host IO.** If a quiet window is not available, the system-level
   `btrfs-stall-report` "after" figure for one `make mutants-diff` is left as a
   request in `.claude/handoffs/reduce-build-disk-writes/`, for the
   coordinator.

Timing: build-lane wall time only. No application timed path is touched, so
`B_interact` and `B_settle` are not involved.

### D10. Admission does not count a daemon left in a finished scope (apply-time blocker)

Found while measuring (2026-11-15), and fixed under the pre-existing-blockers
rule. `scripts/with-memory-cap.sh` admitted a new scope only if the caps of
every scope in `lushtext-heavy.slice` plus its own fit the slice budget. A
scope lives while any process is in it, and the sccache server is started by
whichever compile first finds it absent, so it lands in that compile's scope
and outlives it. A 36G `make mutants-smoke` warm-up left such a scope: 36 of
48 GiB stayed admitted after the run ended, so every measured round (36G,
`LUSHTEXT_ADMISSION_NOWAIT=1`) failed with 75, and other agents' heavy work
waited. The server never idles out on a busy host.

- Each scope is now named `lushtext-cap-<pid>-<ns>.scope` after the command's
  PID (`systemd-run --scope` execs the command in place), and `admitted_kib`
  counts its cap only while that PID is still in the scope. The daemon's real
  memory still counts against the slice's `MemoryMax` and `MemAvailable`.
- Scopes an older copy of the script started keep counting while they exist,
  until every worktree carries this script. The pinned scope here was
  released with `systemctl --user set-property --runtime <scope>
  MemoryMax=infinity`, which kills nothing.
- **Rejected: `sccache --stop-server`.** It fails other agents' compiles in
  flight.
- **Rejected: starting the server outside the scope first.** A server that
  idles out during a long test or CBMC phase is restarted inside the scope by
  the next compile.
- The self-test proves both directions, failing first: a leftover daemon does
  not keep its 2G cap admitted against a 3G budget, and a command that still
  runs does.

### D11. A flaky watchdog unit test (apply-time blocker)

The first measured slot round's unmutated baseline failed
`ui::responsiveness_probe::watchdog::tests::a_blocking_dispatch_is_recorded_between_its_bounds`
(`lateness 756 µs below the lower bound 38000 µs`) at a 1-min load of 8-12,
so cargo-mutants tested no mutant (exit 4). The test is on `main` and
untouched by this change; it was fixed under the pre-existing-blockers rule.

- Cause: the test iterated its context for a fixed 70 ms window. While every
  iteration took longer than the 2 ms tick, the `G_PRIORITY_HIGH` watchdog was
  ready at every prepare and starved the `G_PRIORITY_DEFAULT` idle that
  stalls. The idle could then run in the window's last iteration, after which
  no watchdog dispatch recorded the stall.
- Fix: iterate until the watchdog has dispatched once after the stall (a 10 s
  deadline), so the dispatch that records the stall always happens. The
  bounds are unchanged. Not the address-space limit: no allocation failed.
- Proved by the two watchdog tests passing alone (`cargo nextest run -p
  lushtext-core --lib -E 'test(responsiveness_probe::watchdog)'`) and by every
  later slot baseline, all at loads of 8-12.

## Ledger axioms, Kani, GTK

- GTK axioms relied on: none. No new toolkit behaviour.
- Kani: none (no proven module touched). The Kani lane changes only its
  environment; D9 compiles one package under it.
- GTK axiom samples: none.

## Risks / Trade-offs

- **A slot's first use is a cold build.** It costs about one copy-mode copy,
  once per slot, not once per run.
- **A slot shared across checkouts rebuilds the members that differ.** That is
  bounded by the workspace members; dependencies stay warm unless `Cargo.lock`
  or the features differ, and D2's growth bound empties a slot that has
  accumulated too many dependency sets.
- **Interrupted in-place runs.** The mutated file sits in a slot, never in the
  checkout, and the next sync restores it. A test asserts this.
- **Every pool slot busy.** The run exits 75 with the holders named, and a
  retry later (or with `MUTANTS_LOCAL_JOBS=1`, which needs only one free slot)
  goes ahead. Nothing waits. This is a deliberate UX trade against starving
  measurements.
- **The CST wrapper's "first argument is not `rustc`" behaviour (D4)** is CST
  API. It is asked to be kept stable, or replaced by a knob.
- **`RLIMIT_AS` false positives.** A test that reserves large address space
  without using it (thread arenas, mmap) would fail its baseline. The baseline
  must pass, so this is caught before any mutant runs, and the D5 limit has a
  2x margin.
- **CI.** If adopted (D3), Kani's `CARGO_INCREMENTAL=0` applies in CI too and changes no
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

## Measured verdicts (2026-11-15)

The coordinator ran the queued rounds (`.claude/handoffs/MEASURE-QUEUE.md`,
the two `reduce-build-disk-writes` entries) through `scripts/measure.sh`.
`runs.log` in the handoff dir has each round's load and top five processes
before and after. By D9 a round is a verdict only when its recorded 1-minute
load at the start was below 4. The end loads are higher because the measured
work itself loads the host.

### D3: Kani compile rounds (task 9.2)

Unit: one `cargo kani --only-codegen` rebuild after one edit, each mode in its
own build dir (`kani-round.sh`). Every round first builds the pristine tree,
unmeasured.

| lane | mode | verdict rounds (start load): seconds | non-verdict (start load) |
| --- | --- | --- | --- |
| proof-strength Kani arm (`lushtext-core`, one mutant) | inc0 | k-c-inc0-r2 (2.24): 38.0; k-c-inc0-r1q (3.49): 38.4 | k-c-inc0-r1 (4.87): 35.7 |
| | inc1 | k-c-inc1-r1 (2.18): 40.9; k-c-inc1-r2 (3.34): 30.5 | none |
| Kani lane (`gtk-lush-widgets`, one edit) | inc0 | k-w-inc0-r1q (3.58): 18.0; k-w-inc0-r2q (3.18): 17.3 | k-w-inc0-r1 (6.21), k-w-inc0-r2 (7.05) |
| | inc1 | k-w-inc1-r1 (3.44): 17.1; k-w-inc1-r1q (2.64): 17.5; k-w-inc1-r2q (2.45): 18.4 | k-w-inc1-r2 (11.51) |

k-w-inc1-r1q's first attempt exited 75. That is `measure.sh`'s designed
back-off: a `git commit` held `LOCK` for over 60 s. The retry is the round
above.

- **Kani lane: `CARGO_INCREMENTAL=0` confirmed.** The medians are 17.65 s
  without incremental and 17.5 s with it, +0.9%, inside the 25% bar. This
  replaces stage 3's non-verdict +12%.
- **Proof-strength Kani arm: `CARGO_INCREMENTAL=0`, final.** The medians over
  the verdict rounds are 38.2 s against 35.7 s, +7%, inside the bar, so the
  arm keeps `KANI_ARM_INCREMENTAL = "0"`.

  The two inc1 rounds differ more than the bar does:
  - In k-c-inc1-r1 and k-c-inc0-r1q, the unmeasured pristine build also
    recompiled `gtk-lush-widgets`, whose `lib.rs` the widgets rounds had
    edited and restored. The timed cargo step then took 21.6 s with
    incremental against 17.8 s without.
  - In the r2 pair, nothing but the mutated package rebuilt, which is the
    state every later mutant of an arm run sees. There, inc0 is +24.6%
    (38.0 s against 30.5 s). The pooled inc0 median against that one inc1
    round is +25.2%.

  So the steady state sits at the bar on one sample. The decision applies the
  bar as written: the median over the verdict rounds. A change that finds the
  arm's rebuilds slow should take more steady-state pairs before switching.

### D9 step 1: mutation lane (task 9.1)

Fixed focused scope: `MUTANTS_RE='chunked_threshold_with|markdown_chunked_cap'`
over `document_scale.rs`, 13 mutants. Every round found 6 caught, 6 missed and
1 unviable, and exited 2, cargo-mutants' missed-mutant status, on every arm.
Main is copy mode with `--jobs 3`; the branch is three mirror slots. The rounds
were interleaved.

| round (start load) | wall | `write_bytes` | new `~/.cache/cargo-build` dirs | cache root | sccache | per-mutant build+test, median (build / test) | slot wall times |
| --- | --- | --- | --- | --- | --- | --- | --- |
| main-q1 (2.87) | 302.6 s | 32.06 GB | 3, 21.9 GB | +0 | +30.6 MB | 61.6 s (12.9 / 4.4) | — |
| branch-q1 (2.94), cold | 487.9 s | 33.76 GB | 0 | +23.96 GB | +494 MB | 50.9 s (8.8 / 3.3) | 315, 486, 253 s |
| main-q2 (3.34) | 300.4 s | 32.14 GB | 3, 22.1 GB | +0 | +30.6 MB | 58.2 s (12.2 / 4.0) | — |
| branch-q2 (2.77), warm | 333.4 s | 21.43 GB | 0 | +1.81 GB | +0 | 15.0 s (7.6 / 3.9) | 167, 331, 94 s |

**Why branch-q1 was cold.** The slots were not empty: the root held 45.9 GB.
The last stage-3 merge of `origin/main` (`28d387ea`) brought main's dependency
refresh (`4da4bf2f`: 761 changed lines of `Cargo.lock`, `sourceview5` 0.11.3
with `gtk_v4_18`). The slots' first run after it rebuilt the dependency graph,
115-117 s of baseline build against 6 s warm, beside the old set, and the root
grew to 69.9 GB. So this is not a one-time first fill. It is the cost of every
`Cargo.lock` or feature change, paid once per slot set. D9 called for slots
warmed by one unmeasured run, and this round was that warm-up, measured.

**Against the D9 bars, per pair:**

| bar | pair 1 (branch cold) | pair 2 (branch warm) |
| --- | --- | --- |
| zero new `~/.cache/cargo-build` entries on the branch | 0: pass | 0: pass |
| sccache growth on the branch below 1 MB | +494 MB: **fail** | +0: pass |
| branch `write_bytes` below main's | 33.76 against 32.06 GB (+5%): **fail** | 21.43 against 32.14 GB (−33%): pass |
| branch wall time at most 125% of main's | 487.9 against 302.6 s (+61%): **fail** | 333.4 against 300.4 s (+11%): pass |

The two-round branch median is 410.7 s against main's 301.5 s, +36%. That
misses the wall-time bar, so it is recorded here as D9's **explicit
exception**, and the maintainer is told in the stage-4 report:

- The steady state passes every bar.
- The refill after a dependency change fails three bars. It costs about
  3 min and 1.7 GB more than one copy-mode run, and its +494 MB of sccache is
  the dependency compiles it stores.
- A slot that already holds two dependency sets (19.4-20.2 GiB now) passes
  the 24 GiB growth bound at the next refill, so that run empties the slot and
  builds it cold.

No further round was run. Main's two rounds are 0.7% apart. The warm verdict
rests on one pair, but its wall time is one slot's shard (331 of 333 s), and
the bytes are independent of load: the stage-3 rounds measured 21.48 and
21.38 GB against 31.77 and 31.73 GB.

**Slot-2 is a static-shard artifact, not IO.** In both branch rounds, shard
1/3 drew 5 of the 6 missed mutants, all of `markdown_chunked_cap`. A missed
mutant runs the whole test phase (43-50 s), while a caught one stops at its
first failure, so slot-2 ran 486 and 331 s against 94-315 s for the others.
The branch-q1 window has one storage episode of 16 s (task 9.3 below).
Balanced, the warm round would take about 200 s, a third of the three slot
times. The mutants are classified below, and that changes this workload.

The six `~/.cache/cargo-build` dirs the main rounds leaked, 44.0 GB, were
deleted afterwards. Each one's dep-info names only a
`/tmp/cargo-mutants-reduce-build-disk-writes-*` copy (`leaked-deleted.txt` in
the handoff dir). Copy mode leaves 21.9-22.1 GB behind per run, confirming
`evidence/after.md`'s correction of the "about 15 GB" estimate.

### D9 step 4: host IO (task 9.3)

`btrfs-stall-report --json` over each 9.1 round, from 1 min before its start
to 15 s after its end. All four windows have complete coverage and no gaps.
The writes are dm-crypt bytes per btrfs member: RAID1 writes the same bytes
to both, and they are counted after `zstd:1`.

| window | verdict | writes | longest commit | most D tasks | worst episode |
| --- | --- | --- | --- | --- | --- |
| main-q1, 14:08:08-14:14:28 | WBT (100% of D samples); "btrfs disks NOT saturated", dm-crypt write await 4.7 ms | 23.42 GB | 429 ms | 3 | 14:11:09-14:11:19, commit 347 ms |
| branch-q1 (cold), 14:16:25-14:25:49 | other 63% (`open_last_lookups` in the run's scope), commit 20%, writeback 17%; "btrfs disks look SATURATED", dm-crypt write await 5.6 ms | 25.03 GB | 433 ms | 25 | 14:20:59-14:21:15 (16 s), commit 433 ms |
| main-q2, 14:49:44-14:56:01 | "Storage was NOT a bottleneck: IO pressure 1.0%, at most 1 task(s) waiting on storage, longest btrfs commit 366 ms (written 21.0 GiB)" | 22.52 GB | 366 ms | 1 | none |
| branch-q2 (warm), 15:00:58-15:07:48 | "Slow btrfs commits: longest 1.0 s at 15:07:26"; commit 60%, metadata-read 30%, other 10%; "btrfs disks look SATURATED", dm-crypt write await 7.0 ms, NVMe write await 40 ms | 19.73 GB | 1,024 ms | 5 | 15:04:35-15:04:45, commit 952 ms; gnome-shell threads in `wait_current_trans` |

How to read it:

- **Device writes.** The warm branch round wrote 12-16% fewer device bytes
  than main (19.73 against 22.52-23.42 GB). The process-tree figure is −33%.
  Each window also holds 6.5-7.4 GB from an unrelated background writer
  (`jetbrains-toolbox`, `below`'s top writers), not subtracted. `below`'s
  per-process figures are not additive, so they are not used for totals.
  Main's source copies sit on tmpfs `/tmp` and never reach the disks: main
  swapped out 0.18 and 1.24 GB instead.
- **The stall verdicts do not favour the branch.** Main-q2 is the only "NOT a
  bottleneck" window. Both branch windows read "SATURATED", and the warm one
  had 1.0 s commits with desktop threads waiting on one.
- **The limits are wide.** There is one window per arm, the windows differ in
  length, background writers were present, and the scope is the focused
  `mutants-smoke` used in 9.1, not the `mutants-diff` of the original
  request. This is the host-level "after" record, not a verdict, and it does
  not support a claim that the branch stalls the desktop less.

### Mutants the focused scope left missed (pre-existing, fixed here)

All six are on `main` and in every round. They were classified under
`.agents/rules/preexisting-blockers.md`:

- **`document_scale.rs:220:5` (`None`, `Some(0)`, `Some(1)`) and `:221:12`
  (`==` → `!=`) are a real test gap.** These are the `test-utils` arm of
  `markdown_chunked_cap`, which every runtime caller of the `_with` scale
  functions passes. Only widget tests drove it, and the mutation lane does not
  run them. The new integration test
  `document_scale::the_runtime_markdown_cap_is_the_model_cap_only_while_the_override_is_held`
  asserts `None` by default, `Some(MODEL_MARKDOWN_CAP)` under the override
  guard, and `None` after the guard drops. It lives in the `lushtext`
  integration binary, which always builds `lushtext-core` with `test-utils`,
  so the Kani oracle module is untouched.
  - Failing first: these rounds, where all four are missed.
  - After the fix, the same focused scope, run on one slot, catches all four:
    13 mutants, 10 caught, 2 missed (the `:229` pair below), 1 unviable
    (`evidence/failing-first.md`).
- **`document_scale.rs:229:5` (`Some(0)`, `Some(1)`) cannot be killed by this
  lane, so they are excluded.** They are the `cfg(not(feature = "test-utils"))`
  arm. Unlike the cfg-disabled stubs excluded beside them, this is the arm
  that ships. But the lane's only test build is the workspace build, where the
  `lushtext` dev-dependency enables `test-utils`, so a mutant there compiles
  unchanged code. They are excluded in `.cargo/mutants.toml` by exact location
  and reason.
  - With the config, `cargo mutants --list --workspace --file … --re
    markdown_chunked_cap` drops exactly these two: the `:220`, `:221` and
    `:242` lines remain. Without the config (`--no-config`), the same listing
    also shows the `:229` pair.
  - `make mutants-smoke` passes `--no-config` by design, so it still runs
    the pair and reports it missed. `docs/mutation-testing.md` now says so.

**Later readers:** the four `:220`/`:221` mutants are now caught. The focused
smoke scope still generates 13 mutants, but now misses 2 where these rounds
missed 6. A config-driven lane generates 2 fewer. So the MISSED counts, and
slot-2's imbalance, cannot be compared with any later run of the same scope.
