---
description: The workflow readability convention — roles, homes, seams, policy purity, and evidence surfaces
paths:
  - "crates/lushtext-core/src/ui/**"
  - "crates/lushtext/tests/widget/**"
  - "docs/workflow-readability-matrix.md"
  - "docs/next/workflow-readability.md"
  - "scripts/check-workflow-boundaries.py"
  - ".cargo/mutants.toml"
---

# Workflow Readability Convention

The **normative** home for the convention; `rust.md`, `widget-wiring.md`,
`build.md`, and `AGENTS.md` point here rather than restating it. For the ordered
procedure — how to add or change a workflow, write the proofs, run the gates — use
the `lushtext-workflow` skill.

The migration programme is **closed**: every workflow has a terminal row in
`docs/workflow-readability-matrix.md` (22 `migrated`, 5 `cross-cutting`, 1
`exempt`, 1 `superseded`), and the gate rejects any transitional status. New work
adds or updates a row and follows the convention; it does not re-open the
programme. `docs/next/workflow-readability.md` is the closed programme record and
is **frozen** except its deferral inventory, which may be appended.

## What a workflow is

One user-initiated operation with ordered stages that crosses the adapter boundary
into coordination and pure policy — Ctrl+S, workspace search, draft recovery. Its
domain vocabulary is what a reader learns first; `rust.md`'s `## Coordination
Vocabulary` is the tier a workflow reaches into, not the entry point.

`docs/workflow-readability-matrix.md` is the completion source of truth: every
workflow has a stable `WFR-*` row. Read it before restructuring, so an `exempt`,
`cross-cutting`, or `superseded` classification is not silently overridden, and
update it in the same change as the code.

## Roles

A migrated workflow assigns each module exactly one role.

- **Narrative facade** — the workflow's public module surface. Narrates the
  ordered stages with their intent named and delegates every stage. It must not
  own timers, admission bookkeeping, generation counters, or GTK widget mutation.
  Where stages are connected by a deferred drain, idle callback, or worker
  completion rather than a direct call, it documents that inversion and names
  where control resumes. The normative size budget is the figure recorded in the
  matrix's "Facade size budget" section; changing it follows the
  retroactive-amendment rule.
- **Seam value objects** — the identity/freshness/intent values below.
- **Pure policy** — `policy.rs`, one per workflow, in that workflow's directory.
- **Coordination** — one module per coordination job, named from the bounded set
  `admission`, `execution`, `retirement`, `watch`, `journal`; a workflow may own
  more than one. `runtime.rs` is not a role name: it says only "machinery".
  `journal` maintains a durable generation-guarded record a later stage reads
  back — installing and clearing it under a freshness guard, writing and deleting
  it on a worker, recovering it at startup with stale-record cleanup, and handing
  it back — the opposite of `retirement`. Where **one** workflow owns several stage
  orders in one directory and more than one needs the same-shaped role, the name
  may qualify a bounded role with the stage order it serves
  (`query_execution.rs`, `replace_execution.rs`) in the workflow's own vocabulary,
  suffix still bounded; do not take an ill-fitting bounded name because the
  fitting one is spent elsewhere. A job no listed name describes requires amending
  `openspec/specs/gtk-adapter-module-boundaries/spec.md`. The bounded set is a
  **review** contract — the gate checks only that declared role paths exist.
- **Evidence** — `evidence.rs`, one per workflow, at the narrowest visibility its
  readers need.

**Called presentation surface — not a role.** A module that only projects the
workflow onto widgets (subclass state and template children, list-factory row
projection, the row model object a factory binds, context-menu and gesture
lifecycle, row accessibility projection, shared dialog chrome, window-side target
resolution, a per-surface capture adapter a role home calls) is outside the
five-name taxonomy: it MUST NOT take a role name and MUST NOT own a `policy.rs` or
`evidence.rs`. Record it in **both** its module doc and its matrix row. Do not
call it "adapter detail" — that label is defined nowhere and was retired.

Splitting a large file into siblings without assigning roles does not satisfy the
convention. Roles are plain modules and narrow owner references; do not add a
trait, manager type, or crate solely to move code. A migrating workflow
**classifies** its pre-convention focused siblings rather than choosing between a
topical decomposition requirement and the role requirement. The topical split is
what the modules *do*; the role, where one applies, is what they *are* to the
workflow, and neither replaces the other.

## Role homes

Two permitted homes. A workflow whose role file names do not collide with a
sibling's keeps **flat**, workflow-scoped role names in the shared directory.
Where a directory hosts several workflows and more than one owns pure policy or an
evidence surface, the roles MAY live in a **per-workflow subdirectory** whose
`mod.rs` is the facade and whose role files keep the unqualified names
(`ui/editor_page/save/`). A workflow-prefixed `save_policy.rs` is not a
substitute: it leaves the `ui/**/policy.rs` mutation scope. Migration never
requires restructuring a whole directory; the choice is recorded in the row.

A home may also be **nested**: where one workflow owns a directory and a widget
subdirectory of it, name one the canonical role home — facade, the single
`policy.rs`, the single `evidence.rs` — while modules in the other take bounded
coordination names or are recorded as called presentation surfaces. The
`ui/**/policy.rs` glob reaches either location, which a move must **verify
afterwards** rather than assume.

**Every `.rs` file in a migrated row's role home is declared.** A file that is not
`mod.rs`, `imp.rs`, a fixed role name, or a bounded/stage-qualified coordination
name must be named by a **backticked repository path** in that row's matrix text;
a bare stem or brace expansion leaves the check blind. Declaring it is the fix —
renaming a presentation surface into a role it does not perform is a false role
claim, and weakening the check is a silent disarm. The check reaches only migrated
rows' role homes — the facade's directory plus the subdirectories that row names,
non-recursive in each — so shared directories such as the `ui/window/` and
`ui/editor_page/` roots are outside it; moving a module up out of a role home is a
classification change that must be reflected in the row, not a way to clear a
finding.

## Seam value objects

Reify a field bundle as a named value object when it crosses **two or more**
function boundaries or is reconstructed at two or more call sites; construct it
once at the entry point and validate it as a unit. A bundle used by exactly one
private helper does not need one — the rule targets seams, not long signatures.

**A value must not be renamed while crossing a seam.** Passing a value that means
one thing into a parameter that names it something else is invisible to review and
to tests; reify the bundle so the mismatched call becomes a type error.

Reuse the existing shapes. **Ticket + Facts + predicate**: a `*Ticket` captures
the dispatch-time expectation, a `*Facts` the observed live state at completion,
and one `*_is_current(ticket, facts)` validates them together
(`DraftRestoreTicket` + `DraftRestoreFacts`; the `Ticket::is_current(&editor)`
variant reads live state directly, as `SaveCompletionTicket` does).
**Coordinator generation identity**: a coordinator already owning the generation
and exposing `is_current(generation)` *is* the seam value object.

`#[expect(clippy::too_many_arguments)]` on a cross-module workflow boundary marks
an unreified seam to fix. Domain catalog construction in `model/` whose parameters
each name a documented external contract field is outside this rule.

