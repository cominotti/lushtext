# Gate order for a workflow change

## Contents

- [Before the first gate](#before-the-first-gate)
- [The ordinary sequence](#the-ordinary-sequence)
- [Mutation parity, when policy moves](#mutation-parity-when-policy-moves)
- [Smoke lanes, last and from clean roots](#smoke-lanes-last-and-from-clean-roots)
- [At ship time](#at-ship-time)

The general gate order, the rustdoc lint gate and the untracked-file hazard are
normative in `.agents/rules/testing.md` ("Proof Obligations of a Change" and
"Gate Order Before a Commit"); this page adds what a workflow change needs on top.

## Before the first gate

**`git add -N` every new file** (testing.md, "Proof Obligations of a Change"). A
change that adds whole new role-home directories otherwise gets a **green**
diff-aware gate computed over a file set that omits all of its new code, and a
diff-scoped mutation run that generates no mutants for it.

```bash
git add -N $(git ls-files --others --exclude-standard)
```

## The ordinary sequence

Run these in order; each is cheap relative to the next.

```bash
make fmt                       # NOT bare `cargo fmt --all` -- that misses tests/widget/
make check                     # fmt check + all-feature Clippy + fast policy audits
cargo check -p lushtext --bins --locked   # default features
```

The default-feature check is not redundant with `make check`, whose
all-feature Clippy hides `unused_imports` and dead-code gate mismatches.

Run the rustdoc lint gate from testing.md for every new facade or role home: a
narrative facade lives in a `pub` module and naturally wants to name its own
coordination modules, `pub(crate)` seam types, and non-`pub` entry points —
every one is a `private_intra_doc_links` error. The fix is always to drop the
link and keep the name in backticks.

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

**Any edit under `crates/lushtext-ui/src/ui/`, `crates/lushtext/tests/widget/`,
`resources/ui/` or `resources/style/` voids the accessibility, visual, and
visual-geometry summaries.** Those gates key on the **directory prefix**, not on
`*.rs`, so even a module-layout `AGENTS.md` under `ui/` invalidates all three.
Run them **last**, after every source edit is final, sequentially, each from a
clean artifact root:

```bash
rm -rf build/smoke/visual-geometry && make visual-geometry-smoke
rm -rf build/smoke/accessibility  && make accessibility-smoke
rm -rf build/smoke/visual         && make visual-smoke
```

An early `SKIP:` (the visual smoke's missing-binary check, for example) exits
before the lane writes a summary, so without the `rm -rf` the previous run's
summary would read as a fresh result. A skipped, filtered,
name-only, or rectangle-only visual-geometry run does not count as proof.

## At ship time

Staging files changes the visual-proof digest. If `make pre-commit` fails on
`check-visual-proof-policy` after you stage, re-run `make visual-geometry-smoke`
against the staged tree — do not unstage to recover the earlier green. Commit
through `scripts/heavy.sh`, and never with `--no-verify` or
`-c commit.gpgsign=false` (testing.md, "Gate Order Before a Commit"; `git.md`).

Two acceptance gates cannot be discharged headlessly and are **user-gated**: the
live `make run` walkthrough against restored workspaces, watching stderr for
`Trying to measure GtkBox ...`, `pixman_region32_init_rect`, `Gtk-CRITICAL`, and
`GLib-GObject-WARNING`; and the manual Orca check in
`docs/accessibility-orca-checklist.md`. State the gap and whose decision it
awaits; do not record either as accepted on the change's own authority.
