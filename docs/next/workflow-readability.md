# Workflow Readability — Programme Record

Status: **Phase 0 complete, six migration changes outstanding.** The convention
is normative, the census is complete, the mechanical gate is wired into
`make check-policy`, and exactly one workflow (`WFR-SEARCH-REPLACE`, search and
preview half) is migrated. Everything else in `ui/` and `model/` is unchanged
and behaviorally untouched. Slots 2 through 7 are authorable now; two items are
deliberately deferred and may never be taken on.

This document answers, in one read: what problem the programme solves, how much
is done, what is next, what is deferred and why, and what would justify taking
the deferred work on. It is the narrative carrier; the normative contract is
spread across **five** capability specs, and a migration must read all of them
that touch its scope:

| Carrier | Job |
| --- | --- |
| `openspec/specs/workflow-readability-boundaries/spec.md` | the workflow module shape, the facade contract and its size-budget rule, seam value objects, intent-first naming, the census matrix, risk tiers, retroactive amendment |
| `openspec/specs/workflow-evidence-surfaces/spec.md` | evidence surfaces, their single visibility rule, the inspection/configuration/actuation/probe seam taxonomy, the evidence→automation projection relationship |
| `openspec/specs/gtk-adapter-module-boundaries/spec.md` | the decomposition contract and **the bounded set of coordination role names** (`admission`, `execution`, `retirement`, `watch`); adding a role name amends *this* spec, not the two above |
| `openspec/specs/mutation-testing/spec.md` | the `ui/**/policy.rs` scope convention and the mutation-parity requirement for relocated policy |
| `openspec/specs/dbus-automation-spine/spec.md` | snapshots project from evidence while the exported D-Bus contract stays unchanged |
| `docs/workflow-readability-matrix.md` | per-workflow status, roles, seams, risk tier, slot; gated by `make check-workflow-boundaries` |
| this file | why, baseline, sequencing, remaining scope, deferrals |

Slot 2's `replace.rs` role-name decision lands on the third row: if no listed
role name describes what that module does, the slot must amend
`gtk-adapter-module-boundaries` rather than overload an existing name.

Full rationale for every decision below lives in the OpenSpec change
**`normalize-workflow-readability-boundaries`** (`proposal.md` + `design.md`,
decisions D1–D10 and Resolved Questions). Look for it under
`openspec/changes/` or `openspec/changes/archive/*-normalize-workflow-readability-boundaries/`.
If this document and the capability specs ever disagree, the specs win and this
document must be corrected in the same change.

## 1. The problem, measured

Fourteen robustness changes landed between 2026-07-10 and 2026-07-24. They are
correct and worth keeping. They left a codebase readable only to someone already
holding the coordination machinery in their head. Four measured symptoms:

1. **The domain layer became a testability parking lot.** 8 of `model/`'s 29
   files were named after mechanism (`save_admission`, `search_flight`,
   `search_retirement`, `plain_disposal`, `migration_ledger`, `editor_memory`,
   `buffer_replacement`, `minimap_analysis`) rather than domain, and 6 of those 8
   had exactly one consumer, almost always in the adjacent `ui/` directory. They
   lived in `model/` because `.cargo/mutants.toml` examined only `model/**` and
   `services/**`: a tooling glob, not a design decision, was shaping the
   architecture.
2. **Field bundles crossed layer seams unnamed, and drifted while crossing.**
   the programme published this as "90 production functions take six or more
   parameters"; the census measured **88** under that same receiver-counted
   definition, and 43 under the strict non-receiver definition (matrix,
   Measurement Definitions). Use 88 receiver-counted or 43 strict, and always say
   which. At
   `ui/editor_page/load_save.rs` the same value was passed as
   `cancel_pending_load` and received as `explicit_destination`, inside
   stale-save rejection. A reader cannot verify that by reading it.
3. **No workflow had a narrator.** Answering "what happens on Ctrl+S" required
   13 hops across 6 files, with control inverted through an `idle_add_once`
   drain, and no document or module narrated it. `load_save.rs` was 1,795 lines
   holding two distinct workflows.
