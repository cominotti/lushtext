# Lane cases

The lane cases of tasks.md, each run once on this branch (2026-11-14/15).
Lane case 4.3 is in `address-space.md`.

## 3.5 Warm slots compile no dependency

The branch rounds of task 9.1 (`after.md`) are `make mutants-smoke` runs on
slots an earlier run had warmed. In the second one (`m91-branch-h2`) every
`Compiling` line in every slot's logs is a workspace member: slot-1 and
slot-2 compiled `lushtext-core` 6 times and `lushtext` 5 times, slot-3 4 and
2 times (the baseline plus one per mutant). No dependency crate was compiled,
and each unmutated baseline built in 12-13 s against main's cold 38-40 s.

## 8.3 Docs-only and code commits

Both commits went through `scripts/heavy.sh --mem none -- git commit -F ...`.

- **Code commit** `cda838ca` (`Makefile`, `scripts/run-mutants.sh`,
  `.githooks/pre-commit`): the hook printed `Running LushText pre-commit checks
  (rustfmt + all-feature Clippy + policy audits)...` and ran `cargo clippy
  --workspace --all-targets --all-features -- -D warnings`. `f19f7878` did the
  same.
- **Docs-only commit** `137611a5` (`evidence/address-space.md`, `tasks.md`):
  the hook printed `precommit-scope: docs-only: 2 staged path(s), all
  documentation` and `Docs-only commit: rustfmt + policy audits (Clippy
  skipped: no Rust build input staged)`, and the log has no `cargo clippy`
  line.

Finding: `core.hooksPath` is the absolute path of the main checkout's
`.githooks/`, so a commit in a worktree runs the hook of whatever the main
checkout has checked out, not the worktree's. The first docs-only commit here
(`e3074e56`) therefore ran `main`'s hook and the full gate. The docs-only
commit above ran this branch's hook through `git -c core.hooksPath=.githooks
commit` (a per-command override that still runs the hook). Two consequences:

- the docs-only path reaches worktrees once `main` carries this hook and the
  main checkout is updated;
- worktrees whose branch predates `scripts/precommit-scope.py` will run the
  new hook, so it checks the classifier exists (`[[ -x ... ]]`) and runs the
  full gate when it does not.

## 7.2 `make flatpak`

`scripts/heavy.sh --mem 16G -- make flatpak`, 2026-11-15 01:47-02:24 (most of
it queued for memory admission behind other agents), exit 0.

- Both `flatpak-builder` calls (`flatpak-deps`, then the build) passed
  `--state-dir=~/.cache/lushtext-build/checkouts/reduce-build-disk-writes-e94ec5dc07b4fa99/flatpak-builder`
  and built into `.../flatpak-app` beside it. The module built in
  `<state-dir>/build/lushtext-1` (Cargo release build, 2m 13s) and every stage
  was committed to the state dir's cache.
- The checkout had no `.flatpak-builder` before the run and has none after,
  so nothing was created or grown there.
- The log has no `Invalid cross-device link` and no `EXDEV`: the state dir and
  the app dir sit on the same filesystem, so `flatpak-builder`'s hardlinks
  work.
- After the run the state dir holds 48 MB (the build dir is removed on
  success) and the app dir 11 MB.
- `make flatpak-install` and `make verify-flatpak-identity` were **skipped**:
  they replace the maintainer's installed `dev.cominotti.lushtext` (installed
  from `lushtext-origin` and `cominotti`), which this stage was not
  authorized to do.

## 5.3 `make proof-strength`, cheapest oracle module

`setsid nohup scripts/heavy.sh --mem none -- make proof-strength
PROOF_STRENGTH_MODULE=crates/lushtext-core/src/ui/markdown_preview/policy.rs
PROOF_STRENGTH_TIER=ci`, 2026-11-15 01:47-02:17 at revision `f91d7c4a`
(the cheapest module in `make proof-strength-list PROOF_STRENGTH_TIER=ci`,
5 mutants).

- **Kill counts match the record.** Both 5, Either 5, Kani survivors 0, Kani
  timeouts 0, vacuity 0, the same as the last recorded run for this module
  (`848f94f8` in `docs/next/formal-verification.md`). The tests arm, now
  `cargo mutants --in-place` in the worktree, ran its unmutated baseline
  (89 s build + 69 s test) and caught all 6 mutants in its scope. The "floor" column (whole-scope
  field-deletion mutants a name filter also returns, dropped and reported)
  reads 47 against the record's 35, because the scope has gained struct
  fields since `848f94f8`; it is not a kill count.
- **No new `~/.cache/cargo-build` entry from this run.** 17 first-level
  entries appeared in the window, all from other work: three are this
  stage's concurrent copy-mode `main` round (their build-script outputs name
  `/tmp/cargo-mutants-reduce-build-disk-writes-*`), the rest name other
  worktrees or belong to other agents. None names the proof-strength worktree.
  The run's build state is in the worktree under the build cache root:
  `target/kani` 921 MB, `target/tests-build` 6.6 GB, `target/tests` 2 MB.
- **Nothing under `target/proof-strength/wt`.** The checkout's
  `target/proof-strength/` holds only `config/` and `results/`.
- **Results in `target/proof-strength/results`**, as before.
- The worktree is clean at `f91d7c4a` afterwards.
