## Why

This is **slot 5b** of the workflow-readability programme recorded in
`docs/next/workflow-readability.md`: **the workspace tree**, and nothing else. It
migrates `WFR-WORKSPACE-TREE` and carries `WFR-AUTOMATION-SPINE` forward
incrementally as every slot since 2a has.

The row exists as its own slot because **slot 5 split during implementation**, and
the reason is recorded in the programme record's "Why slot 5 split into 5a and 5b"
rather than restated here as opinion: slot 5's mandatory pre-implementation
`data-safety` pass found **eleven** findings including a normal-usage
data-destruction bug — inline rename validated only empty-or-unchanged while the
platform rename silently replaces a regular destination, so renaming a file onto
an existing sibling's name destroyed that file with no prompt, no warning, and no
undo. `.agents/rules/preexisting-blockers.md` has no exceptions, so seven fixes
landed first and consumed the change's capacity. `WFR-NOTES-BOOKMARKS` migrated
as slot 5a; this row's structural migration is what remains.

**Prerequisite, mechanically checkable: slot 5a must be complete, and the tree
row must still be `pending`.** This row is `tier-3` and the convention requires at
least two completed lower-risk migrations first; nine rows are migrated, so the
gate is satisfied many times over — but it is confirmed by reading the matrix and
the ledger, not this proposal. See task 0.1.

### What 5a left behind is decisions, not questions

This is the best-prepared slot in the programme, and the preparation is precise
enough to be *verified* rather than re-derived. Every item below is recorded in
the archived slot-5a change and re-verified in task 0.2:

| Already decided | Where | What this change owes it |
| --- | --- | --- |
| **Role home: option (2), nested.** Canonical `ui/sidebar/`, bounded coordination role modules inside `ui/sidebar/workspace_section/`, exactly one `policy.rs` and one `evidence.rs` at the canonical home | 5a `evidence/shared-ownership-decisions.md` §2.5 | implement it. This is the **first adopter** of the nested role home statement (c) that 5a landed |
| **Nearly every module classified**, both directories, including two **dissolutions** (`tree_index.rs`, `watch_targets.rs`) and **nine** called presentation surfaces | same, §2.5 "Every module classified" | implement the classification as written, or record a deviation with its reason. **One gap to fill rather than inherit**: 5a's map gives `scan_admission.rs` and `scan_execution.rs` no named source and never classifies `tree_loading.rs` (1,269 — the row's largest file), so "every module classified" was not literally true. This change classifies it as a third dissolution (see What Changes) |
| **One row, one facade, delegated hard** — option (1), projected **≈351 of 370** across eleven stage orders. No budget escalation, no census-row split | same, §2.6; `evidence/facade-measurements.md` | **exercise it.** The projection was never tested against real facade text, and the subtrahends have already moved (see below) |
| **`journal` verdict per record**: `workspaces.json` is **not** a journal — `execution` with latest-generation supersession, named `persist_execution.rs` | same, §2.7 | implement `persist_execution.rs`; do not re-litigate the verdict |
| **Buried service policy**: all ten shared services **stay**; `services -> ui` relocation forbidden outright | same, §2.8 | confirm, do not re-open |
| **Six no-materialization code facts**, verified from the code | 5a `evidence/evidence-surface-materialization.md` | this row's evidence surface is the rule's **primary test**. Re-verify each site's line number, which 5a's own fixes may have moved |
| **`policy.rs` (261), `seams.rs` (133), `test_policy.rs` (88)** already exist, are pure, and are inside the `ui/**/policy.rs` mutation scope | landed with 5a's fixes | extend rather than re-create. `FileOperationTicket` + `FileOperationFacts` is already the row's reified file-operation seam |
| **The reconciled stage trace**: 11 stage orders, 27 deferral primitives plus 11 non-primitive callback resumptions = 38 resumption points, against a census floor of **5** | 5a `evidence/stage-traces.md` | narrate every stage order and correct the `Workflow Stage Traces` floor, the **widest correction in the programme** at ~7.6x. **But re-derive the count**: authoring found a twelfth candidate the reconciled trace does not name (see below) |
| **`WorkspaceSidebarWidthPreset` is `WFR-SHELL-LAYOUT`'s**, and should move to `ui/sidebar/width_preset.rs` when this facade is written | 5a `evidence/shared-ownership-decisions.md` §2.10, B.2 | move it as cross-cutting. It is 103 of the facade's current lines and the budget projection depends on the move |

**What is explicitly *not* inherited**: slot 5a's own `[~]` deferred live/manual
proof, slot 4's two `[~]` items, and slot 4's three B.3 simplify candidates. Task
0.10 confirms none has moved into this row's files rather than assuming it.

### The census cells moved again — and the first draft of this section got the units wrong

Five consecutive slots found their measured cells wrong in both directions, and
slot 4 made re-derivation a stated obligation. This slot found a variant of the
same failure **and then committed it**, which is worth recording rather than
quietly fixing, because it is the sharpest available argument for the obligation
being mechanical rather than a matter of care:

