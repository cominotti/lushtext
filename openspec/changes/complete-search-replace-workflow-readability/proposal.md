## Why

This is **slot 2b** of the workflow-readability programme recorded in
`docs/next/workflow-readability.md`, and the change that finishes
`WFR-SEARCH-REPLACE`. Slot 1 migrated that workflow's search and preview half and
deliberately stopped at the water line: the row is `migrated` while its
**tier-3 half — the Replace All durable write path and its undo journal —**
remains unconverted. Slot 2 was split into 2a
(`migrate-command-palette-workflow-readability`, the tier-2
`WFR-COMMAND-PALETTE` migration) and this change, so that the convention's
requirement of **two completed lower-risk migrations before a tier-3 workflow**
is satisfied by observable fact rather than by task ordering inside one change.

**Prerequisite, non-negotiable.** This change may not begin until 2a is archived,
the matrix marks `WFR-COMMAND-PALETTE` migrated with complete roles, the slot
ledger marks slot 2a complete, and `make check-workflow-boundaries` passes. That
is the second proof. Its absence is a blocker, not a formality.

Slot 2b owns four of slot 1's five residue obligations (2a discharged the fifth,
the facade line budget). Authoring re-read the code rather than trusting the
residue text, and **two of the four are not what the residue says they are**:

- **`activate_undo_replacements` is already a delegation.** The residue says
  stage 4 "reads transaction state and mutates widgets inline in the facade" and
  that slot 2 must finish delegating it. Slot 1's own result-cap fix already
  finished that: `ui/search_panel/mod.rs` now contains a documented one-line call
  to `replace::hand_back_undo_backup`, reading no transaction state and mutating
  no widget. The obligation is therefore to **retire the item with evidence** and
  fix the asymmetry that does still exist, one layer out:
  `ui/window/search.rs` re-reads and re-mutates panel state inline — claiming the
  replace transaction, showing the undo button on two early-return paths, and
  reserving undo capacity — before it spawns the undo worker, and installs the
  remainder backup inline afterwards. That is the durable-write invoker, so it is
  squarely this change's scope.
- **`model/workspace_search.rs` must not relocate.** The residue asks for a
  relocation decision and the matrix records "2 consumers, both search", implying
  a single-workflow module that moves into `ui/search_panel/policy.rs`. The
  reference set is larger and it forbids the move: `services/content_search/search.rs`
  consumes five of its types, `model/content_search.rs` embeds two of them in
  public enum variants, and `crates/lushtext-core/benches/benchmarks.rs`
  addresses two directly. Moving it under `ui/` would invert dependency direction
  (`services -> ui`). It is already pure, already mutation-scoped through
  `model/**`, and already has seven co-located unit tests. **The decision is that
  it stays**, and the matrix's undercounted consumer cell is corrected in the same
  change so slot 3 does not re-litigate it.

The two obligations that are real are the substantial ones:

- **`replace.rs`'s role name, which exposes a genuine gap in the bounded role
  set.** Slot 1 left `replace.rs` (994 lines) with a workflow-descriptive name
  because it owns both the preview attempt and the durable undo journal, and
  naming its job before the journal migrated would have to be redone. Reading it
  now, it holds **two cohesive coordination jobs**: a preview half (ticket issue,
  capacity reservation, single-flight coalescing, worker dispatch,
  publish-or-retire, checked-selection apply) and a journal half
  (generation-guarded install and clear, worker-side disk save and delete,
  startup recovery with stale cleanup, capacity retry, and the hand-back).

  The two halves are **not** state-disjoint, and the split has to be designed
  around that rather than assume it away. Three fields on `SearchPreviewState`
  (`crates/lushtext-core/src/ui/search_panel/imp.rs:153-189`) are touched by
  both: `replace_transaction_pending` (`:164`) is written by the journal half's
  transaction gate but read by the preview half in
  `begin_confirmed_replacement` (`replace.rs:713`) and
  `update_replace_button_sensitivity` (`:827`);
  `replace_transaction_generation` (`:165`) is *reserved* by preview selection and
  *consumed* by the durable apply (`:199-235`); and `undo_backup_generation`
  (`:159`) is threaded through `begin_replace_transaction` into the journal
  installs. So the seam is a real handoff of three named values, not a clean
  cut — which is an argument for the split (each field gets one owner and one
  reader-facing operation) rather than against it, but it means the boundary is
  designed, not discovered.

  Either way **both candidate names collide.** The directory already spends
  `execution.rs` on streaming search and `retirement.rs` on bounded disposal, for
  the *same* workflow. Slot 2a's spec delta added the stage-order qualification
  rule that resolves the collision itself. What remains unresolved is that the
  journal half's job — maintaining a durable, generation-guarded record that a
  later stage reads back, including startup recovery — is described by **none** of
  `admission`, `execution`, `retirement`, or `watch`; `retirement` in particular
  means the opposite, destroying a payload the workflow is finished with. That
  missing name must be added to the bounded set, which is the one amendment the
  convention explicitly sanctions.