4. **Two parallel introspection APIs existed and did not know about each
   other.** 18 typed `Automation*Snapshot` types (read-only over D-Bus,
   documented, drift-gated) alongside 639 `#[cfg(feature = "test-utils")]` sites
   growing a shadow API of 300 externally reachable `*_for_test` functions over
   overlapping state, with no types, no documentation, and no drift gate.

None of this called for a new abstraction layer. The hexagonal boundary held
(`model/` and `services/` contain zero GTK imports) and the machinery modules
were individually well written. What was missing: **a workflow had no home where
its whole story fits** — pure policy, coordination, adapter, and evidence were
scattered across four directories and two vocabularies.

## 2. Baseline: how much Phase 0 actually migrated

This section exists because a future session could read the completed capability
specs and reasonably conclude the work is done. It is not. Phase 0 established
the shape, proved it once, and enumerated everything else.

| Quantity | Phase 0 planned (design.md D10) | Phase 0 actual | Note |
| --- | --- | --- | --- |
| Share of `ui/` + `model/` migrated | 4,762 of 79,017 lines ≈ 6% | unchanged: the exemplar workflow's censused `ui/` + `model/` footprint is 4,762 lines | ≈6% is the workflow's footprint, not the diff size; the other ~94% is untouched |
| Policy modules relocated | 2 of 8 | 2 relocated (`model/search_flight.rs`, `model/search_retirement.rs` → `ui/search_panel/policy.rs`); `model/` went 29 → 27 files | the denominator is better stated as **2 of 7 relocation candidates**: 4 of the 8 mechanism modules are cross-cutting and stay, and the census found 3 previously unlisted single-workflow modules (`workspace_scan`, `workspace_persistence`, `workspace_search`) |
| Test seams addressed | 48 of 639 | the exemplar workflow held exactly **48** of the 639 `#[cfg(feature = "test-utils")]` sites, so the planned figure was right; it now holds 40, and its 23 `*_for_test` functions became 7. Repo-wide: 639 → 631 sites, 351 → 335 `*_for_test` functions | the drop is smaller than the seam count because consolidation replaced 23 scattered getters with one typed surface plus one test-policy module, both still feature-gated |
| Long signatures reified | 2 of 90 | **1 seam** reified (`ReplacePreviewTicket` + `ReplacePreviewFacts`), and **0** long signatures shortened | census finding 3: this workflow contains exactly one function with ≥6 non-receiver parameters and it is `SearchResultItem::new_match`, a row-item constructor the convention exempts. The programme's real unit is *seams reified*, not signatures shortened. Later changes MUST report seams, and may report signatures only as a secondary number. |

**Reconciling the exemplar's seam figures**, because three different numbers
describe them and a slot-2 author sizing its own evidence work must not mix them.
The workflow held **23 `*_for_test` functions** before the migration. They went:
8 inspection getters retired into `evidence.rs` (which is the matrix Product
Matrix cell's "8 inspection fns"), 5 configuration setters plus 3 configuration
readers collapsed into `SearchPanelTestPolicy`, and **7 remaining** — 5 actuation
seams on the replace/undo transaction and 2 accessibility probes. 8 + 5 + 3 + 7 =
23. Separately, the matrix's "Migrated Workflow Roles" prose mentions "23
observation getters" moved out of `mod.rs`; that counts observation accessors
relocated into `evidence.rs`, which is a different population from the 23
`*_for_test` functions. For an **unmigrated** workflow, size evidence work from
the Product Matrix's per-kind `Seams (i/c/a/p)` tuple, which is the population
consolidation acts on. `WFR-SEARCH-REPLACE`'s own cell no longer holds a tuple —
once a row is migrated the cell records what the migration did instead, which is
why the reconciliation above exists.

Two further Phase 0 facts a later session should not have to rediscover:

- **The exemplar's facade measures 350 physical lines** (75 module-doc narration
  lines, 166 non-comment non-blank), narrating 6 inversions across two stage
  orders, down from a 578-line `mod.rs` that also held the accessibility
  projection and 23 observation getters. It measured 357 when the migration
  landed and 350 after the result-cap fix delegated one more stage. **No
  normative facade line budget is declared yet**; slot 2 sets it, per the
  matrix's "Facade size budget" section.