**This proposal's first draft claimed slot 5a's freshly corrected cell had gone
stale by ~900 lines. That claim was false, and the cause was a units error.**
Slot 5a's census is **production-only** — its
`evidence/census-reverification.md` states the method verbatim in its third line,
excluding `#[cfg(test)]` modules by brace tracking — while the first draft
compared it against **raw** file totals. Four of the eight files it accused of
growing had grown **only in their `#[cfg(test)]` module and had zero production
growth**: `tree_index.rs` (production 844, `#[cfg(test)]` opens at `:845`),
`watch_targets.rs` (264, `:265`), `icon_presentation.rs` (80, `:81`), and
`refresh.rs` (666, `:667`). A fifth, `watch.rs`, grew by **10** production lines
(583 → 593), not the 23 the raw total suggested.

**Slot 5a's cell was right, and four of its per-file figures are exactly
reproducible.** Re-derived in production units at this change's authoring:

| File | 5a production | Now | Production growth |
| --- | --- | --- | --- |
| `workspace_section/actions.rs` | 534 | **707** | **+173** (5a's rename/cleanup fixes) |
| `workspaces.rs` | 843 | **864** | **+21** (the M-4 load-generation guard) |
| `watch.rs` | 583 | **593** | **+10** (the watch-target repair operation) |
| `ui/sidebar/mod.rs` | 406 | **415** | **+9** (module declarations, one re-export pair) |
| `tree_index.rs`, `watch_targets.rs`, `icon_presentation.rs`, `refresh.rs` | 844 / 264 / 80 / 666 | unchanged | **0** — raw growth is `#[cfg(test)]` only |
| new: `policy.rs` / `seams.rs` / `test_policy.rs` | — | 134 / 92 / 88 | **+314** |

Total production growth **+527** (213 in existing files, 314 in the three new
ones), giving a row total of **11,741 production lines across 23 files** — which
is 5a's 11,214 plus 527, and independently equals the direct per-file sum, so the
figure is checkable two ways. Excluded: `ui/sidebar/file_tree_item.rs` (150,
listed in the matrix's "Surfaces With No Coordination Tier").

Two consequences follow, and both are load-bearing:

1. **Task 0.3's re-derivation must state its unit on every figure**, because the
   error was not a miscount but a comparison between two different metrics. The
   figures in this proposal's Impact section are **raw** where they describe files
   to be moved wholesale and **production** where they describe the row's size,
   and each says which.
2. **One matrix cell is genuinely stale**: it records 5a's new `policy.rs` at
   **190** lines, which is neither its current raw size (**261**) nor its
   production size (**134**). Task 0.3 corrects it, and 9.6 writes the corrected
   figure with its unit named.

The seam population re-derives to **60 functions across 111
`#[cfg(feature = "test-utils")]` gate sites** — byte-exactly matching the matrix
cell at authoring, and still the largest seam population in the programme by more
than double (slot 4's largest single row held 28 across 55). **Two ungated
`_for_benchmark` seams sit outside that census entirely** and are named here
because task 6.1's evidence narrowing would break the bench target without them:
`workspace_section/mod.rs:608 child_cache_rebuild_operation_evidence_for_benchmark`
(a `pub` function imported by `benches/benchmarks.rs:95`) and
`services/workspace_watch.rs:267 merge_backend_result_for_benchmark`.

### Facade budget: the projection must now be re-measured before it is trusted

**No amendment is proposed and the budget line is not to be edited.** Slot 5a's
handoff sharpened the programme's understanding of what stresses the number:
its own five-stage-order facade fits in **178**, so *stage-order count alone is
not the pressure* — the exemplar's 369 comes from twelve prose inversions plus a
large value-type surface in one file.

But 5a's ≈351 projection was arithmetic over `ui/sidebar/mod.rs` **at 406 lines**,
and the file is now **415**. **Three** of its four subtrahends are line-range
extractions — `WorkspaceSidebarWidthPreset` (recorded at 171–273),
`SidebarFileRowStateSnapshot` (56–91), and `WorkspacePersistenceFlushError`
(34–54) — and all three ranges are stale by construction, because the file grew
by nine lines. This proposal deliberately does **not** guess the new ranges:
task 2.1 re-derives every subtrahend against the current file, which is the only
honest way to re-project. The projection also predates the twelfth stage-order
candidate below, which costs narration lines if 0.4 confirms it. The escalation
path is fixed in advance, in order:

1. **delegate harder** — every stage body delegated, each inversion compressed to
   one line, module-ownership detail folded into the role table and the
   shared-state table: slot 2b's exact sequence, which brought a facade from 379
   back to 369;
2. if that is not enough, **escalate in-change with the measured count**, now
   costing a **nine-row** retroactive re-check;
3. a **census-row split** remains available only on the evidence 5a already
   weighed and **rejected** — it found the two halves genuinely *not* independent,
   because the workspace list's add/unlist creates and destroys the very sections
   the file tree lives in, `load_workspaces` is the single entry point for both,
   and both share `current_scope`, `workspaces_file`, and the persistence
   debounce. Reversing that conclusion needs new evidence, not a budget problem.

Do none of these by editing the budget line quietly, and do not add a physical
line to `ui/search_panel/mod.rs` at 369/370, nor push the notes (178), save
(223), load (271), palette (335), or four slot-4 facades over.

### This row is the reason the no-materialization rule exists

Slot 5a landed the `workflow-evidence-surfaces` statement that reading an
evidence surface must not materialize toolkit state, and proved the *discipline*
on a surface over a fully materialized `AdwTabView` — while recording plainly
that "the tree surface will prove the *hazard*". That is this change.

Six offending code facts are already verified and recorded (5a's evidence file
labels them "five" while tabulating six — task 0.5 reconciles the count):
`find_store_for_dir` calls `row.children()` and then **inserts into the
`dir_stores` cache**, so a nominal read materializes a child store, starts a
background scan, and mutates a cache; `visible_child_stores` calls
`row.children()` with **no `is_expanded()` filter**, materializing every
flattened row's children; `expanded_store_index` is safe *only* because of an
`is_expanded()` guard; `set_expanded(true)` at four sites materializes children
**and** fires the `notify::expanded` hook that queues a watcher restart;
`derive_expanded_paths_from_model` increments the very capture counters the
surface must report; and `find_dir_row` evicts from its cache on a lookup. The
surface this change builds MUST derive from `expanded_paths` — the authoritative
live set `.agents/rules/ui.md` already names as authoritative — and must prove it
with reads taken **both** collapsed and expanded.

The surface also owes the child-collection half of the same statement, which no
prior row has needed: this row's observable state lives across a **variable-sized
set of per-workspace section widgets**, so every aggregated field must be
bounded, must answer honestly with zero workspaces, and must **skip a disposed
section rather than panicking on it**. Slot 5a's disposal proof caught a real
panic on a *transitively* reached `TemplateChild`; this row reaches N sections
plus a window.

### This row renames and deletes the user's own documents

Every other consideration is subordinate to this one. `WFR-WORKSPACE-TREE` is
`tier-3` throughout, and its file operations are the only paths in this slot that
touch **the user's own documents** rather than app-owned metadata.

Slot 5's data-safety pass over exactly this code found eleven findings, and the
programme record's lesson is explicit: *a tier-3 migration slot must budget for
the pass finding more than one defect, because the pass is aimed at exactly the
code the migration is about to restructure.* This change therefore plans the
`data-safety` pass as a section with named candidates and its own capacity, before
and after the diff, and treats a confirmed finding as blocking work in this change
per `preexisting-blockers.md` — including the possibility that it again forces a
scope decision, which must be recorded as a deviation rather than absorbed
silently.

Two named data-safety items are already this row's:

- **M-4's driven race test.** The fix landed in `ui/sidebar/workspaces.rs` (a
  tree file): `load_workspaces` now captures `requested_generation()` before
  dispatch and skips `build_sections_from_file` when a mutation superseded it.
  Slot 5a recorded it as proved **by the guard's shape only** — "no driven race
  test", because forcing a "New Workspace" between the load dispatch and its
  completion needs a load-worker delay seam — and named it "the highest-value
  remaining test for 5b". This change owns it, and the seam it needs is a
  **third** counted configuration seam that must be justified individually.
- **The `sidecar_resolved` flush-versus-in-flight coverage gap** is **not** this
  row's, and the boundary is recorded rather than assumed: the four unproven
  pass-2 defects live in `ui/window/notes/bookmark_execution.rs`, and their
  drivers are tab close and Save As — neither a tree file nor a tree entry point.
  Task 7.6 records the boundary with the owning row named so a later slot does
  not read the omission as an oversight.

The handoff homes 5a created were also checked for findings that live in **this
row's** files: `docs/next/persistent-format-hardening.md`'s M-8 (a transient read
failure quarantines the live `workspaces.json`, after which the sidebar persists
an empty configuration) has its defect in cross-cutting
`services/recovery_metadata.rs`, not here — but its *consequence* is this row
writing empty state to disk, so task 7.5 confirms the row's persistence path
cannot be the amplifier and records the verdict either way.

### Inheritances this slot is the named recipient of

Each verified against the archives and the matrix rather than taken on trust:

| Inherited item | Source | What this change owes it |
| --- | --- | --- |
| **11 pre-existing surviving field-deletion mutants in `services/file_tree.rs`** | slot 4 B.2, re-handed by 5a B.2 as "not triaged, go to 5b with the row" | triage per `.agents/rules/build.md` order — missed behavior, then tests, then a small refactor, then a narrow documented exclusion. Baseline, not regressions. The file already carries one narrowly scoped `exclude_re` entry for a symlink match guard: that is the shape, and it must not be widened |
| `cargo-mutants` 27's `--re` does not filter struct-field-deletion mutants | slot 4 B.2, restated by 5a | report the floor in every focused run and do not attribute its pre-existing survivors to this change. This change also **spec-states** the obligation (see Capabilities) |
| **Two relocations, with parity rather than gain**: `model/workspace_scan.rs` (231) and `model/workspace_persistence.rs` (338) | matrix `Policy Module Census`; 5a left both outstanding | relocate with before/after mutation parity from the exact `make mutants-diff` invocation with file-level anchors. Both are already inside `examine_globs`, so parity is a real claim. Consumer sets re-verified by import at authoring: persistence is exactly `ui/sidebar/imp.rs` + `workspaces.rs`; scan is three `ui/sidebar` importers **plus two references to the public `model::workspace_scan` path in `crates/lushtext-core/benches/benchmarks.rs`** (`:57`, `:3198`), which a move breaks |
| **`WorkspaceWatchTicket` remains unreified** | matrix "Seam Value Objects", still `required` | reify `{targets_generation, lifetime_generation}` in the row's existing `seams.rs`, keeping the retire-versus-restart consequences distinguishable |
| **Two production `.imp()` reach-throughs** at `ui/automation.rs:766` and `:927` reading `imp.sidebar.imp().workspace_filter_animation_active` | 5a A.7 and the matrix's reach-through table, left for 5b "because they need that row's evidence surface to project from" | retire both through a named accessor or an evidence projection, with the readiness blocker and the snapshot field keeping identical values. Line numbers confirmed exact at authoring |
| **`window.workspace` remains unprojected** — 10 fields, the row's whole snapshot object, plus three readiness blockers and one predicate | 5a registered only `window.notes`; matrix and `docs/automation-reference.md` confirm no `workspace` row in the Evidence Projection Map | register the object with the drift gate and project every field from the evidence surface without widening |
| **The nested role home has no adopter yet** | 5a B.2 | be its first, and report how it read |
| **`WFR-SHELL-LAYOUT` decisions 5a made for slot 7** | 5a B.2 | `workspace-sidebar-animation` follows the animation, not the row name; `WorkspaceSidebarWidthPreset` moves out of `ui/sidebar/mod.rs` when this facade is written; the `recent_documents.loading` ungated read slot 3b left is still open. Honour all three; absorb none |
| Named operations on migrated facades, to call rather than reach into | slot 4 B.2, 5a B.2 | `migrate_note_sidecars_after_rename`, `show_local_history_for_path`, `resolve_notes_for_editor`, `notes_evidence()`. **These stay calls**; this change must not restructure a migrated row |
| Gate blindness to untracked files | slot 4's friction section, now in `.agents/rules/build.md` | `git add -N` every new file **early**, before the first diff-aware gate, and treat a green diff-aware gate over untracked files as unproven |
| The programme's ledger line for 5b says its artifacts are "the existing slot-5 change" | `docs/next/workflow-readability.md` slot table | that is now false — this is a separate change. Correct the ledger's artifact cell and name this change in the slot/name table |

### A twelfth stage-order candidate the inherited trace does not name

The inherited trace is a decision to verify, not a number to adopt, and
authoring's own read found an ordered stage order it does not name: **workspace
folder add and remove**. It has its own entry point, its own ordered stages, and
its own persistence terminal:
`ui/sidebar/dialogs.rs:71 show_add_folder_dialog` →
`workspaces.rs:305 handle_add_folder_to_workspace` (which resolves folder
identity off the GTK thread) → `:353 apply_add_folder_to_workspace` → persist;
`:397 handle_remove_folder_from_workspace`; and the section-side request route at
`workspace_section/mod.rs:315 connect_remove_folder_requested`.
`.agents/rules/ui.md` names the "add-folder request" explicitly as a section
callback the sidebar handles itself, and `Add Folder` is already in the row's
`Entry points` cell — so the stage order is documented from two directions while
the trace omits it.

Task 0.4 decides whether it is a twelfth stage order or a stage *within* the
workspace-list order, and the decision has three consequences rather than one: it
changes the number this change corrects the `Workflow Stage Traces` floor to, it
changes the facade's narration budget, and it decides whether
`list_execution.rs` owns folder membership or a separate coordination module
does. `apply_add_folder_to_workspace` also takes **six parameters across a
private boundary** and is a seam-rule candidate task 4.4 must weigh.

### Two adjacencies this change resolves rather than inherits

1. **Two pre-convention modules map to no bounded role name, and the spec's only
   stated response is escalation.** `tree_index.rs` (969) is pure index arithmetic
   *plus* child-store lookup and cache maintenance; `watch_targets.rs` (337) is
   pure mirror arithmetic *plus* two generation newtypes *plus* a snapshot. Slot
   5a decided both are **dissolved** across `policy.rs`, `seams.rs`,
   `evidence.rs`, and `scan_execution.rs` — and recorded that dissolving
   `tree_index.rs` is "the honest alternative to inventing an `index` role". The
   spec does not say that is legitimate: it says a coordination job no name
   describes MUST be added by amendment. The missing step is the prior question —
   *is this module one coordination job at all* — and this change states it.
2. **Two mutation reporting obligations exist only as task notes.** The
   unfilterable field-deletion floor and the parity-versus-gain separation have
   been carried as prose in slots 4, 5a, and now here, and this change is the
   one that both triages eleven such survivors and performs two relocations
   alongside new policy extraction. Stating them normatively is cheaper than
   re-deriving them in slot 6.

## What Changes

- **Migrate `WFR-WORKSPACE-TREE` into the nested role home 5a decided**, giving
  `ui/sidebar/mod.rs` a narrative facade and every module in both directories
  exactly one classification: the facade, seam value objects, pure policy,
  evidence, a bounded — stage-order-qualified where the shape repeats —
  coordination role, or a **called presentation surface, which is not a role**.
  Implement the module map as recorded: `list_execution.rs`,
  `persist_execution.rs`, `filter_execution.rs` at the canonical home;
  `watch.rs` (**already correctly named, not renamed for symmetry**),
  `scan_admission.rs`, `scan_execution.rs`, `refresh_execution.rs`,
  `folder_execution.rs`, `file_execution.rs`, `peek_execution.rs`,
  `reorder_execution.rs` nested. `width_preset.rs` leaves as cross-cutting —
  which touches three consumers outside this row (see Impact).
- **Dissolve the three modules that are not one coordination job** across
  existing roles rather than adding a role name for any of their topics:
  `tree_index.rs` (969) and `watch_targets.rs` (337) as 5a decided, **plus
  `tree_loading.rs` (1,269) — the row's largest file, which 5a's module map
  never classified.** Authoring read it: it holds the process-global scan
  admission permit, limit, and high-water counters (`:73-104`) and the admission
  retry (`:419-478`) → `scan_admission.rs`; the child scan worker, child-store
  identity/mirror/splice, batched reconciliation, directory-state clearing, and
  the deferred expansion restore → `scan_execution.rs`; and the drag-hover empty
  child model with its two seams (`:130`, `:157`, `:163`) → `reorder_execution.rs`,
  which is consistent with 5a's own stage-trace reconciliation reassigning
  `tree_loading.rs:143` from the scan order to the DnD shield. Three dissolutions
  rather than two **strengthens** this change's amendment basis: the pattern is
  not a one-off. Update `ui/sidebar/AGENTS.md` in the same breath so its
  Responsibilities, Local Contracts, and Editing Rules describe the migrated
  shape.
- **Extend the existing `policy.rs`** with the decisions still inline in the GTK
  adapters — refresh coalescing and the full-versus-directories verdict, the
  desired-versus-current top-level diff that drives the splice window, the
  readiness predicate, the expansion transition rule with its ambiguity fallback,
  the persistence error-to-message mapping and terminal-effect routing, the
  auto-expand-versus-remembered-intent decision, DnD post-drop index and hover
  verdict, peek formatters, icon selection, the context-menu key predicate and
  header hit test, and the row accessible-description builder — pinning the caps
  this row owns to concrete literals with the user-facing reason beside them.
- **Reify `WorkspaceWatchTicket`** in the row's `seams.rs` and **re-audit** the
  already-reified `WorkspaceScanTicket` and the dissolved watch-target generation
  newtypes against the two-boundary rule.
- **Build one `evidence.rs`** folding in every pre-convention typed observation
  (`WorkspaceScanPressureEvidence`, `WorkspaceWatchMailboxSnapshot`,
  `WatchTargetSnapshot`, `SidebarFileRowStateSnapshot`, the refresh and
  reconciliation metrics, the child-cache rebuild metrics, the scan admission
  counters, the expansion capture metrics, and the DnD hover fallback count),
  discharging the three standing proofs plus **no-materialization** and
  **child-collection** honesty.
- **Retire the programme's largest seam population**: 60 functions across 111
  gate sites. Inspection functions retire into the surface with no remaining
  callers, including the destructive-read "take touched rows" seam whose reset
  must be separated from its observation; configuration seams collapse into the
  existing `test_policy.rs`, including the eight test-only **fields inside
  `RefreshRuntimeState` and `WatchRuntimeState`** that no `static` grep finds and
  `WatchRuntimeState`'s permanent restart-suppression flag; actuation seams are
  classified as programme-level deferrals; oracles and probes are preserved with
  their reason. **Exactly one** new seam is budgeted — the load-worker delay M-4's
  race test needs — and it is counted and justified individually.
- **Relocate the two workspace policy modules with mutation parity**, handling
  the bench path break explicitly (update the bench imports or keep a precisely
  scoped `pub` subset, per slot 3a's `save_admission` precedent), and confirm
  `model/workspace.rs` stays as domain.
- **Triage the 11 inherited `services/file_tree.rs` field-deletion survivors** in
  the documented order, reporting the unfilterable focused-run floor.
- **Project `window.workspace` from evidence without widening the contract** —
  ten fields, the `workspace-persist`, `workspace-tree-refresh`, and
  `workspace-filter-animation` blockers, and the `workspace-refresh-complete`
  predicate — registering the object with the drift gate, and retiring the two
  production `ui/automation.rs` reach-throughs. Honour 5a's decision that
  `workspace-sidebar-animation` is `WFR-SHELL-LAYOUT`'s.
- **Advance the matrix and the programme record in the same change**: a
  `Migrated Workflow Roles` subsection, corrected `Current size`, `Entry points`,
  `Seams`, `Seam value object`, `Evidence surface`, `Owned pure policy`, and
  `Status` cells naming their pooled populations, the `Workflow Stage Traces`
  floor correction, the `Policy Module Census` rows for the relocations, the
  reach-through table rows closed, the slot-5b ledger line flipped to complete
  with `WFR-AUTOMATION-SPINE (partial)` carried onto slot 6, a "Baseline after
  slot 5b" table, and a "Convention friction slot 5b hit" section. **Evidence
  pointers in live `openspec/changes/<name>/evidence/...` form**; an
  archive-prefixed pointer fails the gate immediately.

**Explicit non-goals.** No change to workspace persistence format or its debounce
window, watcher backend or its mailbox caps, scan slicing or placeholder
behavior, expansion-state semantics, DnD reorder rules, peek behavior, inline
rename or file-operation semantics, note or bookmark handling, any user-visible
string, or the exported D-Bus contract. **No restructuring of a migrated row**:
`WFR-NOTES-BOOKMARKS`, `WFR-COMMAND-PALETTE`, `WFR-LOCAL-HISTORY`,
`WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`, `WFR-DOCUMENT-SAVE`,
`WFR-DOCUMENT-LOAD`, `WFR-BUFFER-REPLACEMENT`, and `WFR-SEARCH-REPLACE` are
called, not rebuilt. `WFR-SHELL-LAYOUT` (slot 7) keeps the sidebar show/hide
animation, its readiness blocker, and the recent-documents surface;
`WFR-MINIMAP` (slot 6) keeps its four `ui/automation.rs` reach-throughs.
`ui/window/startup_data.rs` stays **cross-cutting**, the ownership 5a decided —
this change does not absorb it because it calls `sidebar.load_workspaces()`.
`services/file_tree.rs`, `workspace_manager.rs`, `workspace_watch.rs`,
`file_peek.rs`, `migration_ledger.rs`, `single_flight.rs`, `model/workspace.rs`,
and `ui/sidebar/file_tree_item.rs` keep their behavior and their homes. No
workflow is reified as an explicit state machine, and no programme-level deferred
actuation seam is retired.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gtk-adapter-module-boundaries`: **a pre-convention module that no bounded role
  name describes is first tested for whether it is one coordination job at all,
  and dissolved across existing roles when it is not.** The role requirement
  today offers exactly one response to a module that fits no listed name —
  "MUST be added to the bounded set by amending this specification" — which is
  correct for a genuinely novel *job* and wrong for a module that is simply not
  one job. This row hits the gap twice: `tree_index.rs` mixes pure index
  arithmetic with child-store cache maintenance, and `watch_targets.rs` mixes
  pure mirror arithmetic with two generation newtypes and a snapshot. Naming
  either would add a role for a pre-convention *topic*, which is what the closed
  taxonomy exists to prevent. The amendment states the prior question and its
  answer: determine cohesion first, dissolve across existing roles when the
  module is not one job, recording each part's destination in the workflow's
  matrix row, and reserve escalation for a genuinely novel single job. **It adds
  no role name and does not widen the bounded coordination set** — it narrows the
  circumstances in which widening is the right move. It also states the scope of
  the stage-order qualification rule that slot 2b read narrowly and every slot
  since has followed: qualification applies to modules a migration creates or
  renames, and a module already carrying a correct bounded role name is not
  renamed for symmetry.

- `mutation-testing`: **a focused mutation run reports the mutant classes its
  filter cannot exclude, and a change that both relocates and extracts pure
  policy reports parity separately from gain.** Both statements describe
  obligations the project has been carrying as per-change prose since slot 4, and
  both are load-bearing in exactly this change. `cargo-mutants` 27's `--re`
  filter does not apply to struct-field-deletion mutants, so a "focused" run of a
  handful of policy mutants silently also runs every field-deletion mutant in
  scope; a change that does not state that floor either appears to have run more
  than it did or inherits eleven pre-existing survivors as if they were its own.
  Separately, a change that relocates existing policy (where a before-count
  exists and parity is a real claim) *and* extracts new policy out of a GTK
  adapter (where no before-count exists and the result is gain from zero) can
  otherwise report one aggregate number in which a parity loss is invisible
  behind a gain. The amendment requires the floor to be stated, forbids
  attributing its pre-existing survivors to the change under review while
  requiring the owning change to triage them in the documented order, and
  requires the two result kinds to be reported separately.

  **Retroactive-amendment obligation.** Under
  `workflow-readability-boundaries`' "Convention amendments are applied
  retroactively", these statements trigger a re-check of the **nine** migrated
  rows (`WFR-SEARCH-REPLACE`, `WFR-COMMAND-PALETTE`, `WFR-DOCUMENT-SAVE`,
  `WFR-DOCUMENT-LOAD`, `WFR-BUFFER-REPLACEMENT`, `WFR-SESSION-RESTORE`,
  `WFR-LOCAL-HISTORY`, `WFR-DRAFT-RECOVERY`, `WFR-NOTES-BOOKMARKS`) for a
  coordination module that is not one cohesive job or whose name states a
  pre-convention topic, plus a re-check of the mutation evidence recorded by the
  changes that relocated policy or ran focused mutation (slots 3a, 4, 5a) for a
  missing floor statement or a merged parity/gain figure. **These are not
  expected to be pure confirmations**: slot 3b, slot 4, and slot 5a all found
  genuine non-compliance — 5a's statement (b) found **six of eight** rows
  recording widget-projection modules under an undefined label — and the
  not-a-confirmation streak now stands at three consecutive amendments. Any gap
  is filled **in this change**, in the matrix and the programme record; an
  archived change's evidence file is history and is **not** rewritten. Nothing
  beyond these statements may be absorbed: the facade line budget, the five role
  names, the bounded coordination set, the seam value-object shape, and the
  evidence-surface visibility and materialization rules are **not** amended.

Note that `openspec validate --strict` fails any change with no `specs/` delta
("Change must have at least one delta"), which is why every migration slot
carries one.

## Impact

**Prerequisites**

Slots 1, 2a, 2b, 3a, 3b, 4, and 5a are archived under
`openspec/changes/archive/2026-08-25-*` through `2026-08-27-*`. The blocking gate
is **slot 5a complete with `WFR-NOTES-BOOKMARKS` migrated,
`WFR-WORKSPACE-TREE` still `pending`, the ledger marking 5b outstanding, and
`make check-workflow-boundaries` passing** — verified mechanically in task 0.1.
The deliverables this change consumes are the declared facade budget (2a), the
stage-order qualification rule (2a), the evidence-to-snapshot drift check and its
per-binding attribution (2a, extended by 3b and 4), the `journal` role name (2b),
the per-workflow subdirectory role home (3a), the evidence-surface reentrancy
constraint (3b), slot 4's row-cell re-derivation obligation and its `seams.rs`
module precedent, and slot 5a's **nested role home**, **called presentation
surface** taxonomy scope, and **no-materialization / child-collection** evidence
statements — of which this row is the first adopter of the first and the primary
test of the last.

**Code touched** (sizes measured at authoring as **upper bounds**; task 0.3
re-derives every one row-scoped and non-`#[cfg(test)]`)

- `crates/lushtext-core/src/ui/sidebar/**` — 23 row files, **11,741 production
  lines** (**raw** totals in parentheses where they differ, because a raw figure
  is the size of a file being moved wholesale while a production figure is the
  row's size): the canonical home (`mod.rs` 415, `workspaces.rs` 864, `imp.rs`
  246, `callbacks.rs` 219, `dialogs.rs` 205, plus 5a's `policy.rs` 134 (raw 261),
  `seams.rs` 92 (raw 133), `test_policy.rs` 88) and `workspace_section/**`
  (`tree_loading.rs` 1,269, `tree_index.rs` 844 (raw 969), `folders.rs` 835,
  `context_menus.rs` 809, `dnd.rs` 769, `mod.rs` 744, `peek.rs` 728,
  `actions.rs` 707, `refresh.rs` 666 (raw 702), `watch.rs` 593 (raw 606),
  `imp.rs` 508, `row_factory.rs` 463, `watch_targets.rs` 264 (raw 337),
  `row_accessibility.rs` 199, `icon_presentation.rs` 80 (raw 155)).
  **`file_tree_item.rs` (150) is not this row's to count** — confirm rather than
  assume. Note it nonetheless owns `pending_rename` (`:35`, `:131`, `:135`), the
  one-shot flag the inline-rename stage order sets in `actions.rs:75` and clears
  in `row_factory.rs:310`, so the row's stage trace crosses it even though its
  lines are not the row's.
- `crates/lushtext-core/src/ui/preferences/imp.rs` (4 references),
  `ui/window/adaptive_shell.rs` (12), and `ui/window/imp.rs` (6) — the three
  consumers of `WorkspaceSidebarWidthPreset` outside this row, all of which the
  cross-cutting move to `ui/sidebar/width_preset.rs` re-points. None is
  restructured; the move is a path change proved by compilation, and
  `adaptive_shell.rs` is `WFR-SHELL-LAYOUT`'s file, so the touch must stay a
  path edit.
- `crates/lushtext-core/src/services/action_catalog/mod.rs` — **15 rows carry the
  owner string `"sidebar/workspace_section"`** (`:1563`–`:1765`), which
  `scripts/check-automation-docs.py` parses at `:252` and renders into
  `docs/automation-reference.md` at `:282`. The module renames in this change
  make those owner strings stale, and the drift gate is what will say so.
- `crates/lushtext-core/src/ui/sidebar/AGENTS.md` — the nearest ownership
  guidance, whose Local Contracts state the scan-flight, watcher-mirror,
  mailbox-cap, DnD-shield, and row-factory ownership rules this change must
  preserve and re-point after every move.
- `crates/lushtext-core/src/model/workspace_scan.rs` (231) and
  `workspace_persistence.rs` (338) — the two relocations, raw file totals
  including co-located tests **deliberately**, because the tests move with the
  module; task 0.3's production-only re-derivation applies to the row size cell,
  not to these.
- `crates/lushtext-core/src/model/workspace.rs` (799) — **expected unchanged**,
  recorded as domain and staying. Confirm, and name any `services` consumer.
- `crates/lushtext-core/benches/benchmarks.rs` — two references to the public
  `model::workspace_scan` path (`:57`, `:3198`) that the scan relocation breaks,
  **plus `:95`, which imports this row's ungated `pub` bench seam**
  `workspace_section::child_cache_rebuild_operation_evidence_for_benchmark`
  (`workspace_section/mod.rs:608`). That seam and
  `services/workspace_watch.rs:267 merge_backend_result_for_benchmark` sit
  outside the 60/111 seam census because neither is gated behind `test-utils`;
  the evidence-surface narrowing in task 6.1 would break the bench target
  without an explicit disposition for both.
- `crates/lushtext-core/src/services/file_tree.rs` (603 production of 1,050) —
  **behavior unchanged**, and the owner of the 11 inherited survivors.
- `crates/lushtext-core/src/services/workspace_manager.rs` (1,329),
  `workspace_watch.rs` (1,030), `file_peek.rs` (489) — behavior unchanged; in
  scope for seam classification only. A `services -> ui` relocation is forbidden.
- `crates/lushtext-core/src/ui/automation.rs` — `window.workspace`, the three
  workspace readiness blockers, and the two production `imp.sidebar.imp()` reads
  at `:766` and `:927` (line numbers confirmed at authoring).
- `crates/lushtext-core/src/ui/window/{imp.rs,documents.rs,actions.rs,dialogs.rs,mod.rs}`
  — called surfaces, module declarations, the scope-consumer refresh, the
  close-time persistence flush route at `dialogs.rs:715`, and the
  `show_local_history_for_path` route. Called, not restructured.
- `crates/lushtext/tests/widget/{sidebar,workspace_section,file_tree_item,window,accessibility}.rs`
  — `workspace_section.rs` alone is **6,135 lines**; the **45 in-scope tree-side
  runtime `.imp().` reads** 5a catalogued move here, with the `TemplateChild`
  handle population recorded as out of scope.
- `scripts/check-automation-docs.py` — `window.workspace` must be registered as a
  new projecting object in the Evidence Projection Map, which today holds rows
  for `window.content_search`, `window.command_palette`, `window.tabs`,
  `window.local_history`, and `window.notes`. The same script's action-catalog
  owner rendering is what surfaces the stale `"sidebar/workspace_section"`
  strings above.
- `docs/workflow-readability-matrix.md` also carries **five dangling evidence
  pointers** (`:94`, `:500`, `:2056`, `:2057`, `:2500`) still in slot 5a's *live*
  form, which no longer resolve now that 5a is archived. The matrix's own rule at
  `:2189` says an archived change's pointers are rewritten to archive form; 5a's
  archiving missed these. They are in a file this change rewrites anyway.
- `scripts/accessibility_warning_allowlist.py` — it keys on module paths, and 5a
  verified its only key is `editor_page::load::execution`, so **no coupling
  exists at authoring**; this stays a conditional confirmation. If a coupling has
  appeared, update it and re-verify it still **rejects** both an unrelated path
  and the stale module name.
- `.cargo/mutants.toml`, `docs/workflow-readability-matrix.md`,
  `docs/next/workflow-readability.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `docs/accessibility-matrix.md`,
  `docs/end-user-coverage.md`, `AGENTS.md`, `README.md`, and any
  `.agents/rules/*.md` or `.agents/skills/**` reference naming a moved path or a
  retired seam — `.agents/rules/ui.md`'s File Tree and Multi-Workspace Sidebar
  sections name several of these modules **by path**.

**Verification**

Everything slot 5a ran, re-aimed at the sidebar, and **headless throughout**.
Behavior equivalence across: a workspace with zero, one, and many folders; an
empty workspace preserved as a real section; a deep tree with long paths and the
no-horizontal-scrollbar contract; expand and collapse, and **a user collapse
racing a deferred restore callback** that must not be resurrected; a directory
scan superseded by a newer one and one whose section is gone when it resumes; a
scan refused by admission and retried; a watcher install superseded during its
worker in **both** its retire and its restart consequence; a watcher whose start
fails terminally, settling readiness as unavailable rather than pending forever;
a mailbox overflow promoting targeted paths to one full refresh; a targeted
in-place refresh after create, rename, and delete including a directory rename
matched by prefix against open tabs; a pending full refresh dominating queued
targeted paths; folder-reorder DnD including an invalid drop position and a hover
that must not expand a folder, materialize descendants, or restart a watch;
`Space` peek including a stale request, a changed path, and **reached by keyboard
focus on a realized row** rather than on the list view; a **double-click that
opens a file row while a double-click on a directory row expands it** — the
`GtkTreeExpander` internal-gesture contract `.agents/rules/ui.md` records as a
three-iteration lesson; inline rename including empty, unchanged, duplicate, and
the focus-out double-fire guard; create with a colliding name; delete
confirmation and cancellation; workspace add, rename, and unlist; a persistence
write that fails and is retried, one superseded by a newer generation, **a load
whose adoption is superseded by a live mutation (M-4, driven)**, and a close-time
flush whose failure must abort close; a workspace scope filter change superseded
before its settle timer fires with `filter_animation_active` settling exactly
once; and entering and leaving focused-folder mode including the four DnD gates
that read drilldown emptiness and a focused folder that disappears from disk.

Plus: `make check` and `make check-policy` including
`check-workflow-boundaries`, `check-automation-docs`,
`check-accessibility-policy`, `check-visual-proof-policy`, and
`check-filesystem-boundary` — the last because this row mutates the user's own
files; **the rustdoc lint gate, which is CI-only and in neither `make check` nor
`make pre-commit` nor `make check-policy`**, and which a **nested** role home
makes more tempting to fail; `make test`, `make test-widget-headless`, and the
focused `make test-workspace-row-states` with **zero `FLAKY:` lines and no retry
relied upon**, and a test count that does not decrease; `make mutants-diff` with
relocation parity **and** gain-from-zero reported separately plus the
`services/file_tree.rs` survivor triage and the field-deletion floor;
`make performance-smoke`; `make test-prop` if any property target is touched; a
`data-safety` pass in explicit mode **before and after** the diff; and the
mandatory proof lanes (`visual-geometry-smoke`, `accessibility-smoke`,
`visual-smoke`) each from a **clean artifact root** and ordered **after every
source, documentation, and rules edit**, because the accessibility policy gate
fingerprints the contents of relevant files.

**Display-dependent proof is deferred for user availability from the start, not
discovered late.** Slot 4 established that isolating an app's state does not
isolate its window: a real Wayland launch maps a surface and takes focus
regardless of `XDG_*` isolation, and it interrupted the user's session. This
change therefore plans **no live launch on its critical path** — which matters
more here than anywhere, because `.agents/rules/widget-wiring.md` names the
sidebar explicitly as the subtree needing a real `make run` cycle with restored
workspaces while watching stderr. That walkthrough is recorded as a `[~]`
deferred item with its exact remaining scope, to be scheduled with the user, and
widget-green is therefore **necessary and not sufficient** for this row's
acceptance. Everything else has a headless path. Acceptance is that tree
contents, expansion state, watcher reconciliation, file-operation outcomes,
persistence, error surfaces, timing characteristics, and the exported D-Bus
contract behave identically to the pre-migration workflow.
