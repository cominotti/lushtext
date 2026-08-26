## Why

This is **slot 3a** of the workflow-readability programme recorded in
`docs/next/workflow-readability.md`. Slot 3 as the record scopes it is "save and
load": `WFR-DOCUMENT-SAVE`, `WFR-DOCUMENT-LOAD`, and the next
`WFR-AUTOMATION-SPINE` projections. **This change takes the save half only**, and
splits the slot into 3a (save) and 3b (load) under the ledger grammar the record
already sanctions for `2a`/`2b`. The rationale is in
[Why the slot splits](#why-the-slot-splits) below, because it is a decision a
reviewer must be able to reject.

Save is where the programme's second measured symptom lives, in the most literal
possible form. The record's headline example of a field bundle that **drifts while
crossing a seam** is this workflow:

> At `ui/editor_page/load_save.rs` the same value was passed as
> `cancel_pending_load` and received as `explicit_destination`, inside stale-save
> rejection. A reader cannot verify that by reading it.

The authoring inventory confirms it is still there, and that it is worse than one
call site. One boolean is stored as `cancel_pending_load` in `QueuedSave`
(`save_runtime.rs:39`) and in `SaveSubmission` (`:77`), and travels through
**three forwarding hops — two of which cross the rename**:
`save_runtime.rs:178-186` and `save_runtime.rs:252-259` forward it under its own
name into `begin_admitted_save`, and `load_save.rs:1387-1393` then hands it
positionally into `queued_save_is_current`'s `explicit_destination` parameter.
Inside the predicate it decides whether the staleness check compares the queued
path against `file_path()` at all (`load_save.rs:1351`). The two meanings coincide today only
because Save As happens to want both. Nothing enforces it, no test can see it,
and the failure modes are asymmetric and both bad: a plain save that wrongly
claims an explicit destination skips the path comparison that protects it from
writing a stale target, and a Save As that stops cancelling the pending load
races a load into a just-saved buffer. That is a data-safety seam, not a naming
nit.

The same function carries the programme's **only** non-catalog
`#[expect(clippy::too_many_arguments)]` (`load_save.rs:1372-1375`), which the
matrix's "Argument-count suppressions" section says the residual sweep asserts to
zero, and which it names `QueuedSaveTicket` as the removal mechanism for.

Save is also the workflow the programme cites for symptom 3 — "13 hops across 6
files to answer what happens on Ctrl+S, with no document or module narrating it"
— sharing a 1,795-line file with load, which is the file the record names as
"1,795 lines holding two workflows".

**Prerequisite, non-negotiable.** This change touches a durable write path that
replaces the user's file bytes. It is `tier-3`, so it may not begin until the
convention has been proven on at least two completed lower-risk migrations. That
gate is satisfied and then some: slots 1, 2a, and 2b are complete and three
workflow halves are migrated. Confirm it mechanically rather than by reading this
paragraph — see task 0.1.

### Why the slot splits

The record permits a slot to split, keeping its number and taking letter
suffixes, and slot 2 set the precedent. Four reasons apply here:

1. **Two tier-3 workflows, not one.** Slot 2 split because it *contained* a
   tier-3 half. Slot 3 contains two independently tier-3 workflows, each with its
   own durable or user-visible failure mode (save replaces file bytes; load
   installs decoded bytes into a live buffer and owns the encoding-recovery and
   cancellation paths). Bundling them puts both proof matrices in one change.
2. **Scale.** The matrix sizes the two rows at 6,672 and 5,301 lines with 34
   test-seam functions each. Slot 2b — one *half* of one workflow — needed a
   954-line task list and five evidence files. One change covering both rows
   would be the largest migration in the programme on its highest-risk paths.
3. **The shared file splits sequentially safely — but item by item, not by line
   range.** `load_save.rs` is 1,795 lines with **zero** in-file tests (all
   coverage is external), and the dominant clusters are a save cluster at 40-101
   and a save-heavy tail from roughly 1192 to the end, with the load half and its
   install slicing in between. **The two halves are not contiguous**, and a
   range-based extraction would be wrong: save-side seams sit at 1058-1143 inside
   the load region, the load-only `set_file_path_for_pending_load` sits at
   1215-1228 inside the save tail, `apply_restore_position` (1469-1530) is called
   from line 814 inside load's completion, and `ViewInteractivityState` (43-47)
   sits in the save cluster while being a field of the load-side
   `LoadInstallationState`. So the extraction is **item-level**, with the
   entanglements enumerated and owner-assigned in task 3.2. What the split does
   buy is sequencing: 3a can lift the save items out and leave a coherent
   load-only residual, and 3b dissolves the file. Doing it the other way round
   would leave the entangled half behind.
4. **The defect belongs to save.** `QueuedSaveTicket`, the argument-count
   suppression, and the renamed-value seam are all save-side, so they should not
   wait behind the larger load half.

**Ordering is fixed: 3a lands before 3b.** They share `load_save.rs`, and 3a's
spec delta establishes the per-workflow role home that 3b reuses. 3b's proposal
states the same dependency.

### The role-home problem this change is the first to hit

Both migrated workflows so far own a dedicated directory, so `mod.rs`,
`policy.rs`, and `evidence.rs` were free. `ui/editor_page/` hosts **eight**
workflows, and the fixed role file names cannot be shared:

- The convention fixes the names: "The pure policy role is named `policy.rs` and
  the evidence role is named `evidence.rs`, one of each per workflow."
- `.cargo/mutants.toml` reaches pure policy through the literal glob
  `crates/lushtext-core/src/ui/**/policy.rs`. A workflow-prefixed
  `save_policy.rs` would be **outside the mutation scope**, which
  `openspec/specs/mutation-testing/spec.md` classifies as a coverage regression
  that blocks the relocation.

So a flat name is mechanically unavailable for at least one of the eight
workflows, and the census already anticipated the answer without saying it out
loud: its relocation target for the minimap's policy is
`ui/editor_page/minimap/policy.rs`, a per-workflow subdirectory. Its target for
save's policy is written `ui/editor_page/policy.rs`, which cannot be right —
that path is one file for eight workflows. **This change corrects that census
cell and closes the adjacency in the spec**: a workflow's roles may live in a
per-workflow subdirectory of a shared directory, whose `mod.rs` is the facade and
whose role files keep the unqualified bounded names.

### Facade budget: the position this change takes

**No amendment is proposed, and the budget line is not to be edited.** Stating
the reasoning explicitly because the record instructs slot 3 to plan against 1
line of headroom rather than 20.

- **That 1 line belongs to `ui/search_panel/mod.rs`, not to a global allowance.**
  The budget is enforced per migrated row's declared facade. The exemplar sits at
  369 of 370. Slot 3a's exposure is therefore narrow and concrete: **this change
  must not add a physical line to the search facade.** Any incidental edit there
  — a rename, an import, a doc touch-up — requires re-measuring it. Task 9
  makes that a checked step rather than a hope.
- **The save facade is measured independently, and gets the whole 370.** The save
  workflow owns **one** stage order with four inversions (matrix trace). The
  exemplar's 369 narrates **twelve** inversions across **two** stage orders; the
  palette's 335 narrates eight across two. A one-stage-order facade with four
  inversions is not the case that breaks this number, and slot 2a's finding still
  holds: what makes a facade long is stage *bodies*, not stage narration.
- **If it does not fit, escalate in-change; do not defer and do not fudge.** The
  response order is fixed: delegate more into the coordination modules' own
  module docs, keep each stage to intent plus delegate plus resumption point,
  and only then treat non-fitting as real evidence the number is wrong. Raising
  it is a convention amendment requiring every migrated row re-checked in the
  same change — three rows now, more later — so the record's own advice is to
  make the case here rather than at slot 6. Task 9.3 carries that procedure with
  the escalation, not a permission to edit the line quietly.

## What Changes

- **Extract `WFR-DOCUMENT-SAVE` out of `ui/editor_page/load_save.rs` into a
  per-workflow role home**, `ui/editor_page/save/`: a narrative facade
  (`mod.rs`), coordination modules named from the bounded set, pure policy
  (`policy.rs`), and an evidence surface (`evidence.rs`). `save_runtime.rs` is
  retired — `runtime` is the name the convention rejects, and the census found it
  naming three different jobs across four files. The residual `load_save.rs` is
  left holding the load half only, with a module-doc note pointing at slot 3b; it
  is not renamed, because 3b dissolves it and a rename would churn a file two
  changes touch.

- **Amend `gtk-adapter-module-boundaries` to close the role-home adjacency.** A
  migrated workflow in a directory that hosts several workflows may keep its
  roles in a per-workflow subdirectory whose `mod.rs` is the facade, because the
  fixed `policy.rs` / `evidence.rs` names cannot be shared and a prefixed policy
  file falls out of the mutation scope. This is permission, not obligation:
  non-colliding workflows keep flat workflow-scoped role names. Per the
  retroactive-amendment rule every already-migrated row is re-checked here.

- **Reify the admission seam as `QueuedSaveTicket` + `QueuedSaveFacts`** with one
  `queued_save_is_current(&ticket, &facts)` predicate, constructed once at the
  workflow entry point, carrying `{save_generation, path, explicit_destination,
  required_modified, close_session_identity}` per the matrix's Seam Value Objects
  section. **The field is named `explicit_destination`** — the user's intent —
  and **not** `cancel_pending_load`, which names only a consequence. The change
  must decide from the code whether one value can honestly carry both meanings:
  if it can, the cancellation site derives it through a named predicate so the
  derivation is visible; if the two can diverge, they are two fields. Either way
  the mismatched positional call becomes a type error rather than a comment.

- **Retire the argument-count suppression.** `begin_admitted_save`'s
  `#[expect(clippy::too_many_arguments)]` goes away because the ticket replaces
  the parameter list, moving the matrix's "Argument-count suppressions" count
  from 2 to 1 and leaving only the domain catalog constructor the rule exempts.

- **Relocate `model/save_admission.rs` (405 lines) to
  `ui/editor_page/save/policy.rs`** with mutation-coverage parity evidence, and
  extract the save half's remaining pure decisions from the GTK adapter into the
  same module — the save-formatting acceptance rule, the buffer mirror-back
  decision, the chunked-vs-direct capture threshold, and the queued-save
  staleness predicate. Those extractions are a coverage **gain** from zero, like
  the palette's and slot 2b's, and must be reported separately from the
  relocation's parity numbers so the two claims are not mixed.

- **Build the save evidence surface** and fold the existing typed observation
  into it rather than leaving a second path: `SaveAdmissionSnapshot` is already a
  typed value, and `save_runtime::snapshot_for_test` plus the save-side hooks in
  `load_save.rs` are the scattered getters the convention retires. The surface
  must make the durable half observable — in-flight save, save generation, the
  admitted ticket's identity, chunked-capture state, the last write's outcome
  classification (`BeforeRename` / `AfterRename` / accepted), and the
  formatting-rewrite mirror-back result. Then migrate the widget tests that reach
  around the widget: `crates/lushtext/tests/widget/window.rs` and
  `editor_page.rs` hold save-related `.imp().` reach-through sites — including
  direct writes to `editor.imp().save.inflight` and reads of
  `window.imp().session.save_failed` — which are ungated, appear in no seam
  census, and shape production field layout. The per-site categorization is
  recorded as evidence.

- **Project automation from evidence without widening the contract.** The
  exported fields this workflow owns are `tabs[].saving` and the `save` readiness
  blocker feeding the `save-complete` predicate; `tabs[].modified` is buffer
  state the row shares. Those keep their names, types, and semantics and start
  projecting from the evidence surface, with new `Evidence Projection Map` rows in
  `docs/automation-reference.md` so the live drift gate covers them. Every other
  new evidence field — generations, ticket identity, capture state, write
  classification — is internal and must **not** reach the snapshot.

- **Advance the programme record and the matrix.** Split slot 3 into `3a` and
  `3b` in both the remaining-scope table and the machine-readable ledger,
  register both change names in the naming table, mark `WFR-DOCUMENT-SAVE`
  migrated with its `Migrated Workflow Roles` subsection, carry
  `WFR-AUTOMATION-SPINE (partial)` onto the complete line and keep it on 3b's
  outstanding line, and correct the census cells the authoring inventory found
  stale.

**Explicit non-goals.** No change to the durable-write ordering contract, the
atomic temp-then-rename sequence, the `BeforeRename` / `AfterRename` failure
classification, EditorConfig save formatting semantics, draft cleanup timing,
notification text, any user-visible string, or the exported D-Bus contract. No
load-side migration: `load_runtime.rs`, `model/file_load.rs`, the install slicing
and its cancellation paths belong to 3b, and this change must not
opportunistically restructure them. No draft, session, or local-history work
(slot 4) beyond the save workflow's existing call-outs. **No actuation seam is
retired**: the file-chooser-bound seams in `ui/window/dialogs.rs`
(`select_save_as_destination_for_test`, `select_save_as_uri_for_test`,
`cancel_save_as_destination_for_test`) drive a step reachable only through a
`GtkFileChooser`, and they stay per the programme-level deferral — counted and
preserved, not grown. No workflow is reified as an explicit state machine.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gtk-adapter-module-boundaries`: the decomposition contract fixes the role file
  names `policy.rs` and `evidence.rs` at one each per workflow, and separately
  says a directory hosting several workflows keeps flat role names without
  restructuring into subdirectories. Those two sentences are consistent only while
  no shared directory has two workflows that own pure policy. `ui/editor_page/`
  hosts eight, and the collision is mechanical rather than stylistic: the
  mutation scope reaches pure policy through the literal glob
  `crates/lushtext-core/src/ui/**/policy.rs`, so a prefixed `save_policy.rs`
  loses mutation coverage, which `mutation-testing` classifies as a blocking
  regression. The delta adds the per-workflow subdirectory as a permitted role
  home, keeps flat names permitted where they do not collide, and states that
  role files inside such a subdirectory stay unqualified.

  **What the delta is not.** It adds no role name, no obligation, and no
  capability: it names a second permitted *location* for roles the convention
  already requires, and the existing scenario that flat names are sufficient is
  preserved rather than replaced. It is the hygiene category the record
  sanctions — "closes a small adjacency the convention already sanctions" — and
  the census had already assumed it by writing `ui/editor_page/minimap/policy.rs`
  as a relocation target.

  **One word of the preserved text is touched, disclosed so it is not mistaken
  for a silent rewrite.** The existing scenario "One directory hosting several
  workflows keeps flat role names" ends "migration does not require restructuring
  the directory into one subdirectory per workflow"; the delta reads "restructuring
  the **whole** directory into one subdirectory per workflow". Without it the
  sentence can be read as forbidding the per-workflow subdirectory the delta
  permits, when what it actually rules out is a wholesale directory
  restructuring. The scenario's force is unchanged.

  **Flag for the reviewer.** Under the retroactive-amendment rule this triggers a
  per-row re-check of every `migrated` row. Both existing rows own dedicated
  directories with `mod.rs` facades, so the expected outcome is two
  confirmations and zero renames, and task 3.4 records them per row rather than
  asserting them collectively. If a re-check turns up a genuine mismatch it must
  be fixed here, because two generations of the convention must not coexist.

  Anything beyond this single location clarification must be escalated, not
  absorbed. In particular the facade line budget is **not** amended by this
  change.

Note that `openspec validate --strict` fails any change with no `specs/` delta
("Change must have at least one delta"), which is why every migration slot
carries one; slot 2a corrected the record text that said otherwise.

## Impact

**Prerequisites**

Slots 1, 2a, and 2b are archived under `openspec/changes/archive/2026-08-25-*`
with their deltas merged into `openspec/specs/`. Three lower-risk proofs precede
this tier-3 change where the convention requires two. The slot-2a deliverables
this change depends on are the declared facade budget, the stage-order
qualification rule, and the working evidence-to-snapshot drift check in
`scripts/check-automation-docs.py`.

**Code touched** (line counts measured at authoring; task 0.5 re-verifies)

- `crates/lushtext-core/src/ui/editor_page/load_save.rs` (1,795) — the save half
  extracted; the load half stays for 3b.
- `crates/lushtext-core/src/ui/editor_page/save_runtime.rs` (337) — retired into
  the save workflow's coordination role.
- `crates/lushtext-core/src/model/save_admission.rs` (405) — relocated to
  `ui/editor_page/save/policy.rs`. Note `crates/lushtext-core/benches/benchmarks.rs`
  addresses `SaveAdmissionSnapshot` directly, so the relocated module needs the
  same `pub` treatment `ui/search_panel/policy.rs` already has for its
  benchmarks; that is precedent, not a new pattern, and it is not a
  dependency-direction problem because a bench is not a service.
- `crates/lushtext-core/src/ui/editor_page/mod.rs` (716) — module declarations.
- `crates/lushtext-core/src/services/editor_io.rs` (3,035) and
  `crates/lushtext-core/src/services/durable_write.rs` (1,228) — **behavior
  unchanged**; in scope only for seam classification and for any buried pure
  policy the change decides about explicitly, per slot 2b's
  `services/search_backup.rs` precedent.
- `crates/lushtext-core/src/ui/window/dialogs.rs` (836),
  `documents.rs` (1,138), and `imp.rs` (1,652) — the window-side save, Save As,
  and close-with-changes invocations delegate to named save operations instead of
  re-reading and re-mutating editor save state inline, following slot 2b's
  window-side fix.
- `crates/lushtext-core/src/ui/automation.rs` — `tabs[].saving` and the `save`
  readiness blocker project from evidence.
- `crates/lushtext/tests/widget/window.rs`, `editor_page.rs` — evidence reads
  replacing save-related `.imp().` reach-through and retired inspection seams.
- `.cargo/mutants.toml`, `docs/workflow-readability-matrix.md`,
  `docs/next/workflow-readability.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `AGENTS.md`, `README.md`, and any
  `.agents/rules/*.md` or `.agents/skills/**` reference naming a moved path.

**Verification**

Everything slot 2b ran, re-aimed at the durable save path: save behavior
equivalence across untitled/Save As, plain overwrite, no-op save of a clean
buffer, save with EditorConfig trailing-whitespace and final-newline rewrites
(the saved bytes and the live buffer must still agree before the tab goes clean),
a superseded save whose stale completion must publish nothing, close-with-changes
and autosave-on-close, and a save whose editor is closed or re-pathed before the
worker returns; failure-path equivalence through the existing fault-injection
seams in `services/durable_write.rs` and `services/editor_io.rs` covering
`BeforeRename` (previous bytes intact, document stays modified) versus
`AfterRename` (durability unconfirmed, never a generic lost save); identity
metadata preservation across atomic replace; `make crash-recovery-smoke`; a
`data-safety` pass before and after the diff; `make mutants-diff` with parity
evidence; and a live run performing real saves and a real Save As against
throwaway fixture files with byte-exact content confirmed and clean stderr.
Save rewrites files, so the live run must never be pointed at the maintainer's
real documents — task 11.9 pre-authorizes the isolated substitution and states
what it leaves uncovered. Acceptance is that the user's files, their durability
classification, and the exported D-Bus contract behave identically to the
pre-migration workflow.