- **A pre-existing product defect was found and fixed while proving behavior
  equivalence** (commit `f0ab1d9`): capped workspace-search results were
  silently discarded because the service's result cap wrote the caller's
  `cancel` flag, so the panel's `if !cancelled` tick arms dropped the
  `ResultCap` notice and every buffered match. Fixed at the content-search
  boundary with a private `WalkStop` value separating caller cancellation from
  service termination. **Slot 2 must not re-plan this.** The matrix records the
  stop semantics under `WFR-SEARCH-REPLACE`.

## 3. Remaining scope

Six changes remain. Order is by increasing risk; every `tier-3` slot follows at
least two completed lower-risk migrations. The matrix's
"Migration Order And Risk Tiers" section is the authoritative per-row mapping;
this table is the change-level view.

| Slot | Scope | Workflows | Artifacts expected |
| --- | --- | --- | --- |
| 1 | **complete** — census, convention, enablers, exemplar | `WFR-SEARCH-REPLACE` search and preview half | proposal + design + tasks + 2 capability specs (`normalize-workflow-readability-boundaries`) |
| 2 | Finish search/replace, migrate the palette, set the facade budget | `WFR-SEARCH-REPLACE` replace/undo half, `WFR-COMMAND-PALETTE`, first `WFR-AUTOMATION-SPINE` projections | proposal + tasks |
| 3 | Save and load | `WFR-DOCUMENT-SAVE`, `WFR-DOCUMENT-LOAD` | proposal + tasks |
| 4 | User-content restore family | `WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`, `WFR-BUFFER-REPLACEMENT` | proposal + tasks |
| 5 | Workspace tree and notes | `WFR-WORKSPACE-TREE`, `WFR-NOTES-BOOKMARKS` | proposal + tasks |
| 6 | Minimap | `WFR-MINIMAP` | proposal + design + tasks |
| 7 | Residual sweep | `WFR-MARKDOWN-PREVIEW`, `WFR-EDITOR-FIND`, `WFR-ENCODING`, `WFR-PRINT`, `WFR-SHELL-LAYOUT`, `WFR-STATUS-NOTIFICATIONS`, `WFR-BUFFER-SNAPSHOT`, `WFR-PLAIN-DISPOSAL`, remaining `exclude_re` entries and argument-count suppressions, matrix completion | proposal + tasks |

**Slot 2 must resolve one ordering question before it starts.** The convention
requires that a `tier-3` workflow not be migrated until the shape has been proven
on **at least two completed lower-risk migrations** (matrix, "Risk Tiers"), and
slot 2 contains a tier-3 half — the Replace All write path and its undo journal.
Only one lower-risk migration is complete: slot 1's tier-2 exemplar. Slot 2
therefore may not open the write half on slot 1's proof alone. It must either
sequence its own tier-2 work (the `WFR-COMMAND-PALETTE` migration) to land and be
verified before the replace/undo half inside the same change, or split into 2a
(palette, tier-2) and 2b (replace/undo, tier-3). Pick one explicitly in the
proposal; do not leave it implicit, and do not read slot 2's position in this
table as a waiver.

**Naming and finding a slot's change.** Slot 1 is the OpenSpec change
`normalize-workflow-readability-boundaries`. Later slots have no reserved names;
name them for their scope with a `-workflow-readability` suffix (for example
`migrate-palette-workflow-readability`) and record the chosen name in this table
when the change is authored, so the next cold session can find it. Before
authoring, check `openspec list` and `openspec/changes/archive/` for an existing
change covering the slot.

`WFR-EDITOR-MEMORY` and `WFR-MIGRATION-LEDGER` have no slot: they are
cross-cutting policy that stays where it is. Their rows exist so a later change
cannot silently relocate them.

**Migration changes are expected to need only a proposal and tasks.** Phase 0
holds the contract; a migration consumes `workflow-readability-boundaries` and
`workflow-evidence-surfaces` and checks off matrix rows. Slot 6 is the one
expected exception, because minimap decomposition has a genuine design question
(pixel-verified geometry under animation frames). **If any other migration finds
it needs a spec delta or a new capability, that is a signal that Phase 0's
contract was incomplete** — and the retroactive-amendment rule in section 7
applies to the fix.

### Slot 1 residue — what slot 2 inherits

