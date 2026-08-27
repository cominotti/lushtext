## Why

This is **slot 5** of the workflow-readability programme recorded in
`docs/next/workflow-readability.md`: **the workspace tree and the notes family**.
It migrates `WFR-NOTES-BOOKMARKS` and `WFR-WORKSPACE-TREE`, and carries
`WFR-AUTOMATION-SPINE` forward incrementally as every slot since 2a has.

**Prerequisite, mechanically checkable: slot 4 must be complete in all four
rows.** Both of this slot's rows are `tier-3`, and the convention requires at
least two completed lower-risk migrations before a tier-3 row is migrated. Slots
1, 2a, 2b, 3a, 3b, and 4 are archived, so the gate is satisfied eight rows over —
but it is confirmed by reading the matrix and the ledger, not this proposal. See
task 0.1.

**Why these two rows, and why together.** They are one story told from two sides,
and the seam between them is a *call*, not a shared state group:

- `WFR-WORKSPACE-TREE` owns the sidebar the user actually browses: workspace
  folders, the `GtkListView` + `GtkTreeListModel` file tree, watcher
  reconciliation, expansion-state authority, folder-reorder DnD, `Space` peek,
  inline rename, and the file operations that **create, rename, and delete the
  user's own documents on disk**.
- `WFR-NOTES-BOOKMARKS` owns the sidecars that hang off those same paths: notes,
  bookmarks, the migration ledger that survives a rename, and the startup
  app-data format-upgrade gate.
- The join is `ui/window/documents.rs:93`, where a rename completes and calls
  `migrate_note_sidecars_after_rename` (`ui/window/notes/mod.rs:431`). A rename
  in the tree is the event that moves a note.

Migrating the tree without first settling that callee's boundary would decide
the same seam twice, from two sides, with the second decision constrained by the
first — the reason slot 4 migrated buffer replacement before its callers.

### Ordering inside the change: notes first, workspace tree last

Task ordering is by increasing risk and by callee-before-caller, and both
criteria agree:

1. **`WFR-NOTES-BOOKMARKS` first.** It is the callee of the rename path, it is
   roughly a third the size, and what its durable writes touch is *app-owned
   sidecar and app-data* — recoverable through the migration ledger and the
   format-upgrade backups.
2. **`WFR-WORKSPACE-TREE` second.** It is the programme's largest row by every
   measure taken at authoring, and its file operations are the only paths in
   this slot that rename or delete **the user's own documents** rather than
   app-owned metadata. It is also the row whose facade is most likely to test the
   line budget (see below).

**Slot 5 is not split into 5a/5b, and the reason is recorded so it is not
re-litigated.** Slots 2 and 3 split because a *gate* needed mechanical
enforcement between halves: 2 carried a tier-3 half while only one lower-risk
proof existed, and 3 carried two independently tier-3 workflows before the
per-workflow role home existed. Neither applies here — eight rows are migrated,
the two-proof rule is satisfied many times over for both rows, and no convention
deliverable of this change is a prerequisite of its own second half. What the two
rows *do* share is the rename→sidecar-migration boundary and the
`services/palette/notes.rs` population, both of which a split would force to be
decided twice. The programme record's slot-5 line is one change with proposal and
tasks, and this change keeps that. **If scale forces a split during
implementation, the task ordering makes the notes row landable on its own**, so
the split point is section 3/4 and needs no re-planning — but it is an
escalation to record, not a default.

**Implementation outcome, recorded here so this section is not read as the final
state: slot 5 did split, at exactly that section 3/4 point, and for a reason this
paragraph did not anticipate** — the mandatory pre-implementation `data-safety`
pass found eleven findings including a normal-usage data-destruction bug, and
`preexisting-blockers.md` made fixing them non-optional. `WFR-NOTES-BOOKMARKS`
migrated as slot 5a; `WFR-WORKSPACE-TREE` moved to slot 5b. See
`docs/next/workflow-readability.md`, "Why slot 5 split into 5a and 5b", and the
deviation block at the head of `tasks.md`.

### This is the programme's largest seam population, by a wide margin

Measured at authoring, `ui/sidebar/**` holds **58 `*_for_test` functions across
106 `#[cfg(feature = "test-utils")]` gate sites**, spread over ten files
(`workspace_section/watch.rs` 15/24, `workspace_section/mod.rs` 15/23,
`refresh.rs` 9/10, `dialogs.rs` 6/6, `dnd.rs` 6/12, `tree_loading.rs` 4/10,
`tree_index.rs` 3/3, plus gate-only sites in `imp.rs`, `watch_targets.rs`, and
`folders.rs`). Slot 4's largest single population was 28 functions across 55
sites, in the row it deliberately migrated last. `ui/window/notes/**` adds 7
across 15.

