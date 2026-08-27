## Why

This is **slot 4** of the workflow-readability programme recorded in
`docs/next/workflow-readability.md`, the **user-content restore family**: the four
workflows that decide what the user sees after a crash, a restart, a mistaken
edit, or an undo. It migrates `WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`,
`WFR-LOCAL-HISTORY`, and `WFR-BUFFER-REPLACEMENT`, and carries
`WFR-AUTOMATION-SPINE` forward incrementally as every slot since 2a has.

**Prerequisite, mechanically checkable: slot 3 must be complete in both halves.**
Three of this slot's four rows are `tier-3`, and the convention requires at least
two completed lower-risk migrations before a tier-3 row is migrated. Slots 1, 2a,
2b, 3a, and 3b are archived, so the gate is satisfied five times over — but it is
confirmed by reading the matrix and the ledger, not the proposal. See task 0.1.

**Why this family, and why together.** The record assigns the four rows to one
slot because they are one story told in four places:

- `WFR-DRAFT-RECOVERY` keeps the durable record that protects an unsaved buffer.
  Slot 3a checked the `journal` role name against document save, **rejected it**,
  and said explicitly that the record protecting an unsaved buffer is the draft
  and "slot 4 is where `journal` will genuinely fit".
- `WFR-SESSION-RESTORE` decides which documents come back and in what order. The
  matrix calls it "the closest workflow in the tree to the target shape" — one
  inversion, already expressed as an explicit bounded-turn policy.
- `WFR-LOCAL-HISTORY` keeps per-document sidecar snapshots and restores from them.
- `WFR-BUFFER-REPLACEMENT` is the **shared installation mechanism** the restore
  paths drive their bytes through. It is the reason the census put local history
  and buffer replacement in this slot rather than leaving them unslotted (census
  finding 7).

Migrating draft restore or local-history restore without first fixing the
boundary they share with buffer replacement would mean deciding that boundary
twice, from two sides, with the second decision constrained by the first.

**Its caller set is wider than the matrix records, and two callers are outside
this slot.** `BufferReplacementWorkflow`'s own variants
(`ui/editor_page/buffer_replacement.rs`) name the truth:
`MemoryEviction`, `DraftRecovery`, `LocalHistoryRestore`, `LocalHistoryUndo`,
`SaveFormatting`. That is **five call sites across four owning workflows** —
`ui/window/drafts.rs` (`WFR-DRAFT-RECOVERY`), `ui/window/local_history.rs` twice
(`WFR-LOCAL-HISTORY` restore and undo), `ui/editor_page/mod.rs` (eviction,
`WFR-EDITOR-MEMORY`, **exempt with no slot**), and
`ui/editor_page/save/execution.rs` (save formatting, `WFR-DOCUMENT-SAVE`,
**already migrated, with a budget-constrained 223-line facade**). **Replace All
undo is not a caller**: `LocalHistoryUndo` is the local-history restore undo, and
the matrix's `Policy Module Census` entry for `buffer_replacement.rs` currently
reads `2 (WFR-LOCAL-HISTORY, Replace All undo)`, which is wrong in both halves.
Direct consumers of the pure module extend further still, into
`model/file_load.rs` and `ui/editor_page/load/policy.rs`
(`WFR-DOCUMENT-LOAD`, migrated). This matters beyond bookkeeping: this row is the
**first** to exercise the new "name the other owning workflows" sentence, so it
must not name a wrong set, and two of its callers are workflows this change may
call but must not restructure.

**Why it is the highest-consequence slot so far.** Save replaces one file's bytes
and load installs one file's bytes. This family decides, after a crash, *whether
the user's unsaved work exists at all* — and its cleanup path deletes user content
by design. `.agents/rules/rust.md` records the contract that makes orphan-body
cleanup safe: inspection records the candidate inode, execution reloads the latest
trusted manifest, acquires the same stable `TargetWriteGuard` used by atomic
replacement, **then rechecks inode before deleting**, because manifest
serialization alone is insufficient — an autosave may finish replacing the body
before it acquires the manifest lock. That contract, the paragraph-boundary
contract in bounded installation, and every draft/session/sidecar durability
ordering are **behavior this change must preserve exactly**.