Slot 1 deliberately migrated only the non-writing half of `WFR-SEARCH-REPLACE`,
so the row is `migrated` for the search and preview half while real obligations
remain. Slot 2 owns all of these:

- **The Replace All write path and its undo journal.** The row's tier-3 half:
  `services/search_backup.rs` plus the durable rewrite path.
- **`replace.rs`'s role name.** It kept a workflow-descriptive name rather than a
  bounded coordination role name, because it owns both the preview and the
  durable undo journal and naming its job before the journal migrates would have
  to be redone. Slot 2 decides its final role name, or names.
- **`activate_undo_replacements` delegation.** The cold facade re-read (task 6.8)
  found this stage reads transaction state and mutates widgets inline in the
  facade, which the facade role forbids. The result-cap fix already extracted
  `hand_back_undo_backup`; slot 2 must finish making stage 4 a delegation like
  its three siblings.
- **`model/workspace_search.rs`** (503 lines, 2 consumers, both search) — a
  census-found single-workflow module still awaiting its relocation decision.
- **The normative facade line budget.** Per the matrix, the first migration after
  the exemplar sets the number from the measured facade (350 physical lines
  today). A budget below roughly 370 would force this facade's narration to
  split, which defeats the facade.
- **The first `WFR-AUTOMATION-SPINE` projections beyond the search fields**, which
  the exemplar already projects from `SearchPanelEvidence`.

Slot 2 must **not** re-plan the result-cap delivery fix (section 2) or the
`WalkStop` stop-semantics split; both landed in slot 1.

### Slot ledger (machine-readable)

`make check-workflow-boundaries` reads the lines in this subsection and compares
them against `docs/workflow-readability-matrix.md`. Each line is exactly:

```
- slot <n> (complete|outstanding): <WFR-ID>[ (partial)][, ...]
```

`<n>` is a slot label, not strictly an integer: a slot that splits keeps its
number and takes a letter suffix (`2a`, `2b`), which the gate accepts. Splitting
a slot therefore means replacing its ledger line with two labelled lines and
splitting the row in the remaining-scope table above; it never means renumbering
slots 3 through 7, because their numbers are cited from the matrix and from
per-row `Slot` cells.

The check fails when a `complete` slot names a row the matrix does not mark
`migrated`; when an `outstanding` slot names a row the matrix marks `migrated`
without the `(partial)` marker; when a named row id is absent from the matrix;
and when a matrix row that is neither `migrated` nor `exempt` and has a slot
assigned does not appear in any `outstanding` slot. Keeping this ledger current
is part of advancing a migration, not paperwork after it.

The `(partial)` marker means "this row's scope is split across slots" and works
in both directions, which some rows need:

- On an `outstanding` line it marks a row the matrix already calls `migrated`
  that still owes work. `WFR-SEARCH-REPLACE` is the current case: migrated for
  its search and preview half, outstanding for its replace/undo half.
- On a `complete` line it marks a row whose share of *that slot* is finished
  while the row itself continues in a later slot, so the entry is exempt from the
  `migrated` requirement. `WFR-AUTOMATION-SPINE` is the case to expect: the
  matrix gives it slot "2 onward, incrementally per migrated workflow", so when
  slot 2 lands it must be written `WFR-AUTOMATION-SPINE (partial)` on the
  complete line and kept on a later outstanding line. Marking that row
  `migrated` to satisfy the gate would be a false claim.

- slot 1 (complete): WFR-SEARCH-REPLACE
- slot 2 (outstanding): WFR-SEARCH-REPLACE (partial), WFR-COMMAND-PALETTE, WFR-AUTOMATION-SPINE
- slot 3 (outstanding): WFR-DOCUMENT-SAVE, WFR-DOCUMENT-LOAD
- slot 4 (outstanding): WFR-DRAFT-RECOVERY, WFR-SESSION-RESTORE, WFR-LOCAL-HISTORY, WFR-BUFFER-REPLACEMENT
- slot 5 (outstanding): WFR-WORKSPACE-TREE, WFR-NOTES-BOOKMARKS
- slot 6 (outstanding): WFR-MINIMAP
- slot 7 (outstanding): WFR-MARKDOWN-PREVIEW, WFR-EDITOR-FIND, WFR-ENCODING, WFR-PRINT, WFR-SHELL-LAYOUT, WFR-STATUS-NOTIFICATIONS, WFR-BUFFER-SNAPSHOT, WFR-PLAIN-DISPOSAL