## Policy purity and placement

A `policy.rs` must contain no `gtk4`, `glib`, `gio`, `libadwaita`, or
`sourceview5` import; that purity is what keeps it inside the mutation scope,
which reaches `ui/**/policy.rs` by convention.

**Because that scope is decided by name, the converse defect is the one to watch
for: pure decision logic in `ui/` under any other file name is silently outside
the scope while every command exits 0.** The gate therefore also **discovers**
GTK-free `ui/` modules not named `policy.rs` and fails on any holding decision
logic with no declared role. A GTK-free facade, `seams.rs`, coordination role,
`evidence.rs`, or `test_policy.rs` is already correctly named; a genuinely
cross-cutting module says so in its module doc and names its owning row.
Declaring a role a module does not perform is a false role claim. Report such a
rename's mutation result as a **gain from zero** where the module was never in
scope, and as a **parity claim** only where an entry did select it.

Pure policy moves beside its consumer only when it has a single **owning
workflow** — eligibility counts owning workflows, not consuming files, so policy
whose only consumer is a coordination adapter serving several workflows is
cross-cutting and stays shared. Do not place a module in `model/` solely to obtain
tooling reach.

A workflow whose pure decision logic is **entirely** cross-cutting owns no
`policy.rs` and is still a **complete** row: it declares no pure policy role and
names the cross-cutting module plus the other owning workflows. That absence is a
conclusion reached by **probing** the workflow's own adapter for separable pure
decisions and recording the negative finding, never an unmet obligation. Never
manufacture a local `policy.rs` by copying part of the shared module, and never
duplicate a shared limit or arithmetic to obtain one, because a forked limit can
drift while both copies still read as correct. A one-line delegating alias under a
second domain name is not a duplicate and may stay when it makes the calling
workflow narrate in its own vocabulary; say so in the alias's doc comment.

## Evidence surfaces

A migrated workflow exposes one typed `evidence.rs` that is the single source of
its observable state; widget tests read that value. Do not add another
`pub fn *_for_test` getter for a counter, pending flag, queue depth, bound, or
freshness token — extend the surface. A new per-field inspection function
regresses to the shadow introspection API the surface replaced, and the gate
ratchets the externally reachable `*_for_test` count recorded under the matrix's
`### Externally reachable test-seam ceiling`.