**Behavior preservation is the default; every deviation is carved out
explicitly.** The only sanctioned deviation is a confirmed data-safety defect,
which `.agents/rules/preexisting-blockers.md` makes non-negotiable to fix in the
same work stream — and slot 2b's two confirmed findings plus slot 3b's one prove
that a readability slot over a durable path is exactly when such defects surface,
because it is the first time anyone reads that path end to end. **Do not treat
the non-goal as permission to defer one.** Two candidates are already routed here
with their missing evidence named; see below.

### Two pre-existing candidates 3b routed to this slot

Both are recorded in slot 3b's `tasks.md` appendix A.9 as UNRESOLVED, with the
missing evidence stated rather than guessed. This change owns the files that hold
the evidence, so it owns the investigations. Neither is assumed to be a defect and
neither is assumed benign.

1. **`installation_incomplete` is invisible to the draft-autosave lane.** Every
   guard in `ui/window/drafts.rs` tests only `is_modified()` / `draft_dirty()` /
   `is_evicted()`. After a cancelled load empties a buffer, the cancelled-clear
   terminal sets `set_modified(false)`, so autosave skips — but one subsequent
   keystroke makes it modified again and the next batch would write the near-empty
   buffer over the draft. **Missing evidence: the `draft_dirty` transition trace in
   `ui/window/drafts.rs`** — specifically whether a draft can hold unsaved edits
   the file does not at the moment a cancelled installation leaves a partial
   buffer. 3b could not produce it from its scoped files; this change's files are
   exactly where it lives. Slot 3a's save path already refuses on this flag
   (`IncompleteLoadInstallation`); the question is whether the autosave lane needs
   the same guard.
2. **The planning completion's dead-editor early return leaves a stored terminal
   unfired**, relying on GTK dispose to reach `dispose_load_resources`. **Missing
   evidence: proof that no path skips dispose.** 3b found none but did not prove
   it. Worst case is a stalled session-restore sequencer — **never
   over-admission or lost content**, and over-admission is the property
   `release_permit` exists to protect, so state both — and
   **this change owns that sequencer**, so it can decide whether an unfired
   terminal is observable and, if so, whether to release it explicitly rather than
   depend on dispose ordering.

3b also handed over a contract this slot must not regress: **every load terminal
now either carries a parked request's background planning owner into a restart or
releases it, and no path drops it**, because `SessionRestorePolicy::release_permit`
counts exactly those releases to decide when to open the next document.

### The census cells are expected to be wrong, and re-verification is budgeted

Four consecutive slots found their row's `Current size` and `Seams (i/c/a/p)` cells
wrong, **in both directions**, always for the same reason: the cells pool shared
service files and called neighbour files that the row does not own. The two
corrections nearest this slot are recorded in matrix rows 88–89 (save and load),
and 3b's handoff states plainly that this row's pre-migration seam cell pooled the
six load-side `test-utils` overrides in `services/editor_io.rs` with save's **and
with drafts'**.

This slot's four cells show the same shape: `WFR-DRAFT-RECOVERY` is sized
`6 files, 8,930 lines (ui 2,578 / model 442 / services 5,910)` where the services
subtotal is dominated by `services/draft_service.rs`, most of which is co-located
tests; `WFR-LOCAL-HISTORY`'s `services 2,777` is the whole of
`services/local_history_service.rs` on the same basis. **Task 0.3 re-derives all
four rows row-scoped and non-`#[cfg(test)]`, and expects the answers to move in
either direction.** Four rows means four corrections, which is why premise
re-verification is a numbered task per row rather than a footnote.

### Facade budget: the position this change takes

**No amendment is proposed, and the budget line is not to be edited.** Two data
points now agree that **stage-order count is what stresses the 370-line budget** —
not inversions, not entry points, not risk tier: save narrates one stage order in
223, load narrates one with seven inversions and seven entry points in 253, while
the exemplar's two stage orders sit at 369 with one line spare.

This change writes **four** facades, each measured independently against the whole
370. The exposure is per facade:

- `WFR-DRAFT-RECOVERY` is the honest risk. It owns **two** stage orders (autosave
  and restore) plus orphan cleanup, and the matrix records **seven distinct worker
  handoffs, the highest inversion count of any workflow**. If any facade in this
  programme proves the number wrong before slot 6, it is this one.
