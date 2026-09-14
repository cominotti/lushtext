## Why

The workflow-readability programme is closed — every row in
`docs/workflow-readability-matrix.md` is terminal — but the agent guidance that
teaches the convention is scattered across four rule files plus `AGENTS.md`,
written as programme history rather than as a standing rule, and partly
unreachable: `.claude/CLAUDE.md` is a dangling symlink and six `.agents/rules/*.md`
files scope themselves with Cursor's `globs:` key, which Claude Code does not
recognise. Two convention obligations that the programme enforced by review — that
every module in a migrated role home is classified, and that the `*_for_test`
shadow API does not grow back — have no gate at all, so the first post-closure
change can silently undo them.

## What Changes

- **Repoint `.claude/CLAUDE.md`** at the real `AGENTS.md`, and convert every
  `.agents/rules/*.md` `globs:` key to the `paths:` list Claude Code actually
  honours (or drop it, making the file explicitly global). `make check-agent-docs`
  gains a check that no `globs:` key remains and that every rule file is either
  path-scoped or explicitly marked global.
- **New rule `.agents/rules/workflow-convention.md`**, path-scoped to the UI
  sources, widget tests, matrix, programme record, gate script, and mutation
  config. It is the one normative home for the role taxonomy, the two/three
  permitted role homes, seam value objects, policy purity, re-derivation, and the
  evidence-surface invariants. The moved sections in `rust.md`,
  `widget-wiring.md`, `build.md`, and `AGENTS.md` become short pointers.
- **Post-closure freeze wording**: guidance that tells a reader to "advance the
  matching slot" or that "unmigrated workflows keep their `*_for_test` getters"
  describes a running programme that no longer exists, and is corrected.
- **New skill `.agents/skills/lushtext-workflow/`** — the procedural half the rule
  deliberately does not carry: the ordered checklist for adding or changing a
  workflow, the three evidence-surface proofs as recipes, and the gate order with
  its smoke-fingerprint hazards.
- **Two new mechanical gates** in `scripts/check-workflow-boundaries.py`:
  every `.rs` file in a migrated row's role home must be declared by that row, and
  the count of externally reachable `*_for_test` declarations may not exceed the
  figure recorded in the matrix.
- Seven matrix rows gain an explicit, machine-readable declaration of the called
  presentation surfaces and non-role modules that already sit in their role homes.

## Capabilities

### New Capabilities
- `agent-guidance-loading`: how repository agent guidance is made reachable and
  scoped — the `.claude` entry point, the `paths:`/global frontmatter contract for
  `.agents/rules/*.md`, and the check that enforces both.

### Modified Capabilities
- `workflow-readability-boundaries`: adds two mechanical obligations to the
  convention's gate — role-home module declaration and the `*_for_test`
  declaration ratchet — and states that after programme closure the record is
  frozen except for its deferral inventory.

## Impact

- `.claude/CLAUDE.md` (symlink), `.agents/rules/*.md` (frontmatter + section
  moves), new `.agents/rules/workflow-convention.md`, `AGENTS.md` (Rules Index,
  Workflow Role Convention section, Recent Changes).
- New `.agents/skills/lushtext-workflow/` plus its registration in
  `.agents/skills/skill-policy.toml`.
- `scripts/check-agent-docs.sh` (or its Python validator) and
  `scripts/check-workflow-boundaries.py`, both with self-tests.
- `docs/workflow-readability-matrix.md` (Measurement Definitions ratchet figure,
  seven role-home declarations) and an appended entry in
  `docs/next/workflow-readability.md`'s deferral inventory.
- No application source changes. Nothing under `crates/lushtext-core/src/` or
  `crates/lushtext/tests/widget/` is touched, so no accessibility, visual, or
  visual-geometry smoke fingerprint is voided.
