# Lane cases

The lane cases of tasks.md, each run once on this branch (2026-11-14). Lane
case 4.3 is in `address-space.md`.

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