- `WFR-SESSION-RESTORE` (one inversion), `WFR-LOCAL-HISTORY` (six, all already
  ticket-guarded across two stage orders), and `WFR-BUFFER-REPLACEMENT` (one,
  already reified) should fit comfortably.
- The change must also not add a physical line to `ui/search_panel/mod.rs` at
  369/370, nor push the save or load facades over.

If the draft facade cannot fit after delegating stage bodies, compressing
inversion bullets, and folding module-ownership detail into the role table — the
exact sequence that brought slot 2b back from 379 to 369 — **escalate in-change
with the measured count**. Raising the number now costs re-checking **four**
migrated rows, and doing it in slot 5 or later costs eight. Make the case
explicitly or make the narration fit. Do neither by editing the line quietly.

## What Changes

Task ordering inside the change is by increasing risk, mirroring the programme's
own sequencing: **buffer replacement, then session restore, then local history,
then draft recovery.** Buffer replacement first because the other two slot-4
restore paths call it and two out-of-slot workflows do too, so its boundary must
be settled before anything is built on it; draft recovery last because it is the
largest, has the most inversions, and owns the cleanup path that deletes user
content.

- **Migrate `WFR-BUFFER-REPLACEMENT` first, and decide the shared-arithmetic
  home.** Its pure policy is expected to be cross-cutting and to **stay**: the
  matrix records `model/buffer_replacement.rs` as cross-cutting on the same
  grounds as `plain_disposal`, and the corrected caller set above is five call
  sites across four owning workflows. That would make this the programme's
  **first migrated row whose `policy` role is legitimately `none`** — a case the
  gate permits but no spec scenario states, which is one of this change's two
  spec deltas. **That outcome is a finding, not a premise**: the delta's own
  scenario requires the workflow's pure logic to be *entirely* cross-cutting, so
  the change probes the GTK adapter's candidate pure decisions — slice
  accounting, terminal classification, supersession — and either extracts them or
  records the negative finding with its evidence. Alongside that, the change
  settles
  the `next_install_boundary` question 3b's matrix row left open: the arithmetic
  is one function in `model/buffer_replacement.rs` and `model/file_load.rs`
  currently re-exports it under a load-flavoured name. Decide explicitly whether
  the alias stays as a named domain synonym or callers reach the cross-cutting
  owner directly. **Do not duplicate it**, which is what the row forbids.

- **Migrate `WFR-SESSION-RESTORE`**, whose bounded-turn policy is already the
  target shape, and whose `SessionRestoreEvidence` is already the tree's only
  canonical `evidence()` accessor. The work is completing the row rather than
  inventing it: give the workflow a facade, fold
  `SessionRestoreRuntimeSnapshot` into the surface rather than leaving a second
  typed path, and resolve the stored-evidence oddity — the window's imp holds
  `Cell<Option<SessionRestoreEvidence>>`, which stores evidence as state instead of
  deriving it. Decide whether that cell is a *last-restore outcome record* (a
  legitimate workflow field the surface projects) or a cached surface (which the
  convention forbids). **Take ownership of `session.save_failed`**, which slot 3a
  routed here after finding it is *session-file* save failure owned by
  `ui/window/session_persistence.rs`, not document-save state.

- **Migrate `WFR-LOCAL-HISTORY`**, which spans **two** directories —
  `ui/window/local_history.rs` (browser, preview, restore) and
  `ui/editor_page/local_history.rs` (baseline and periodic capture). The fixed
  role names are one per workflow, so this workflow cannot own a `policy.rs` and
  an `evidence.rs` in both. Resolve it with the split slot 3b already used for the
  recent-documents surface — **the coordination/presentation line** — giving the
  workflow one canonical role home for its coordination, policy, and evidence, and
  leaving the other directory's file as a called surface whose ownership is
  recorded. If that split cannot be made honestly, escalate rather than shipping
  two `policy.rs` files for one row.

- **Migrate `WFR-DRAFT-RECOVERY` last**, and **use the `journal` role name slot
  3a reserved for it**. The test is slot 3a's and 3b re-applied it: *does a later
  stage of the same workflow restore from the record*, not *does it touch the
  disk*. The draft manifest and bodies pass it outright — startup recovery reads
  them back — and slot 2b's definition already places the mutation-serialization
  gate and its byte reservation inside the journal rather than in a separate
  `admission`. Check `journal` first per the handoff, and record the mapping for
  the autosave stage order, the restore stage order, and orphan cleanup
  separately.

