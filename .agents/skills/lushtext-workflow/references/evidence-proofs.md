# Evidence-surface proofs

## Contents

- [The three proofs](#the-three-proofs)
- [Proof 1 — side-effect freedom across mutation](#proof-1--side-effect-freedom-across-mutation)
- [Proof 2 — disposal honesty](#proof-2--disposal-honesty)
- [Proof 3 — materialization neutrality](#proof-3--materialization-neutrality)
- [Shape reminders](#shape-reminders)

The invariants these prove are normative in
`.agents/rules/workflow-convention.md`. This file is the recipe.

## The three proofs

| Proof | Question | Owed by |
| --- | --- | --- |
| Side-effect freedom | does reading the surface change anything? | every surface |
| Disposal honesty | does reading a disposed widget's surface answer instead of panicking? | every surface with a `TemplateChild`-derived field |
| Materialization neutrality | does reading make the toolkit create children? | every surface whose workflow owns a lazy GTK collection |

## Proof 1 — side-effect freedom across mutation

**The test drives the workflow, then reads.** It does not read while a borrow is
held — that is the panic the constraint prevents, not a demonstration of it.

Recipe:

1. Build the widget and read the surface once; keep the value.
2. Drive **each** operation that takes a mutable borrow of the state the accessor
   reads — every install, clear, submit, supersede, retire, and journal write the
   workflow has.
3. After each one, read the surface again and assert the fields that operation was
   supposed to change did, and that reading twice in a row with no intervening
   operation returns identical values.

Reference implementations, both in `crates/lushtext/tests/widget/`:

- `search_panel::test_evidence_reads_stay_side_effect_free_across_journal_mutation`
- `editor_page::test_load_evidence_reads_stay_side_effect_free_across_load_mutation`

**Do not compare the whole surface when it mixes per-session state with a
process-wide counter.** A counter advanced from a worker thread
(`SNAPSHOT_WORKER_DROPS` in `ui/buffer_snapshot.rs`) can move between two
consecutive reads without either read causing it, so a whole-surface equality
silently asserts "no background work landed while I was looking" — which is not
the rule, is not stable, and fails at random under load while *looking* exactly
like the accessor mutating its own metric. Compare the part the invariant is
about. For the counter itself, quiesce the owning lane first and assert it was
still quiescent afterwards; only then is an advancement attributable to the reads.

## Proof 2 — disposal honesty

GTK4 clears template children in `dispose()`, **before** Rust's `Drop`. Any field
derived from a `TemplateChild` must be read through `try_get()` and answer
honestly when the child is gone; the panicking accessor turns a teardown
observation into a crash.

The transitive half is the one that bites: a surface built from the workflow's own
**production accessors** inherits their panics, because production only ever calls
them on a live window, so those accessors are correct and the surface is not.
Derive the fact defensively inside the surface. Never widen a production accessor
to `try_get()` for the surface's benefit — that changes production behaviour to
satisfy an observation.

Recipe:

1. Build the widget and read the surface (baseline).
2. **Detach any still-parented child first** — for a popover,
   `menu_button.set_popover(None)`. Calling `run_dispose()` on a child whose parent
   still holds a reference emits *"has a parent ... during dispose"*, a
   `Gtk-CRITICAL` the widget lane fails on, and the observation then reads as a
   teardown-order bug in the test rather than a fact about the surface.
3. `run_dispose()`.
4. Read the surface again and assert it returns — with honest values for the gone
   children — rather than panicking with *"Failed to retrieve template child"*.

A field aggregated over a variable-sized set of child widgets must also be
bounded, answer honestly when the set is empty, and **skip** a disposed child
rather than panicking on it.

## Proof 3 — materialization neutrality

A GTK collection may create its children on demand — `GtkTreeListModel` is the one
in this tree — so an accessor that walks one *performs* work: it can materialize
descendants, register stores, start background scans, and restart filesystem
watches, while every field it produced still reads as a pure observation.

**Prove it, do not assert it.** Recipe:

1. Read the surface with the collection **unmaterialized**; capture the admission
   counters, registries, generations, and derivation metrics.
2. Materialize the collection through the workflow's normal path.
3. Read the surface again.
4. Assert every counter, registry size, generation, and metric is identical before
   and after **each** read.

If the workflow's own code reaches such an accessor safely only because of a
guard, derive the field from the workflow's authoritative state instead of
repeating the guarded walk.

## Shape reminders

- **One accessor reads the whole surface.** A workflow spanning two separately
  observable GObjects — where one exists in tests without the other — may expose
  one accessor per object (`workspace_tree_evidence` / `workspace_section_evidence`;
  `WFR-RECENT-DOCUMENTS` follows that precedent). State the plural in the module
  doc and the matrix row so it reads as a followed precedent, not a relapse.
- **Compute then build.** Drop every `Ref` before the struct literal, so no borrow
  outlives the value it produced. Never add a second, narrower accessor to make a
  nested read possible.
- **Gate it at the narrowest visibility its readers need.** A `test-utils`-gated
  surface means production never reads it, which is usually right; automation
  snapshot fields project from the surface only where production already reads it.
- Test-only timing and limit overrides belong in the workflow's one
  `test_policy.rs`, and no override storage may compile without the test feature.
- Widget tests needing accessibility metadata proof without a live bridge use
  `ui::accessibility::test_audit::AccessibleAudit`; it does not replace
  `make accessibility-smoke`.