Those figures are **upper bounds to be re-derived, not the answer** (task 0.3):
they are file-scoped rather than row-scoped, they predate the four-kind
classification, and `ui/sidebar/file_tree_item.rs` is explicitly listed in the
matrix's "Surfaces With No Coordination Tier" and therefore is **not** this row's
to count.

### The census cells are expected to be wrong, and re-verification is now normative

Five consecutive slots found their `Current size` and `Seams (i/c/a/p)` cells
wrong **in both directions**, and slot 4 turned that habit into a stated
obligation: a migrating workflow re-derives its row's measured cells, row-scoped,
excluding `#[cfg(test)]` modules, naming the shared population the old cell had
pooled. Slot 4's own re-check then found **two of four** already-migrated rows
non-compliant, so this obligation is real work rather than a formality.

This slot's two cells show the familiar shape. `WFR-WORKSPACE-TREE` is sized
`28 files, 16,947 lines (ui 11,682 / model 1,368 / services 3,897)`, whose
services subtotal pools `services/file_tree.rs`, `workspace_manager.rs`,
`workspace_watch.rs`, and `file_peek.rs` whole, including their co-located tests.
`WFR-NOTES-BOOKMARKS` is sized `22 files, 12,521 lines (ui 4,977 / model 770 /
services 6,774)`, and slot 4's amendment re-check **already named one of its
pooled populations from the other side**: `services/palette/notes.rs` is 2,163
production lines shared with migrated `WFR-COMMAND-PALETTE`, and
`services/palette/tests.rs` is a 1,223-line `#[cfg(test)] mod tests;` in its own
file that a naive per-file production scan counts as production. Authoring
measured `ui/sidebar/**` at 11,364 production lines across 21 files including
`file_tree_item.rs`; treat that as the upper bound to correct.

### Facade budget: this is the row most likely to test 370 before the minimap

**No amendment is proposed and the budget line is not to be edited.** Four slots
now agree that **stage-order count is what stresses the budget** — the exemplar's
two stage orders sit at 369 of 370 with one line spare, while a one-stage-order
facade with seven inversions fits in 253.

`WFR-WORKSPACE-TREE` has, on a first read of the code, **eleven ordered stage
orders in one row**: directory scan and expansion, watcher install plus mailbox
reconcile, targeted in-place refresh, folder-reorder DnD, file
create/rename/delete, `Space` peek, workspace list add/rename/unlist with
debounced persistence, workspace-list load, the workspace scope filter fade and
its settle timer, and focused-folder drilldown. The matrix names slot 6 (minimap)
as the slot most likely to prove 370 wrong; on stage-order count **this row gets
there first**, against a budget derived from a two-stage-order facade. Task 0.4
reconciles the count and task 2.6 decides against it before any facade text is
written.

The response is fixed in advance, in order:

1. delegate every stage body, compress each inversion to one line, and fold
   module-ownership detail into the role table and the shared-state table — the
   exact sequence that brought slot 2b back from 379 to 369;