- **Reify the seams that are not yet reified, and reuse the ones that are.** All
  four rows already carry seam value objects (`DraftRestoreTicket` +
  `DraftRestoreFacts`, `DraftMutationIntent`, `DraftCleanupContinuation`,
  `SessionRestorePlanPermit` + `SessionRestoreAdmission`,
  `BaselineCaptureTicket`/`PeriodicCaptureTicket` + their `*Facts`,
  `LocalHistoryReplacementTicket`, `BufferReplacementTicket` +
  `BufferReplacementSession`), so this slot's seam work is **auditing them against
  the two-boundary rule and reifying what the audit finds**, not inventing a
  parallel shape. Report **seams reified** as the primary unit, with long
  signatures shortened as the secondary figure the programme record demands.

- **Extract pure policy per workflow with mutation evidence, distinguishing gain
  from parity.** `model/draft.rs` (9 consumers), `model/session.rs` (8),
  `model/local_history.rs` (6), and `model/buffer_replacement.rs` (cross-cutting)
  are all recorded as domain and staying, so this slot is expected to be
  **gain-from-zero extraction out of GTK adapters** rather than relocation — the
  palette, search-panel, and load pattern. Any relocation that does happen owes
  before/after parity numbers from the exact `make mutants-diff` invocation, with
  file-level anchors and never line-precise ones.

- **Build one evidence surface per workflow**, folding in the four pre-convention
  typed observations the matrix names (`OrphanCleanupRuntimeSnapshot`,
  `SessionRestoreRuntimeSnapshot`, `LocalHistoryPreviewCoordinatorSnapshot`,
  `LocalHistoryPreviewInstallSnapshot`) rather than leaving second paths. Each
  surface owes the three proof obligations now in stated convention: the
  **tight-borrow** discipline (compute derived scalars and drop each borrow before
  the struct literal), the **disposed-widget** rule (any field derived from a
  `TemplateChild` reads through `try_get()` and answers honestly for a disposed
  page — a disposed widget is a stage), and the **reentrancy proof test** that
  drives the workflow through each operation taking a mutable borrow, reads the
  surface *after* each one, and asserts repeated reads of unchanged state are
  identical. **Four surfaces means four proof tests**, and 3b's experience says
  not to accept "it must already hold".

- **Migrate the widget-test reach-through this slot inherited.** Slot 3a
  catalogued the ungated `.imp().drafts.*` / `.imp().session.*` sites in
  `crates/lushtext/tests/widget/window.rs` with per-site line numbers and reported
  40. **The current tree holds 35 sites: 21 `.imp().drafts.` and 14
  `.imp().session.`** — the session half is 15 field occurrences across 14 lines,
  because one line reads two fields. 3a's own enumerated list cannot reach 40
  either, so the figure is corrected rather than inherited, and task 6.12's delta
  baseline uses 35. Ungated reach-through appears in no seam
  census yet shapes production field layout. Follow slot 3a's finding: **an
  ungated `imp()` write is usually a real drive in disguise** — reach for an
  existing configuration seam plus a real drive before adding a counted actuation
  seam. Reads become evidence reads. Every added seam is counted and justified
  individually.

- **Project automation from evidence without widening the contract.** The exported
  surface this family owns is the `local_history` snapshot object
  (`browse_available`, `automatic_capture_available`, `availability`,
  `active_document_file_backed`), `tabs[].draft_present`, and the
  `draft-autosave` and `session-restore` readiness blockers with the
  `session-restore-complete` and `recovery-restore-complete` predicates. Those
  keep their names, types, and semantics and start projecting from evidence, with
  new `Evidence Projection Map` rows in `docs/automation-reference.md`. **The drift
  gate itself may need to grow again**: 3b taught it to attribute a projected
  `tabs[]` field by the binding it is read through because two surfaces projected
  into one snapshot object; `tabs[].draft_present` makes that **three**. Prove
  no-widening by capture and diff, not assertion.