## 4. The unblock point

**The migration changes became authorable after sections 1 and 2 of Phase 0 —
the census and the settled conventions — not after the exemplar.** The exemplar
only refines the task template. Any of slots 2 through 7 can be authored today
without waiting for its predecessor to land, though they must *land* in order,
because each `tier-3` slot depends on the lower-risk proofs before it.

What each migration takes from the census, all already recorded in the matrix:

| Input | Where | Why it unblocks authoring |
| --- | --- | --- |
| Seam value-object name and the exact bundle it carries | matrix, "Seam Value Objects" (`QueuedSaveTicket` + `QueuedSaveFacts`, `LoadRequestTicket`, `WorkspaceWatchTicket`, `NotesBrowserTicket`) plus the per-row "Seam value object" cell | the tasks can name the type and the fields instead of discovering them |
| Per-kind test seam counts | matrix "Seams (i/c/a/p)" cell per row, plus the Test Seam Census partition | sizes the evidence-surface work and separates the deferred actuation seams |
| Risk tier and slot | matrix per-row `Risk` and `Slot` cells, and "Migration Order And Risk Tiers" | fixes the ordering constraint and the required proof depth |
| Ordered stages and every control-flow inversion | matrix "Workflow Stage Traces" | the facade's narration is written from the trace, not re-derived |
| Owned pure policy and its relocation target | matrix "Policy Module Census" | says what moves, what is cross-cutting, and what needs a decision |
| Settled conventions (role names, facade budget rule, zero argument suppressions, cross-cutting eligibility) | matrix "Settled Conventions" and the two capability specs | the shape does not have to be re-litigated per slot |

## 5. Sequencing rationale

**Why census before migration.** Enumerating every workflow first is the
mechanism that prevents "pilot, then refine". The three known outliers —
`minimap.rs` (3,779 lines with pixel-verified geometry), `editor_memory` (five
real consumers), and the freshly decomposed `markdown_preview` — were classified
as deferred, exempt, and deferred *before* the shape became normative, so no
later change has to argue them into or out of the pattern. The census also found
three unlisted single-workflow policy modules and corrected four of the
programme's own headline figures, which a pilot-first order would have
discovered mid-migration.

**Why vertical slices.** Migration proceeds one workflow at a time across all of
its layers, not one layer at a time across all workflows. The deliverable is a
workflow that reads as a story, and that only materializes when facade, seam
value object, policy, coordination, and evidence land together. Horizontal
slicing would touch every file four times and produce no readable workflow until
the final tier. Vertical slicing also makes a stalled programme coherent:
migrated workflows are readable, unmigrated ones are unchanged, and the matrix
says which is which.

**Why this risk order.** Renames and value objects are compiler-verified, policy
relocation is verified by mutation parity, and evidence consolidation is verified
by test-suite equivalence plus the automation docs gate. The exemplar was chosen
as the workflow closest to the target shape whose migrated half touches no user
data. Refactoring immediately after a robustness programme is the highest-risk
refactor category, so user-data workflows come only after the pattern has two
proofs.

## 6. Rejected alternatives

Recorded so a later session does not re-propose them. Full reasoning is in the
change's `design.md` (D1, D7).

- **A new `policy/` layer as a sibling of `model/` and `services/`.** Rejected:
  it preserves the split-brain reading experience — the workflow's logic is still
  in another directory — while adding a fourth place to look whose only
  justification is mutation tooling. The dependency data says these modules are
  not shared policy; they are the pure half of one workflow.
- **A naming-only pass: leave everything in `model/` and fix the names.**
  Rejected: it keeps the parking-lot incentive alive. The next robustness change
  would add `model/notes_admission.rs` for exactly the same tooling reason. The
  fix had to remove the incentive, which is why the mutation scope became a
  naming convention (`ui/**/policy.rs`).
- **Horizontal slicing: do facades everywhere, then value objects everywhere,
  then evidence everywhere.** Rejected: four passes over every file, no readable
  workflow until the last tier, and no coherent stopping point.