- **One accessor reads the whole surface.** A workflow spanning two separately
  observable GObjects — where one exists in tests without the other — may expose
  one accessor **per object** (`workspace_tree_evidence` /
  `workspace_section_evidence`; `WFR-RECENT-DOCUMENTS` follows it). Each still
  reads its object's whole surface, and the plural is stated in the module doc and
  the matrix row.
- **Reading must not mutate** state, timers, queues, or generation counters, and
  must not require a particular stage.
- **A disposed widget is a stage.** GTK4 clears template children in `dispose()`,
  before `Drop`, so any field derived from a `TemplateChild` is read through
  `try_get()`. This applies **transitively**: a surface built from the workflow's
  own production accessors inherits their panics, because production only calls
  them on a live window. Derive the fact defensively; never widen a production
  accessor for the surface's benefit. Disposing a still-parented widget is itself
  a `Gtk-CRITICAL` (*has a parent … during dispose*), which the widget lane fails
  on; detach first (for a popover, `menu_button.set_popover(None)`), then dispose.
- **Reading must not make the toolkit do work.** A GTK collection may create
  children on demand (`GtkTreeListModel`), so walking one *performs* work:
  materializing descendants, registering stores, restarting watches. A surface
  MUST NOT call such an accessor, nor a derivation that mutates a cache or
  advances a counter **the surface itself reports**. Derive from the workflow's
  authoritative state instead of repeating a guarded walk, and prove it rather
  than asserting it.
- **A field aggregated over a variable-sized set of child widgets** must be
  bounded, answer honestly when the set is empty, and **skip a disposed child
  rather than panicking on it**.
- **No field may be read from inside a mutable borrow of the state the accessor
  reads** — a runtime panic, not a compile error. Compute every derived scalar and
  drop each `Ref` before building the struct literal, record the constraint in the
  module doc, and never add a second, narrower accessor to make a nested read
  possible.
- **A surface mixing per-session state with process-wide counters is not compared
  as one value in a reentrancy proof**, because a counter advanced from a worker
  thread (`SNAPSHOT_WORKER_DROPS`) can move between two consecutive reads without
  either read causing it. Compare the part the invariant is about; for the counter
  itself, quiesce the owning lane first and assert it was still quiescent
  afterwards.
- A surface is an internal type at the narrowest visibility its readers need,
  never added to the public D-Bus schema; automation snapshot fields *project*
  from it.
- Test-only timing and limit overrides live in the workflow's one
  `test_policy.rs`, and no override storage may compile without the test feature.
  Test-only actuation seams are a deferred category, not a pattern to extend; see
  the `gtk-testing` skill's seam taxonomy before adding one.

Every surface owes **three driven proofs** — side-effect freedom across each
operation that takes a mutable borrow, disposal honesty, and, where a lazy
collection is in reach, materialization neutrality. Reference implementations are
`search_panel::test_evidence_reads_stay_side_effect_free_across_journal_mutation`
and `editor_page::test_load_evidence_reads_stay_side_effect_free_across_load_mutation`;
the recipes are in the `lushtext-workflow` skill. Do not write a test that reads
the surface *while* a borrow is held: that is the panic the constraint prevents,
not a demonstration of it.

## Intent-first naming

Public, `pub(crate)`, `pub(super)`, and cross-module workflow operations are named
for the workflow intent they express, not the mechanism they use. Private helpers
inside a coordination module may keep mechanism names when the owning module makes
the mechanism obvious.

## Re-deriving a row's measured cells

A change that migrates or materially restructures a workflow **re-derives** its
row's measured cells — current size, per-kind test seam counts, pure-policy
consumer count — from the code and corrects them in the same change rather than
inheriting earlier figures. Re-derivation is **row-scoped**: count only what the
workflow owns, never pooling shared service files, cross-cutting modules, or
neighbouring files it merely calls. Size figures count production lines, excluding
`#[cfg(test)]` modules — including a co-located test module in its own file behind
`#[cfg(test)] mod tests;`, which a naive per-file scan counts as production. Name
any shared population an old cell pooled, with the rows that share it. A
correction may move a figure in **either** direction, and an unchanged cell is not
the expected outcome.

## The gate

`make check-workflow-boundaries` (also in `make check-policy`) enforces the
mechanical half; `build.md` describes what it runs and when. Amending the
convention requires re-migrating every already-migrated workflow in the same
change: two generations of the convention must not coexist.