- **Advance the matrix and the programme record in the same change.** Four
  `Migrated Workflow Roles` subsections, four corrected `Current size` and
  `Seams` cells, `Seam Value Objects` and `Workflow Stage Traces` updates, the
  slot-4 ledger line flipped to complete with `WFR-AUTOMATION-SPINE (partial)`
  carried onto slot 5's outstanding line, a "Baseline after slot 4" table, and a
  "Convention friction slot 4 hit" section for slots 5 through 7. **Evidence
  pointers are recorded in live `openspec/changes/<name>/evidence/...` form**; an
  archive-prefixed pointer fails the gate immediately because the archive
  directory does not exist yet.

**Explicit non-goals.** No change to autosave timing or its first-dirty delay, the
draft manifest or body format, draft eviction policy, orphan-cleanup deletion
semantics beyond the guard candidate above, session file format or restore
ordering, local-history sidecar format or retention, snapshot capture cadence,
bounded install/clear slice budgets or the paragraph-boundary contract, error
copy, any user-visible string, or the exported D-Bus contract. **No
`WFR-NOTES-BOOKMARKS` work**: `NoteSourceRefreshCoordinator` and the notes
surfaces are slot 5's and must not be pulled in. **The restore-position group
(`ui/editor_page/restore_position.rs`) does not move** — that decision is closed:
it is cross-cutting with five owning workflows, this slot's session-restore row is
one of them, and it is called rather than absorbed. `ui/plain_disposal.rs`,
`ui/buffer_snapshot.rs`, and `model/editor_memory.rs` are cross-cutting or exempt
and unchanged. No workflow is reified as an explicit state machine, and no
programme-level deferred actuation seam is retired.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workflow-readability-boundaries`: two hygiene amendments, both closing
  adjacencies the convention already sanctions rather than adding obligations or
  capabilities.

  **(a) A migrated workflow whose only pure policy is cross-cutting is a complete
  row.** `WFR-BUFFER-REPLACEMENT` is the first such row: the matrix records
  `model/buffer_replacement.rs` as cross-cutting and staying, so the row is
  complete with `policy: none` plus the cross-cutting owner named. The workflow
  boundary check already permits `policy: none`, but no spec scenario says a row
  is complete in that state, so the permission reads today as gate tolerance
  rather than convention. This mirrors the settled allowance for a workflow with
  no qualifying seam bundle, which census finding 3 added for exactly the same
  reason.

  **(b) A migration re-derives its row's measured cells rather than inheriting
  them.** Four consecutive slots corrected their `Current size` and
  `Seams (i/c/a/p)` cells, always because the census pooled shared service files
  and called neighbours. The friction sections say "re-derive, and expect the
  answer to move in either direction" three times over. Writing it into the
  requirement that makes the matrix the completion source of truth converts a
  rediscovered-every-slot instruction into a stated one, with row-scoping and
  non-`#[cfg(test)]` counting named as the method.

  **Retroactive-amendment obligation.** Under section 8 this triggers a per-row
  re-check of **four** migrated rows (`WFR-SEARCH-REPLACE`,
  `WFR-COMMAND-PALETTE`, `WFR-DOCUMENT-SAVE`, `WFR-DOCUMENT-LOAD`) against both
  statements. Both are expected to be confirmations — every one of those rows did
  re-derive its cells, and none declares `policy: none` — but 3b proved that "it
  must already hold" is not a discharge, so each row is checked individually and
  any gap is filled **in this change**. Nothing beyond these two statements may be
  absorbed; the facade line budget and the bounded coordination role set are
  **not** amended.

Note that `openspec validate --strict` fails any change with no `specs/` delta
("Change must have at least one delta"), which is why every migration slot carries
one; slot 2a corrected the record text that said otherwise.

## Impact

**Prerequisites**

Slots 1, 2a, 2b, 3a, and 3b are archived under
`openspec/changes/archive/2026-08-25-*` and `2026-08-26-*`. The blocking gate is
**slot 3 complete in both halves with `WFR-DOCUMENT-SAVE` and
`WFR-DOCUMENT-LOAD` marked `migrated`, their `Migrated Workflow Roles`
subsections complete, the ledger marking slots 3a and 3b complete, and
`make check-workflow-boundaries` passing** — verified mechanically in task 0.1,
not read from this proposal. The deliverables this change consumes are the
declared facade budget (2a), the stage-order qualification rule (2a), the
evidence-to-snapshot drift check and its per-binding attribution (2a, extended by
3b), the `journal` role name (2b), the per-workflow subdirectory role home (3a,
second adopter 3b), and the evidence-surface reentrancy constraint (3b).

