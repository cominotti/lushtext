# Gate order for a workflow change

## Contents

- [Before the first gate](#before-the-first-gate)
- [The ordinary sequence](#the-ordinary-sequence)
- [Mutation parity, when policy moves](#mutation-parity-when-policy-moves)
- [Smoke lanes, last and from clean roots](#smoke-lanes-last-and-from-clean-roots)
- [At ship time](#at-ship-time)

## Before the first gate

**`git add -N` every new file.** Diff-aware gates —
`make check-visual-proof-policy`, the diff-aware half of
`make check-accessibility-policy`, and `make mutants-diff` — build their changed
file set from `git diff <base>`, which does not list untracked paths at all. A
change that adds whole new role-home directories otherwise gets a **green** gate
computed over a file set that omits all of its new code, and a diff-scoped
mutation run that generates no mutants for it.

```bash
git add -N $(git ls-files --others --exclude-standard)
```

A green diff-aware gate on a worktree with untracked new source files is not
evidence. When adding the files changes a gate's digest and it starts failing, the
fix is to re-run the lane, not to unstage the files and take the earlier green.

## The ordinary sequence

Run these in order; each is cheap relative to the next.

```bash
make fmt                       # NOT bare `cargo fmt --all` -- that misses tests/widget/
make check                     # fmt check + all-feature Clippy + fast policy audits
cargo check -p lushtext-core --lib   # default features
```

`cargo check --lib` with **default** features is not redundant with `make check`.
`make check` runs Clippy with `--all-features`, which hides `unused_imports` and
dead-code gate mismatches that a default-feature build emits — a real
pre-existing-blocker class in this tree.

```bash
RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" \
  cargo doc --workspace --no-deps
```

The rustdoc gate is **not** in `make check`, `make pre-commit`, or
`make check-policy`, but CI's `Lint` job enforces it. Always run it for a new
facade or role home: a narrative facade lives in a `pub` module and naturally
wants to name its own coordination modules, `pub(crate)` seam types, and non-`pub`
entry points — every one is a `private_intra_doc_links` error. The fix is always
to drop the link and keep the name in backticks.

```bash
make check-workflow-boundaries   # also inside make check-policy
```

Fails on: policy impurity, a `policy.rs` the mutation scope cannot reach, a
GTK-free `ui/` module holding decision logic with no declared role, an undeclared
`.rs` file in a migrated role home, incomplete or absent roles for a `migrated`
row, a claimed path that does not exist, an over-budget facade, an externally
reachable `*_for_test` count above the recorded figure, and matrix/ledger
disagreement.

```bash
make test        # or a narrower nextest filter while iterating
```

## Mutation parity, when policy moves

Required evidence, not optional, whenever pure policy relocates between
directories:

```bash
make mutants-diff
```

Record generated and killed counts **before and after** the move, measured from
the tool on both sides. Report a **relocation parity** only when an entry already
selected the file; otherwise report a **gain from zero**. Never run this
concurrently with the widget lane — both saturate the machine, and a mutation run
under widget-lane load produces timeouts that read as survivors.

## Smoke lanes, last and from clean roots

**Any edit under `crates/lushtext-core/src/ui/` or `crates/lushtext/tests/widget/`
voids the accessibility, visual, and visual-geometry summaries.** Those gates key
on the **directory prefix**, not on `*.rs`, so even a module-layout `AGENTS.md`
under `ui/` invalidates all three. Run them **last**, after every source edit is
final, sequentially, each from a clean artifact root:

```bash
rm -rf build/smoke/visual-geometry && make visual-geometry-smoke
rm -rf build/smoke/accessibility  && make accessibility-smoke
rm -rf build/smoke/visual         && make visual-smoke
```

A stale case directory from a previous run can make the root summary report
failures or evidence the current binary did not produce. A skipped, filtered,
name-only, or rectangle-only visual-geometry run does not count as proof.

## At ship time

Staging files changes the visual-proof digest. If `make pre-commit` fails on
`check-visual-proof-policy` after you stage, re-run `make visual-geometry-smoke`
against the staged tree — do not unstage to recover the earlier green.

```bash
make pre-commit
```

Never use `--no-verify`, and never `-c commit.gpgsign=false`.

Two acceptance gates cannot be discharged headlessly and are **user-gated**: the
live `make run` walkthrough against restored workspaces, watching stderr for
`Trying to measure GtkBox ...`, `pixman_region32_init_rect`, `Gtk-CRITICAL`, and
`GLib-GObject-WARNING`; and the manual Orca check in
`docs/accessibility-orca-checklist.md`. State the gap and whose decision it
awaits; do not record either as accepted on the change's own authority.