Also rejected, at the seam level: making evidence surfaces public and driving
D-Bus automation directly from them. That widens an external contract for an
internal readability goal. Automation snapshots project from evidence instead,
and the exported D-Bus contract is unchanged.

## 7. Deferred work

These are **not** in any slot. Each needs its own change with its own
justification, and neither is required for the programme to be complete.

Do not confuse this section with the matrix's row status `deferred`. That status
means "will be migrated, but later than its risk tier alone suggests", and every
row carrying it (`WFR-MINIMAP` in slot 6, `WFR-MARKDOWN-PREVIEW` in slot 7) has a
slot and is planned work. The two items below are **programme-level deferrals**:
unslotted, unscoped, and possibly permanent.

### Actuation test seams (~98 functions)

**What.** Functions such as `autosave_tick_for_test` and
`cancel_open_file_for_test` that drive a workflow rather than observing or
configuring it. Phase 0 consolidated inspection seams and collapsed
configuration seams; actuation seams were classified and left alone.

**Why deferred.** They exist because the real path runs through a
`GtkFileChooser` or `AdwAlertDialog` that headless tests cannot drive. They are
evidence of a **missing workflow/dialog-presentation boundary**, not of
misplaced test code. Removing them means introducing that boundary, which is a
design change with real behavioral risk in save, close-with-changes, and open
flows. Smuggling it into a readability sweep would put user-data dialogs at risk
for a tidiness gain.

**Justification bar.** Take it on when a change independently needs the
presentation boundary — for example a portal-first file-chooser migration, a
headless-drivable dialog contract, or repeated defects traced to dialog-coupled
workflow code — and can pay for real-session proof of every affected dialog
path. Until then, actuation seams stay, and the `gtk-testing` skill records them
as legitimate.

### State-machine reification of inverted drains

**What.** Turning workflows whose control inverts through timers, idle drains,
and worker completions into explicit state machines. `WFR-DRAFT-RECOVERY` alone
has seven distinct worker handoffs; save has four inversions across six files.

**Why deferred.** It was the highest-risk item in the exploration and may never
be justified. Phase 0's non-goals are explicit: nothing in the readability
programme alters control flow. The narrative facade is the cheap substitute — it
names each inversion and its resumption point, which was the actual readability
complaint — and it carries none of the risk of re-sequencing a durable write
path.

**Justification bar.** Take it on only if, after the facades exist, a *specific*
workflow still cannot be reasoned about from its narration, and that opacity is
causing real defects — not because state machines are tidier. Any such change
must be single-workflow, must preserve timing and persistence behavior
observably, and needs mutation plus real-session proof. "May never be justified"
is the expected outcome.

## 8. Retroactive-amendment rule

**A change that amends the convention MUST re-migrate every workflow already
marked `migrated` in the same change. Two generations of the convention MUST NOT
coexist in the tree.**

This is the rule that keeps a multi-change programme from manufacturing forked
versions of its own convention — precisely the disease being treated. It applies
to the role names, the facade budget number, the seam value-object shape, the
evidence-surface visibility rule, and anything else recorded as settled in the
matrix's "Settled Conventions" section or the capability specs.

Practical consequence: the cheapest moment to correct a convention is while
exactly one workflow is migrated. That moment is slot 2. The same rule is
recorded in `docs/workflow-readability-matrix.md`'s "Completion Rule" and in
`AGENTS.md`.

## 9. Gates

| Gate | What it protects |
| --- | --- |
| `make check-workflow-boundaries` (also in `make check-policy`) | `policy.rs` purity, mutation-scope reach, migrated-row role completeness, matrix evidence existence, facade budget when declared, and this record's slot ledger against the matrix |
| `make mutants-diff` | mutation-coverage parity for every relocated policy module |
| `make check-automation-docs` | the D-Bus contract stays unchanged while snapshots become evidence projections |
| `make check-agent-docs`, `make check-agent-skills` | standing guidance stays consistent with the convention |
| `make test`, `make test-widget-headless` | behavior and test-count equivalence; the project test count must not decrease |

`.agents/rules/documentation.md` requires updating both this record and the
matrix when workflow structure changes, so a migration cannot advance the code
while leaving the programme record stale.