**Code touched** (sizes measured at authoring; task 0.3 re-derives every one
row-scoped and non-`#[cfg(test)]`)

- `crates/lushtext-core/src/ui/window/drafts.rs` (2,460) — the draft workflow's
  facade, journal, coordination, policy, and evidence roles; holds 28
  `*_for_test` functions across 55 gate sites, the largest seam population in the
  slot.
- `crates/lushtext-core/src/ui/window/draft_ordering.rs` (119, mostly tests) —
  candidate pure policy for the draft workflow; decide ownership explicitly.
- `crates/lushtext-core/src/ui/window/session_persistence.rs` (1,110) and
  `session_restore.rs` (417) — the session workflow's roles; `session_restore.rs`
  already holds `SessionRestoreEvidence` and its `evidence()` accessor.
- `crates/lushtext-core/src/ui/window/dialogs.rs` (836) — **a slot-4 consumer, not
  a bystander.** It calls `stage_close_discard_drafts` at two sites, and it owns
  the close-save session end to end over `imp().session`
  (`next_close_save_identity` / `active_close_save_identity`), which is *migrated
  save's* identity living in the session state group. It also hosts three
  chooser-bound actuation seams. In scope for the state-group split, the
  reach-through sweep, and the seam sweep; **not** in scope for restructuring what
  3a migrated.
- `crates/lushtext-core/src/ui/window/startup_data.rs` (435) — **shared** between
  session restore and draft restore; decide ownership rather than absorbing it.
- `crates/lushtext-core/src/ui/window/local_history.rs` (1,633) and
  `crates/lushtext-core/src/ui/editor_page/local_history.rs` (953) — the
  two-directory row; the coordination/presentation split decides the canonical
  role home.
- `crates/lushtext-core/src/ui/editor_page/buffer_replacement.rs` (1,029) — the
  shared installation workflow; 8 `*_for_test` functions across 26 gate sites,
  and the adapter whose slice accounting, terminal classification, and
  supersession logic the `policy: none` probe examines.
