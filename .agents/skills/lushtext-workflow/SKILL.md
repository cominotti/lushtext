---
name: lushtext-workflow
description: "Procedure for adding or restructuring a LushText workflow under the readability convention: the WFR-* matrix row, role home, policy.rs, seam value objects, coordination names, evidence.rs and its proofs, and the gate order. Use when a change under crates/lushtext-ui/src/ui/** adds or moves a module, changes a workflow's policy, evidence surface or matrix row, or when make check-workflow-boundaries fails."
---

The convention is normative in `.agents/rules/workflow-convention.md`. This skill
is the **procedure**: the ordered checklist, the proofs, and the gate order. Read
the rule for what must be true; follow this for what to do.

The migration programme is closed — every workflow has a terminal `WFR-*` row.
You are adding to or amending a settled convention, not migrating a backlog.

## Checklist

### 1. Find or create the row

`docs/workflow-readability-matrix.md` is the completion source of truth.

- Changing an existing workflow: read its `WFR-*` row **before** touching code. A
  row marked `exempt`, `cross-cutting`, or `superseded` records a decision with
  evidence; do not override it by restructuring.
- A genuinely new user operation with ordered stages: add a row. A surface with no
  ordered stages goes in the matrix's no-coordination-tier list instead.
- Sharing one GTK subclass's `imp` struct is **not** evidence that two surfaces
  are one workflow. Derive the stage orders, resumption points, shared
  coordination state, and external entry surface first.

### 2. Choose the role home and record it

Choose a role-home shape from `workflow-convention.md`, "Role homes" —
**flat**, **per-workflow subdirectory** (needed when a sibling already owns
`policy.rs` or `evidence.rs`; never a prefixed `save_policy.rs`), or **nested** —
and record the choice in the row.

### 3. Write `policy.rs` first, and prove the scope reaches it

Extract the pure decisions before wiring anything. No `gtk4`, `glib`, `gio`,
`libadwaita`, or `sourceview5` import.

```bash
make mutants-list | grep 'ui/<your-path>/policy.rs'
```

An empty result means the file is outside the mutation scope — usually a
misnamed file or a wrong home. Fix the placement, not the config. Record the
before/after mutant counts in the row: a **gain from zero** when the module did
not exist, a **relocation parity** when it moved, measured from the tool on both
sides. The two claims are different and must not be mixed.

If the workflow's pure decisions are all cross-cutting, it owns **no**
`policy.rs`, and the row is still complete — but you must **probe** the adapter
for separable pure decisions and record the negative finding. Never fork a shared
limit or copy part of a shared module to manufacture a local one.

### 4. Reify the seams

Apply `workflow-convention.md`, "Seam value objects": a bundle crossing two or
more boundaries becomes one value object built at the entry point, reusing
`*Ticket` + `*Facts` + `*_is_current` or a coordinator's generation identity. An
`#[expect(clippy::too_many_arguments)]` on a cross-module boundary is an
unreified seam to fix.

### 5. Name coordination for the job

Use the bounded names in `workflow-convention.md`, "Roles" (`admission`,
`execution`, `retirement`, `watch`, `journal`, stage-qualified only when one
workflow owns several orders; never `runtime.rs`). A job none of the five
describes means amending `openspec/specs/gtk-adapter-module-boundaries/spec.md`,
not overloading a name.

### 6. Declare the presentation surfaces

Anything in the role home that only projects onto widgets is a **called
presentation surface**: no role name, no `policy.rs`, no `evidence.rs`. Say so in
its module doc **and** name it by a backticked repository path in the row's
`Migrated Workflow Roles` subsection — a bare stem or a brace expansion leaves the
gate blind and the file reads as undeclared.

### 7. Build the evidence surface and its proofs

One `evidence.rs`, read by widget tests. Do not add a `*_for_test` getter per
field: the gate ratchets that count against the figure recorded under the matrix's
`### Externally reachable test-seam ceiling`.

See [references/evidence-proofs.md](references/evidence-proofs.md) for the three
driven proofs, their reference implementations, and the disposal/materialization
mechanics.

### 8. Narrate the facade

`//!` docs name the ordered stages in intent terms and, for every deferred drain,
idle callback, or worker completion, the point where control resumes. Keep timers,
budgets, generation counters, and widget mutation out of it. Stay inside the 370
physical-line budget. Use the `rust-comments` skill for the doc-comment pass.

Public module docs must not intra-doc-link private items; run the rustdoc gate
(`.agents/rules/testing.md`, "Proof Obligations of a Change"), which the local
gates do not include.

### 9. Re-derive the row's measured cells

Row-scoped, from the code, in this change: current size, per-kind test seam
counts, pure-policy consumer count. Exclude `#[cfg(test)]` modules, including a
co-located test module behind `#[cfg(test)] mod tests;`. Name any shared
population an old cell pooled. A correction may move a figure **either** way.

### 10. Run the gates in order

See [references/gate-order.md](references/gate-order.md). Two hazards worth
knowing before you start: untracked files are invisible to every diff-aware gate,
and any edit under `crates/lushtext-ui/src/ui/`, `resources/ui/`, or
`resources/style/` (or automation/smoke sources) voids the accessibility, visual,
and visual-geometry summaries (`make check-smoke-proof` at `pre-push` /
`verify-local`; edits solely under `crates/lushtext/tests/widget/` still run
`make check-accessibility-policy` static checks and `make test-widget-headless`).

## What not to do

- Do not split a file into siblings without assigning roles; that is a topical
  split wearing the convention's clothes. Pre-convention focused siblings are
  **classified** — each becomes a role or a called presentation surface — not
  renamed into roles: the topical split is what the modules *do*, the role is what
  they *are* to the workflow, and neither replaces the other.
- Do not add a trait, manager type, or crate to express a role split.
- Do not place a module in `model/` to obtain mutation or test reach.
- Do not add a test-only actuation seam; report the dialog/timer boundary that is
  missing instead, and consult the `gtk-testing` skill's seam taxonomy.
- Do not amend the convention without re-migrating every migrated workflow in the
  same change.
- Do not rewrite `docs/next/workflow-readability.md`; it is frozen except its
  deferral inventory, which may be appended.
