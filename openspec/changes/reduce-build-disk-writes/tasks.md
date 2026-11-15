Rules for every task:

- Run every heavy command through `scripts/heavy.sh [--mem <cap>] -- <cmd>`,
  one command per call, on CPUs 8-23.
- Run every timed verdict through `scripts/measure.sh`. A run is a verdict only
  if its recorded 1-min load is below 4; otherwise label it non-verdict and
  retry.
- Headless only.
- Do not touch
  `window::test_autosave_during_a_local_history_restore_keeps_the_previous_draft`
  or the sidebar directory-retention test (PR #99).
- Commit and push per task group.

## 1. Failing-first self-tests (no lane behaviour change)

- [x] 1.1 Write `scripts/build-cache.sh --self-test` against a temporary
  `LUSHTEXT_BUILD_CACHE`/`XDG_CACHE_HOME`. Cover:
  - the root precedence;
  - distinct directories for two temporary git worktrees;
  - the `checkout-path` and `CACHEDIR.TAG` files;
  - `checkout-path` printing the same directory as `checkout` while creating
    nothing under the root;
  - prune removing only a checkout directory whose recorded path is gone
    (slots and live checkouts are kept).

  Start with a stub that prints a fixed path, record the failing output in
  `evidence/failing-first.md`, then implement D1.
- [x] 1.2 Write `scripts/mutants-mirror.py --self-test` (temporary git repos).
  Cover:
  - **sync:** tracked, untracked non-ignored, ignored, deleted-tracked and
    `.git`; an unchanged file keeps its mtime; a changed file gets a new
    mtime; an extra slot file is removed; a "mutated" slot file is restored;
    an executable file stays executable and a mode-only change is applied; a
    tracked file symlink and a tracked directory symlink (like
    `.claude/rules`) are mirrored as symlinks with their targets, nothing is
    written through them, and a symlink resolving outside the slot is
    refused;
  - **merge:** two slot outputs merged into one `mutants.out`, each outcome
    exactly once, one `Baseline` per slot, counters summed, start and end
    times taken as earliest and latest, previous report rotated to `.old`;
    two slots that both wrote `log/baseline.log` and the same `_001` log and
    diff names keep every file under per-slot names, and every `log_path` in
    the merged `outcomes.json` names a file that exists; a slot whose shard
    was empty (no `outcomes.json`) merges without error;
  - **the address-space scan:** logs containing
    `memory allocation of 123 bytes failed`, GLib's
    `failed to allocate 123 bytes`, `failed to spawn thread` and
    `os error 12` land in `address-space-caught.txt`; a caught mutant's log
    without any of them does not;
  - **exit-status precedence.**

  Capture real cargo-mutants 27.0.0 output from a probe crate under the
  scratchpad first (two `--shard` runs with `--in-place`, one of them empty),
  and pin the merge to it. Fixture repositories set a local `user.name`,
  `user.email` and `commit.gpgsign=false`, so the self-test never signs and
  never reads the developer's identity. Record the failing run.
- [x] 1.3 Write the `scripts/test-address-space-runner.sh --self-test` cases.
  Cover:
  - a process allocating below the limit succeeds;
  - one allocating past it fails with a nonzero status, using
    `python3 -c 'bytearray(N)'`;
  - arguments and exit status pass through unchanged;
  - signals propagate (`exec`).
- [x] 1.4 Write `scripts/precommit-scope.py --self-test` (temporary git repos).
  Cover:
  - **`docs-only` passes for:** a `docs/` edit, an `openspec/` edit, a root
    `*.md` edit and a `.claude/` deletion;
  - **`docs-only` refuses:** a `crates/**/*.md` edit, a `.rs` edit, a `.rs`
    deletion, a rename from `docs/a.md` to `crates/a.md`, a merge in progress
    (`MERGE_HEAD`), an empty index and a git error;
  - **`check-includes`:** fails on fixtures containing
    `include_str!("../../docs/x.md")` and
    `#![doc = include_str!("../../README.md")]`, and on a `build.rs` naming
    `openspec/`; passes on the current tree.

  Record the failing run.
- [x] 1.5 Update the `proof-strength.py --self-test` assertions to the D6
  layout: a worktree under `<checkout-cache>/proof-strength/wt`,
  `CARGO_BUILD_BUILD_DIR` pinned at `wt/target/kani/kani`, the tests arm's
  `--in-place` command with the pinned `wt/target/tests-build` /
  `wt/target/tests` dirs and **no `--jobs`**, and ownership accepting the
  cache location. The self-test runs under a temporary `LUSHTEXT_BUILD_CACHE`
  and asserts that importing the module created nothing under it. Watch them
  fail on the current code. Update `kani-shards.py --self-test` too, if
  task 6.1 changes what it pins.
- [x] 1.6 Add `make check-build-cache-helpers`, which runs the five self-tests
  (and task 3.3's `run-mutants.sh --self-test` once it exists)
  plus `shellcheck` on the new shell scripts, and add it and
  `check-precommit-scope` to `check-policy`. Neither needs cargo. It also runs
  three text audits over `Makefile`, `scripts/` and `.githooks/`, each with a
  failing fixture in its self-test:
  - no `CARGO_INCREMENTAL=1` anywhere;
  - no `CARGO_INCREMENTAL` in the widget, test, Clippy or dev-build targets
    and the scripts they run (`run-widget-tests.sh`, `widget-shards.py`);
  - no build-cache location under the home directory (`.cache/lushtext-build`
    or `XDG_CACHE_HOME` used for a build path) outside `build-cache.sh`.
    `clear-lushtext-xdg.sh`'s app-data cache path is not a build cache and is
    allowed by name.

  Confirm `make check-policy` fails before the implementations land, and
  record that.

## 2. Cache root (D1)

- [x] 2.1 Implement `scripts/build-cache.sh` (`root`, `checkout [<path>]`,
  `checkout-path [<path>]`, `prune [--dry-run]`, `--self-test`) and make
  task 1.1 pass.
- [x] 2.2 Add `make build-cache-prune` and a recursively expanded
  `BUILD_CACHE_CHECKOUT = $(shell ./scripts/build-cache.sh checkout-path)`
  Makefile variable (`=`, not `:=`, so it is expanded only by the recipes that
  use it, and `checkout-path` creates nothing). The recipes that write there
  call `./scripts/build-cache.sh checkout` first. Confirm with
  `make -n help` under a temporary `LUSHTEXT_BUILD_CACHE` that nothing is
  created.

## 3. Mutation slots (D2, D3, D4)

- [x] 3.1 Implement `scripts/mutants-mirror.py` (`sync`, `merge`,
  `--self-test`) and make task 1.2 pass.
- [x] 3.2 Add `scripts/rustc-passthrough.sh` (`exec "$@"`).
- [x] 3.3 In `scripts/run-mutants.sh`, add `MUTANTS_MIRROR=1` handling for
  `smoke`, `diff` and `full`:
  - non-blocking slot acquisition over `MUTANTS_POOL_SIZE`, with exit 75 and
    the holders named, and an `owner` file per slot;
  - the slot lock on a close-on-exec descriptor (or a holder process that does
    not pass it on), a holder named only when its lock is really held, and a
    stale `owner` reported as stale;
  - D2's growth bound (`MUTANTS_SLOT_MAX_GIB`, default 12) checked at
    acquisition;
  - sync, then one `cargo mutants --in-place --output ...` per slot (D2's
    output layout: `--output <checkout>` when N = 1), with `--shard i/N` when
    N > 1;
  - **no `--jobs`**: mirror mode drops `MUTANTS_JOBS` from the arguments and
    unsets `CARGO_MUTANTS_JOBS`, and uses `MUTANTS_LOCAL_JOBS` as the slot
    count;
  - a refusal of `MUTANTS_SHARD` together with N > 1;
  - the per-slot environment: `CARGO_TARGET_DIR`, `CARGO_BUILD_BUILD_DIR`,
    `CARGO_INCREMENTAL=0` (unless D3's measurement says otherwise),
    `RUSTC_WORKSPACE_WRAPPER` set to the slot-stable copy
    `<root>/mutants/rustc-passthrough.sh`, and the existing build-jobs and
    test-thread caps. Probe once, in a scratch crate, whether changing the
    workspace wrapper's path rebuilds members, and record the answer in D2;
  - merge, then the exit-status precedence.

  Add a `run-mutants.sh --self-test` (no cargo: a stub `cargo` on `PATH`
  records its arguments) under a temporary `LUSHTEXT_BUILD_CACHE`, covering:
  exit 75 with the holders named while a test process holds every slot's
  lock; a stale `owner` with a free lock taken normally; the `MUTANTS_SHARD`
  refusal with N > 1; no `--jobs` in any mirror-mode command; `--shard i/N`
  per slot; and the CI modes' argument lists unchanged. Wire it into
  `check-build-cache-helpers`.

  Keep `ci-pr`, `ci-full` and `list` byte-for-byte unchanged in behaviour.
  `MUTANTS_IN_PLACE=1` (in-checkout) keeps its clean-tree guard.
- [x] 3.4 Have the Makefile `mutants-smoke|diff|full` targets set
  `MUTANTS_MIRROR=1`, keep `MUTANTS_LOCAL_JOBS=3` as the slot count, and keep
  the 36G scope. Update the comment block above `MUTANTS_LOCAL_JOBS` with the
  new mechanics and the baseline figures.
- [ ] 3.5 Lane case: run `make mutants-smoke` twice in a row on warm slots and
  confirm from the second run's cargo output that no dependency crate was
  compiled (the warm-slot scenario).
- [ ] 3.6 Lane case: confirm the slot healing scenario end to end. Interrupt a
  `make mutants-smoke` (SIGTERM through `heavy.sh`) while a mutant is applied,
  rerun, and confirm that the slot file matches the checkout before testing
  resumes and that the checkout was never modified (`git status` clean
  throughout).

## 4. Per-test address-space limit (D5)

- [x] 4.1 Implement `scripts/test-address-space-runner.sh` and make task 1.3
  pass. Wire `CARGO_TARGET_<HOST>_RUNNER` (host triple from `rustc -vV`) into
  the slot environment and the proof-strength tests arm.
- [x] 4.2 Derive the limit. Run the unmutated baseline of the default mutation
  scope (`cargo nextest run --workspace` in a slot, through the runner) at 2,
  4, 8 and 16 GiB until it passes. Then do the same for the proof-strength tests
  arm's `property-tests` baseline. Set `LUSHTEXT_TEST_ADDRESS_SPACE` to twice
  the smallest passing value, at least 4 GiB, in the Makefile with a comment
  citing the measurement, and record it in `evidence/address-space.md`. If
  16 GiB does not pass, or the derived default exceeds 16 GiB (so it would not
  stop the recorded 15 GB runaway), adopt no limit and record the explicit
  exception in design.md D5 (D5's two exception bullets).
- [x] 4.3 Lane case: in a scratch probe crate (not the repository), create a
  test that allocates past the limit only when a constant is mutated. Run it
  through `run-mutants.sh` in mirror mode against a temporary
  `LUSHTEXT_BUILD_CACHE`, and confirm that `address-space-caught.txt` lists the
  mutant and the run is not discarded.

## 5. Proof-strength relocation (D6)

- [x] 5.1 Move the default `WORKTREE` to `<checkout-cache>/proof-strength/wt`
  via `build-cache.sh`. Extend `worktree_owned` to accept it, keep the results
  in `target/proof-strength/results`, and keep the relative
  `PROOF_STRENGTH_WORKTREE` semantics.
- [x] 5.2 Switch the tests arm to `cargo mutants --in-place` inside the clean
  worktree, without `--jobs` (retire `TESTS_JOBS`), with the pinned tests
  build/target dirs, D3, D4 and D5. Resolve the default worktree with
  `build-cache.sh checkout-path` and create it only in `ensure_worktree`. Make
  task 1.5 pass.
- [ ] 5.3 Lane case: run `make proof-strength` once for the cheapest oracle
  module (`make proof-strength-list`; `PROOF_STRENGTH_TIER=ci`), detached
  (`setsid nohup scripts/heavy.sh --mem none -- make proof-strength ...`).
  Confirm that:
  - no new `~/.cache/cargo-build` entry appears (listing before and after);
  - nothing is written under `target/proof-strength/wt`;
  - the results land in `target/proof-strength/results`;
  - the kill counts equal the last recorded run for that module and revision
    in `docs/next/formal-verification.md`, or the difference is explained.

## 6. Kani lane (D3)

- [ ] 6.1 After task 9.2's first Kani round (where `incremental/` lands and its
  size), and only if it shows an incremental cache in Kani's own triple tree:
  have `make kani` export `CARGO_INCREMENTAL=0` for `kani-shards.py run`.
  Otherwise leave `make kani` unchanged and record why in D3. The
  proof-strength Kani arm gets `CARGO_INCREMENTAL=0` through the driver's env
  either way, subject to its own D3 measurement. Nothing else in the Kani lane
  changes (`KANI_TARGET_DIR` stays `target/kani`).

## 7. Flatpak state and app dirs (D7)

- [x] 7.1 Add `FLATPAK_STATE_DIR ?= $(BUILD_CACHE_CHECKOUT)/flatpak-builder`
  (passed as `--state-dir`) and `FLATPAK_BUILD_DIR ?=
  $(BUILD_CACHE_CHECKOUT)/flatpak-app` to `flatpak-deps`, `flatpak` and
  `flatpak-install`; today's `FLATPAK_BUILD_DIR := build-flatpak` becomes
  that `?=` default. Update `scripts/release.sh` (default build dir
  `<checkout-cache>/flatpak-release-app`, plus the state dir beside it) and
  `scripts/generate-cominotti-flatpak-repo.sh`
  (`--state-dir="$out_dir/flatpak-builder"`, beside `$out_dir/build-dir`,
  D7). Update `scripts/test-cominotti-flatpak-repo.sh` to run under a
  temporary `LUSHTEXT_BUILD_CACHE` and to assert that the stub received that
  `--state-dir`, and run it.
- [ ] 7.2 Lane case: run `make flatpak` once through `heavy.sh`. Confirm that no
  `.flatpak-builder` directory is created or grown in the checkout (compare the
  existing one's size before and after), that the build succeeds, and that the
  log has no `Invalid cross-device link` / `EXDEV`. Then run
  `make verify-flatpak-identity` after `make flatpak-install`, only if the
  maintainer's installed dev Flatpak may be replaced. Otherwise record the
  skip.

## 8. Docs-only pre-commit (D8)

- [x] 8.1 Implement `scripts/precommit-scope.py` (`docs-only`,
  `check-includes`, `--self-test`) and make task 1.4 pass. Add
  `make check-precommit-scope` (runs `check-includes` and the self-test) to
  `check-policy`.
- [x] 8.2 Add `pre-commit-docs: check-fmt check-policy` to the Makefile, and
  change `.githooks/pre-commit` to D8's form. Leave `pre-commit`, `check` and
  CI's Clippy command untouched, and diff them against `main` to prove it.
- [ ] 8.3 Lane case: make one docs-only commit and one code commit on this
  branch through `heavy.sh --mem none -- git commit -F ...`. Confirm from the
  hook output that the first skipped Clippy and the second ran it.

## 9. Measurements (D9)

- [ ] 9.1 Mutation lane, four rounds interleaved main → branch → main →
  branch, through `measure.sh`, on the fixed focused scope (diff saved under
  `evidence/`). Record:
  - wall time;
  - process-tree `write_bytes`;
  - new `~/.cache/cargo-build` entries (branch: 0);
  - `<root>` growth;
  - sccache growth (branch: about 0);
  - per-mutant build and test durations from `outcomes.json`;
  - per-slot wall time.

  Apply D9's pass bars (zero new entries, sccache below 1 MB per run, fewer
  bytes written than main in every round pair, median wall time at most 25%
  above main's or an explicit recorded exception). Delete the main rounds'
  leaked build dirs afterwards (list them first) and record what was deleted.
  This round also verifies the "about 15 GB per local run" figure, which
  stage 1 derived from one cold build times three jobs rather than measured.
- [ ] 9.2 D3 per lane. Run slots with and without `CARGO_INCREMENTAL=0` under
  separate `LUSHTEXT_BUILD_CACHE` roots, two interleaved rounds per mode. Then
  one package compile of the cheapest Kani shard with and without, each with its
  own `CARGO_BUILD_BUILD_DIR`, two rounds; the first records where
  `incremental/` lands and its size (task 6.1 depends on it). Then the
  proof-strength Kani arm's per-mutant rebuild (one applied mutant of the
  cheapest oracle module) with and without, two rounds. Apply the 25% pass
  bar, record the decision for each lane in design.md D3 with the figures
  (the tests arm follows the slots and says so), and adjust tasks 3.3, 5.2 and
  6.1 if a lane keeps incremental.
- [ ] 9.3 Write `evidence/after.md` with the before/after table. Put the host
  IO "after" request (`btrfs-stall-report` over one representative
  `make mutants-diff`, branch versus main, in a quiet window) in
  `.claude/handoffs/reduce-build-disk-writes/` for the coordinator, unless a
  quiet window was available and the figure was taken here.

## 10. Documentation (`.agents/rules/documentation.md`)

- [ ] 10.1 `.agents/rules/build.md`:
  - Mutation Testing: slots, pool, the exit-75 behaviour, the merge, the
    address-space limit and its triage list, `make build-cache-prune`, and the
    retired "discard a run that hit its cap" wording for test processes;
  - Formal Verification: Kani's `CARGO_INCREMENTAL=0` and the proof-strength
    worktree location;
  - Flatpak: the state and app dirs, and the old `.flatpak-builder/` being
    safe to delete;
  - Dev Builds (the gate paragraph that says "The pre-commit hook runs
    `make pre-commit`"): the docs-only hook path, its allowlist and the
    include audit.
- [ ] 10.2 `.agents/rules/verification-runs.md` "Shared-machine agent work":
  slot acquisition never waits, and exit 75 means retry later; the docs-only
  hook still runs `check-policy`, so commits still go through `heavy.sh`.
- [ ] 10.3 `docs/mutation-testing.md`: the local commands, slots, the
  proof-strength isolation and tests-arm text (worktree location and in-place
  arm), the parallel-shard example that sets
  `PROOF_STRENGTH_WORKTREE=target/proof-strength/wt1` (a relative path, so it
  would put a build tree back in the checkout; point it at the cache), and the
  address-space-caught triage. Update `scripts/proof-strength.py`'s module
  docstring, which names `target/proof-strength/wt`.
- [ ] 10.4 `AGENTS.md` (Build Commands: `build-cache-prune`; Testing: the
  mutation lane line; Rust Linting Policy: the paragraph that names
  `.githooks/pre-commit`, which now skips Clippy on docs-only commits) and
  `README.md`, if it describes the mutation commands or the hook. Keep the
  Rules Index entry for `build.md` in sync.
- [ ] 10.5 Run `make check-agent-docs` and fix whatever it reports.

## 11. Verification

- [ ] 11.1 `scripts/heavy.sh -- make check`. Include `make check-policy` with
  the new targets, and `shellcheck` clean.
- [ ] 11.2 `openspec validate reduce-build-disk-writes --strict`.
- [ ] 11.3 Lane cases 3.5, 3.6, 4.3, 5.3, 7.2 and 8.3 recorded. Then run
  `make mutants-smoke` once end to end in slots, with the checkout clean
  before and after, no new `~/.cache/cargo-build` entry, and a `mutants.out`
  that is complete.
- [ ] 11.4 Kani: none (no proven module touched). GTK axioms: none. Widget
  shards: none (no widget test touched). Record those three lines in the
  stage handoff.