- `crates/lushtext-core/src/ui/sidebar/workspace_section/context_menus.rs`,
  `ui/sidebar/workspace_section/mod.rs`, `ui/sidebar/callbacks.rs` — the
  sidebar context-menu entry point into local history, which the row's `Entry
  points` cell omits. In scope for the entry-point correction, not for
  restructuring the sidebar (`WFR-WORKSPACE-TREE`, slot 5).
- `crates/lushtext-core/src/ui/window/actions.rs` and `documents.rs` — the
  `win.show-local-history` action, the post-rename sidecar migration, and the
  restore-undo invocation; called surfaces whose ownership is recorded.
- `crates/lushtext-core/src/model/draft.rs`, `session.rs`, `local_history.rs`,
  `buffer_replacement.rs` — **expected unchanged**; all four are recorded as
  domain or cross-cutting and staying. Confirm rather than assume, and record the
  `next_install_boundary` decision.
- `crates/lushtext-core/src/model/file_load.rs` and
  `crates/lushtext-core/src/ui/editor_page/load/policy.rs` — direct consumers of
  the pure replacement module, both owned by migrated `WFR-DOCUMENT-LOAD`. In
  scope only if the `next_install_boundary` alias decision removes or
  re-documents the re-export; that fan-out also reaches
  `crates/lushtext-core/benches/benchmarks.rs`,
  `crates/lushtext-core/tests/properties/file_load.rs`, and two rustdoc
  intra-doc links in `load/policy.rs` and `load/execution.rs` — the exact
  CI-only-gate shape task 9.4 catches.
- `crates/lushtext-core/src/services/draft_service.rs` (+ `draft_service/`),
  `session_service.rs`, `local_history_service.rs`, `recovery_metadata.rs` —
  **behavior unchanged**; in scope for seam classification and for any buried
  pure policy the change decides about explicitly, per slot 2b's
  `services/search_backup.rs` precedent. `recovery_metadata.rs` is shared between
  drafts and session; decide, do not split by guess.
- `crates/lushtext-core/src/ui/window/imp.rs` and `mod.rs`,
  `crates/lushtext-core/src/ui/editor_page/imp.rs` and `mod.rs` — module
  declarations, re-homed state, the `Cell<Option<SessionRestoreEvidence>>`
  decision, and **the three shared imp state groups this change must split
  deliberately rather than by default**: `SessionState`, which mixes session
  fields with migrated save's close-save identity pair and with
  `close_safety_inflight` / `close_safety_bypass` whose own doc comments say
  "draft/session"; `DraftState`; and `LocalHistoryState`, which migrated save
  reads as its `SaveCompletionTicket` freshness identity and migrated load reads
  at two sites, and which `save/mod.rs` already documents as slot 4's to own.
- `crates/lushtext-core/src/ui/automation.rs` — `local_history`,
  `tabs[].draft_present`, and the two readiness blockers project from evidence.
- `crates/lushtext/tests/widget/window.rs` (the 35 re-derived sites),
  `editor_page.rs`, and any widget module reading retired inspection seams.
- `scripts/check-automation-docs.py` — only if `tabs[].draft_present` becoming a
  third surface projecting into one snapshot object requires extending the
  per-binding attribution 3b added.
- `scripts/crash-recovery-smoke-driver.py` — it asserts against draft manifest
  layout and `tabs[].draft_present`, so it is coupled to this family's durable
  format and its one projected tab field. In scope for confirmation beside the
  smoke lane, not for loosening.
- `scripts/accessibility_warning_allowlist.py` — 3b's A.9c records that this
  allowlist keys on module paths, so a rename can silently turn an expected
  `tracing::error!` into an "unexpected warning". A grep at authoring found **no
  current coupling to this family's modules**, so this is a cheap confirmation
  rather than budgeted work.
- `.cargo/mutants.toml`, `docs/workflow-readability-matrix.md`,
  `docs/next/workflow-readability.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `AGENTS.md`, `README.md`, and any
  `.agents/rules/*.md` or `.agents/skills/**` reference naming a moved path.

**Verification**

Everything slot 3 ran, re-aimed at restore. Behavior equivalence across: a crash
recovery of a file-backed draft and of an untitled draft; a draft large enough to
require chunked installation and one whose largest paragraph exceeds the slice
budget; autosave of a first-dirty edit, a superseded autosave, and a failed
autosave; orphan-body cleanup where the inode matches and where an autosave
replaced the body between inspection and execution; session restore of zero, one,
and many descriptors, a cancelled restore, and a restore whose editor closes
mid-turn; local-history baseline capture, periodic capture, a stale capture, a
preview install, a restore, and a restore undo; a buffer replacement cancelled
mid-slice. Plus: `make check` and `make check-policy` including
`check-workflow-boundaries` and `check-automation-docs`; **the rustdoc lint gate,
which is CI-only and in neither `make check` nor `make pre-commit` nor
`make check-policy`** — four new `pub` facades naming their own private
coordination modules is precisely the `private_intra_doc_links` shape slot 3a
shipped to CI; `make test` and `make test-widget-headless` with **zero `FLAKY:`
lines and no retry relied upon**, and a test count that does not decrease;
`make mutants-diff` with gain-and-parity evidence per workflow;
`make crash-recovery-smoke`, which is this family's own end-to-end lane;
`make performance-smoke` plus the Criterion comparison, because bounded
installation is a performance contract; `make test-prop` if any property target
is touched; a `data-safety` pass in explicit mode **before and after** the diff,
including the two routed candidates; and the mandatory proof lanes
(`visual-geometry-smoke`, `accessibility-smoke`, `visual-smoke`) each from a
**clean artifact root** and ordered **after every source, documentation, and
rules edit**, because the accessibility policy gate fingerprints the contents of
relevant files and an edit after the lane voids the proof. Finally a live
`make run` against restored workspaces with real drafts, a killed and relaunched
session, and a local-history restore, with clean stderr. Acceptance is that
recovered content, restore ordering, cleanup decisions, error surfaces, timing
characteristics, and the exported D-Bus contract behave identically to the
pre-migration workflows.