- **The replace/undo evidence surface, which is where the real risk lives.** The
  exemplar's `evidence.rs` covers the preview half well and the durable half
  barely. `replace_transaction_pending` is observable only folded into
  `replace_preview_pending`, so no test can distinguish "preview worker busy"
  from "apply transaction claimed" — the exact state on which
  `hand_back_undo_backup` and `begin_confirmed_replacement` branch. The
  transaction generation, the undo-backup generation, and the preview generation
  are entirely unobservable, though every generation-guarded install, clear, disk
  save, and disk delete compares against one of them. There is no in-flight
  disk-job counter analogous to `preview_selection_jobs`. The consequence is
  visible in the tests: `crates/lushtext/tests/widget/search_panel.rs` reaches
  around the widget into the `search_backup` service at **35 direct call sites**
  (16 `load`, 4 `save`, 15 `delete`), because the workflow exposes no honest way
  to ask whether the journal landed.

Everything above is bookkeeping around a path that **rewrites the user's files**
and keeps the only record that can put them back. That is why it waited for two
proofs, and why its verification section is the largest part of this change.

## What Changes

- **Split `ui/search_panel/replace.rs` into role-named coordination modules**
  along the preview / journal seam, retiring the workflow-descriptive name slot 1
  left in place. The preview half keeps the preview-attempt lifecycle and the
  three preview widget-mutation helpers; the journal half takes the transaction
  gate, generation-guarded install and clear, disk save and delete, startup
  recovery, capacity retry, and the hand-back. The three fields both halves touch
  each get one owning module and one named operation the other half calls, so the
  handoff is explicit rather than shared mutable state.

- **Amend `gtk-adapter-module-boundaries` to add the one missing role name.**
  `journal`: the coordination job of maintaining a durable, generation-guarded
  record that a later stage of the same workflow reads back — installing and
  clearing it under a freshness guard, writing and deleting it on a worker,
  recovering it at startup with stale-record cleanup, and handing it back. None of
  `admission`, `execution`, `retirement`, or `watch` describes it, and
  `retirement` means the opposite. The stage-order qualification rule that
  resolves the *name collision* is not part of this change: slot 2a added it,
  because the palette's own split exercised it first. Per the retroactive-amendment
  rule, every already-migrated row is re-checked against the amended set here.

- **Extract the durable half's pure decisions into pure policy.** From
  `ui/search_panel/replace.rs` into the workflow's existing
  `ui/search_panel/policy.rs`: the preview reservation and shrink-to weights, the
  retained-byte cast, the undo-capacity admission arithmetic currently inline in
  `try_reserve_undo_replacement`, and the generation-match predicates inline in
  the generation-guarded install and clear. None of that logic is under mutation
  today.

- **Decide and act on `services/search_backup.rs`'s buried policy.** Its three
  large loaders hide rules that are pure once given inputs and are each written
  more than once: the journal-activation decision, the per-entry payload budget
  arm (twice), the manifest entry-count cap check (three times), the
  manifest/marker/`.json` payload-file filter (twice), manifest entry dedup, the
  retained-weight admission, and the cleanup-replacement eligibility rule. These
  cannot move to `ui/`, because a service must not depend on the adapter; the
  change decides explicitly whether they become a `services/search_backup/policy.rs`
  or stay as private pure functions with direct unit tests, and records the
  reason. They are already inside the mutation scope, so the win is testability
  of staleness, budget, and eligibility rules without a tempdir — not coverage.