2. if that is not enough, **escalate in-change with the measured count**, which
   now costs a **ten-row** retroactive re-check (eight migrated rows plus this
   change's two);
3. a **row split** — separating workspace-list/persistence from file-tree/watch
   into two matrix rows — is available only if the two halves genuinely have
   separate entry points, state groups, and seam populations, and it is recorded
   as a census correction with that evidence. It MUST NOT be used as budget
   avoidance for one workflow that simply narrates a lot.

Do none of these by editing the budget line quietly. The change must also not add
a physical line to `ui/search_panel/mod.rs` at 369/370, nor push the save (223),
load (271), palette (335), or slot-4 facades (167/165/216/310) over.

### Inheritances this slot is the named recipient of

Every one is verified against the archives and the matrix rather than taken on
trust:

| Inherited item | Source | What this change owes it |
| --- | --- | --- |
| `ui/window/startup_data.rs` is **slot 5's**, owned by neither slot-4 restore row | slot 4 tasks B.2 and `evidence/shared-ownership-decisions.md` §2.2; the ownership sentence is also in `ui/window/session_restore/mod.rs:97` and `ui/window/drafts/mod.rs:124` | decide its role home inside `WFR-NOTES-BOOKMARKS` (task 2.2). It *calls* `load_session_and_drafts` and `start_autosave_timer`; those stay calls into migrated facades |
| `NoteSourceRefreshCoordinator` retirement into the shared `SingleFlightCoordinator` | deferred by slot 2a, restated by slot 4 B.2; the matrix's `WFR-NOTES-BOOKMARKS` seam cell states the blocker | decide it (task 3.4). The stated blocker was that deduping the type changes `NotesBrowserRuntimeSnapshot`'s shape — **that snapshot is this row's, and this change folds it into the row's evidence surface anyway**, so the blocker is resolved by this slot's own work rather than deferred again |
| **11 pre-existing surviving field-deletion mutants in `services/file_tree.rs`** | slot 4 B.2 and its A.16 focused-run note | triage per `.agents/rules/build.md` order — missed behavior first, then tests, then a small refactor, then a narrow documented exclusion (task 5.6). They are baseline, not regressions, and they are `WFR-WORKSPACE-TREE`'s |
| `cargo-mutants` 27's `--re` does not filter struct-field-deletion mutants | slot 4 B.2 | budget the floor in every focused run and do not attribute its survivors to this change |
| `WorkspaceWatchTicket` and `NotesBrowserTicket` remain unreified | matrix "Seam Value Objects"; unchanged by slot 4 | reify both, in the established Ticket/Facts/predicate shape, gathered in a `seams.rs` module — **the precedent name slot 4 established**, not a new one |
| Named operations on slot-4 facades, to call rather than reach into | slot 4 B.2 table | `show_local_history_for_path` (the sidebar context-menu entry point whose omission slot 4 corrected), `adopt_startup_draft_records`, `session_restore_evidence()`, `draft_evidence()` |
| The notes/sidecar adjacency slot 4 deliberately did not touch | slot 4 B.2 | `ui/window/local_history/restore_execution.rs` calls `resolve_notes_for_editor` from two restore terminals and `local_history/journal.rs` records a `MigrationKind` through the shared migration ledger. **Those stay calls**; this change must not restructure a migrated row |
| Gate blindness to untracked files | slot 4's friction section, now in `.agents/rules/build.md` | `git add -N` every new directory **early**, before the first diff-aware gate, and treat a green diff-aware gate on untracked new files as unproven (task 0.9) |
| The three deferred simplify candidates in slot 4 B.3 | slot 4 B.3 | **verified: none is this slot's to adopt.** They are in `drafts/journal.rs` and `local_history/preview_execution.rs` — files this change does not touch — and in `ui/window/imp.rs`, which this change *does* touch for its state groups and module declarations, but the candidate there is a `current_window_width` helper duplicated with `local_history/preview_execution.rs` and owned by that migrated row's dedup decision. All three remain **slot 7's**; task 0.10 confirms none has moved into this slot's files |
| Slot 4's two `[~]` deferred acceptance items | slot 4 tasks 10.7 and 10.10 | **not absorbed.** The live-session paned proof and the quiet-machine `bench-compare` remain slot 4's, user-availability-gated. This change neither ticks them nor re-plans them |

### Two adjacencies this slot must resolve rather than inherit

1. **`gtk-adapter-module-boundaries` already constrains both of these
   directories, by pre-convention module names.** Its "Window notes are organized
   by existing workflows" requirement names `ui/window/notes`'s focused siblings
   (bookmark lifecycle, note editors, browser/palette coordination), and its
   "Workspace-section wiring has focused owners" requirement names `imp.rs`,
   `row_factory.rs`, `context_menus.rs`, and `row_accessibility.rs`. **None of
   those is a role name.** The role requirement in the same spec says every
   module of a decomposed workflow carries exactly one role. Reconciling the two
   is this change's spec work, and it is naming rather than restructuring: the
   focused siblings keep their behavior and become either stage-order-qualified
   bounded coordination modules or recorded presentation surfaces.
2. **An evidence surface over a lazily materialized tree.** Every surface so far
   read one widget, or a window plus one editor page. This row's observable state
   lives across **a variable-sized set of per-workspace section widgets** whose
   file tree is a `GtkTreeListModel` that **creates child models on demand**.
   `.agents/rules/ui.md` already forbids rediscovering expansion by walking the
   flattened model outside bootstrap, and warns that hover-driven child-model
   creation can materialize descendants and restart watches. An evidence accessor
   that walks that model to answer a question would therefore *do work* — which
   is the same class of hazard as slot 3a's template-child read, discovered
   before it is shipped rather than after.

## What Changes

- **Migrate `WFR-NOTES-BOOKMARKS` first.** Give it a narrative facade at
  `ui/window/notes/mod.rs`, role-named coordination for its stage orders
  (browser source build and query, bookmark lifecycle, note editors, and the
  rename-driven sidecar migration whose control resumes **in a later process
  run** through the migration ledger — the longest-lived inversion in the
  codebase), one `policy.rs`, one `evidence.rs`, and a `seams.rs` holding the
  reified `NotesBrowserTicket`. **Check `journal` first** for the migration
  ledger and the sidecar records, per slot 3a's reusable test — *does a later
  stage of the same workflow read the record back* — and record the verdict per
  stage order rather than once for the row.
- **Decide `ui/window/startup_data.rs`'s role home explicitly.** It is the
  startup app-data format-upgrade gate and its census home is this row. Decide
  whether it becomes a coordination role module inside the notes role home or
  stays a called surface at `ui/window/` with its ownership recorded in its own
  module doc. Do not let the notes row absorb it by default because it is
  migrated first, and do not re-open the slot-4 decision that it belongs to
  neither restore row.
- **Resolve the `NoteSourceRefreshCoordinator` question slot 2a deferred.** Two
  independent instances exist — `command_palette_note_refreshes` on the window
  imp serving migrated `WFR-COMMAND-PALETTE`, and `source_refreshes` in
  `ui/window/notes/mod.rs` serving the browser. Decide between retiring the type
  into `services::single_flight::SingleFlightCoordinator` and keeping it, with
  the deciding evidence being what each actually owns beyond
  submit/finish/supersede. If it is retired, the palette's instance changes type
  too, which touches a **migrated** row: that is permitted only as a
  type-level substitution with the palette's evidence surface and exported
  snapshot fields proved unchanged, never as a restructuring of a migrated
  workflow.
- **Migrate `WFR-WORKSPACE-TREE` second, and decide its role home by collision
  analysis.** `ui/sidebar/` hosts one workflow but is a **nested** pair of
  directories: the orchestrator plus `workspace_section/`, which owns a distinct
  GObject and already carries a bounded role name (`watch.rs`) alongside
  coordination (`refresh.rs`, `tree_loading.rs`, `tree_index.rs`,
  `watch_targets.rs`) and presentation (`row_factory.rs`, `context_menus.rs`,
  `row_accessibility.rs`, `icon_presentation.rs`). Choose the canonical role home
  and give every module in both directories exactly one declared role, with **one**
  `policy.rs` and **one** `evidence.rs` for the row.
- **Reify `WorkspaceWatchTicket`** (`{targets_generation, lifetime_generation}`,
  today a loose tuple compared clause-by-clause at two sites in
  `workspace_section/watch.rs`) beside the already-reified `WorkspaceScanTicket`,
  and re-audit the scan side against the two-boundary rule.
- **Relocate the two single-workflow policy modules the census found**, with
  mutation parity: `model/workspace_scan.rs` (231 lines) and
  `model/workspace_persistence.rs` (338), both recorded as single-workflow and
  relocating with this row. This is the programme's **first slot since 3a to
  carry a real relocation**, so it owes before/after parity numbers from the
  exact `make mutants-diff` invocation with file-level anchors, reported
  separately from any gain-from-zero extraction out of the GTK adapters.
  `model/workspace.rs` (28 consumers) and the notes domain modules
  (`note.rs`, `bookmark.rs`, `sidecar_identity.rs`, `folder_note.rs`,
  `document_note.rs`) are recorded as domain and staying — **confirm, do not
  assume**, and check for a `services` consumer, which forbids relocation
  outright.
- **Build one evidence surface per workflow**, folding in the pre-convention
  typed observations rather than leaving second paths:
  `WorkspaceScanPressureEvidence`, `WorkspaceWatchMailboxSnapshot`,
  `WatchTargetSnapshot`, and `SidebarFileRowStateSnapshot` on the tree side;
  `NotesBrowserRuntimeSnapshot`, `OpenEditorNoteSnapshots`, and
  `NoteSourceRefreshCoordinatorSnapshot` on the notes side. Each surface owes the
  three stated proofs — tight-borrow discipline, the disposed-widget rule through
  `try_get()`, and the reentrancy proof that drives the workflow through each
  mutable-borrow operation and reads the surface **after** each one — plus, new
  here, proof that reading it **materializes nothing**.
- **Migrate the widget-test reach-through both rows carry**, catalogued by field
  name rather than by line. Follow slot 3a's finding that an ungated `imp()`
  *write* is usually a real drive in disguise: reach for an existing configuration
  seam plus a real drive before adding a counted actuation seam. **Production
  code has the same problem here**, which no previous slot hit:
  `ui/automation.rs:766` and `:927` read `imp.sidebar.imp()
  .workspace_filter_animation_active` directly, a production widget-internals
  reach that must become a named accessor or an evidence projection.
- **Project automation from evidence without widening the contract.** This slot
  owns **two whole snapshot objects** — `window.workspace` (10 fields) and
  `window.notes` (6) — plus the `workspace-persist`,
  `workspace-tree-refresh`, and `workspace-filter-animation` readiness blockers
  and the `workspace-refresh-complete` predicate. Two ownership questions come
  with them: whether `workspace-sidebar-animation` is this row's or
  `WFR-SHELL-LAYOUT`'s (slot 7), and how the drift gate handles
  `snapshot-field-active-document-file-backed`, **one documented field id already
  bound to two snapshot objects** (`notes.*` and migrated `local_history.*`) and
  therefore to two evidence surfaces. Prove no-widening by capture and diff, not
  by assertion.
- **Advance the matrix and the programme record in the same change**: two
  `Migrated Workflow Roles` subsections, two corrected `Current size`,
  `Entry points`, and `Seams` cells naming their pooled populations, `Seam Value
  Objects` and `Workflow Stage Traces` updates, the `Policy Module Census` rows
  for the two relocated modules, the slot-5 ledger line flipped to complete with
  `WFR-AUTOMATION-SPINE (partial)` carried onto slot 6's outstanding line, a
  "Baseline after slot 5" table, and a "Convention friction slot 5 hit" section
  for slots 6 and 7. **Evidence pointers in live
  `openspec/changes/<name>/evidence/...` form**; an archive-prefixed pointer fails
  the gate immediately.

**Explicit non-goals.** No change to workspace persistence format or its debounce
window, watcher backend or its mailbox caps, scan slicing or placeholder
behavior, expansion-state semantics, DnD reorder rules, peek behavior, inline
rename or file-operation semantics, note or bookmark sidecar formats, migration
ledger format or its startup reconcile ordering, format-upgrade backup or
diagnostic behavior, note scoring, any user-visible string, or the exported D-Bus
contract. **No restructuring of a migrated row**: `WFR-COMMAND-PALETTE`,
`WFR-LOCAL-HISTORY`, `WFR-DRAFT-RECOVERY`, and `WFR-SESSION-RESTORE` are called,
not rebuilt, and the only permitted touch is the `NoteSourceRefreshCoordinator`
type substitution above, proved neutral. `WFR-SHELL-LAYOUT` (slot 7) keeps the
sidebar show/hide animation and the recent-documents surface. `services/fuzzy.rs`,
`services/single_flight.rs`, `services/sync.rs`, `model/editor_memory.rs`,
`model/migration_ledger.rs`, and `ui/plain_disposal.rs` are cross-cutting or
exempt and unchanged. No workflow is reified as an explicit state machine, and no
programme-level deferred actuation seam is retired.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gtk-adapter-module-boundaries`: **reconcile the two pre-convention
  decomposition requirements with the role contract in the same spec, state the
  scope of the role taxonomy, and state the nested role home.** All three
  statements close adjacencies the convention already sanctions or describe
  practice the project already follows; **none adds a role name, and none widens
  the bounded coordination set.**

  **(a) A migrating workflow classifies its pre-convention focused siblings
  rather than choosing between two requirements.** "Window notes are organized by
  existing workflows" and "Workspace-section wiring has focused owners" both
  mandate module *topics* (`bookmarks`, `editors`, `browser`; `row_factory`,
  `context_menus`, `row_accessibility`), while "Decomposed workflow modules carry
  named roles" mandates that every module of a migrated workflow carries exactly
  one *role*. Read together today, a migration of either directory appears to be
  required to violate one of them. The amendment says what actually holds: the
  topical decomposition and its behavior obligations survive migration unchanged,
  and each sibling is classified as the facade, as a bounded — optionally
  stage-order-qualified — coordination role, or as a called presentation surface.

  **(b) A called presentation surface is not a role, and the five-name taxonomy
  is scoped accordingly.** This is the one statement that is not purely an
  adjacency, so it is stated plainly rather than folded into (a). Statement (a)
  needs a category for a module that only projects a workflow onto widgets, and
  the role taxonomy enumerates exactly five names, none of which fits. The
  amendment therefore states the taxonomy's **scope**: such a module is outside
  the set of decomposed workflow modules the requirement governs, MUST NOT take
  one of the five role names, MUST NOT own a `policy.rs` or an `evidence.rs`, and
  is recorded in its own module doc and in the workflow's matrix row.

  **This describes practice the project already has rather than inventing a
  category**: slot 4 shipped exactly such a module — `WFR-LOCAL-HISTORY`'s
  per-tab capture surface at `ui/editor_page/local_history.rs` — recorded in the
  matrix and in its own module doc, and *not* as a role. What did not exist was
  spec text saying that is legitimate. The alternative considered and **rejected**
  was adding a sixth role name: that widens the closed taxonomy for modules that
  perform no workflow coordination, and it would retroactively make slot 4's
  recorded surface a role it never declared. Task 1.3's eight-row re-check
  therefore asks, per row, whether the row owns a module that is none of the five
  and whether it is recorded in **both** places — and that question is **not**
  expected to be a pure confirmation.

  **(c) The two permitted role homes may be nested.** A workflow that owns a
  directory and a widget subdirectory of it MAY keep its facade, its single
  `policy.rs`, and its single `evidence.rs` in the parent while bounded
  coordination role modules live in the subdirectory whose widget they
  coordinate. This is the nested case of the two-directory resolution slot 4
  proved for `WFR-LOCAL-HISTORY` — canonical role home plus recorded called
  surfaces — and it changes neither the one-`policy.rs`-per-workflow rule nor the
  `ui/**/policy.rs` mutation glob, which reaches either location.

- `workflow-evidence-surfaces`: **reading an evidence surface must not
  materialize toolkit state, and a surface over a variable-sized child collection
  must be bounded and honest at its extremes.** The existing scenario says
  reading has no effect on "workflow state, timers, queues, or generation
  counters", which does not name the hazard this row carries: a `GtkTreeListModel`
  creates child models on demand, so an accessor that walks it to answer a
  question performs work, can materialize descendants, and can restart watches —
  while every field it produced still reads as a pure observation. This is the
  same class as the disposed-widget rule slot 3a discovered by panicking, and it
  is stated before it ships rather than after. The amendment also states the
  collection case the disposed-widget rule implies but does not cover: a field
  aggregated over N child widgets is bounded, answers honestly when the
  collection is empty, and skips a disposed child rather than panicking on it.

  **Retroactive-amendment obligation.** Under section 8 these **four** statements
  — (a), (b), and (c) on `gtk-adapter-module-boundaries` plus the
  evidence-surface one — trigger a per-row re-check of **eight** migrated rows
  (`WFR-SEARCH-REPLACE`, `WFR-COMMAND-PALETTE`, `WFR-DOCUMENT-SAVE`,
  `WFR-DOCUMENT-LOAD`, `WFR-BUFFER-REPLACEMENT`, `WFR-SESSION-RESTORE`,
  `WFR-LOCAL-HISTORY`, `WFR-DRAFT-RECOVERY`). Three are *expected* to be
  confirmations — no migrated row is in a directory governed by (a), none has a
  nested role home, and none reads a lazily materialized model. **(b) is not**:
  `WFR-LOCAL-HISTORY` already owns a module that is none of the five roles, so
  every row must be checked for one, and for whether it is recorded in both its
  module doc and its matrix row. Slot 3b and slot 4 both proved that "it must
  already hold" is not a discharge, and slot 4's re-check found two of four rows
  genuinely non-compliant. Each row is checked individually and any gap is filled
  **in this change**. Nothing beyond these four statements may be absorbed: the
  facade line budget, **the five role names, the bounded coordination role set**,
  the seam value-object shape, and the evidence-surface visibility rule are
  **not** amended.

Note that `openspec validate --strict` fails any change with no `specs/` delta
("Change must have at least one delta"), which is why every migration slot carries
one; slot 2a corrected the record text that said otherwise.

## Impact

**Prerequisites**

Slots 1, 2a, 2b, 3a, 3b, and 4 are archived under
`openspec/changes/archive/2026-08-25-*`, `2026-08-26-*`, and `2026-08-27-*`. The
blocking gate is **slot 4 complete in all four rows with their `Migrated Workflow
Roles` subsections naming paths that exist, the ledger marking slots 1 through 4
complete and slot 5 outstanding, and `make check-workflow-boundaries` passing** —
verified mechanically in task 0.1, not read from this proposal. The deliverables
this change consumes are the declared facade budget (2a), the stage-order
qualification rule (2a), the evidence-to-snapshot drift check and its per-binding
attribution (2a, extended by 3b and 4), the `journal` role name (2b), the
per-workflow subdirectory role home (3a, now six adopters), the evidence-surface
reentrancy constraint (3b), and slot 4's row-cell re-derivation obligation, its
`policy: none` allowance, its `seams.rs` module precedent, and its
canonical-role-home-plus-called-surface resolution for a workflow spanning two
directories.

**Code touched** (sizes measured at authoring as **upper bounds**; task 0.3
re-derives every one row-scoped and non-`#[cfg(test)]`)

- `crates/lushtext-core/src/ui/sidebar/**` — 21 files, 11,364 production lines at
  authoring: the orchestrator (`mod.rs` 406, `workspaces.rs` 843, `callbacks.rs`
  219, `dialogs.rs` 205, `imp.rs` 246) and `workspace_section/**`
  (`tree_loading.rs` 1,269, `tree_index.rs` 844, `context_menus.rs` 809,
  `folders.rs` 835, `dnd.rs` 769, `mod.rs` 744, `peek.rs` 728, `refresh.rs` 666,
  `watch.rs` 583, `actions.rs` 534, `imp.rs` 508, `row_factory.rs` 463,
  `watch_targets.rs` 264, `row_accessibility.rs` 199, `icon_presentation.rs` 80).
  **`file_tree_item.rs` (150) is listed in the matrix's "Surfaces With No
  Coordination Tier" and is not this row's to count** — confirm rather than
  assume.
- `crates/lushtext-core/src/ui/sidebar/AGENTS.md` — the nearest ownership guidance
  for this subtree, whose "Local Contracts" section states the scan-flight,
  watcher-mirror, mailbox-cap, DnD-shield, and row-factory ownership rules this
  change must preserve and re-point after any move.
- `crates/lushtext-core/src/ui/window/notes/**` — `browser.rs` 1,749,
  `editors.rs` 929, `mod.rs` 892, `bookmarks.rs` 795.
- `crates/lushtext-core/src/ui/window/startup_data.rs` (435) — the format-upgrade
  gate slot 4 routed here; ownership decided in task 2.2.
- `crates/lushtext-core/src/model/workspace_scan.rs` (231) and
  `workspace_persistence.rs` (338) — the two relocations, with parity evidence.
  **These two figures, and the `services/` and `model/` figures below, are raw
  file totals including co-located tests**, deliberately so for the relocations
  because the tests move with the module; task 0.3's production-only
  re-derivation applies to the row *size* cells, not to these.
- `crates/lushtext-core/src/model/workspace.rs` (799),
  `note.rs` (296), `bookmark.rs` (186), `sidecar_identity.rs` (172),
  `folder_note.rs` (85), `document_note.rs` (31) — **expected unchanged**; all
  recorded as domain and staying. Confirm, and name any `services` consumer.
- `crates/lushtext-core/src/services/file_tree.rs` (1,050 total, 603 production) —
  **behavior unchanged**, and the owner of the 11 inherited surviving
  field-deletion mutants.
- `crates/lushtext-core/src/services/workspace_manager.rs` (1,329),
  `workspace_watch.rs` (1,030), `file_peek.rs` (489) — behavior unchanged; in
  scope for seam classification and for any buried pure policy decided explicitly,
  per slot 2b's `services/search_backup.rs` precedent. A `services -> ui`
  relocation is forbidden outright.
- `crates/lushtext-core/src/services/palette/notes.rs` (3,428 with its
  `#[cfg(test)] mod tests;`, 2,163 production lines **shared** with migrated
  `WFR-COMMAND-PALETTE`), `services/palette/mod.rs`, `services/note_storage.rs`
  (337), `bookmark_service.rs` (694), `bookmark_excerpt.rs` (888),
  `folder_note_service.rs` (948), `document_note_service.rs` (676),
  `migration_ledger.rs` (476, cross-cutting and staying), and
  `services/format_upgrade/**` (3,013 across six files) — behavior unchanged.
- `crates/lushtext-core/src/ui/window/documents.rs` (the rename→sidecar-migration
  join at `:93`), `ui/window/actions.rs`, `ui/window/imp.rs` (the sidebar/notes
  state groups and the `show_local_history_for_path` route at `:860`), and
  `ui/window/mod.rs` — called surfaces and module declarations.
- `crates/lushtext-core/src/ui/automation.rs` — `window.workspace`,
  `window.notes`, the three workspace readiness blockers, and the two production
  `imp.sidebar.imp()` reads at `:766` and `:927`.
- `crates/lushtext/tests/widget/{sidebar,workspace_section,file_tree_item,window,
  command_palette,accessibility}.rs` — `workspace_section.rs` alone is 5,846 lines
  and 123 tests; re-derive the ungated `.imp().` reach-through by field name.
- `scripts/check-automation-docs.py` — two new projecting objects must be
  registered (`window.workspace` and `window.notes` appear in the Evidence
  Projection Map today only by their absence), and the shared
  `snapshot-field-active-document-file-backed` binding may require extending the
  per-binding attribution to one documented field across two objects.
- `scripts/accessibility_warning_allowlist.py` — it can key on module paths, so a
  rename could silently turn an expected `tracing::error!` into an "unexpected
  warning". Its only module-path key today is
  `editor_page::load::execution`, so **no coupling to this slot's modules exists
  at authoring** and task 9.11 stays a conditional confirmation rather than
  budgeted work; if a coupling has appeared, update it and re-verify it still
  **rejects** both an unrelated path and the stale module name.
- `scripts/prepare-command-palette-notes-fixture.py`,
  `scripts/run-command-palette-notes-smoke.sh`,
  `scripts/run-format-upgrade-manual-test.sh` — coupled to this family's fixtures
  and app-data layout; in scope for confirmation, not for loosening.
- `.cargo/mutants.toml`, `docs/workflow-readability-matrix.md`,
  `docs/next/workflow-readability.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `docs/end-user-coverage.md`, `AGENTS.md`,
  `README.md`, and any `.agents/rules/*.md` or `.agents/skills/**` reference
  naming a moved path or a retired seam.

**Verification**

Everything slot 4 ran, re-aimed at the sidebar and the notes family, and
**headless throughout**. Behavior equivalence across: a workspace with zero, one,
and many folders; a deep tree with long paths; expand, collapse, and a user
collapse racing a deferred restore callback; a directory scan superseded by a
newer one; a watcher install superseded during its worker; a mailbox overflow
that promotes targeted paths to one full refresh; a targeted in-place refresh
after create, rename, and delete; folder-reorder DnD including an invalid drop
position; `Space` peek reached by keyboard focus on a realized row; a double-click
that opens a file row while a double-click on a directory row expands it, which is
the `GtkTreeExpander` internal-gesture contract `.agents/rules/ui.md` records as a
three-iteration lesson; a workspace scope filter change superseded before its
settle timer fires; entering and leaving focused-folder mode with the four DnD
gates that read drilldown emptiness; inline rename including the focus-out
double-fire guard;
workspace add, rename, and unlist with debounced persistence, a failed write, and
a close-time flush; a rename that migrates note sidecars and one whose migration
is deferred to the ledger and reconciled on the next launch; bookmark toggle and
label edit with no editor, one bookmark, and many; document and folder note
open/save/discard; notes browser with no notes, one, many, and a query with no
matches, plus rapid mode switching that must not let a stale completion publish;
and a startup format-upgrade gate for equal, older-upgradeable, and newer app
data. Plus: `make check` and `make check-policy` including
`check-workflow-boundaries`, `check-automation-docs`,
`check-accessibility-policy`, `check-visual-proof-policy`, and
`check-filesystem-boundary` — the last because this row mutates the user's files;
**the rustdoc lint gate, which is CI-only and in neither `make check` nor
`make pre-commit` nor `make check-policy`**; `make test`,
`make test-widget-headless`, and the focused `make test-workspace-row-states`
with **zero `FLAKY:` lines and no retry relied upon**, and a test count that does
not decrease; `make mutants-diff` with relocation parity **and** gain-from-zero
reported separately, plus the `services/file_tree.rs` survivor triage;
`make command-palette-notes-smoke`; `make performance-smoke`;
`make test-prop` if any property target is touched; a `data-safety` pass in
explicit mode **before and after** the diff; and the mandatory proof lanes
(`visual-geometry-smoke`, `accessibility-smoke`, `visual-smoke`) each from a
**clean artifact root** and ordered **after every source, documentation, and rules
edit**, because the accessibility policy gate fingerprints the contents of
relevant files.

**Display-dependent proof is deferred for user availability from the start, not
discovered late.** Slot 4 established that isolating an app's state does not
isolate its window: a real Wayland launch maps a surface and takes focus
regardless of where its state lives, and it interrupted the user's session. This
change therefore plans **no live launch on its critical path**. The live `make
run` sidebar/paned-warning walkthrough and the three manual fixture lanes
(`make run-format-upgrade-newer-manual-test`,
`make run-format-upgrade-older-manual-test`,
`make run-command-palette-notes-manual-test`) are recorded as `[~]` deferred
items with their exact remaining scope, to be scheduled with the user. Everything
else has a headless path. Acceptance is that tree contents, expansion state,
watcher reconciliation, file-operation outcomes, persistence, note and bookmark
records, migration reconcile, format-upgrade decisions, error surfaces, timing
characteristics, and the exported D-Bus contract behave identically to the
pre-migration workflows.