- **Extend the replace/undo evidence surface** so the durable half is observable
  without reaching around the widget: transaction pending as its own field
  distinct from preview pending, the transaction / undo-backup / preview
  generations, the preview capacity-retry armed state, the installed backup's
  entry count and retained weight, an in-flight journal disk-job counter, and the
  last apply result. Then migrate the widget tests that currently call
  `search_backup::load` directly to read evidence where the question is "did the
  workflow record it", keeping direct service reads only where the question is
  genuinely "what is on disk".

- **Finish the facade.** Retire the residue item for
  `activate_undo_replacements` with the evidence that it is already a delegation;
  update the facade's Replace All stage narration and role table for the new
  module names; and make `ui/window/search.rs`'s undo invocation delegate instead
  of re-reading and re-mutating panel state inline.

- **Keep `model/workspace_search.rs` in `model/`**, correct the matrix's
  consumer count and classification, and record it under the modules confirmed as
  domain so no later slot re-opens it.

- **Complete the row.** `WFR-SEARCH-REPLACE` becomes migrated for the whole
  workflow: the `(partial)` marker comes off its ledger entry, its
  `Migrated Workflow Roles` subsection names the new modules, and its risk cell
  records that the tier-3 half is now covered. The slot ledger flips slot 2b to
  complete and carries `WFR-AUTOMATION-SPINE` forward to slot 3's outstanding
  line, because that row continues past this change.

**Automation, stated honestly.** Slot 1 already projects every
`window.content_search` field except `visible` from the search panel's evidence
surface, so this change's `WFR-AUTOMATION-SPINE` share is **not** a set of new
projections. It is a no-widening obligation: the substantial new evidence fields
above — generations, disk-job counts, backup weights — must **not** reach the
exported snapshot, the documented `content_search` fields must keep their names,
types, and meanings, and the evidence-to-snapshot drift gate must cover the
extended surface.

**Explicit non-goals.** This change does not re-plan slot 1's capped-result
delivery fix or the `WalkStop` stop-semantics split. It does not migrate the
palette (2a) or touch `WFR-BUFFER-REPLACEMENT`, which is cross-cutting and
belongs to slot 4 even though Replace All undo is one of its two callers. It does
not change the on-disk journal format, the durable-write ordering contract, the
undo semantics, replacement matching or preview row generation, any user-visible
string, or the exported D-Bus contract. It does not retire actuation test seams:
`replace.rs`'s **five** remaining `_for_test` functions —
`clear_undo_backup_for_test`, `reserve_undo_backup_generation_for_test`,
`set_persisted_undo_backup_for_generation_for_test`,
`begin_replace_transaction_for_test`, and `finish_replace_transaction_for_test` —
are actuation seams that drive steps otherwise reachable only through a worker
completion or a transaction gate, and they stay per the programme-level deferral.
It does not reify any workflow as an explicit state machine.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gtk-adapter-module-boundaries`: the bounded set of coordination role names
  (`admission`, `execution`, `retirement`, `watch`) has no name for
  generation-guarded durable persistence with startup recovery. The Replace All
  stage order's journal half is exactly that job, and `retirement` — the closest
  name — means the opposite, destroying a payload the workflow is finished with.
  The delta adds `journal` to the bounded set, with a scenario distinguishing it
  from `retirement` and `execution`.

  **Two properties of the delta that are easy to miss in the diff, disclosed
  explicitly.**

  1. **The role list becomes a closed enumeration.** The pre-amendment sentence
     read "a bounded set of role names that state the job the module performs,
     **such as** admission, execution, retirement, or watch"; the delta replaces
     it with "`admission`, `execution`, `retirement`, `watch`, and `journal`". The
     diff looks like it only appends a fifth name, so the tightening is called out
     here. It is deliberate and it changes nothing operationally: the same
     requirement already stated that a job no existing name describes "MUST be
     added to the bounded set by amending this specification", so an off-list name
     always required an amendment. It also aligns the spec with how the convention
     is written everywhere else it is normative — `.agents/rules/rust.md` and
     `docs/next/workflow-readability.md` both enumerate the set rather than
     exemplify it. Any future off-list name still requires amending this spec.
  2. **`journal` is defined to include its admission gate.** The role covers the
     mutual-exclusion gate serializing the workflow's apply and undo transactions
     and the disposal reservation those transactions take, alongside the
     generation-guarded install and clear, the worker-side write and delete,
     startup recovery with stale-record cleanup, and the hand-back. Splitting the
     gate and reservation into a separate `undo_admission.rs` was considered and
     rejected: two small jobs whose only purpose is protecting one durable record
     do not justify a third module plus a sixth narrated stage in the facade. The
     definition says so, so the next adopter copies the true boundary.

**Flag for the reviewer, per the record's instruction to raise spec-delta needs
loudly.** This delta *is* the sanctioned kind — the bounded-role-name requirement
explicitly instructs a change needing a new role name to amend the spec rather
than overload an existing one, and the programme record predicted this decision
would land on this row. It is Phase 0's escape hatch being used as designed, not
evidence that Phase 0 mis-specified the convention. Any need beyond this single
role-name addition must be raised loudly rather than absorbed.

Note also that `openspec validate --strict` fails any change with no `specs/`
delta, so the record's "proposal and tasks only" expectation is not achievable as
written; 2a corrects that record text.

## Impact

**Prerequisites**

Slot 1 is already archived at
`openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/`,
with its five deltas merged into `openspec/specs/` and its mutation evidence at
that archive path. The outstanding gate is **slot 2a archived with
`WFR-COMMAND-PALETTE` marked migrated**, which is this change's first task and is
blocking: it is the second of the two proofs a tier-3 migration requires. Slot 2a
also owns the stage-order qualification rule and the evidence-to-snapshot drift
check that this change depends on.

**Code touched**

- `crates/lushtext-core/src/ui/search_panel/replace.rs` (994) — split and
  retired.
- `crates/lushtext-core/src/ui/search_panel/policy.rs` (448) — gains the durable
  half's pure decisions; already `pub` for the GTK-free policy benchmarks.
- `crates/lushtext-core/src/ui/search_panel/evidence.rs` (417) — extended, with
  the reentrancy constraint its own module doc records.
- `crates/lushtext-core/src/ui/search_panel/mod.rs` (350) — Replace All stage
  narration and role table; must stay within the 370-line budget 2a declared.
- `crates/lushtext-core/src/ui/window/search.rs` — the inline undo invocation
  becomes a delegation.
- `crates/lushtext-core/src/services/search_backup.rs` (1,334) — buried policy
  extracted or unit-tested in place, per the recorded decision.
- `crates/lushtext-core/src/model/workspace_search.rs` (503) — **unchanged**; the
  decision is recorded, not applied.
- `crates/lushtext/tests/widget/search_panel.rs` — evidence reads replacing
  around-the-widget `search_backup::load` calls where the question is about
  workflow state.
- `.cargo/mutants.toml`, `docs/workflow-readability-matrix.md`,
  `docs/next/workflow-readability.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `AGENTS.md`, `README.md`, and any
  `.agents/rules/*.md` or skill reference naming a moved path.

**Verification**

Everything 2a runs, plus the proof a tier-3 workflow requires: Replace All and
undo behavior equivalence across empty, single-file, multi-file, partial-check,
and superseded-preview cases; journal failure-path equivalence using the existing
fault-injection seams in `services/content_search/replace.rs` (before-rename
failure, after-metadata hooks) covering `BeforeRename` versus `AfterRename`
classification; startup recovery against a healthy journal, a journal with
diagnostics, a duplicate-path journal, an over-cap journal, and a
cleanup-in-progress marker; crash-recovery smoke; a `data-safety` review of the
diff; and a live `make run` performing a real Replace All and undo against
throwaway fixture files with byte-exact restoration confirmed and clean stderr.
Acceptance is that the user's files and their undo journal behave identically to
the pre-migration workflow, with no new runtime warnings and no change to the
exported D-Bus contract.
