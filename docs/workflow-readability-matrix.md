# Workflow Readability Matrix

This matrix is the completion source of truth for deciding whether a LushText
workflow follows the workflow readability convention. The normative contract
lives in five capability specs, and a migration must read every one that touches
its scope: `openspec/specs/workflow-readability-boundaries/spec.md` (module
shape, facade contract and budget rule, seam value objects, naming, risk tiers,
retroactive amendment), `openspec/specs/workflow-evidence-surfaces/spec.md`
(evidence surfaces, their single visibility rule, the seam taxonomy, the
evidence-to-automation projection),
`openspec/specs/gtk-adapter-module-boundaries/spec.md` (the decomposition
contract and the **bounded set of coordination role names** — adding a role name
amends that spec), `openspec/specs/mutation-testing/spec.md` (the
`ui/**/policy.rs` scope convention and relocation parity), and
`openspec/specs/dbus-automation-spine/spec.md` (snapshots projecting from
evidence). This file maps that contract to concrete workflows, file sets, policy
ownership, test seams, seam value objects, risk tiers, and migration status.

`docs/next/workflow-readability.md` is the programme record beside this matrix:
it holds why the programme exists with its measured baseline, how much is
actually migrated, the remaining per-change scope with its machine-readable slot
ledger, the sequencing rationale, the rejected alternatives, and the deferred
work. Read it first if you are starting cold; read this matrix when you need a
specific workflow's status or obligations. `make check-workflow-boundaries`
compares the two, so they cannot drift.

A workflow is a user-initiated operation with ordered stages that crosses the
adapter boundary into coordination and pure policy. Rows are one per workflow,
not one per widget. Surfaces with no coordination tier are listed in
[Surfaces With No Coordination Tier](#surfaces-with-no-coordination-tier) so
census completeness is provable without inflating the row count.

All counts in this document were measured against the tree at the time of the
census (change `normalize-workflow-readability-boundaries`, section 1). The
measurement definitions are stated in
[Measurement Definitions](#measurement-definitions) because several of them have
more than one defensible denominator.

## Status Labels

- `pending`: the workflow has not been migrated; its target shape is recorded
  here and its migration slot is assigned.
- `migrated`: the workflow follows the convention and its row names the
  evidence.
- `partially-conforming`: some required roles already exist in the tree from
  earlier work; the row names which roles remain.
- `exempt`: the workflow or module must not be forced into the convention. The
  row states why, and a later migration change MUST NOT override it.
- `deferred`: the workflow will be migrated, but deliberately later than its
  risk tier alone would suggest. The row states the reason.
- `cross-cutting`: the module is shared coordination or shared policy and stays
  in a shared location rather than moving beside one workflow.

## Risk Tiers

- `tier-1`: no user-data persistence, no async completion seam, no
  pixel-verified geometry invariant. Failure is visible and local.
- `tier-2`: async worker with a freshness seam, or a read-only projection of
  user data. Failure can silently show stale state.
- `tier-3`: persists or mutates user documents, drafts, sessions, sidecars, or
  local history. Failure can lose user data.

Per the convention, a `tier-3` workflow MUST NOT be migrated before the
convention has been proven on at least two completed lower-risk migrations.

## Reading The Matrix

| Column | Meaning |
| --- | --- |
| Row id | Stable id used by docs, the policy check, and later migration changes. |
| Workflow | Product-facing workflow name. |
| Current size | Files and lines across `ui/`, `model/`, and `services/` for this workflow. |
| Entry points | Actions, accelerators, or callbacks that start the workflow. |
| Owned pure policy | Pure policy modules this workflow owns, with consumer classification. |
| Seams (i/c/a/p) | `*_for_test` functions by kind: inspection / configuration / actuation / probe-reset. `sites` is the `#[cfg(feature = "test-utils")]` attribute count. |
| Seam value object | The identity/freshness value object at the workflow's seam. `exists:` already reified; `required:` must be introduced by the migration. |
| Evidence surface | Current typed observation state for this workflow. |
| Risk | Risk tier. |
| Slot | Planned migration change. |
| Status | Status label. |

## Product Matrix

| Row id | Workflow | Current size | Entry points | Owned pure policy | Seams (i/c/a/p) | Seam value object | Evidence surface | Risk | Slot | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| WFR-SEARCH-REPLACE | Workspace search and Replace All | **14 files, 5,527 production lines** in `ui/search_panel/**`, counting non-`#[cfg(test)]` lines only. The workflow additionally **calls** `services/content_search/**` (1,978 production lines, shared with `WFR-EDITOR-FIND` and the fault-injection lane), `services/search_backup.rs` (1,073, which slot 2b decided stays in services), and `services/saved_searches.rs` (68, shared with the palette's saved-search source), none of which it owns. The pre-migration cell read `19 files, 13,686 lines (ui 5,422 / model 2,369 / services 5,895); exemplar scope 4,762` and pooled all three of those service populations, counting whole files including their co-located tests; slot 4's amendment re-check corrected it | `win.begin-search`, `Ctrl+Shift+F`, search entry changed, Replace All button, Undo button | relocated to `crates/lushtext-core/src/ui/search_panel/policy.rs`: slot 1 moved the two `model/` search policy modules named in the [Policy Module Census](#policy-module-census); slot 2b added the Replace All durable half's decisions (preview reservation and shrink-to weights, the saturating retained-byte cast, the undo-capacity admission plan, the journal generation predicate, `ReplaceApplyCounts`). Mutation parity proved for both, and slot 2b's move was out-of-scope-to-in-scope, so it *gained* 11 mutants with zero survivors. Domain `model/workspace_search.rs` stays | both halves migrated: slot 1 retired 8 inspection fns into `evidence.rs` and collapsed 5 configuration setters plus 6 override statics into `SearchPanelTestPolicy`; slot 2b added 10 evidence fields and needed no new inspection fn, no new override, and no new actuation seam. 5 actuation seams remain on the replace/undo transaction (`clear_undo_backup_for_test`, `reserve_undo_backup_generation_for_test`, `set_persisted_undo_backup_for_generation_for_test`, `begin_replace_transaction_for_test`, `finish_replace_transaction_for_test`) plus 2 accessibility probes, all deferred at programme level. Write-side seams in `services/content_search/replace.rs` are classified under [Migrated Workflow Roles](#wfr-search-replace) | **3 seams reified** (the programme's primary unit; long signatures shortened remains 0 for this row, and no function in it has 6 or more non-receiver parameters, so neither the receiver-counted 88 nor the strict 43 signature figure applies here). exists: `WorkspaceSearchRequest` + `WorkspaceSearchStart` (search side), `ReplacePreviewTicket` + `ReplacePreviewFacts` (preview-freshness side). new: `UndoRestoreClaim` (the window's undo-restore claim seam, naming the transaction-busy and capacity-deferred refusals the panel must restore the affordance for) | exists: `SearchPanelEvidence` via `evidence()`, extended by slot 2b with the apply transaction's own pending flag, all three generations, the preview capacity-retry state, the installed journal's entry count and retained weight, in-flight and cumulative journal disk-job counters, and the last apply's counts; automation `window.content_search` projects from it and slot 2b widened nothing | tier-2 (search/preview half), tier-3 (Replace All write half) — **both now covered** | 1 (search/preview half) + 2b (replace/undo half) | migrated |
| WFR-COMMAND-PALETTE | Command palette, file index, notes browse modes | **10 files, 2,534 production lines** in `ui/command_palette/**`, counting non-`#[cfg(test)]` lines only. The workflow additionally **calls** `services/palette/**` (4,829 production lines once the `#[cfg(test)] mod tests;` file `services/palette/tests.rs` (1,223) is excluded), of which `notes.rs` (2,163) is shared with `WFR-NOTES-BOOKMARKS` (slot 5). The pre-migration cell read `16 files, 11,179 lines (ui 2,528 / model 754 / services 7,897)`; its `ui` subtotal pooled `ui/window/focus_indexing.rs` (856), which the census attributed to this row but which stays window code, and its `services` subtotal pooled the whole palette service half with the notes row's share. Slot 4's amendment re-check completed the correction | `win.toggle-command-palette` (`Ctrl+Shift+P`), palette mode dropdown and Tab cycling, sidebar file create/delete/rename and watcher reconciliation, `win.notes-show-notes` (the census cell said `Ctrl+P` / `Ctrl+K`; the real palette accelerator is `Ctrl+Shift+P` and `Ctrl+K` belongs to the recent-Open popover) | relocated to `crates/lushtext-core/src/ui/command_palette/policy.rs` (queue admission byte/cap math, batch-kind selection and flush guard, mutation-generation arbitration and replay, retirement-cap classification, header-skipping navigation, presentation decisions); mutation coverage gained, not relocated — see [Migrated Workflow Roles](#wfr-command-palette). Domain `model/palette.rs` (17 consumers) stays | palette half migrated: 5 inspection fns retired into `evidence.rs`, 2 configuration setters plus 2 override statics collapsed into `CommandPaletteTestPolicy`; 3 process-global retirement counters retained as lifecycle probes (per-widget folding would change their meaning from "this process observed a last-owned at-cap retirement" to a count no test asks for); 3 actuation seams deferred. The pre-migration cell read `15/10/2/0 = 27 fns, 40 sites, 4 override statics`, which was row-scoped: `ui/command_palette/**` held 12 of those functions and 22 of the gate-attribute sites, and the other 15 live in `services/palette/**` and `ui/window/notes/**` shared with `WFR-NOTES-BOOKMARKS` (slot 5) | exists: `PaletteSearchCoordinator` generation identity + `is_current` (query seam; the convention accepts a coordinator that owns the generation as the seam value object). new: `FileIndexMutationTicket` + `FileIndexMutationFacts` + `arbitrate` (file-index mutation seam) | exists: `CommandPaletteEvidence` via `evidence()`; automation `window.command_palette` and both palette readiness blockers project from it | tier-2 | 2a | migrated |
| WFR-DOCUMENT-SAVE | Save, Save As, save formatting, durability | **5 files, 1,855 production lines** in `ui/editor_page/save/**`, counting non-`#[cfg(test)]` lines only: facade 223, admission 459, execution 555, policy 444 (915 total, of which 471 are the module's co-located unit tests), evidence 174. The workflow additionally **calls** the window-side invocations in `ui/window/dialogs.rs` and `ui/window/documents.rs` and the shared `services/editor_io.rs` and `services/durable_write.rs` durable-write path, which it does not own; counting those three neighbours as files is how the pre-migration cell reached its total. That cell read `7 files, 6,672 lines (ui 2,132 / model 991 / services 3,549)` and was wrong in both directions: it counted the whole of `editor_io.rs` (3,035) and `durable_write.rs` (1,228), both shared with `WFR-DOCUMENT-LOAD` and every other write path, and it counted `load_save.rs` (1,795) whole although the save half was roughly a third of it | `win.save`, `win.save-as`, `Ctrl+S`, close-with-changes dialog, autosave-on-close | relocated to `crates/lushtext-core/src/ui/editor_page/save/policy.rs`: the former model/save_admission.rs (405 lines) moved with mutation parity proved, and the save half's remaining pure decisions were extracted from the GTK adapter into the same module — the queued-save staleness predicate and its `QueuedSaveTicket`/`QueuedSaveFacts` seam, the pending-load pre-emption derivation, the saved-text disposition (formatting acceptance plus the buffer mirror-back), the capture-mode naming, and the durable write classification. Those extractions are a coverage **gain from zero**, reported separately from the relocation's parity numbers. The chunked-capture *threshold* is **not** owned here: it belongs to cross-cutting `ui/buffer_snapshot.rs` (`WFR-BUFFER-SNAPSHOT`, slot 7) and duplicating it would fork a shared limit | migrated: 3 inspection fns retired into `evidence.rs` (`save_runtime::snapshot_for_test`, `transient_save_admission_snapshot_for_test`, `save_uses_chunked_snapshot_for_test`, `save_snapshot_inflight_for_test` — four call surfaces over three mechanisms), and **no** `*_for_test` inspection function remains on the save path. 3 actuation seams preserved on the editor side (`reset_transient_save_admission_for_test`, `pause_next_save_snapshot_for_test`, `resume_save_snapshot_for_test`) plus the 3 chooser-bound Save As seams in `ui/window/dialogs.rs`, all deferred at programme level. **1 new actuation seam, counted and justified**: `expire_close_save_session_for_test`, replacing an ungated `session.active_close_save_identity` write. 4 of the 5 ungated `imp()` write sites became real drives of the workflow. The pre-migration cell read `10/11/9/4 = 34 fns, 44 sites, 5 override statics`, which was **not** row-scoped: it pooled `services/editor_io.rs` (6 load-side, 3 save-side, 1 shared), `services/durable_write.rs`, and `services/filesystem/write.rs` seams shared with `WFR-DOCUMENT-LOAD` (3b) and the fault-injection lane. Row-scoped, `load_save.rs` held 18 `*_for_test` functions of which 6 were save-side, and the retired save_runtime.rs held 2. The 5 save/load `test-utils` override statics live in `services/editor_io.rs` and stay there, because the service owns the behavior they override | **done**: `QueuedSaveTicket` + `QueuedSaveFacts` + `queued_save_is_current`, constructed once at the workflow entry point and validated as a unit. exists: `SaveCompletionTicket` (completion seam), unchanged and distinct | exists: `SaveEvidence` via `save_evidence()`; the exported snapshot field `tabs[].saving` projects from it and is covered by the Evidence Projection Map drift gate. The `save` readiness blocker and the readiness aggregate read the same workflow-owned state through the facade's cheap `is_saving()` accessor rather than building a whole surface per editor per poll — identical by construction, since both read the one `save.inflight` cell. The exported D-Bus contract is unchanged | tier-3 — **now covered**. The durable write path, its `BeforeRename`/`AfterRename` classification, and the buffer-versus-disk agreement before the tab goes clean are all narrated by the facade and asserted from evidence | 3a | migrated |
| WFR-DOCUMENT-LOAD | Open document, reopen with encoding, recent documents | **7 files, 2,375 production lines** in `ui/editor_page/load/**`, counting non-`#[cfg(test)]` lines only: facade 253, admission 634, execution 720, retirement 219, policy 278 (474 total, of which 196 are the module's co-located unit tests), evidence 206, test policy 65. The workflow additionally **calls** the window-side invocations in `ui/window/documents.rs`, `ui/window/encoding.rs`, `ui/window/recent_open.rs`, `ui/window/session_restore/`, and `ui/window/search.rs`, the shared `services/editor_io.rs` read/decode path, and the two cross-cutting editor-page groups it left behind (`ui/editor_page/document_identity.rs` 102, `ui/editor_page/restore_position.rs` 93), none of which it owns. The pre-migration cell read `10 files, 5,301 lines (ui 3,265 / model 661 / services 1,375)` and was wrong in both directions: the `ui` subtotal pooled window files this row only calls, and the `services` subtotal counted the whole of `editor_io.rs`, shared with `WFR-DOCUMENT-SAVE` and every other read/write path. Row-scoped, the workflow's own pre-migration `ui` code was the retired load_save.rs residual (1,212) plus the retired load_runtime.rs (423) = 1,635 | `win.open-file`, `win.open-recent`, `Ctrl+O`, `Ctrl+K`, sidebar row activation, session restore, reopen-with-encoding | extracted into `crates/lushtext-core/src/ui/editor_page/load/policy.rs`: the chunked-versus-direct install threshold, the clear-slice budget and the **paragraph-boundary** rule that keeps bounded installation linear, the install-phase and abort-disposition classification, the two freshness predicates and the `LoadRequestTicket` seam, the failure-state rule, and the user-cancellation publication rule. That is a coverage **gain from zero** (44 generated, 41 killed, 3 unviable, 0 missed), reported without any relocation parity because **`model/file_load.rs` stays in `model/`** — `services/editor_io.rs` depends on it, so moving it under `ui/` would invert dependency direction. The install *boundary* arithmetic (`next_install_boundary`) is **not** owned here either: it is shared with `model/buffer_replacement.rs` (`WFR-BUFFER-REPLACEMENT`, slot 4) and duplicating it would fork a shared limit | migrated: **10 inspection surfaces retired** into `evidence.rs` (`load_runtime::snapshot_for_test`, `load_runtime::disposal_wakeup_armed_for_test`, `load_projection_suspended_for_test`, `transient_load_admission_snapshot_for_test`, `transient_load_disposal_wakeup_armed_for_test`, `load_installation_slice_count_for_test`, `load_installation_active_for_test`, `load_installation_weight_for_test`, `load_generation_for_test`, `load_cancel_token_for_test`), and **no** `*_for_test` inspection function remains on the load path. **2 configuration seams collapsed into 1** test-policy value in `load/test_policy.rs`, entirely behind `#[cfg(feature = "test-utils")]`, keeping both public setter names. **7 actuation seams preserved and 0 added**: `apply_load_result_for_test`, `apply_reload_error_for_test`, `apply_loaded_content_for_test`, `reset_transient_load_admission_for_test`, and the 3 chooser-bound seams in `ui/window/dialogs.rs`; the retired `load_runtime::reset_for_test` was folded into the editor-page seam rather than kept as a second surface, so the count fell from 8 to 7. The 6 load-side `test-utils` overrides in `services/editor_io.rs` stay there, because the service owns the behavior they change. The pre-migration cell read `23/7/3/1 = 34 fns, 55 sites, 3 override statics`, which was **not** row-scoped: it pooled service seams shared with `WFR-DOCUMENT-SAVE` (3a) and `WFR-DRAFT-RECOVERY` (slot 4) | **done**: `LoadRequestTicket` + `load_request_is_current`, constructed once at the workflow entry point and validated as a unit. The deliberately weaker `installation_is_current` stays separate, because an installation must re-read the live token rather than assert its dispatch-time identity | **`LoadEvidence` via `load_evidence()`**, folding in the pre-convention typed `FileLoadAdmissionSnapshot` rather than leaving a second path. The exported snapshot field `tabs[].load_state` projects from it and is covered by the Evidence Projection Map drift gate; the `file-load` readiness blocker and its six predicates read the same `load_state` cell through the cheap lifecycle accessor rather than building a whole surface per tab per poll — identical by construction. Every other field is internal. `OpenPopoverRowLayoutSnapshot` is **removed from this cell**: it describes recent-document popover row layout, not load state, and its hosting files were an outright census gap — see [The recent-documents surface census gap](#the-recent-documents-surface-census-gap) | tier-3 — **now covered**. The bounded install state machine, its paragraph-boundary contract, the publish-refused-as-stale verdict, and the cancelled-clear path are all narrated by the facade and asserted from evidence | 3b | migrated |
| WFR-DRAFT-RECOVERY | Draft autosave, crash recovery, orphan cleanup | **10 files, 3,153 production lines** in `ui/window/drafts/**`, counting non-`#[cfg(test)]` lines only: autosave_execution 738, journal 588, policy 341 (of 734; 393 are the module's co-located unit tests), restore_execution 341, facade 289, seams 208, evidence 201, test_policy 181, admission 210, retirement 56. The workflow additionally **calls** `services/draft_service.rs` and its `draft_service/` submodule, `services/recovery_metadata.rs`, and `WFR-SESSION-RESTORE`'s `collect_session`, none of which it owns. The pre-migration cell read `6 files, 8,930 lines (ui 2,578 / model 442 / services 5,910)` and was wrong by a wide margin in one direction: its `services` subtotal was almost entirely `services/draft_service.rs`, most of which is co-located tests, and it counted `model/draft.rs` whole | first-dirty autosave timer, the always-running 5 s tick, startup recovery scan, restored-draft inline alert with `Discard...` / `Save...`, close-time flush (sync and async), draft delete, and deferred orphan cleanup | **extracted into `crates/lushtext-core/src/ui/window/drafts/policy.rs`**: candidate eligibility including the `installation_incomplete` **data-safety guard**, autosave admission (mark-pending rather than queue), the post-snapshot freshness predicate, pipeline failure accounting and its user-facing message, orphan-cleanup continuation with its exponential backoff and cap, the grouped cleanup failure message, and the `DraftMutationOrder` epoch allocator relocated whole from the retired draft-ordering module with its co-located tests. That is a **gain from zero** for the extracted decisions plus a **relocation** for the ordering allocator, reported separately. `model/draft.rs` **stays in `model/`** as domain: re-derived as owning workflows its consumer count is **2** (`WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`), and `services/draft_service.rs` depends on it | migrated: **7 inspection fns retired** into `evidence.rs` (`draft_autosave_inflight_for_test`, `draft_pipeline_max_retained_bodies_for_test`, `orphan_cleanup_runtime_snapshot_for_test`, `lazy_draft_restore_inflight_for_test`, `draft_restore_inflight_for_test`, `draft_mutation_inflight_for_test`, `draft_delete_tombstoned_for_test`), and no `*_for_test` inspection function remains. The keyed tombstone question became `draft_delete_is_tombstoned(&str)`, a named workflow question rather than a per-field getter, because a field cannot take an argument. **10 configuration seams plus 9 delay/fail hooks collapsed into 1** test-policy value in `drafts/test_policy.rs`, entirely behind `#[cfg(feature = "test-utils")]`, keeping every public setter name — the slot's largest configuration population. **4 actuation seams preserved and 0 added** (`autosave_tick_for_test`, `schedule_orphan_cleanup_for_test`, `dispose_orphan_cleanup_for_test`, plus `set_next_draft_body_disposal_probe_for_test` as a probe). The 6 load-side `test-utils` overrides in `services/editor_io.rs` are **shared** with save and load and stay in the service. The pre-migration cell read `7/18/3/0 = 28 fns, 53 sites, 14 override statics`, which counted 10 always-compiled private worker helpers as caller-visible seams; row-scoped and caller-visible it was 18 | **re-audited**: `DraftRestoreTicket` + `DraftRestoreFacts` + `draft_restore_is_current` (the Ticket/Facts/predicate shape, validated when the worker returns **and again** inside the replacement terminal), `DraftMutationIntent` (main-thread intent assigned before any document-sized work, with **epoch equality** rather than ordering so wraparound stays correct), and the pipeline's `DirtyDraftCandidate` / `DirtyDraftCompletion` / `AcceptedDraft` triple, gathered in `drafts/seams.rs`. `DraftCleanupContinuation` is **reclassified**: it is the journal's manifest offset, not a separate seam value object. No `#[expect(clippy::too_many_arguments)]` introduced; the workspace count holds at 1 | **`DraftEvidence` via `draft_evidence()`**, folding in `OrphanCleanupRuntimeSnapshot` so no second typed path remains. It makes the durable path observable: manifest entry count and authority, the mutation lane's ownership, tombstone and pending-delete counts, autosave in-flight and pending state, retained body weight **and its high-water mark**, cleanup worker counts and continuation offset, and the readiness verdict. The manifest is read as a **count, never cloned**. The `draft-autosave` readiness blocker reads the same six cells through the facade's cheap accessor, identical by construction because both call `policy::draft_workflow_blocks_readiness`. `tabs[].draft_present` is **not** re-sourced: it is a per-tab document-identity fact read through the editor page's existing `draft_id()` operation, while this surface is window-level — recorded rather than fabricated as a projection, which is why the drift gate did **not** need a third `tabs` surface | tier-3 — **now covered**. The autosave admission guard that protects a good draft from a partially installed buffer, the one-body-at-a-time pipeline, the delete ordering that keeps the manifest a durable retry marker until the body is gone, and the orphan-cleanup inode/guard/recheck contract are all narrated by the facade and asserted from evidence | 4 | migrated |
| WFR-SESSION-RESTORE | Session persistence and bounded restore | **6 files, 1,758 production lines** in `ui/window/session_restore/**`, counting non-`#[cfg(test)]` lines only: policy 553 (of 1,172; 619 are the module's co-located unit tests), journal 406, admission 279, evidence 200, facade 165, execution 155. The workflow additionally **calls** `services/session_service.rs`, `services/draft_service.rs`, and `services/recovery_metadata.rs`, and reads `ui/window/startup_data.rs`'s completion flag, none of which it owns. The pre-migration cell read `5 files, 2,599 lines (ui 1,962 / model 300 / services 337)` and pooled `model/session.rs` plus a partial services subtotal | app startup (through `ui/window/startup_data.rs`'s gate, which this row does **not** own), window close, tab mutation persistence | **extracted into `crates/lushtext-core/src/ui/window/session_restore/policy.rs`**, in two parts reported separately. **Relocated**: the bounded-turn admission policy (`SessionRestorePolicy`, its permits, its turn planner, its terminal accounting), which was already explicit policy and merely mislocated in a GTK adapter file. **Gained from zero**: the journal's pure half, previously inline in the adapter — session-tab identity, the close-time merge that preserves not-yet-admitted descriptors, the startup preload-graph fit to its disposal reservation, and the recovery-diagnostic summary. `model/session.rs` **stays in `model/`**: its census cell said 8 consumers, but re-derived as owning workflows it is **1**, and `services/session_service.rs` depends on it, so relocating under `ui/` would invert dependency direction — the 3b `model/file_load.rs` precedent | migrated: **2 inspection fns retired** into `evidence.rs` (`session_restore_runtime_snapshot_for_test`, `startup_session_descriptors_pending_for_test`), and no `*_for_test` inspection function remains. **0 configuration seams** — this workflow has none. **2 actuation seams preserved and 0 added** (`restore_session_for_test`, `cancel_session_restore_for_test`), both driving startup and cancellation paths no headless test can otherwise reach. The pre-migration cell read `2/0/2/0 = 4 fns, 5 sites`, which was **correct and row-scoped** | **re-audited**: `SessionRestorePlanPermit` (generation-bound planning ownership, crossing plan -> mount -> load terminal -> release) and `SessionRestoreAdmission` (one admitted descriptor with its permit, built by the planner and consumed by the mounter). Both qualify; neither is renamed across a seam | **`SessionRestoreEvidence` via `session_restore_evidence()`** — a **new window-level surface**. The pre-migration type of the same name was the *policy's internal counters*, not a workflow surface; it is renamed `SessionRestoreTurnMetrics` and the new surface projects it, folding in `SessionRestoreRuntimeSnapshot` so no second typed path remains. `session.save_failed` and the shared `close_safety_*` flags are now surface fields, so the widget tests' 15 `imp().session` reads are gone. The `session-restore` readiness blocker reads the same `restoring` cell through the facade's cheap `session_restore_in_progress()` accessor — identical by construction. The exported D-Bus contract is unchanged | tier-3 — **now covered**. The bounded-turn admission, the exactly-once planning-terminal accounting, the close-time merge that protects unreached descriptors, and the user-first selection settle are all narrated by the facade and asserted from evidence | 4 | migrated |
| WFR-LOCAL-HISTORY | Local history capture, preview, restore | **8 files, 3,009 production lines**: 2,302 in the canonical role home `ui/window/local_history/**` (preview_execution 959, restore_execution 340, policy 319 of 650 with 331 co-located unit tests, journal 238, facade 215, evidence 171, test_policy 60) plus **707** in the called capture surface `ui/editor_page/local_history.rs`. The workflow additionally **calls** `services/local_history_service.rs` (shared with migrated `WFR-DOCUMENT-SAVE`) and `services/recovery_metadata.rs`, neither of which it owns; counting the first whole is how the pre-migration cell reached `4 files, 5,536 lines (ui 2,586 / model 173 / services 2,777)` | `win.show-local-history`, the editor context menu, **the sidebar context menu** (an entry point the census cell omitted entirely), the command palette, baseline capture on first edit, the periodic capture timer, the restore and undo-restore actions, post-rename lineage migration, and **a Save-origin capture driven by migrated `WFR-DOCUMENT-SAVE`** | **extracted into `crates/lushtext-core/src/ui/window/local_history/policy.rs`** — the workflow's one policy module, even though its capture half lives in another directory. Viewer geometry and its clamps, which snapshots the user is shown (the legacy-empty-baseline rule and its deliberately conservative two-and-two threshold), the preview install plan and its completion predicate, both capture freshness tickets with their predicates, the periodic reschedule rule, and row presentation. That is a coverage **gain from zero**. `model/local_history.rs` **stays in `model/`** as domain: re-derived as owning workflows its consumer count is **2** (`WFR-LOCAL-HISTORY`, `WFR-DOCUMENT-SAVE`), and `services/local_history_service.rs` depends on it | migrated: **8 inspection fns retired** into `evidence.rs` across **both** directories (`local_history_preview_install_snapshot_for_test`, `local_history_baseline_candidate_present_for_test`, `local_history_baseline_retry_pending_for_test`, `local_history_automatic_capture_inflight_for_test`, `local_history_periodic_snapshot_inflight_for_test`, `local_history_periodic_timer_pending_for_test`, `has_local_history_restore_undo_for_test`, and the `local_history_preview_install_delay_for_test` reader), and no `*_for_test` inspection function remains. **4 configuration seams collapsed into 1** test-policy value in `local_history/test_policy.rs`, entirely behind `#[cfg(feature = "test-utils")]`, keeping every public setter name; the re-exported `set_local_history_preview_read_delay_for_test` **stays in `services/local_history_service.rs`**, which owns the behavior it changes — the `editor_io` precedent. **4 actuation seams preserved and 0 added**. The pre-migration cell read `9/11/4/0 = 24 fns, 33 sites, 4 override statics`, which pooled the service's own seams | **re-audited**: `BaselineCaptureTicket` + `BaselineCaptureFacts` and `PeriodicCaptureTicket` + `PeriodicCaptureFacts` (both Ticket/Facts/predicate, now in the canonical `policy.rs` so the capture surface **calls** them rather than defining its own), and `LocalHistoryReplacementTicket` (the `is_current(&editor)` variant, whose handoff to `BufferReplacementTicket` is a named operation on the migrated replacement facade, not a reach into its state) | **`LocalHistoryEvidence` via `local_history_evidence()`**, folding in **both** pre-convention typed observations (`LocalHistoryPreviewCoordinatorSnapshot`, `LocalHistoryPreviewInstallSnapshot`) so no second typed path remains. **One surface for a workflow spanning two directories.** The exported `local_history` snapshot object projects from it and is covered by the Evidence Projection Map drift gate, which slot 4 extended to register this third projecting surface and then **proved rejects a real rename**. The exported D-Bus contract is unchanged | tier-3 — **now covered**. The safety-snapshot ordering that makes a restore non-destructive, the bounded preview install, and both capture freshness contracts are narrated by the facade and asserted from evidence | 4 | migrated |
| WFR-BUFFER-REPLACEMENT | Bounded buffer install and clear slices | **4 files, 1,472 production lines** in `ui/editor_page/buffer_replacement/**`, counting non-`#[cfg(test)]` lines only: facade 168, execution 921 (of 972; 51 are the carried-over `pairing_tests`), policy 212 (of 339; 127 are the module's co-located unit tests), evidence 171. The workflow additionally **calls** cross-cutting `model/buffer_replacement.rs` (93 production of 186), which it does not own. The pre-migration cell read `2 files, 1,215 lines (ui 1,029 / model 186)` and counted whole files including tests plus the cross-cutting module it calls | **five call sites across four owning workflows**, from `BufferReplacementWorkflow`'s own variants: draft restore install (`ui/window/drafts/restore_execution.rs`), local-history restore **and** local-history restore undo (`ui/window/local_history/restore_execution.rs`, twice), memory eviction (`ui/editor_page/mod.rs`), and save formatting mirror-back (`ui/editor_page/save/execution.rs`). The pre-migration cell said "Replace All undo", which is **not** a caller — `LocalHistoryUndo` is local history's own undo — and omitted memory eviction and save formatting entirely | **extracted into `crates/lushtext-core/src/ui/editor_page/buffer_replacement/policy.rs`**: the seam value types, the start disposition, the cancellation disposition (whether a partially mutated buffer owes the user a bounded clear pass), the clear-turn progress and insertion-completion rules, the turn-admission rule that lets a cancelled clear run even after the caller goes stale, terminal classification, guard-restoration on disposal, and the bounded-turn metrics accounting. That is a coverage **gain from zero**; see [Migrated Workflow Roles](#wfr-buffer-replacement). `model/buffer_replacement.rs` stays as cross-cutting: it owns the direct/sliced threshold, the clear slice budget, and the **paragraph-boundary** `next_replacement_boundary`, called by three owning workflows and duplicated by none | migrated: **4 inspection fns retired** into `evidence.rs` (`buffer_replacement_in_progress_for_test`, `buffer_replacement_projection_suspended_for_test`, `buffer_replacement_slice_count_for_test`, `buffer_replacement_terminal_diagnostic_for_test`), and no `*_for_test` inspection function remains on this path. **0 configuration seams** — this workflow has none. **4 actuation seams preserved and 0 added** (`replace_buffer_for_test`, `replace_buffer_returning_cancelled_body_for_test`, `dispose_buffer_replacement_for_test`, `make_buffer_replacement_stale_after_slices_for_test`), each driving a step reachable only through a caller workflow or a resumed slice turn, which is the programme-level deferred category. The pre-migration cell read `4/0/4/0 = 8 fns, 26 sites`, which was **correct and row-scoped** — the only one of slot 4's four seam cells that needed no correction | **re-audited**: `BufferReplacementTicket` (caller-owned freshness identity, crossing entry -> park -> session -> every turn -> terminal -> caller callback, reconstructed at 5 call sites) and `BufferReplacementRequest` (the intent bundle: ticket + body + freshness check + terminal callback, built once at the entry point and validated as a unit by its four kind-paired constructors). `BufferReplacementSession` is **reclassified**: it is coordination-owned GTK runtime, not a seam value object, and the pre-migration cell naming it as one was wrong | **`BufferReplacementEvidence` via `buffer_replacement_evidence()`**, a new surface. No exported D-Bus field projects from it; every field is internal | tier-3 — **now covered**. The cancellation contract that a half-installed document is never left visible, the paragraph-boundary call, and the exactly-once terminal are all narrated by the facade and asserted from evidence | 4 | migrated |
| WFR-WORKSPACE-TREE | Workspace folders, file tree, watch, reconcile | 28 files, 16,947 lines (ui 11,682 / model 1,368 / services 3,897) | New Workspace, Add Folder, refresh button, row activation, context menus, `Space` peek, watcher events | `model/workspace_scan.rs` (3 consumers → single-workflow, relocates); `model/workspace.rs` (28 consumers → domain, stays); `model/workspace_persistence.rs` (2 consumers → single-workflow, relocates) | 24/7/29/5 = 65 fns, 116 sites | exists: `WorkspaceScanTicket` (scan side). required: `WorkspaceWatchTicket` (watch-install side; `{targets_generation, lifetime_generation}` compared loosely at 2 sites) | partial: `WorkspaceScanPressureEvidence`, `WorkspaceWatchMailboxSnapshot` | tier-3 | 5 | pending |
| WFR-NOTES-BOOKMARKS | Notes, bookmarks, sidecar migration, format upgrade | 22 files, 12,521 lines (ui 4,977 / model 770 / services 6,774) | `win.notes-*`, `win.toggle-bookmark`, `win.edit-bookmark-label`, rename-driven sidecar migration, startup reconcile | `model/note.rs`, `model/bookmark.rs`, `model/sidecar_identity.rs` (6/9/11 consumers → domain, stay) | 2/4/4/0 = 10 fns, 16 sites, 2 override statics; slot 2a additionally left this row the 15 palette-row seam functions that live in `services/palette/**` and `ui/window/notes/**` | required: `NotesBrowserTicket` (carries `{generation, mode}`; the `is_current(generation) && mode == mode && !disposed` triple is duplicated at 2 sites). **Named slot-5 task from slot 2a:** retire `NoteSourceRefreshCoordinator` into the shared `SingleFlightCoordinator`. Slot 2a deferred it, and the reason is **not** that the state is shared — there are two independent instances, `command_palette_note_refreshes` on the window imp serving the palette and `source_refreshes` in `ui/window/notes/mod.rs` serving the Notes browser. The reason is that deduping the *type* changes `NotesBrowserRuntimeSnapshot`'s shape, which is this row's surface area | partial: `NotesBrowserRuntimeSnapshot` | tier-3 | 5 | pending |
| WFR-MARKDOWN-PREVIEW | Markdown preview render, images, footnotes, tables | 11 files, 11,274 lines (ui 8,334 / services 2,940); re-measured after `continue-markdown-preview-past-oversized-blocks` added `ui/markdown_preview/continuation.rs` (1,170) and `text_flow.rs` (265) while `mod.rs` fell 2,541 → 1,985, and grew `services/markdown_render.rs` 556 → 2,940, of which ~1,700 are co-located `#[cfg(test)]` planner tests. The `ui` subtotal spans `ui/markdown_preview/**` (7,762) plus `ui/window/preview.rs` (572) | `Alt+P`, `win.toggle-preview-mode`, side-by-side action, buffer changed | none in `model/` | 12/4/3/2 = 21 fns, 56 sites, 3 override statics (unchanged by the continuation change) | exists: `MarkdownRenderSession::is_current(generation)`; plus the planner/projector batch seam `MarkdownCarrySignature` + `MarkdownOpenContainer` (expected/open containers per batch, chained across turns), `MarkdownBlockOmission` (omission reason, scope, and unretained charge crossing the same seam), and the projector-side `MarkdownProjectionContinuation` + `ContinuationBreach` that holds and validates it | partial: `MarkdownImageAdmissionSnapshot` | tier-2 | 7 | deferred — see [Outlier Resolutions](#outlier-resolutions) |
| WFR-MINIMAP | Minimap strip, markers, native source map geometry | 2 files, 3,965 lines (ui 3,779 / model 186) | `win.toggle-minimap`, `Ctrl+Shift+M`, buffer/viewport/sidebar reflow | `model/minimap_analysis.rs` (1 consumer → single-consumer, relocates) | 9/1/1/0 = 11 fns, 16 sites | exists: `MinimapAnalysisSession` (`{generation, lifetime}`) | partial: `MinimapAnalysisSnapshot` | tier-2 logic, high proof cost | 6 | deferred — see [Outlier Resolutions](#outlier-resolutions) |
| WFR-BUFFER-SNAPSHOT | Bounded GTK buffer text capture | 1 file, 1,149 lines (ui) | called by save, draft autosave, encoding analysis, preview, local history | `model/plain_disposal.rs` is consumed through `plain-disposal`, not owned here | 5/0/4/0 = 9 fns, 40 sites | exists: `BufferSnapshotHandle` + `BufferSnapshotPayload` | partial: `BufferSnapshotMetrics`, `BufferSnapshotStateForTest`, `BufferSnapshotCountersForTest` | tier-2 | 7 | cross-cutting |
| WFR-PLAIN-DISPOSAL | Off-GTK retirement of large owned payloads | 2 files, 2,227 lines (ui 1,535 / model 692) | called by 21 files across 10 workflows | `model/plain_disposal.rs` (1 consumer file, but its consumer is this module's own adapter → cross-cutting, stays) | 4/1/1/2 = 8 fns, 18 sites | exists: `DisposalOwned<T>` + `DisposalPermit` | exists: `DisposalPressureEvidence` | tier-3 | 7 | cross-cutting — see [Cross-Cutting Coordination](#cross-cutting-coordination) |
| WFR-EDITOR-FIND | In-tab find and replace | 3 files, 824 lines (ui) | `Ctrl+F`, `Ctrl+H`, `Ctrl+G`, `Ctrl+Shift+G`, `Escape` | none | 0/0/0/0 = 0 fns, 0 sites | none required — fully synchronous over `GtkSourceSearchContext`; no generation counter, no worker completion, no bundle crossing two boundaries | none needed | tier-1 | 7 | pending |
| WFR-ENCODING | Encoding and line-ending controls | 1 file, 907 lines (ui) | `win.show-encoding-controls`, `win.show-line-ending-controls`, `win.show-file-health` | `model/encoding.rs` (15 consumers → domain, stays) | 0/2/0/0 = 2 fns, 4 sites, 1 override static | none required — dialog surface; the write crosses into `WFR-DOCUMENT-SAVE`, which owns the seam | none needed | tier-1 | 7 | pending |
| WFR-PRINT | Print document | 1 file, 172 lines (ui) | `win.print` | none | 0/0/0/1 = 1 fn, 8 sites | none required — one synchronous snapshot handed to the print runner | exists: `PrintDocumentSnapshot` | tier-1 | 7 | pending |
| WFR-SHELL-LAYOUT | Window shell, tabs, split views, focus mode, zoom | 19 files, 8,449 lines (ui) | `win.toggle-sidebar`, `win.toggle-properties`, `F9`, `win.toggle-focus-mode`, `win.new-tab`, tab actions, breakpoints, resize | none | 1/2/8/0 = 11 fns, 47 sites, 1 override static | none required — allocation-driven geometry with no worker completion seam; `SettleBurst` readiness already carries the pending state | none needed | tier-1 | 7 | pending |
| WFR-STATUS-NOTIFICATIONS | Status lane, inline alerts, notification lifecycle | 6 files, 2,019 lines (ui 887 / services 1,132) | any workflow result, progress heartbeat, inline alert actions | none | 1/0/0/0 = 1 fn, 1 site | none required — owner/surface identity is already a scalar pair validated inside `services/notifications.rs` | none needed | tier-1 | 7 | pending |
| WFR-AUTOMATION-SPINE | Read-only D-Bus automation and action catalog | 5 files, 6,897 lines (ui 2,146 / model 2,195 / services 2,556) | D-Bus method calls, `scripts/lushtext-automation.py` | `model/action_catalog.rs` (3 consumers → domain, stays) | 0/0/2/0 = 2 fns, 2 sites | none required — the exported contract is the value object | exists: 18 `Automation*Snapshot` types; these become projections of workflow evidence as each workflow migrates. **Four projections exist**: `window.content_search` from `SearchPanelEvidence` (slot 1); `window.command_palette` plus both palette readiness blockers from `CommandPaletteEvidence` (slot 2a); `tabs[].saving` from `SaveEvidence` (slot 3a); and `tabs[].load_state` from `LoadEvidence` (slot 3b). `make check-automation-docs` gates all four against the `Evidence Projection Map` in `docs/automation-reference.md`. Since slot 3b, `tabs` is fed by **two** evidence surfaces at once, so the gate attributes a projected field by the local binding it is read through and keys the documented map by evidence type; without that, each surface would appear to project the other's fields | tier-1 | 2a onward, incrementally per migrated workflow | pending |
| WFR-EDITOR-MEMORY | Editor residency and memory policy | 1 file, 469 lines (model) | consumed by editor page load/save and window focus indexing | `model/editor_memory.rs` (7 consumer files across 5 modules and 3 workflows → cross-cutting) | n/a | exists: `EditorResidencyLedger` | none | tier-2 | none | exempt — see [Outlier Resolutions](#outlier-resolutions) |
| WFR-MIGRATION-LEDGER | Sidecar migration ledger | 2 files, 701 lines (model 225 / services 476) | note sidecar rename, startup reconcile | `model/migration_ledger.rs` (5 consumer files across notes, local history, and `services/` → cross-cutting) | n/a | exists: `MigrationLedgerEntry::matches_paths` | none | tier-3 | none | cross-cutting |

## Workflow Stage Traces

These are the ordered stages a reader must follow today to trace each workflow,
with every control-flow inversion marked. An inversion is a point where the
workflow stops being a call chain and resumes from a timer, idle callback,
worker completion, or bounded per-turn continuation. The convention requires a
migrated workflow's facade to narrate these stages and name each resumption
point, so these traces are the input to each facade.

Notation: `→` is a direct call, `⇢` is an inversion with the resumption point
named after it.

### WFR-SEARCH-REPLACE

Search: `Ctrl+Shift+F` → `start_search(spec)` → retirement backpressure gate
(defers when generations are saturated) → `clear_results` → workspace-folder
guard → `WorkspaceSearchFlight::submit` → `spawn_search_request` ⇢ content-search
walker threads, resuming in progress and result callbacks per generation →
`refresh_results_display` ⇢ `SearchRetirementSession`, resuming on later GTK
turns under `SearchRetirementSliceBudget` until rows are released.

Replace: preview button → `enter_preview_mode` → `current_query_spec()` +
`advance_preview_generation()` → `ReplacePreviewRequest` → `spawn_preview_request`
⇢ worker builds rows under `ReplacePreviewBudget`, resuming in a completion
closure that revalidates generation, pending flag, and query spec → Replace All
→ `activate_confirm_replacements` → `spawn_preview_selection` ⇢ worker selects
checked replacements, resuming in a second completion closure → replace callback
into the content-search write path with its undo journal → Undo →
`BufferReplacementTicket` install ⇢ bounded slices across turns.

Six inversions. The two preview completions were the unreified seam.

**Post-migration note.** This trace is the census snapshot, kept as the record of
what the exemplar started from. The migration renamed several of these steps for
intent and split the coordination modules: `clear_results` now delegates to
`detach_visible_results`, `enqueue_search_retirement` became
`retire_detached_results`, `advance_preview_generation` split into
`invalidate_active_preview` plus `issue_preview_ticket`, `retire_preview_state`
became `release_superseded_preview`, and `spawn_preview_selection` became
`apply_checked_replacements` taking one `ReplacePreviewTicket`, and the undo
hand-back became `hand_back_undo_backup` so replace stage 4 is delegated like
the other stages instead of being inlined in the facade. The current
stage order and every resumption point are narrated in the facade,
`crates/lushtext-core/src/ui/search_panel/mod.rs`.

**Post-migration note, slot 2b (replace/undo half).** The Replace All trace now
reads: preview button -> `activate_replace_preview` -> `enter_preview_mode`
(`replace_execution`) -> `issue_preview_ticket` -> `spawn_preview_request` (whose
capacity refusal parks the request on `preview_capacity_wakeup`) -> worker builds
rows under `ReplacePreviewBudget`, resuming in a completion closure that
revalidates the ticket -> `activate_confirm_replacements` ->
`begin_confirmed_replacement`, which claims `journal::begin_replace_transaction`
-> `apply_checked_replacements` -> worker partitions the checked rows, resuming
in a second completion closure -> the window's Replace All callback takes
`journal::take_replace_transaction`, reserves capacity through
`journal::try_reserve_undo_replacement`, calls
`journal::supersede_prior_undo_for_replace`, performs the durable write in
`services/content_search`, then returns through
`journal::record_replace_apply_counts` and either
`journal::publish_undo_journal_for_generation` or
`journal::clear_undo_backup_for_generation`, always ending in
`journal::finish_replace_transaction` -> Undo -> `activate_undo_replacements` ->
`journal::hand_back_undo_backup` -> the window claims the panel with
`journal::begin_undo_restore` (returning `UndoRestoreClaim`) and reports with
`journal::finish_undo_restore`, which reinstalls a remaining journal or clears it
and releases the transaction. Startup recovery is a fifth entry point:
`journal::load_persisted_undo_backup`, which re-enters itself from
`undo_capacity_wakeup` when disposal admission defers it.

Post-split owner of each shared field, because the two Replace All modules are
not state-disjoint: `journal.rs` owns `replace_transaction_pending`,
`replace_transaction_generation`, and `undo_backup_generation`, and
`replace_execution` reads the first two only through
`replace_transaction_claimed` and `replace_transaction_generation_reserved`.

**Twelve** inversions across both stage orders — 2 in the search stage order and
10 in Replace All, counting one per point where control leaves the workflow and
later resumes at a named place. The census recorded six, and the facade narrated
four of the Replace All ten until slot 2b's review caught the gap: the trace's
counts were floors, as slot 2a also found. The facade, `replace_execution.rs`, and
`journal.rs` now agree on twelve.

**Stop semantics at the content-search boundary.** The walker's stop reasons are
now distinguished by owner (`WalkStop` in
`crates/lushtext-core/src/services/content_search/search.rs`). The caller's
`cancel` flag means "discard this flight" and is written only by the panel's
supersede/close path; a result-cap or identity-limit stop ends production
without touching it, so the streaming tick still drains the buffered matches,
the terminating event, and `Done`. Before this separation the cap wrote the
caller's flag, and the panel's `if !cancelled` tick arms silently discarded the
`ResultCap` notice plus every match still in the bounded channel.

### WFR-DOCUMENT-SAVE

**Migrated by slot 3a.** The trace below names the current operations and
modules; the facade at `ui/editor_page/save/mod.rs` narrates the same order in
prose, and this entry is the index into it.

`win.save` / `win.save-as` / close-with-changes →
`save_file_async` | `save_file_async_to_path` | `save_file_async_for_close`
(facade) → `admission::queue_save_request` → refusal gates, save generation
advanced and ownership published → one `QueuedSaveTicket` built →
`admission::submit` → `SaveAdmissionPolicy::queue` → `schedule_drain()`
⇢ **(1) `glib::idle_add_local_once`, resuming in `admission::drain`** →
`policy::queued_save_is_current(&ticket, &facts)` retirement pass →
`SaveAdmissionPolicy::admit_next` → `execution::begin_admitted_save` →
ticket revalidated → `SaveViewInteractivity::suspend` →
`SaveCompletionTicket::capture` → buffer capture, either
`snapshot_buffer_text_direct` or ⇢ **(2) chunked async capture, resuming in the
snapshot callback** → `execution::write_snapshot_async` →
`spawn_blocking_then` ⇢ **(3) worker write through `editor_io` and
`filesystem::write::atomic_replace`, resuming in the completion closure** →
`SaveCompletionTicket::is_current(editor)` →
`policy::classify_saved_text` → when formatting rewrote the text, ⇢ **(4) bounded
buffer replacement, resuming in its terminal callback** (the only place the tab
is marked clean on that path) → `finish_accepted_save` →
`SavePayloadPermit` drop ⇢ **(5) `idle_add_once`, resuming in
`admission::release_on_main`** → window-side notifications, draft cleanup,
`adopt_saved_destination` for Save As, accessibility refresh.

**Five inversions, not four.** The census recorded four; slot 2a's finding that
census inversion counts are floors holds again here. The fifth is the mirror-back
inversion through the bounded buffer-replacement workflow, which the pre-migration
trace folded into "save-formatting acceptance and buffer mirror-back" as if it
were a straight-line step. It is the inversion that most needed naming: it is
where a clean tab and the bytes on disk are reconciled, and skipping it would
show a clean tab whose visible text differs from the file.

**Shared-field owners a reader needs at this seam** (full table in the facade's
module doc): `cancel_load` and all `imp().load*` state belong to
`WFR-DOCUMENT-LOAD` (slot 3b); the restore-position group is cross-cutting with
five owning workflows and save never touches it; `size_check`, `file_path`, and
`canonical_file_path` are shared editor-page document identity; the chunked
threshold belongs to `WFR-BUFFER-SNAPSHOT` (slot 7); local history and drafts
belong to slot 4.

### WFR-DOCUMENT-LOAD

**Current, after slot 3b.** `win.open-file`, `win.open-recent`, `Ctrl+O`,
`Ctrl+K`, sidebar activation, or session restore → `open_document(path)` →
`document_identity::set_file_path_for_pending_load` → tab creation →
`connect_load_completed_once` / `connect_load_failed_once` →
`load_file_async(path)` → `admission::begin_load_request` → park, or rotate
identity into one `LoadRequestTicket` → `spawn_blocking_then` ⇢ **planning
worker**, resuming in the planning completion closure → `admission::submit` ⇢
**idle drain**, resuming in `admission::drain` (⇢ **disposal-capacity wakeup**
when the disposal lane has no room, resuming in `admission::schedule_drain`) →
`admission::dispatch` → `spawn_blocking_then` ⇢ **read/decode worker**, resuming
in `execution::accept_admitted_load_outcome`, where a stale load is refused →
`policy::requires_chunked_install` → `execution::install_loaded_direct`, or
`execution::start_chunked_install` ⇢ **bounded install slices** ending on
paragraph boundaries, resuming per slice in `execution::run_install_slice` →
`execution::complete_loaded_installation` → `execution::finish_load_finalization`
⇢ **charge release**, resuming in `admission::release_on_main`.

When the workflow is asked to stop: `retirement::cancel_load` or
`retirement::dispose_load_resources` → `retirement::abort_installation` ⇢
**cancelled-clear slices**, resuming per slice in
`retirement::run_cancelled_clear_slice`, which is where a cancelled load's
partial content is cleared and where a parked request restarts.

**Seven inversions, not the four the census recorded** — the planning worker, the
idle drain, the disposal-capacity wakeup, the read/decode worker, the per-slice
install resumption, the cancelled-clear resumption, and the charge release. The
census counted the read worker, the drain, the install slices, and finalization.
That is the fourth confirmation that **census inversion counts are floors, not
totals**.

Shared-field owners are recorded in the facade's "State this workflow shares
with others" table: the restore-position group lives in
`ui/editor_page/restore_position.rs` (five owning workflows) and document
identity and metadata in `ui/editor_page/document_identity.rs`; load calls both
and owns neither.

### WFR-DRAFT-RECOVERY

**Current, after slot 4.** Three stage orders over one durable record.

**Autosave.** First dirty edit ⇢ **(1) 750 ms `SupersedingTimer`**, or ⇢ **(2) the
5 s repeating tick**, both resuming in `autosave_execution::autosave_tick` →
`policy::autosave_admission` marks pending rather than queueing when the lane is
owned → `collect_dirty_draft_candidates` with
`policy::draft_candidate_is_eligible` (whose `installation_incomplete` term is
slot 4's confirmed data-safety fix) → one `DraftMutationIntent` per candidate,
assigned **before** any document-sized work → `drive_dirty_draft_pipeline` ⇢
**(3) chunked snapshot**, resuming in `finish_snapshot` which re-validates with
`policy::captured_snapshot_is_current` ⇢ **(4) body worker**, resuming in a
completion that admits the **next** candidate → `commit_dirty_draft_pipeline` ⇢
**(5) manifest worker** → accept per completion under matching generation and
`DraftMutationOrder::is_current`.

**Close flush.** `flush_dirty_drafts_async` ⇢ **(6) lane-drain poll** ⇢ **(7)
chunked snapshot** ⇢ **(8) body worker** ⇢ **(9) manifest worker** ⇢ **(10)
`wait_for_draft_mutations_then` poll**, then the caller's `on_done`. The
synchronous `flush_dirty_drafts` remains as the deliberate blocking variant for
process exit, but **no production path reaches it**: `ui/window/dialogs.rs` closes
through `flush_dirty_drafts_async`, and the synchronous entry point is currently
exercised only by widget tests. Whether to retire it or give it a caller is an
open question this row records rather than answers.

**Restore.** Startup hands over the records through
`adopt_startup_draft_records` → `take_preloaded_draft` moves one eager body under
a **replacement** reservation, or demotes every eager body to a compact marker →
`queue_lazy_draft_restore` → `drive_lazy_draft_restore_queue` ⇢ **(11) capacity
wakeup** ⇢ **(12) body-resolve worker**, resuming in `finish_draft_restore` →
`draft_restore_is_current(ticket, facts)` → `apply_draft` ⇢ **(13) bounded
buffer-replacement terminal**, resuming in the closure that calls
`finish_applied_draft`.

**Deletes and cleanup.** `delete_draft_by_id` → intent, tombstone, queue →
`drive_pending_draft_mutations` ⇢ **(14) delete worker**, body first and manifest
only if that succeeded. `schedule_orphan_cleanup` ⇢ **(15) start timer** →
`run_orphan_cleanup_pass` ⇢ **(17) cleanup worker** (manifest lock → trusted
reload → target guard → **inode recheck** → delete) → `finish_orphan_cleanup_pass`
→ `policy::orphan_cleanup_follow_up` ⇢ **(16) backoff timer**.

**Seventeen deferred inversions, where the census recorded seven worker
handoffs.** The ten it missed are two autosave timers, three main-loop polls, one
capacity wakeup, two chunked snapshots, the replacement terminal, and one of the
two cleanup timers — the census counted only `spawn_blocking_then` sites. **A slot
that counts only worker handoffs will undercount by roughly two thirds.** Two
ownership hand-outs: `flush_dirty_drafts_async`'s `on_done` and the guarded
body's `on_complete`.

**Roles.** Facade `mod.rs`; coordination `journal`, `admission`,
`autosave_execution`, `restore_execution`, **`retirement`**; seam value objects
`seams.rs`; pure policy `policy.rs`; evidence `evidence.rs`; test-only
`test_policy.rs`. Two notes for later slots:

- **`retirement` is the fifth coordination module and was added by review.** The
  facade originally held `release_eager_preloads` and `detach_eager_preload_bodies`
  inline. That is deferred off-GTK destruction of a document-sized payload through
  the disposal lane, which is exactly the `retirement` bounded role name, so
  keeping it in the facade made the facade own mechanism. Moving it out took the
  facade from 310 to **279** lines. If a facade is holding something that a
  bounded role name already describes, that is the signal.
- **`seams.rs` is the programme's first dedicated seam value-object module**, and
  the name is now precedent. Earlier migrated rows kept their seam values beside
  the coordination module that constructed them; this workflow has enough of them
  (a Ticket/Facts pair with its predicate, a main-thread intent, a
  candidate/completion/accepted triple, and two worker-boundary payloads) that a
  single home reads better. Use `seams.rs`, not a new invented filename.

### WFR-SESSION-RESTORE

**Current, after slot 4.** Two stage orders over one durable record.

**Persist.** `collect_session` ⇢ **(1) 500 ms debounce**, resuming in a closure
that re-checks both restore state and descriptor readiness ⇢ **(2) worker write**,
resuming in a completion that records or clears failure against the debounce
generation. `save_session_for_close_async` inverts the same way ⇢ **(3)**, and
switches to `collect_session_for_close`, which layers not-yet-admitted descriptors
over the mounted pages — unless startup descriptors are still pending, in which
case the persisted file is loaded and **merged** rather than overwritten, and the
close is *refused* if recovery evidence cannot be preserved.

**Restore.** `load_session_and_drafts` → publish a cancel token → reserve
progress-disposal capacity for the preload graph, or ⇢ **(4) capacity wakeup**,
resuming in `journal::start_startup_journal_read` against the same token ⇢ **(5)
worker read** (`load_restore_state_cancellable` +
`policy::fit_startup_preloads_to_reservation`), resuming in the completion that
hands the draft records to `WFR-DRAFT-RECOVERY` and calls `begin_session_restore`
→ `SessionRestorePolicy::new` ⇢ **(6) per-turn `idle_add_local_once`**, resuming
in `admission::run_scheduled_session_restore_turn` → `plan_turn` bounded at 4
pages and 2 planning permits → `execution::mount_restored_page` per admission →
`load_file_async_with_planning_terminal` ⇢ **(7) the load workflow's planning
terminal**, resuming in `release_session_restore_plan_permit` → re-arm while
`needs_next_turn()`, else `finish_session_restore` →
`execution::settle_restore_selection` (user-first) → one terminal projection
publication → `evidence::record_restore_outcome`.

**Seven deferred inversions, where the census recorded one.** Inversion 7 is the
one the whole sequencer is built around: `release_permit` counts exactly those
releases, so every load terminal must either carry a parked request's planning
owner into a restart or release it — the contract slot 3b fixed and handed here.
One ownership hand-out: `save_session_for_close_async`'s `on_done`.

### WFR-LOCAL-HISTORY

**Current, after slot 4.** Two stage orders in two directories, over one sidecar
record.

**Capture** (`ui/editor_page/local_history.rs`, the called surface). A clean
document becomes modified → availability, path, and suppression checks →
`capture_local_history_baseline` acquires the process-wide permit or enqueues a
**weak** waiter → `BaselineCaptureTicket` ⇢ **(1) worker capture**, resuming in a
**failure-only** completion validated by `policy::baseline_capture_is_current`.
Permit `Drop` ⇢ **(2) `MainContext::invoke`**, resuming in the waiter drain.
`schedule_local_history_periodic_capture` ⇢ **(3) `SupersedingTimer`** →
`run_local_history_periodic_capture` ⇢ **(4) chunked snapshot** →
`persist_periodic_snapshot_if_current` (`policy::periodic_capture_is_current`) ⇢
**(5) worker capture**, resuming in a reschedule governed by
`policy::should_reschedule_periodic_capture`.

**Browse and restore** (`ui/window/local_history/`, the canonical home).
`show_local_history_dialog` or `show_local_history_for_path` ⇢ **(6)/(7) listing
workers** → `policy::filter_visible_snapshots` →
`preview_execution::present_local_history_browser`. Selection →
`LocalHistoryPreviewCoordinator::submit` → `start_preview_load` ⇢ **(8) capacity
wakeup** ⇢ **(9) body-read worker**, resuming in `finish_preview_load` which
accepts only the current generation and starts any queued latest →
`policy::preview_install_plan` ⇢ **(10) install slice**, resuming in
`run_preview_install_slice` against the cross-cutting paragraph boundary.
Restore ⇢ **(11) restore capacity wakeup** ⇢ **(12) chunked undo capture** ⇢
**(13) restore-safety worker** — the **current** buffer is persisted as a
`RestoreSafety` snapshot *before* the replacement starts — → `replace_buffer_bounded`
⇢ **(14) replacement terminal**. Undo ⇢ **(15) its own replacement terminal**.
Rename ⇢ **(16) lineage migration worker**.

**Sixteen deferred inversions, where the census recorded six.** Its `Entry
points` cell also omitted the **sidebar context-menu path** and the
**Save-origin capture driven by migrated `WFR-DOCUMENT-SAVE`**; both are corrected
in the row.

### WFR-BUFFER-REPLACEMENT

**Current, after slot 4.** Caller builds one `BufferReplacementRequest` →
`replace_buffer_bounded` (facade) → `execution::accept_request` → any live
session is cancelled as `Superseded`, then `policy::start_disposition` decides
begin-now versus park-as-pending (and a displaced pending request's terminal
fires immediately) → `execution::begin` → `BufferReplacementPlan::for_sizes`
(cross-cutting) → the projection/editability guard → `begin_irreversible_action`
→ either `run_direct` in one turn, or ⇢ **(1) `glib::timeout_add_local_once(1ms)`,
resuming in `run_turn`, which dispatches by phase** → clear turns using
`next_clear_char_count` and a line-start-extended deletion, then
`policy::after_clear_slice` → install turns using the cross-cutting
**paragraph-boundary** `next_replacement_boundary`, then
`policy::insertion_is_complete` → on cancellation, `policy::cancel_disposition`
decides finish-now versus a bounded cancelled-clear pass, which
`policy::turn_may_run` deliberately allows to run even after the caller's own
freshness check goes stale → `finish_session` →
`policy::terminal_is_complete`, `policy::guard_restores_on_terminal`,
`evidence::record_terminal` → the caller's terminal callback →
`release_owner_and_start_pending`.

**One deferred inversion, three phase resume points, and four ownership
hand-outs.** The census said "one inversion, already reified" and slot 4 confirms
it — **the only one of slot 4's four rows whose census inversion count needed no
correction.** What the census understated is the resume points (`run_clear_turn`,
`run_install_turn`, `run_cancelled_clear_turn`, all reached through `run_turn`)
and the ownership hand-outs: the caller's terminal callback, the guarded body's
cancel return, the guarded body's completion return, and the synchronous eviction
of a displaced pending request.

**Its caller set was wrong in the census, and the correction matters**: "Replace
All undo" is **not** a caller. `BufferReplacementWorkflow::LocalHistoryUndo` is
local history's own undo affordance. See the row's `Entry points` cell for the
verified five call sites across four owning workflows.

### WFR-WORKSPACE-TREE

Folder add, row expand, or refresh → `WorkspaceScanTicket` plus lifetime
generation → `spawn_blocking_then(scan_directory)` ⇢ worker scan, resuming
against `WorkspaceScanTicket::is_current(ticket, lifetime)` →
`apply_scanned_children` → expansion-state restore ⇢
`schedule_child_state_restore` deferred callbacks that must read the live
expansion set at apply time rather than a snapshot taken at schedule time.

Watch: target computation → install worker ⇢ resuming against a loose
`(generation, lifetime)` comparison → mailbox reconcile ⇢ targeted in-place
refresh.

Five inversions. The watch-install resumption is the unreified seam. The
deferred-restore inversion is the one most easily read wrong, because a stale
snapshot there resurrects a user's collapse.

### WFR-NOTES-BOOKMARKS

`win.notes-show-notes` → browser dialog → mode selection →
`NoteSourceRefreshCoordinator` bounded source build under `NoteSourceAdmission`
⇢ worker build, resuming against the generation-plus-mode-plus-disposed triple →
query change → `PaletteSearchCoordinator` ⇢ worker query, resuming against the
same triple → row publish → closed-file bookmark excerpts ⇢ worker preview.

Rename: `migrate_note_sidecars_after_rename` → migration ledger entry ⇢
`reconcile_pending_migrations_on_startup`, resuming on a later app launch. This
is the longest-lived inversion in the codebase: control resumes in a different
process run.

Four inversions. The duplicated generation-plus-mode triple is the unreified
seam.

### WFR-COMMAND-PALETTE

Query: `Ctrl+Shift+P` → `open()` → mode selection → query changed → `Debounce` ⇢
debounce fires → `PaletteSearchCoordinator::submit` → `PaletteSearchStart` →
`query_execution::dispatch_query_worker` ⇢ worker search over `FileIndex`,
resuming against `is_current(generation)` → row publish, and a retained latest
request starts from inside that completion.

Index: build request → `FileIndexBuildCoordinator` with `FileIndexBuildLedger`
⇢ worker build → full replacement. Separately, incremental mutation: bounded
queue admission → 75 ms flush debounce ⇢ flush → disposal-capacity reservation,
which on refusal arms the capacity wakeup ⇢ retry → batch dispatch under
`FileIndexMutationTicket` ⇢ worker mutation through `FileIndexMutationLedger` →
arbitration, which on loss re-arms the flush debounce ⇢ replay through a
rebuild, then a tail flush when the queue refilled ⇢ retirement of the released
index.

**Eight inversions across the two stage orders**, three of them (the two
debounces and the disposal-capacity wakeup) timer- or wakeup-driven rather than
coordinator-guarded. The pre-migration trace recorded "five inversions, all
coordinator-guarded", which under-counted; slot 2a corrected it from the code.

### WFR-MARKDOWN-PREVIEW

`Alt+P` or buffer change → `render_markdown` → buffer snapshot →
`render_markdown_generation` → plan under `MarkdownPlanLimit` ⇢ worker
preprocess, resuming against `MarkdownRenderSession::is_current(generation)` →
`render_event_batch` ⇢ bounded install batches across turns → image decode under
`MarkdownImageAdmission` ⇢ decode worker → anchored block width repair ⇢ idle
plus layout-settle after the shell transition completes.

Five inversions. Seam already reified; facade and evidence surface missing.

### WFR-MINIMAP

`win.toggle-minimap` or buffer/viewport change → availability classification
from the O(1) live-buffer byte estimate → `MinimapAnalysisSession` carrying
generation and lifetime ⇢ sliced cancellable GTK iterator resuming per turn →
marker collection → `queue_minimap_draw` → native geometry sync ⇢ `SettleBurst`
reflow settle after a shell transition, optionally holding already-rendered
native pixels under `RenderHoldOverlay` until the settled repair and a quiet
repaint window complete.

Three inversions, one of which is frame-timing sensitive and pixel-verified.
That combination is why this row is deferred to slot 6.

### WFR-BUFFER-SNAPSHOT

Caller requests capture → `BufferSnapshotAdmission` reserve → direct capture for
admitted small buffers, or `ChunkedSnapshotSession` ⇢ chunk capture resuming per
turn, with coalescing and final destruction performed on a worker under the same
permit → `BufferSnapshotPayload` handed to the caller's guarded workflow.

One inversion. Consumed by save, draft autosave, encoding analysis, preview, and
local history, which is why it is cross-cutting.

### WFR-PLAIN-DISPOSAL

Workflow wraps a payload in `DisposalOwned<T>` → `PlainDisposalAdmission`
reserve → disposal terminal registered ⇢ payload destruction on a worker lane,
resuming through `idle_add_once` wakeup → `DisposalPressureEvidence` updated.

One inversion, serving 10 workflows.

### Workflows with no inversion

`WFR-EDITOR-FIND`: `Ctrl+F` → reveal bar → entry changed → synchronous
`GtkSourceSearchContext` scan → count label → navigate.

`WFR-ENCODING`: action → grouped dialog rows → user choice → hand-off to
`WFR-DOCUMENT-SAVE` or `WFR-DOCUMENT-LOAD`, which own the seam.

`WFR-PRINT`: `win.print` → `PrintDocumentSnapshot` → print runner.

`WFR-STATUS-NOTIFICATIONS`: workflow result → owner-and-surface identity in
`services/notifications.rs` → status lane or inline alert → `SupersedingTimer`
pulse. The timer re-arms a visual effect rather than resuming a workflow, so it
is not an inversion in the sense used here.

`WFR-AUTOMATION-SPINE`: D-Bus call → snapshot gather → typed reply.

`WFR-SHELL-LAYOUT`: action or breakpoint → property set → `size_allocate` clamp
→ `SettleBurst`-gated notify handlers → persistence only on explicit intent or
settled animation. This row is a residual grouping of 19 shell surfaces that
share the window adapter and have no coordination seam; slot 7 may split it if
the facade work shows it holds more than one story.

## Measurement Definitions

Several baseline numbers in this programme have more than one defensible
denominator. The census fixes the definitions so later changes can show
progress against a stable number.

| Quantity | Definition used here | Value |
| --- | --- | --- |
| Test seam attribute sites | `#[cfg(feature = "test-utils")]` attribute occurrences in `crates/lushtext-core/src` | 639 |
| Test seam functions | `fn *_for_test` definitions in `crates/lushtext-core/src` | 351 |
| Externally reachable seam functions | `pub fn` + `pub(crate) fn` + `pub(super) fn` `*_for_test` | 300 (277 + 23) |
| Configuration override statics | test-gated `static` whose name carries a delay, limit, max, slice, override, or injected-failure role | 45 |
| Long signatures, strict | production `fn` with 6 or more non-receiver parameters, `crates/lushtext-core/src` | 43 |
| Long signatures, receiver-counted | production `fn` with 6 or more parameters counting `&self`, i.e. 5 or more non-receiver | 88 |
| Argument-count suppressions | `#[expect(clippy::too_many_arguments)]` in the workspace | 1 (was 2; slot 3a removed the workflow-code one) |
| `exclude_re` entries | entries in `.cargo/mutants.toml` | 71 (50 services-scoped, 20 ui-scoped, 1 unscoped) |
| Minimap mutation exclusions | ui-scoped `exclude_re` entries naming minimap methods | 14 entries, 66 method names, 17 physical TOML lines |
| Internal snapshot types | `pub struct *Snapshot` excluding `Automation*Snapshot` | 33 |
| Automation snapshot types | `pub struct Automation*Snapshot` | 18 |
| Existing evidence types | `struct *Evidence` | 4 |

The programme baseline of "90 production functions take six or more
parameters" corresponds to the **receiver-counted** definition (88 measured).
The strict non-receiver definition yields 43. Later changes MUST state which
definition they are reporting against.

## Policy Module Census

Every module in `crates/lushtext-core/src/model/` was checked, not only the
eight known mechanism modules. Consumer counts are files that reference the
module, excluding the module itself.

### Mechanism-named modules confirmed by the census

| Module | Lines | Consumer files | Owning workflows | Classification |
| --- | --- | --- | --- | --- |
| `save_admission.rs` | 405 | the retired ui/editor_page/save_runtime.rs, the retired ui/editor_page/load_save.rs | 1 (`WFR-DOCUMENT-SAVE`) | **relocated by slot 3a** to `ui/editor_page/save/policy.rs`. The census target read ui/editor_page/policy.rs, which could not be right — that is one file for the eight workflows the directory hosts — so slot 3a corrected it to the per-workflow subdirectory. The consumer list was also short: `model/mod.rs`, `crates/lushtext-core/benches/benchmarks.rs`, and the widget tests referenced it too, and both external consumers are why a precisely scoped `pub` subset survives the move |
| `search_flight.rs` | 191 | `ui/search_panel/imp.rs`, `runtime.rs` (both in `ui/search_panel/`) | 1 (`WFR-SEARCH-REPLACE`) | single-consumer → relocates to `ui/search_panel/policy.rs` |
| `search_retirement.rs` | 80 | `runtime.rs` (in `ui/search_panel/`) | 1 (`WFR-SEARCH-REPLACE`) | single-consumer → relocates to `ui/search_panel/policy.rs` |
| `minimap_analysis.rs` | 186 | `ui/editor_page/minimap.rs` | 1 (`WFR-MINIMAP`) | single-consumer → relocates to `ui/editor_page/minimap/policy.rs` |
| `plain_disposal.rs` | 692 | `ui/plain_disposal.rs` | its own adapter, serving 10 workflows | cross-cutting → stays |
| `buffer_replacement.rs` | 186 total, **93 production** | `ui/editor_page/buffer_replacement/execution.rs`, `ui/editor_page/load/policy.rs`, `ui/window/local_history/policy.rs` and `ui/window/local_history/preview_execution.rs`, and `model/file_load.rs` (a one-line delegating synonym `next_install_boundary`, which `ui/editor_page/load/execution.rs`, `benches/benchmarks.rs`, and `tests/properties/file_load.rs` call) | **3** owning workflows consume the pure module (`WFR-BUFFER-REPLACEMENT`, `WFR-DOCUMENT-LOAD`, `WFR-LOCAL-HISTORY`), and **4** own the five `replace_buffer_bounded` call sites: `WFR-DRAFT-RECOVERY`, `WFR-LOCAL-HISTORY` (restore and undo), `WFR-EDITOR-MEMORY` (exempt, no slot), `WFR-DOCUMENT-SAVE` (migrated) | cross-cutting → stays. **Slot 4 corrected this row, which read `2 (WFR-LOCAL-HISTORY, Replace All undo)` and was wrong in both halves: the count was a consuming-file count rather than an owning-workflow count, and **Replace All undo is not a consumer at all** — `BufferReplacementWorkflow::LocalHistoryUndo` is local history's own undo affordance, not the search/replace journal's.** The paragraph-boundary arithmetic has exactly one implementation and must not be duplicated; see task 2.1's alias decision |
| `editor_memory.rs` | 469 | `ui/window/focus_indexing.rs`, `ui/window/imp.rs`, `ui/editor_page/mod.rs`, `ui/editor_page/save/admission.rs`, the retired ui/editor_page/load_runtime.rs | 3 | cross-cutting → stays, exempt |
| `migration_ledger.rs` | 225 | `ui/window/notes/mod.rs`, `ui/window/local_history/journal.rs`, `services/migration_ledger.rs` | 2 plus a service | cross-cutting → stays |

**Post-migration note.** This table is the census snapshot, kept as the record
of what each classification was decided from; its line counts and consumer
names are as-censused and are deliberately not rewritten. Two rows have since
been acted on. `search_flight.rs` and `search_retirement.rs` are gone from
`model/`: both relocated into `ui/search_panel/policy.rs`, with mutation-coverage
parity recorded in
`openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`.
Their consuming `runtime.rs` is also gone, split by the same migration into
`ui/search_panel/execution.rs` (streaming search) and
`ui/search_panel/retirement.rs` (bounded disposal). See
[Migrated Workflow Roles](#migrated-workflow-roles) for the current shape.

### Additional single-workflow modules the census found

These were not in the known set of eight and must be considered by their
workflow's migration change.

| Module | Lines | Consumer files | Classification |
| --- | --- | --- | --- |
| `workspace_scan.rs` | 231 | 3, all in `ui/sidebar/workspace_section/` | single-workflow → relocates with `WFR-WORKSPACE-TREE` |
| `workspace_persistence.rs` | 338 | 2, both workspace sidebar | single-workflow → relocates with `WFR-WORKSPACE-TREE` |
| ~~`workspace_search.rs`~~ | 503 | the census cell said "2, both search" and undercounted | **resolved by slot 2b: it is domain and stays in `model/`.** See [Modules confirmed as domain and staying in `model/`](#modules-confirmed-as-domain-and-staying-in-model) |

~~`model/file_load.rs` (4 consumers) is domain-shaped but sits close to the
boundary; `WFR-DOCUMENT-LOAD`'s migration in slot 3 must decide it explicitly
rather than inheriting this row.~~ **Decided by slot 3b: it stays in `model/`.**
See [Modules confirmed as domain and staying in `model/`](#modules-confirmed-as-domain-and-staying-in-model).
**This decision is closed; a later slot must not re-open it.**

### The recent-documents surface census gap

**`ui/open_popover/**` and `ui/window/recent_open.rs` appear in no row's file
set at all** — not this row's, not `WFR-SHELL-LAYOUT`'s, not any other's. Slot 3b
found this while deciding `OpenPopoverRowLayoutSnapshot`, which the census had
listed in `WFR-DOCUMENT-LOAD`'s `Evidence surface` cell.

The resolution splits along the coordination/presentation line the convention
already uses elsewhere:

| Concern | Owner |
| --- | --- |
| Opening a recent document (`win.open-recent`, `Ctrl+K`) as a **load entry point** | `WFR-DOCUMENT-LOAD`, migrated by slot 3b. The entry point is `load_file_async`, reached through `open_document` |
| The recent-documents **list surface**: popover row layout, row geometry, the list's own loading state (`recent_documents.loading`), and `OpenPopoverRowLayoutSnapshot` | **`WFR-SHELL-LAYOUT` (slot 7)**, newly assigned here. It is window-shell presentation, not load state: a row-layout snapshot answers "how is this row laid out", which no load stage asks |

`OpenPopoverRowLayoutSnapshot` therefore **stays in `ui/open_popover/mod.rs`**
and is **not** folded into `LoadEvidence`. Folding it in would have put popover
geometry inside a workflow surface whose contract is the load lifecycle, and
would have made `WFR-DOCUMENT-LOAD`'s migration a false claim about files it does
not own.

This is recorded the way slot 2a recorded that `ui/window/focus_indexing.rs` had
been attributed to the palette row while remaining window code: **the attribution
was wrong, the files are now assigned, and slot 7 inherits them rather than
rediscovering the gap.** Slot 7 also inherits the one ungated test read this left
in place, `window.imp().recent_documents.loading` in
`crates/lushtext/tests/widget/open_popover.rs`.

### Modules confirmed as domain and staying in `model/`

`automation.rs`, `content_search.rs`, `workspace.rs`, `palette.rs`,
`encoding.rs`, `draft.rs`, `action_catalog.rs`, `session.rs`, `note.rs`,
`recent_document.rs`, `bookmark.rs`, `local_history.rs`, `sidecar_identity.rs`,
`formatting_overrides.rs`, `folder_note.rs`, `document_note.rs`. Each names a
domain concept and has three or more consumers, or is a domain type with a
single natural owner.

**`workspace_search.rs` (503 lines) joins this list — decided by slot 2b.** The
census listed it above as a single-workflow module awaiting a relocation
decision on the strength of "2 consumers, both search". Slot 2b re-derived the
reference set and it is larger, and it forbids the move:

| Consumer | What it uses |
| --- | --- |
| `services/content_search/search.rs` | imports `WorkspaceSearchFallbackClaim`, `WorkspaceSearchFallbackLedger`, `WorkspaceSearchFallbackLimits`, `WorkspaceSearchFallbackMetrics`, `WorkspaceSearchTraversalPlan`, and names `WorkspaceSearchIncompleteReason` |
| `model/content_search.rs` | embeds `WorkspaceSearchFallbackMetrics` and `WorkspaceSearchIncompleteReason` in public enum variants |
| `ui/search_panel/execution.rs` | imports `WorkspaceSearchTraversalPlan` |
| `crates/lushtext-core/benches/benchmarks.rs` | addresses `WorkspaceSearchFallbackMetrics` and `WorkspaceSearchTraversalPlan` directly |
| `crates/lushtext-core/tests/workspace_terminology.rs` | names the file path literally |

A service and a `model/` sibling both depend on it, so relocating it under
`ui/search_panel/` would invert dependency direction (`services -> ui`), which
the convention forbids outright. It is already pure (no GTK-family import),
already mutation-scoped through `model/**`, and already carries co-located unit
tests, so the relocation would trade a dependency-direction violation for
nothing. **This decision is closed; a later slot must not re-open it.**

**`model/file_load.rs` (462 lines: 279 production, 183 co-located tests) joins
this list — decided by slot 3b.** The census recorded it as "4 consumers,
domain-shaped but sits close to the boundary" and deferred the decision to this
slot. The re-derived reference set is **6 production files**, and it forbids the
move outright:

| Consumer | Layer | What it uses |
| --- | --- | --- |
| `model/mod.rs` | model | the module declaration |
| **`services/editor_io.rs`** | **services** | `transient_load_weight`, `FileLoadFacts`, `FileLoadPlan` — a **service depending on it**, so relocating under `ui/` would invert dependency direction (`services -> ui`), which the convention forbids outright |
| `ui/plain_disposal.rs` | ui | `decoded_body_reservation_weight` and the shared transient budget — `WFR-PLAIN-DISPOSAL` (cross-cutting, slot 7) |
| `ui/editor_page/load/admission.rs` | ui | the admission policy, request, snapshot, priority, and reservation weight — `WFR-DOCUMENT-LOAD` |
| `ui/editor_page/load/policy.rs` | ui | `SYNCHRONOUS_INSTALL_THRESHOLD_BYTES` — `WFR-DOCUMENT-LOAD` |
| `ui/editor_page/save/policy.rs` | ui | the shared transient budget constant — `WFR-DOCUMENT-SAVE` |

Plus 3 test/bench files. **The consumer count was low and the layer spread was
the point**: its `ui/` consumers span **three** owning workflows
(`WFR-PLAIN-DISPOSAL`, `WFR-DOCUMENT-LOAD`, `WFR-DOCUMENT-SAVE`) — one more than
authoring expected, because slot 3a's relocation of `save_admission.rs` into
`ui/editor_page/save/policy.rs` added a `ui/` consumer. Cross-cutting eligibility
counts **owning workflows**, so three owners clears the bar without the service
argument even being needed.

It is already pure, already mutation-scoped through `model/**`, and already
carries co-located unit tests, so the move would trade a dependency-direction
violation for nothing. **This decision is closed; a later slot must not re-open
it.**

**Six grep false-positive families, recorded so the overcount is not
re-derived.** An earlier authoring pass reported nine `ui/` consumers; six were
name collisions with unrelated symbols rather than references to this module:
`file_load_active` in `ui/automation.rs`; `connect_file_loaded` in
`ui/window/notes/mod.rs` and `ui/window/focus_indexing.rs`;
`file_loaded_callbacks` in `ui/editor_page/imp.rs` and `ui/editor_page/mod.rs`;
and a test function name in `ui/window/drafts/`. Match on the import or path,
never on the substring `file_load`.

After the relocations above, `model/` retains 22 of its current 29 files. The
first two have happened: `model/` now holds 27 files, and the two search policy
modules live in `ui/search_panel/policy.rs`.

## Test Seam Census

The four seam kinds and their dispositions come from
`openspec/specs/workflow-evidence-surfaces/spec.md`. The classification below
partitions the **351 `fn *_for_test` definitions**, which is the population that
consolidation acts on. The 639 attribute sites are a different denominator: one
gated `impl` or `mod` block can cover many functions, and many sites gate
struct fields, imports, or in-body hook calls rather than seams.

| Kind | Functions | Disposition |
| --- | --- | --- |
| Inspection | 139 | Consolidate into the workflow's evidence surface |
| Configuration | 96 functions plus 45 override statics | Collapse into one per-workflow test policy value |
| Actuation | 98 | Deferred; each is a missing workflow/presentation boundary |
| Probe / reset | 18 | Retain; no non-test equivalent exists |

The programme's original table recorded inspection as 351 with 300 public. That
351 is the **whole** `_for_test` population, and its 45-configuration and
~150-actuation rows are subsets of the same population rather than disjoint
additions. The partition above supersedes it. Per-workflow counts are in the
[Product Matrix](#product-matrix).

## Seam Value Objects

The repository already contains the target idiom in two shapes, so the
convention is being named and completed rather than invented.

**Ticket plus Facts plus predicate.** A `*Ticket` captures the expectation at
dispatch, a `*Facts` captures observed live state at completion, and a
`*_is_current(ticket, facts)` predicate validates them as a unit. Existing
instances: `BaselineCaptureTicket` + `BaselineCaptureFacts`,
`PeriodicCaptureTicket` + `PeriodicCaptureFacts`, `DraftRestoreTicket` +
`DraftRestoreFacts`. The variant `Ticket::is_current(&editor)` reads live state
directly: `SaveCompletionTicket`, `LocalHistoryReplacementTicket`.

**Coordinator generation identity.** A coordinator owns the generation and
exposes `is_current(generation)`: `SingleFlightCoordinator`,
`MarkdownRenderSession`, `LocalHistoryPreviewCoordinator`,
`FileIndexBuildCoordinator`, `NoteSourceRefreshCoordinator`,
`WorkspaceScanTicket::is_current`.

Three seams remain unreified. Each is named here so its migration change does
not have to rediscover it.

### done: `SessionRestorePlanPermit` + `SessionRestoreAdmission` (`WFR-SESSION-RESTORE`)

**Re-audited, not newly reified.** Both existed and both pass the two-boundary
rule. They live in `ui/window/session_restore/policy.rs`, so the identity types
are GTK-free.

`SessionRestorePlanPermit` carries `{generation, id}` and is the sequencer's whole
basis: it is reserved by `plan_turn`, travels through page mounting into the load
workflow's planning terminal, and comes back to `release_permit` — three module
boundaries. Its `generation()` accessor is what makes a **stale** generation's
terminal un-countable rather than merely late, and slot 4 added the test that
pins it: every prior assertion compared a permit's generation against the
policy's, so both could have reported the same wrong value.

`SessionRestoreAdmission` bundles `{ordinal, tab, permit}` — one descriptor
admitted for this turn together with the permit it reserved. It is constructed
once by the planner and consumed once by the mounter, and keeping the permit
*inside* it is what makes a file-backed admission without a permit
unconstructible.

No value is renamed while crossing these seams.

### done: `BaselineCaptureTicket` / `PeriodicCaptureTicket` + Facts, and `LocalHistoryReplacementTicket` (`WFR-LOCAL-HISTORY`)

**Re-audited, and relocated to the workflow's one `policy.rs`.** All three
existed; what slot 4 changed is *where they live*. The two capture Ticket/Facts
pairs were defined in `ui/editor_page/local_history.rs`, the called surface, and
now live in the canonical `ui/window/local_history/policy.rs` — so the capture
surface **calls** its own workflow's policy rather than defining a private copy.
That is what makes the one-`policy.rs`-per-workflow rule honest for a
two-directory row rather than nominal.

`BaselineCaptureTicket` + `BaselineCaptureFacts` +
`baseline_capture_is_current`: the subtle field is `baseline_slot_empty`. A newer
clean baseline may already have filled the slot, and returning the failed older
text would hand a later capture the wrong "last clean" content.

`PeriodicCaptureTicket` + `PeriodicCaptureFacts` + `periodic_capture_is_current`:
four generations plus path, modified, and live availability. `edit_generation` is
the one that makes a snapshot captured across an edit refuse itself.

`LocalHistoryReplacementTicket` uses the `is_current(&editor)` variant rather than
a separate `Facts`, because its three generations are read from live state at
validation time and there is nothing to capture separately. Its handoff to
`BufferReplacementTicket` is a **named operation** on the migrated replacement
facade, not a reach into its state.

### done: `DraftRestoreTicket` + `DraftRestoreFacts`, `DraftMutationIntent`, and the pipeline triple (`WFR-DRAFT-RECOVERY`)

**Re-audited, with one reclassification.** All gathered in
`ui/window/drafts/seams.rs`.

`DraftRestoreTicket` + `DraftRestoreFacts` + `draft_restore_is_current` is the
reference Ticket/Facts/predicate shape on the highest-consequence path in the
tree, and it is validated **twice**: when the worker returns, and again inside the
bounded replacement's terminal, because the install spans main-loop turns and a
tab reopened, renamed, or edited in between must not receive a stale recovery
body.

`DraftMutationIntent` carries `{draft_id, sequence, epoch}` and is assigned
**synchronously on GTK before any document-sized or filesystem work**, which is
the only reason a later delete can invalidate an older autosave. Freshness uses
**epoch equality, not numeric ordering**, so wraparound stays correct — a test
pins that at `u64::MAX`.

The pipeline's `DirtyDraftCandidate` / `DirtyDraftCompletion` / `AcceptedDraft`
triple qualifies too: each crosses the snapshot, worker, and commit boundaries,
and splitting them is what keeps a *completion* from carrying the buffer handle a
*candidate* needs.

**`DraftCleanupContinuation` is reclassified.** The census named it a seam value
object; it is the journal's **manifest offset**, a `usize` cursor into the record
the journal owns. Recorded here so a later slot does not look for a seam that was
never one.

Task 6.5 asked for particular attention to the archetype defect on the cleanup
path — a value meaning "the inode recorded at inspection" received by a parameter
naming it something else, which would authorize deleting the wrong body. That
value is `DraftOrphanCleanupCandidate::inode` in `services/draft_service.rs`,
travels as a named struct field from inspection to execution, and is compared
against a freshly-read `fs_metadata::inode` under the same `TargetWriteGuard`
atomic replacement uses. It is never passed positionally, so the defect is
unrepresentable there. Unchanged by this migration.

### done: `BufferReplacementTicket` + `BufferReplacementRequest` (`WFR-BUFFER-REPLACEMENT`)

**Re-audited, not newly reified, and one census claim corrected.** Slot 4 audited
the row's existing values against the two-boundary rule rather than inventing a
parallel shape.

`BufferReplacementTicket` carries `{workflow, generation}` and is the **caller-owned
freshness identity**: it crosses request construction → the parked pending slot →
the session → every scheduled turn → the terminal outcome → the caller's terminal
callback, and it is reconstructed at **five** call sites across four owning
workflows. It qualifies on both halves of the rule. It lives in
`ui/editor_page/buffer_replacement/policy.rs`, so the identity type is GTK-free.

`BufferReplacementRequest` is the workflow's **intent bundle**: ticket + body +
freshness check + terminal callback, constructed **once** at the entry point and
validated as a unit. Its four kind-paired constructors are what make the
archetype defect unrepresentable here — every constructor accepting a guarded
callback also demands a guarded body, so a guarded callback cannot reach a plain
body or the reverse, and terminal teardown matches every legal pairing
exhaustively with no runtime panic arm.

**`BufferReplacementSession` is reclassified.** The pre-migration cell named it a
seam value object; it is not. It holds a `glib::WeakRef`, a `sourceview5::Buffer`,
a `glib::SourceId`, and the boxed callbacks — coordination-owned GTK runtime that
never crosses a workflow boundary. Recorded here so a later slot does not look for
a seam that was never one.

No value is renamed while crossing a seam on this path, and the migration
introduced no `#[expect(clippy::too_many_arguments)]`; the workspace count holds
at 1, the exempt domain catalog constructor.

### done: `LoadRequestTicket` (`WFR-DOCUMENT-LOAD`)

Reified by slot 3b in `ui/editor_page/load/policy.rs`, carrying
`{load_generation, cancel_token}` with the pure predicate
`load_request_is_current` and an `is_current(&editor)` inherent method in
`ui/editor_page/load/admission.rs`. Constructed **once** at the workflow entry
point and validated as a unit at the planning completion, at every admission
drain, and at the worker read completion.

**No `*Facts` companion, and the reason is from the code rather than the
prescription.** Every clause the completion compared — load generation,
cancellation-token identity, and the token's own flag — is *live editor state*
read at validation time, so the `is_current(&editor)` variant the matrix
prescribed is the right shape; a `Facts` value would only have re-wrapped the
editor. This is the same shape as `SaveCompletionTicket`, and deliberately not
the Ticket + Facts shape of `QueuedSaveTicket`, whose predicate compares
dispatch-time expectation against separately-captured observations.

**What it removed.** The pair was grouped inside the retired
`load_runtime`'s request type, then exploded back into loose `generation` and
`cancel` parameters at both dispatch sites and compared clause by clause at the
completion — three places where a value could arrive in a parameter naming it
something else. It is now one value, and a mismatched call is a type error.

**A second, deliberately weaker predicate stayed separate.**
`installation_is_current` guards each bounded install slice and compares only the
generation and the editor's *current* token, without token identity. Merging it
into the ticket's predicate would have changed behavior: an installation is
already the newest request's own work, so it must re-read the live token rather
than assert it is still the one it was dispatched with. The two predicates are
distinct because the windows they guard are distinct.

### done: `QueuedSaveTicket` + `QueuedSaveFacts` (`WFR-DOCUMENT-SAVE`)

Reified by slot 3a in `ui/editor_page/save/policy.rs`, carrying
`{save_generation, path, explicit_destination, required_modified,
close_session_identity}` and validated by one
`queued_save_is_current(&ticket, &facts)` predicate. The ticket is built once, in
the queue stage, and carried through the drain and admission unchanged.

What it removed: five loose parameters threaded through the retired
`SaveSubmission` → `QueuedSave` → `begin_admitted_save` →
`queued_save_is_current` chain; the clause-by-clause freshness comparison
rebuilt at each call site; and the programme's only non-catalog
`#[expect(clippy::too_many_arguments)]`.

**The `explicit_destination` versus `cancel_pending_load` resolution.** This is
the seam that held the archetype defect: one boolean stored as
`cancel_pending_load` and handed positionally into a parameter named
`explicit_destination`, across three forwarding hops of which two crossed the
rename. Slot 3a decided **from the code** that one value can honestly carry both
meanings — all three callers (plain save, Save As, close-with-changes save) want
them together — so there is **one** field, named for the user's intent, and the
cancellation consequence is derived through the named pure predicate
`save_may_preempt_pending_load`.

Two distinct mechanisms keep the defect from returning, and conflating them
overstates what the derivation does. **`QueuedSaveTicket` supplies the type
safety**: the freshness predicate takes the ticket and `QueuedSaveFacts` instead
of five positional scalars, so a value can no longer land in a parameter that
names it something else — the miswired call is a type error. **The derivation
supplies the readability**: `save_may_preempt_pending_load` is `bool -> bool` and
proves nothing to the compiler, but it puts the inference in the code under a
name rather than leaving one boolean silently serving two meanings.

Both failure modes are pinned by a pure test rather than left to review: a plain
save that wrongly claimed an explicit destination would skip the path comparison
protecting it from writing a stale target, and a Save As that stopped pre-empting
the pending load would race a load into a just-saved buffer.

### done: `ReplacePreviewTicket` + `ReplacePreviewFacts` (`WFR-SEARCH-REPLACE`)

Reified by slot 1 in `ui/search_panel/policy.rs`. The ticket carries
`{generation, query_spec}` and is validated as one unit against live
`ReplacePreviewFacts {generation, pending, query_spec}` by
`ReplacePreviewTicket::is_current`. It is constructed at exactly one place,
`issue_preview_ticket`, which both preview entry points (preview generation and
checked apply) call, so generation and query spec can no longer drift apart.

What it removed: the loose
`spawn_preview_selection(generation, expected_query_spec, ...)` boundary is now
`apply_checked_replacements(ticket, ...)`; the duplicated three-clause freshness
comparison at the two worker completions is one `is_current` call; and the two
generation-only re-dispatch variants are one documented `may_dispatch`
predicate, which deliberately omits the query clause because a retained request
has not produced a result yet. `ReplacePreviewRequest` now holds the ticket
instead of separate generation and query-spec fields.

### required: `WorkspaceWatchTicket` (`WFR-WORKSPACE-TREE`)

Carries `{targets_generation, lifetime_generation}`. Today the pair travels as
a loose `(section_weak, generation, lifetime)` tuple into the watch-install
worker and is compared clause-by-clause in the completion closure
(`ui/sidebar/workspace_section/watch.rs:278` and `:282`). Distinct from
`WorkspaceScanTicket`, which already reifies the scan side of the same workflow.

### Rows that correctly require no value object

`WFR-EDITOR-FIND`, `WFR-ENCODING`, `WFR-PRINT`, `WFR-SHELL-LAYOUT`,
`WFR-STATUS-NOTIFICATIONS`, and `WFR-AUTOMATION-SPINE` have no field bundle
crossing two or more boundaries. Each row states the evidence. These are
complete rows, not unresolved ones: the convention explicitly does not force
reification of every long signature.

## Evidence Surface Baseline

The tree already contains 4 `*Evidence` types and 33 internal `*Snapshot`
types alongside the 18 `Automation*Snapshot` types. Only one workflow
(`WFR-SESSION-RESTORE`) exposes a canonical `evidence()` accessor. The
consolidation work is therefore mostly canonicalizing and completing existing
typed observation, not replacing untyped getters wholesale.

Existing `*Evidence` types: `SessionRestoreEvidence` (`pub(super)`),
`DisposalPressureEvidence` (`pub`), `WorkspaceScanPressureEvidence` (`pub`),
`NoteScoringEquivalenceEvidence` (`pub`). Their inconsistent visibility was
itself a census finding, and it is settled: the one rule is the narrowest
visibility an evidence surface's readers require, with a pre-existing wider type
narrowed to it, so the surface stays an internal type of the owning crate. See
[Settled Conventions](#settled-conventions) and
`openspec/specs/workflow-evidence-surfaces/spec.md`.

## Outlier Resolutions

The three known outliers are resolved here, before the convention becomes
normative.

### `ui/editor_page/minimap.rs` — deferred

3,779 lines, the largest single file in `ui/`. Owns `MinimapAnalysisSession`
(already a reified `{generation, lifetime}` seam) and the single consumer of
`model/minimap_analysis.rs`, so it *conforms in principle*: single-consumer
policy relocates cleanly and the seam is already a value object.

Deferred to migration slot 6 for three reasons that are about proof cost, not
about fit:

1. Its rendered output carries pixel-verified visual geometry invariants with
   named `pixel_anchors` and animation-frame stability requirements. Any change
   that can reflow the editor while the minimap is visible needs same-session
   visual geometry proof with stream-frame capture, which is the most expensive
   acceptance gate in the repo.
2. Retiring its 14 `exclude_re` entries and 66 enumerated method names is only
   safe once pure projection math sits in a `policy.rs` that the mutation scope
   reaches by convention. Doing that before the convention has two proofs risks
   dropping mutation coverage silently.
3. At 3,779 lines the facade extraction is the largest decomposition in the
   programme, so it benefits most from a settled facade shape.

A later change MUST NOT promote it earlier to "get the big win first".

### `model/editor_memory.rs` — exempt

469 lines, referenced from 7 files across 5 modules and 3 distinct workflows
(`WFR-DOCUMENT-SAVE`, `WFR-DOCUMENT-LOAD`, `WFR-COMMAND-PALETTE` focus
indexing, plus the shell's editor page lifecycle). `EditorResidencyLedger` is
residency policy shared by every workflow that can retain or release an editor.

It stays in `model/` and is recorded as cross-cutting. A later migration change
MUST NOT force it into one workflow's `policy.rs`; doing so would make two
unrelated workflows import a third workflow's private policy module.

### `ui/markdown_preview/**` — deferred

Decomposed by an earlier change into 2,541 lines plus four modules, then
further split by `continue-markdown-preview-past-oversized-blocks` into
`continuation.rs` (generation-owned cross-turn projection state) and
`text_flow.rs` (stateless text-flow primitives), leaving `mod.rs` at 1,985 lines
(11,274 lines across 11 files including `ui/window/preview.rs` and
`services/markdown_render.rs`). That
decomposition already satisfies the module-boundary half of the convention:
responsibilities are split, and `MarkdownRenderSession::is_current(generation)`,
`MarkdownCarrySignature`/`MarkdownOpenContainer`, and `MarkdownBlockOmission`
already reify the workflow's seams.

What it lacks is the narrative facade and a single evidence surface; the
continuation split was forced by the file-size rule and deliberately declared no
coordination roles, so the row stays `deferred` rather than advancing. Because the
expensive half is done and must not be redone, it does not justify its own
migration change; it is deferred to the residual sweep (slot 7), which adds the
facade and evidence surface only.

This row is `deferred` rather than `conforming` deliberately: `conforming` would
imply no remaining work and let the sweep skip it.

## Cross-Cutting Coordination

### `plain_disposal` placement — resolved: cross-cutting, stays

`model/plain_disposal.rs` has exactly one consumer file, which made it look
like a co-location candidate. The census resolves this against relocation:

- Its single consumer is `ui/plain_disposal.rs`, the module's **own adapter**.
  The pair is one cross-cutting coordination module already correctly split
  across the purity boundary, not a workflow's private policy.
- That adapter is consumed by **21 files across 10 workflows**, and
  `DisposalOwned<T>` appears in the signatures of search/replace, drafts, load,
  local history, buffer replacement, markdown preview, command palette, notes,
  session persistence, and buffer snapshot. Co-locating it under any one
  workflow would make the other nine import a peer workflow's internals.

It also stays out of GTK Lush scope. It encodes LushText's payload admission
budget and retirement policy rather than generic toolkit machinery, which fails
the family's leaf-crate test and is excluded by this programme's non-goals.
Renaming is not required: `plain_disposal` already names the domain concept
(retiring plain, non-GTK payloads off the main thread) rather than a mechanism.

`WFR-BUFFER-SNAPSHOT` and `WFR-BUFFER-REPLACEMENT` are cross-cutting on the
same grounds and are recorded as such.

## Migration Order And Risk Tiers

Order is by increasing risk, and every `tier-3` slot follows at least two
completed lower-risk migrations.

**Slot 2 was the one place where that rule needed an explicit decision, and the
decision was to split it into 2a and 2b.** Slot 2 carried a tier-3 half (the
Replace All write path and its undo journal) while only one completed lower-risk
migration preceded it, slot 1's tier-2 exemplar. The alternative was to sequence
the tier-2 `WFR-COMMAND-PALETTE` migration first inside one change. The split was
taken because the two-proof rule wants a *completed* migration and completion is
observable only at the change boundary: sequenced inside one change the gate is a
promise in a task list, whereas as two changes `make check-workflow-boundaries`
enforces it — 2b cannot pass until this matrix marks `WFR-COMMAND-PALETTE`
migrated and the programme record's ledger marks slot 2a complete. Slots 3
through 7 keep their numbers.

| Slot | Change scope | Workflows | Highest tier |
| --- | --- | --- | --- |
| 1 | Census, convention, enablers, exemplar (`normalize-workflow-readability-boundaries`, complete) | `WFR-SEARCH-REPLACE` search and preview half only | tier-2 |
| 2a | Palette migration, facade budget, first automation projections beyond search (`migrate-command-palette-workflow-readability`) | `WFR-COMMAND-PALETTE`, first `WFR-AUTOMATION-SPINE` projections beyond the search fields | tier-2 |
| 2b | Search/replace completion (`complete-search-replace-workflow-readability`) | `WFR-SEARCH-REPLACE` replace and undo half, continuing `WFR-AUTOMATION-SPINE` projections | tier-3 |
| 3a | Save (`migrate-document-save-workflow-readability`) | `WFR-DOCUMENT-SAVE` | tier-3 |
| 3b | Load (`migrate-document-load-workflow-readability`) | `WFR-DOCUMENT-LOAD` | tier-3 |
| 4 | User-content restore family | `WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`, `WFR-BUFFER-REPLACEMENT` | tier-3 |
| 5 | Workspace tree and notes | `WFR-WORKSPACE-TREE`, `WFR-NOTES-BOOKMARKS` | tier-3 |
| 6 | Minimap | `WFR-MINIMAP` | tier-2 logic, highest proof cost |
| 7 | Residual sweep | `WFR-MARKDOWN-PREVIEW`, `WFR-EDITOR-FIND`, `WFR-ENCODING`, `WFR-PRINT`, `WFR-SHELL-LAYOUT`, `WFR-STATUS-NOTIFICATIONS`, `WFR-BUFFER-SNAPSHOT`, `WFR-PLAIN-DISPOSAL`, remaining `exclude_re` and argument suppressions, matrix completion | tier-3 (disposal) |

Slot 1 is the only slot whose workflow is not migrated end to end, by design:
the exemplar deliberately scopes to the non-writing half so the pattern is
proven before any user-data path is touched.

**Artifacts each slot is expected to need.** Slot 1 carried proposal, design,
tasks, and two new capability specs. Slots 2a through 5 and 7 are expected to
need a **proposal and tasks, plus the minimum spec delta strict validation
requires**, because this matrix and the capability specs already hold the
contract: a migration consumes the convention and checks off rows. The earlier
wording said "only a proposal and tasks", which `openspec validate --strict`
cannot satisfy — it fails any change with no `specs/` delta ("Change must have at
least one delta"), so every slot carries one. What still signals an incomplete
phase-0 contract is a delta that *adds obligations or capabilities*, not a delta
that restates a fulfilled future-tense requirement or closes an adjacency the
convention already sanctions. Slot 6 (minimap) is the expected exception for a
**design** document, for its pixel-verified geometry under animation frames. When
a migration does need a new obligation or capability, the retroactive-amendment
rule in [Completion Rule](#completion-rule) applies to the fix.

**Slot 1 residue: all six obligations are discharged.** `WFR-SEARCH-REPLACE` is
now `migrated` end to end. Two obligations were paid by slot 2a — the normative
facade line budget number (declared at 370, see
[Facade size budget](#facade-size-budget)) and the first
`WFR-AUTOMATION-SPINE` projections beyond the search fields
(`window.command_palette` and both palette readiness blockers). The remaining
four were paid by slot 2b:

| Obligation | Outcome |
| --- | --- |
| the Replace All write path and its undo journal | migrated; `journal.rs` owns it, and the write-side seams are classified under [WFR-SEARCH-REPLACE](#wfr-search-replace) |
| `replace.rs`'s final coordination role name or names | `replace_execution.rs` plus `journal.rs`; `journal` was added to the bounded set |
| making `activate_undo_replacements` a delegation | **already done by slot 1's result-cap fix.** The residue text was stale: the facade holds a one-line call to `journal::hand_back_undo_backup` that reads no transaction state and mutates no widget. The residual asymmetry was one layer out, in `ui/window/search.rs`'s undo path, which claimed the transaction, re-showed the undo button on two early returns, reserved capacity, and installed the remainder backup inline. Slot 2b fixed **that**, through `journal::begin_undo_restore` (returning `UndoRestoreClaim`) and `journal::finish_undo_restore`. A future session must not re-open the facade item, nor conclude the window-side work was skipped |
| `model/workspace_search.rs`'s relocation decision | **it stays in `model/`**; see [Modules confirmed as domain and staying in `model/`](#modules-confirmed-as-domain-and-staying-in-model) |

Slot 2b did **not** re-plan the capped-result delivery fix or the `WalkStop`
stop-semantics split — both landed in slot 1 and are recorded under
[WFR-SEARCH-REPLACE](#wfr-search-replace).

The change-level view of these slots, with the same list in a machine-readable
slot ledger that `make check-workflow-boundaries` compares against this matrix,
lives in `docs/next/workflow-readability.md`. Advancing a slot means updating
both files in the same change.

`WFR-EDITOR-MEMORY` and `WFR-MIGRATION-LEDGER` have no slot. They are
cross-cutting policy that stays where it is; their rows exist so a later change
cannot silently relocate them.

## Census Findings For Convention Settling

These findings were the inputs to slot 1's convention section. They are
recorded here because they change what the convention must say, and a later
session must not have to rediscover them.

**Disposition after section 2.** Findings 1, 2, 3, and 4 produced spec amendments:
the established seam shape must be reused rather than paralleled, evidence surfaces
share one visibility rule and fold in pre-existing typed observation, a workflow
with no qualifying bundle counts as a complete row, and workflow code asserts zero
argument-count suppressions. Findings 5 and 8 are definitional and live in
[Measurement Definitions](#measurement-definitions) and the
[Policy Module Census](#policy-module-census). Findings 6 and 7 are absorbed into
this matrix's risk tiers and migration slots; the programme record written in
section 7 must carry them forward rather than restating the superseded figures.

1. **The `Ticket` + `Facts` + `*_is_current` idiom already exists** in three
   workflows. The convention should name and require this existing idiom rather
   than introduce a differently shaped value object.
2. **Evidence surfaces already exist in four places** with inconsistent
   visibility (`pub` versus `pub(super)`), and 33 internal `*Snapshot` types
   already carry typed observation. The convention must fix one visibility rule
   and say how existing snapshot types fold into a workflow's single surface.
3. **The seam value object rule and the long-signature metric do not agree.**
   `WFR-SEARCH-REPLACE` contains exactly one function with 6 or more
   non-receiver parameters, and it is `SearchResultItem::new_match`, a row-item
   constructor the convention explicitly exempts. The exemplar's real seam work
   is a two-field bundle that the long-signature metric cannot see. The
   programme's "reifies 2 of 90 long signatures" figure should be restated in
   terms of seams reified, not signatures shortened.
4. **`#[expect(clippy::too_many_arguments)]` has only two occurrences**, and one
   is `model/action_catalog.rs:178`, a catalog row builder rather than a
   workflow seam. A hard zero is reachable for workflow code; the catalog
   builder needs an explicit decision.
5. **The `exclude_re` figure needs restating.** There are 71 entries: 50
   services-scoped, 20 ui-scoped, 1 unscoped. The minimap block is 14 entries
   enumerating 66 method names over 17 physical lines, not roughly 40 entries.
6. **The exemplar's risk description needs qualification.** `WFR-SEARCH-REPLACE`
   as a whole reaches tier-3, because Replace All mutates user files and owns an
   undo journal (`services/search_backup.rs`, 1,334 lines). The exemplar's
   scope excludes that write path, so slot 1 stays tier-2, but the row must not
   be described as a workflow that touches no user data.
7. **The remaining-scope table omits two workflows.** `WFR-LOCAL-HISTORY` and
   `WFR-BUFFER-REPLACEMENT` persist and install user content but appear in no
   named migration change. This matrix assigns them to slot 4 alongside the
   draft and session restore family, because `buffer_replacement` is the shared
   installation mechanism for local-history restore and Replace All undo.
8. **The census found three previously unlisted single-workflow policy
   modules** (`workspace_scan.rs`, `workspace_persistence.rs`,
   `workspace_search.rs`), so the relocation count is larger than the known
   eight mechanism modules implied.

## Surfaces With No Coordination Tier

These are enumerated so census completeness is provable. None is a workflow:
each is a widget, helper, or infrastructure module with no ordered stages and no
coordination role, and none carries a seam value object obligation.

`ui/preferences/**`, `ui/properties_panel/**`,
`ui/theme.rs`, `ui/accessibility.rs`, `ui/editor_page/accessibility.rs`,
`ui/sidebar/file_tree_item.rs`, `services/filesystem/**`,
`services/durable_write.rs`, `services/json_store.rs`,
`services/json_format.rs`, `services/single_flight.rs`, `services/sync.rs`.

`services/single_flight.rs` and `services/sync.rs` are shared coordination
primitives consumed through the workflows that own their generations; they are
covered by `WFR-*` rows rather than owning one.

The census originally also listed a shrinkable-bin module under `ui/` here. That
widget had already graduated into the GTK Lush family as
`crates/gtk-lush/widgets/src/clip_bin/`, so the entry named a path that no
longer exists; it is dropped rather than repointed, because GTK Lush crates are
governed by `crates/gtk-lush/GOVERNANCE.md` and are outside this convention's
scope.

Crate infrastructure outside the census: `lib.rs`, `main.rs`, `app.rs`,
`config.rs`, `fuzzing.rs`.

**Coverage proof.** 198 files exist under `crates/lushtext-core/src`. 195 are
attributed to a matrix row or to the list above; the remaining 3 are the crate
infrastructure files named above. Layer totals at census time: `ui/` 67,200
lines across 104 files, `model/` 11,817 across 29 files, `services/` 47,273
across 61 files.

## Settled Conventions

These four conventions were settled from the census evidence above and are recorded
normatively in the capability specs. A later change that wants to revisit one must
amend the spec and re-migrate every already-migrated row in the same change.

### Role file names

| Role | File name | Notes |
| --- | --- | --- |
| Facade | the workflow's public module surface | narrates stages, delegates everything |
| Pure policy | `policy.rs` | one per workflow; no GTK-family imports |
| Evidence | `evidence.rs` | one per workflow; one visibility rule — the narrowest visibility its readers require, and a pre-existing wider evidence type is narrowed to it rather than left wide |
| Coordination | bounded set of job names: `admission`, `execution`, `retirement`, `watch`, `journal`, optionally prefixed with the stage order served (`index_execution`, `replace_execution`) | a workflow may own more than one |

The convention deliberately does **not** fix a single coordination file name. The
census found `runtime` already naming three different jobs across four files, and
`ui/editor_page/` and `ui/window/` host 8 and 12 workflows respectively, so one fixed
name would force a subdirectory-per-workflow restructuring of roughly 20 workflows.
Role names are scoped within a shared directory instead. A coordination job that no
listed role name describes requires a spec amendment to add the name.

**Two permitted role homes, chosen per workflow.** The `policy.rs` and
`evidence.rs` names above are fixed at one each per workflow, so two workflows in
one directory cannot both use them, and a workflow-prefixed `save_policy.rs` is
not an available substitute: the default mutation scope reaches pure policy
through the literal `crates/lushtext-core/src/ui/**/policy.rs` glob, so a
prefixed file leaves the scope, which
[`mutation-testing`](../openspec/specs/mutation-testing/spec.md) classifies as a
blocking coverage regression. Slot 3a closed that adjacency: where a directory
hosts more than one workflow, a migrated workflow's roles MAY live in a
**per-workflow subdirectory** of it whose `mod.rs` is the facade and whose role
files keep the unqualified names `policy.rs`, `evidence.rs`, and the unqualified
bounded coordination names. The subdirectory is named for the workflow in its own
domain vocabulary (`ui/editor_page/save/`). This is a permitted home, not a
required one — a workflow whose role file names do not collide with a sibling's
keeps flat, workflow-scoped names in the shared directory, and migration still
never requires restructuring a whole directory into one subdirectory per
workflow. Each migrated row records which home it chose under
[Migrated Workflow Roles](#migrated-workflow-roles).

**Stage-order qualification.** Where **one** workflow owns more than one ordered
stage order in a single directory and more than one of those stage orders needs a
coordination module of the same shape, the module name MAY qualify a bounded role
name with the stage order it serves. The qualifier uses the workflow's own domain
vocabulary and the suffix stays a bounded role name, so the bounded set is not
widened. A workflow must not take an ill-fitting bounded name merely because the
fitting one is already spent on a different stage order of the same workflow.
Slot 2a hit this first: the palette owns a query search and an incremental
file-index mutation, and both need an `execution` module, so they are
`query_execution.rs` and `index_execution.rs` while `retirement.rs` stays
unqualified because only one stage order retires anything.

**The bounded set is reviewed, not gated.** `make check-workflow-boundaries`
validates that a migrated row's declared role paths *exist*; it does not verify
that a coordination module's name is drawn from the set. No migration may rely on
a gate to reject an off-set name.

### Facade size budget

**Declared and active.** The number was derived from the exemplar's facade
(`crates/lushtext-core/src/ui/search_panel/mod.rs`) as it measured **at slot 1**:
**350 physical lines**, of which 75 were the module-doc stage narration and 166
were non-comment, non-blank lines, narrating 6 of its inversions across two stage
orders. It measured 357 lines when the migration landed and 350 after the
result-cap fix delegated the undo hand-back out of the facade. **That 350 is
history, not the current size**: slot 2b migrated the row's Replace All half, and
the facade now measures **369 physical lines** narrating **all twelve** of the
workflow's inversions (2 in the search stage order, 10 in Replace All). It
replaced a 578-line
`mod.rs` that also held the accessibility projection and 23 observation getters;
those moved to `accessibility.rs` and `evidence.rs` respectively. Physical lines
are the metric the mechanical check uses, so that is the number a budget should
be compared against.

Per `openspec/specs/workflow-readability-boundaries/spec.md`, the **first
migration change after this exemplar** set the normative number from the
exemplar's measured facade. Slot 1 was the exemplar, not that migration, so it
recorded the measurement and left the number unset. **Slot 2a
(`migrate-command-palette-workflow-readability`) set it at 370**, and under the
retroactive-amendment rule that was also the cheapest moment, because exactly one
other workflow was migrated. The derivation: 350 measured, plus modest headroom,
bounded below by this section's own finding that a budget under roughly 370 lines
would force the exemplar facade's narration to be split, which defeats the
facade. Slot 2a re-measured the exemplar at 350 before declaring the number and
verified that facade against it (see [Retroactive amendment](#retroactive-amendment)).

**The headroom is now 1 line, and that is this section's own "real evidence"
trigger firing.** The risk this paragraph described was taken deliberately and has
now materialized. Slot 2a declared 370 against a 350-line facade — 20 lines of
headroom. Slot 2b finished that same workflow and the facade reached **369**,
leaving **one line**. Getting there was not free: the first honest narration of
the completed Replace All stage order landed at 379, and it was brought back under
budget only by folding module-ownership detail into the role table, compressing
every inversion bullet, and delegating the options-row reveal out of the facade
entirely. The budget was never edited, which is the rule working — but a facade
one line from its ceiling is not a margin, and **slot 3 must plan against 1 line,
not 20.**

The paragraph below still states the correct response, and it still applies. What
has changed is the honest reading of its last sentence: this section says that if
an honest role split cannot fit 370, that is "real evidence the number is wrong,
and it must be corrected while as few workflows as possible are migrated." Slot 2b
*did* fit, so the number is not yet proven wrong — but the next workflow to narrate
two stage orders will decide it, and only two workflows are migrated today. A
slot-3 facade that cannot fit after honest delegation should raise the number
through the spec rather than mangle its narration, and should do it then rather
than later, because the re-migration cost grows with every migrated row.

A loose
budget enforces nothing, so the number is tight; the consequence is that a facade
narrating two stage orders may not fit on the first attempt. The response is
always to delegate more work into the coordination modules, never to raise the
number, because raising it is a convention amendment that requires re-migrating
every migrated row in the same change. If an honest role split still cannot fit
370, that is real evidence the number is wrong, and it must be corrected while as
few workflows as possible are migrated. Slot 2a's own palette facade fit within
the budget, which is the number's first independent test.

**How to declare it.** The budget lives in this section as one
machine-readable line, exactly:

```
- normative facade line budget: <integer>
```

`make check-workflow-boundaries` reads that line. While it is absent the facade
size check is inert. Once present, the check counts the physical lines of every
`migrated` row's declared `facade` path and fails when one exceeds the budget,
naming the row, the facade path, its measured size, and the budget. Only the
first such line in this section is read, so the budget cannot be declared twice.
Changing the number is a convention amendment: it must go through the spec and
re-check every already-migrated row in the same change. A later migration MUST
NOT re-derive the number as if it were still unset.

- normative facade line budget: 370

**Measured facades, after slot 3b.** The number was not edited and no escalation
was needed.

| Facade | Measured | Margin |
| --- | --- | --- |
| `ui/search_panel/mod.rs` (exemplar, two stage orders, twelve inversions) | 369 | 1 |
| `ui/command_palette/mod.rs` (two stage orders, eight inversions) | 335 | 35 |
| `ui/editor_page/save/mod.rs` (one stage order, five inversions) | 223 | 147 |
| `ui/editor_page/load/mod.rs` (one stage order, **seven** inversions, seven entry points) | 253 | 117 |

Slot 3a's data point is confirmed and sharpened: **what stresses the budget is
the number of stage orders, not the inversion count, the entry-point count, or
the risk tier.** The load facade narrates two more inversions and four more entry
points than the save facade and still fits with 117 lines to spare, while the
exemplar's two stage orders sit 1 line under. Slot 6 (minimap) remains the slot
most likely to prove the number wrong.

### Argument-count suppressions

The residual sweep asserts **zero** `#[expect(clippy::too_many_arguments)]` in
workflow adapter and coordination code, with no allowlist. **That assertion is
already true: slot 3a removed the only workflow-code suppression**, on
`begin_admitted_save`, by reifying its parameter list as `QueuedSaveTicket`. The
workspace count is now **1**, and the survivor is the domain catalog constructor
in `model/action_catalog.rs`, which the workflow-seam rule exempts because each
of its parameters names a documented external contract field. The residual sweep
inherits a discharged obligation here rather than a pending one.

Treat any new suppression on a cross-module workflow boundary as an unreified
seam, not an accepted exception.

### Evidence-surface reentrancy

**Settled by slot 3b**, which promoted it from a per-workflow module note on the
exemplar's `evidence.rs` into stated convention in
`openspec/specs/workflow-evidence-surfaces/spec.md`.

Because one accessor reads the whole surface, and because a GTK workflow's state
lives behind interior mutability, reading the surface takes shared borrows. It
follows that **no evidence field may be read from inside a mutable borrow of the
same state**: doing so panics at runtime rather than failing to compile. The
constraint is a property of the convention, not of any one workflow, so it is
recorded once.

Every migrating workflow MUST:

- record the constraint where its surface is defined;
- compute derived scalars and drop each borrow before building the surface's
  struct literal, so no borrow outlives the value it produced;
- **never** add a second, narrower accessor to make a nested read possible; and
- prove it with a test that **drives the workflow through each operation taking a
  mutable borrow of the state the accessor reads, reads the surface *after* each
  one, and asserts that repeated reads of unchanged state are identical.**

The proof's shape is easy to state backwards, and stating it backwards mandates
the exact panicking read the constraint forbids. A test that reads the surface
*while* a borrow is held is not a proof of the constraint; it is the failure.

### Cross-cutting eligibility

Relocation eligibility is decided by the number of **owning workflows**, not
consuming files. Pure policy whose only consumer is its own coordination adapter is
cross-cutting when that adapter serves several workflows. This is what keeps
`plain_disposal`, `buffer_snapshot`, `buffer_replacement`, `editor_memory`, and
`migration_ledger` in shared locations.

## Migrated Workflow Roles

Every row whose status is `migrated` MUST have a subsection here naming the
roles it gained. `make check-workflow-boundaries` reads this section: it fails
when a `migrated` row has no subsection, when a required role is unnamed, and
when a named path does not exist. It also fails when this section declares roles
for a row that is not marked `migrated`, so the two halves cannot drift.

Format, one subsection per migrated row id:

```
### WFR-EXAMPLE

- facade: `ui/example/mod.rs`
- coordination: `ui/example/execution.rs`, `ui/example/retirement.rs`
- policy: `ui/example/policy.rs`
- evidence: `ui/example/evidence.rs`
- mutation parity: `openspec/changes/<change>/evidence/<file>.md`
```

`facade` and `evidence` MUST name real paths. `coordination`, `policy`, and
`mutation parity` MAY be the literal `none` when the workflow owns no
coordination module, owns no pure policy, or relocated no policy module.

### WFR-SEARCH-REPLACE

- facade: `ui/search_panel/mod.rs`
- coordination: `ui/search_panel/execution.rs`, `ui/search_panel/retirement.rs`, `ui/search_panel/replace_execution.rs`, `ui/search_panel/journal.rs`
- policy: `ui/search_panel/policy.rs`
- evidence: `ui/search_panel/evidence.rs`
- mutation parity: `openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md` (slot 1), `openspec/changes/archive/2026-08-25-complete-search-replace-workflow-readability/evidence/mutation-parity-replace-policy.md` (slot 2b)
- slot 2b's other evidence, all under `openspec/changes/archive/2026-08-25-complete-search-replace-workflow-readability/evidence/`: `test-counts.md` (before/after counts and their counting method), `widget-test-search-backup-site-migration.md` (the per-site categorization of the 35 around-the-widget service reaches), `live-run.md` plus `live-run-stderr.log` (the live-session Replace All and undo, including why `make run` itself was unsafe here)

Notes on this row, which is the exemplar and therefore the reference for the
migrations that follow. Slot 1 migrated its search and preview half; slot 2b
migrated the Replace All write path and its undo journal, so the row is now
migrated end to end.

- `runtime.rs` is gone. Its streaming-search half became `execution.rs` and its
  bounded-disposal half became `retirement.rs`, which is what the two-role split
  the census predicted looks like in practice.
- **`replace.rs` is gone, and its role name is decided.** Slot 1 left it with a
  workflow-descriptive name because it owned both the Replace All preview *and*
  the durable undo journal. Reading it for slot 2b confirmed those are two
  cohesive coordination jobs, so it split along that seam:
  - `replace_execution.rs` — the preview attempt and the checked apply: ticket
    issue, generation open, capacity reservation and its retry parking,
    single-flight coalescing, worker dispatch, publish-or-retire, the queued
    drain, preview mode enter/exit, search-state invalidation, the
    checked-selection claim and apply, and the three preview widget-mutation
    helpers. It has the same submit/dispatch/arbitrate shape as `execution`, and
    `execution.rs` in this directory is already the *search* stage order's
    execution module, so it takes the **stage-order qualifier**.
  - `journal.rs` — the durable undo journal: the transaction gate, the
    generation reservation, the generation-guarded install and clear, the
    worker-side disk save and delete, startup recovery with stale cleanup and
    diagnostics, the disposal-capacity retry, the undo affordance, and the
    hand-back. **No pre-existing bounded role name described this job**, and
    `retirement` means its opposite, so slot 2b amended
    `gtk-adapter-module-boundaries` to add `journal`. It is unqualified because
    nothing else in the directory claims that role.

  `execution.rs` was deliberately **not** renamed to `search_execution.rs`. The
  spec's qualification rule puts the qualifier on the module whose fitting name is
  already spent, and renaming a stable already-migrated coordination module —
  plus, for symmetry, `retirement.rs` — would add churn to a tier-3 change that
  rewrites the user's files. Slot 2a qualified both palette execution modules
  because it created both in one change; here only one is new.
- **The two Replace All modules are not state-disjoint, and the split names the
  handoff.** Three fields on `imp::SearchPreviewState` are touched by both
  halves; `journal.rs` owns all three, and `replace_execution` reaches them only
  through named crossing predicates:

  | Shared field | Owner | Crossing operation |
  | --- | --- | --- |
  | `replace_transaction_pending` | `journal.rs` | `replace_transaction_claimed` |
  | `replace_transaction_generation` | `journal.rs` | `replace_transaction_generation_reserved` |
  | `undo_backup_generation` | `journal.rs` | none in-panel; crosses to the window inside `ReplaceJournalFreshness` |
- `retire_undo_backup_off_main` became `release_superseded_undo_journal`. Slot 1's
  residue text claimed the old name promised off-main retirement that the body did
  not deliver; **that premise was wrong** and is recorded here so it is not
  re-litigated. Releasing the last `Arc` runs `DisposalOwned`'s drop, which submits
  the payload to the disposal lane, so destruction *is* off-main. The rename fixes
  two different problems: `retire_` collided with the `retirement` role name, and
  `_off_main` attributed to this function a guarantee that belongs to the guarded
  owner it releases.
- `accessibility.rs` holds the accessible-state projection that used to sit in
  `mod.rs`. It is adapter detail, not a coordination role: the facade may not own
  widget mutation.
- `test_policy.rs` holds `SearchPanelTestPolicy`, the workflow's single
  test-only timing/limit value. The whole module is behind
  `#[cfg(feature = "test-utils")]`, so a production build compiles no override
  storage at all. Slot 2b added no new override.
- `policy.rs` is `pub` because the GTK-free policy benchmarks in
  `crates/lushtext-core/benches/benchmarks.rs` address `WorkspaceSearchFlight`
  and `SearchRetirementSliceBudget` directly. Nothing else outside the workflow
  uses it. Slot 2b added the Replace All weights, the undo-reservation plan, the
  journal generation predicate, and `ReplaceApplyCounts` to it.
- **Write-side seam classification** (`services/content_search/replace.rs`):
  configuration — the undo-byte-cap thread-local plus its setter; probe — the
  active-journal assertion before each write; actuation — the before-rename
  fault-injection guard and the after-metadata hook registry; inspection — three
  functions over those fault-injection registries. The actuation seams stay: they
  are the fault-injection mechanism this row's failure-path verification depends
  on.
- **`model/workspace_search.rs` stays in `model/`.** Slot 2b re-derived its
  reference set, found it larger than the census cell recorded, and closed the
  decision; see
  [Modules confirmed as domain and staying in `model/`](#modules-confirmed-as-domain-and-staying-in-model).

### WFR-DOCUMENT-SAVE

- facade: `crates/lushtext-core/src/ui/editor_page/save/mod.rs`
- coordination: `crates/lushtext-core/src/ui/editor_page/save/admission.rs`, `crates/lushtext-core/src/ui/editor_page/save/execution.rs`
- policy: `crates/lushtext-core/src/ui/editor_page/save/policy.rs`
- evidence: `crates/lushtext-core/src/ui/editor_page/save/evidence.rs`
- mutation parity: `openspec/changes/migrate-document-save-workflow-readability/evidence/mutation-parity-save-policy.md`

Notes on this row, which is the third migration, the first tier-3 workflow to be
migrated on its own, and the first to use a per-workflow subdirectory role home:

- **This row's role home is a per-workflow subdirectory, and that is why the
  convention now permits one.** `ui/editor_page/` hosts eight workflows, and the
  role file names `policy.rs` and `evidence.rs` are fixed at one each per
  workflow. A workflow-prefixed `save_policy.rs` was mechanically unavailable,
  not merely unattractive: the default mutation scope reaches pure policy through
  the literal `crates/lushtext-core/src/ui/**/policy.rs` glob, so a prefixed file
  leaves the scope, which `mutation-testing` classifies as a blocking coverage
  regression. The next `ui/editor_page/` workflow to migrate should copy this
  boundary rather than re-derive it: subdirectory named for the workflow in its
  own domain vocabulary, `mod.rs` as the facade, role files unqualified inside.
- `save_runtime.rs` is gone. `runtime` is the name the convention rejects, and
  the census found it naming three different jobs across four files. Its
  process-wide coordinator, queue, drain, and exactly-once charge release are the
  workflow's **admission** job and now say so.
- **The workflow owns one stage order, so neither coordination name needs a
  stage-order qualifier.** `admission.rs` is everything before document text is
  copied; `execution.rs` is everything after. `journal` was checked and rejected
  — see the note below.
- **`journal` does not fit, and the reason generalizes.** The programme record
  predicted `journal` would look applicable in slot 3, and it does at first
  glance: a save writes durably. But `journal` names a durable,
  generation-guarded record that *a later stage of the same workflow reads back*,
  with startup recovery. A save replaces the user's file bytes and no later stage
  of the save workflow reads them back; the record that protects an unsaved
  buffer is the draft, which belongs to `WFR-DRAFT-RECOVERY` (slot 4). Pulling
  draft persistence in to justify the name would have been the overload the
  bounded set exists to prevent. **The test is "does a later stage of *this*
  workflow restore from it", not "does it touch the disk durably".**
- **The archetype defect is closed by construction.** One boolean was stored as
  `cancel_pending_load` and handed positionally into a parameter named
  `explicit_destination`, across three forwarding hops of which two crossed the
  rename. It is now one ticket field named for the user's intent
  (`explicit_destination`), and the cancellation consequence is derived through
  the named pure predicate `save_may_preempt_pending_load`. All three callers
  wanted both meanings, so one field is honest. **`QueuedSaveTicket` is what makes
  the mismatch a type error** — the predicate takes the ticket rather than five
  positional scalars — while the `bool -> bool` derivation is what makes the
  inference readable rather than implied. Both failure modes are pinned by a pure test:
  a plain save that wrongly claims an explicit destination would skip the
  stale-target path comparison, and a Save As that stopped pre-empting the
  pending load would race a load into a just-saved buffer.
- **`begin_admitted_save`'s `#[expect(clippy::too_many_arguments)]` is gone**,
  because the ticket replaced the parameter list. See
  [Argument-count suppressions](#argument-count-suppressions).
- **Two freshness seams, deliberately kept distinct.** `QueuedSaveTicket` +
  `QueuedSaveFacts` + `queued_save_is_current` guard *admission*;
  `SaveCompletionTicket::is_current` guards *completion*. One behavioral detail
  is disclosed rather than hidden: `QueuedSaveFacts` is captured as a unit, so
  when a ticket names a close session the window lookup now also runs on paths
  where an earlier clause has already failed. It is a pure scalar read on a path
  that is about to cancel the request anyway, and validating the seam as a unit
  is what the convention requires.
- **The chunked-capture threshold was deliberately *not* extracted here.** Task
  planning expected it to become save policy, but the pure comparison already
  exists as `char_count_requires_chunked_snapshot` in cross-cutting
  `ui/buffer_snapshot.rs` (`WFR-BUFFER-SNAPSHOT`, slot 7). Duplicating it would
  fork a shared limit. What this row owns is naming the two modes
  (`SaveCaptureMode`) so the choice is observable, and the evidence field is a
  live classification of the current buffer — which is what the retired
  `save_uses_chunked_snapshot_for_test` seam meant.
- **`services/editor_io.rs` and `services/durable_write.rs` keep their pure
  rules as private functions with direct unit tests; no `services/*/policy.rs`
  was created.** They are already inside the mutation scope through
  `services/**`, so a policy module would buy no coverage, and they cannot move
  under `ui/` without inverting dependency direction. `editor_io.rs` is shared
  with the load workflow, so **slot 3b must follow this decision rather than
  re-litigate it**; if 3b finds the load side genuinely changes the answer, that
  is a new decision to state, not a correction of this one.
- **The five `test-utils` override statics stay in `services/editor_io.rs`**, so
  this workflow has no `test_policy.rs`. The service owns the behavior being
  overridden — write delay, path-scoped save failure injection, transient weight
  — and a second policy value in `ui/` would shadow it. The fault-injection
  seams are the mechanism this change's failure-path verification depends on and
  are classified, not retired.
- Mutation coverage has two separate claims that must not be mixed: the
  relocation of the admission policy is a **parity** claim, and the newly
  extracted decisions are a **gain from zero**. The evidence file reports them
  separately.

### WFR-DOCUMENT-LOAD

- facade: `crates/lushtext-core/src/ui/editor_page/load/mod.rs`
- coordination: `crates/lushtext-core/src/ui/editor_page/load/admission.rs`, `crates/lushtext-core/src/ui/editor_page/load/execution.rs`, `crates/lushtext-core/src/ui/editor_page/load/retirement.rs`
- policy: `crates/lushtext-core/src/ui/editor_page/load/policy.rs`
- evidence: `crates/lushtext-core/src/ui/editor_page/load/evidence.rs`
- mutation parity: `openspec/changes/migrate-document-load-workflow-readability/evidence/mutation-gain-load-policy.md` — **records a gain from zero, not a relocation parity**, because `model/file_load.rs` stays in `model/`

Notes on this row, the fourth migration, the second tier-3 workflow, and the
**second adopter of the per-workflow subdirectory role home**:

- **`ui/editor_page/load/` copies slot 3a's boundary rather than re-deriving it.**
  Subdirectory named for the workflow in its own domain vocabulary, `mod.rs` as
  the facade, role files unqualified inside. The nested `ui/**/policy.rs` glob was
  re-verified reachable after the move (44 mutants generated), which is what the
  convention needed before a third `ui/editor_page/` workflow adopts the shape:
  one adopter proves the glob resolves, two prove it is not a special case.
- **`load_save.rs` no longer exists, and neither does `load_runtime.rs`.** Slot 3a
  lifted the save half out; this change dissolved the rest. The file the programme
  cites as its third measured symptom — "1,795 lines holding two workflows" — is
  gone, and `runtime` is again the name the convention rejects: `load_runtime.rs`
  held the coordinator, the queue, the drain, the worker dispatch, and the charge
  release, which are now `admission.rs`.
- **Three coordination modules, not two, and the third is the tier-3 half.** The
  bounded set's `retirement` genuinely fits: `retirement.rs` gives back the
  decoded `DisposalOwned<String>` payload off-GTK, the admission charge, the
  partially installed buffer, and the load identity. Making it its own module is
  what makes the data-safety-critical path legible — cancellation empties the
  buffer on purpose, so `installation_incomplete` must stay set until a retry
  installs one exact payload, or a save would write a truncated file over the
  user's document. It calls into `execution` for the shared install state machine
  and `execution` dispatches its `ClearingCancelled` phase back; the mutual
  reference is named in the facade rather than hidden.
- **`journal` was checked first and rejected, on slot 3a's reusable test.** The
  test is "does a later stage of *this* workflow restore from it", not "does it
  touch the disk". Load keeps no durable record at all: it reads one, installs it,
  and forgets it. Slot 4 is still where `journal` fits.
- **The workflow owns one stage order, so no coordination name needs a
  stage-order qualifier.** Slot 2a's qualification rule and slot 2b's narrow
  reading of it both leave these three names unqualified.
- **The seam was reified and a second, weaker predicate was deliberately kept
  separate.** See [done: `LoadRequestTicket`](#done-loadrequestticket-wfr-document-load).
  No `#[expect(clippy::too_many_arguments)]` was introduced anywhere in this
  workflow, and the workspace count stays at 1 (the domain catalog constructor).
- **The paragraph-boundary contract moved as a whole and is measured, not
  asserted.** The clear-slice half is `policy::clear_slice_char_count` plus
  `policy::clear_slice_extends_to_paragraph_end`; the install half stays
  `next_install_boundary` in `model/file_load.rs`, shared with
  `WFR-BUFFER-REPLACEMENT`. A widget test loads a single paragraph over the slice
  budget and asserts it installs in **exactly one** slice, and loads a
  same-sized paragraph-rich document and asserts it slices, with both elapsed
  times recorded. See
  `openspec/changes/migrate-document-load-workflow-readability/evidence/install-slicing-linearity.md`.
- **`services/editor_io.rs` keeps its pure rules as private functions with direct
  unit tests, following slot 3a rather than re-litigating it.** The load side does
  not change that answer: `services/**` is already inside the mutation scope, so a
  services/editor_io/policy.rs module would buy no coverage, and moving encoding
  detection, decode-failure classification, or transient-weight arithmetic under
  `ui/` would invert dependency direction. The 6 load-side `test-utils` overrides
  stay there too, because the service owns the behavior they change; a second
  policy value in `ui/` would shadow them.
- **One test-policy value, and it holds only what `ui/` owns.**
  `load/test_policy.rs` is entirely behind `#[cfg(feature = "test-utils")]`, so a
  production build compiles no override storage. It collapsed the two module-level
  probe statics the retired `load_runtime.rs` carried while keeping both public
  setter names.
- **The evidence surface gained a verdict the workflow used to throw away.**
  `LoadOutcome::RefusedAsStale` is distinct from `Failed` and from `Cancelled`: a
  completion the workflow declines to publish is neither a user-visible failure
  nor a user cancellation, and conflating them would hide the freshness seam. It
  is recorded at the two publish-refusal sites rather than inferred by a test.
- **A data-safety fix landed with the migration, not after it.** Every terminal
  now either carries a parked request's background planning owner into the restart
  or releases it; the pre-migration finalization path dropped it, which would
  strand whoever was waiting on the terminal — the session-restore sequencer
  counts exactly those releases to decide when to open the next document. See
  the appendix's data-safety record.
- **The recent-documents surface was an outright census gap**, now assigned. See
  [The recent-documents surface census gap](#the-recent-documents-surface-census-gap).

### WFR-SESSION-RESTORE

**Migrated by slot 4, second of the slot's four rows** — the row the matrix called
"the closest workflow in the tree to the target shape", which proved half right:
its bounded-turn policy really was already policy and merely mislocated, but its
journal half was entirely inline in the GTK adapter.

- facade: `crates/lushtext-core/src/ui/window/session_restore/mod.rs` — **165** physical lines of the 370 budget
- coordination: `crates/lushtext-core/src/ui/window/session_restore/journal.rs`, `crates/lushtext-core/src/ui/window/session_restore/admission.rs`, `crates/lushtext-core/src/ui/window/session_restore/execution.rs`
- policy: `crates/lushtext-core/src/ui/window/session_restore/policy.rs`
- evidence: `crates/lushtext-core/src/ui/window/session_restore/evidence.rs`
- mutation parity: `openspec/changes/migrate-user-content-restore-workflow-readability/evidence/mutation-session-restore-policy.md` — **reports a relocation and a gain from zero separately**, because the admission half moved and the journal half did not exist as policy

**Three coordination modules, and `journal` is the one that needed justifying.**
The session file passes slot 3a's test twice over: the next launch reads it back,
**and within one run a close reads it back** so a restore that never finished
cannot delete the descriptors it had not reached. Per slot 2b's definition the
record's mutual-exclusion gate lives inside the journal — here that gate is the
`save_debounce` generation, which decides both which write wins and whether a
late success may clear a newer failure.

**The stored-evidence oddity, resolved.** `ui/window/imp.rs` held
`Cell<Option<SessionRestoreEvidence>>`. The verdict is that it is a **last-restore
outcome record**, not a cached surface: the runtime owning those counters is
*taken* at the terminal, so without retaining them an observer could never learn
how the restore that just finished behaved. It is renamed
`last_restore_outcome: Cell<Option<SessionRestoreTurnMetrics>>` so it cannot read
as a cache of the surface, and the surface **projects** it.

**`session.save_failed` ownership, carried forward from slot 3a verbatim because
it is the reusable lesson:** a field whose name contains "save" is not thereby
save-workflow state. It is session-*file* save failure, written and cleared only
by this workflow's journal, and its three widget-test read sites are now evidence
reads.

**Shared-field owners a reader needs at this seam** (full table in the facade's
module doc): `close_safety_inflight` and `close_safety_bypass` are **genuinely
shared** with `WFR-DRAFT-RECOVERY` — one close-safety pass runs both halves and
the bypass releases the final close only after both — so both workflows project
them and neither owns them; the close-save identity pair belongs to migrated
`WFR-DOCUMENT-SAVE`; `tab_projection_publications` and the projection batch belong
to the tab workflow; the draft records are handed to `WFR-DRAFT-RECOVERY` through
its own `adopt_startup_draft_records`; and `ui/window/startup_data.rs` is owned by
**neither** this row nor drafts.

### WFR-LOCAL-HISTORY

**Migrated by slot 4, third of the slot's four rows** — the two-directory row, and
the one the slot existed to force a decision about.

- facade: `crates/lushtext-core/src/ui/window/local_history/mod.rs` — **216** physical lines of the 370 budget
- coordination: `crates/lushtext-core/src/ui/window/local_history/journal.rs`, `crates/lushtext-core/src/ui/window/local_history/preview_execution.rs`, `crates/lushtext-core/src/ui/window/local_history/restore_execution.rs`
- policy: `crates/lushtext-core/src/ui/window/local_history/policy.rs`
- evidence: `crates/lushtext-core/src/ui/window/local_history/evidence.rs`
- mutation parity: `openspec/changes/migrate-user-content-restore-workflow-readability/evidence/mutation-local-history-policy.md` — **records a gain from zero**, because `model/local_history.rs` stays in `model/` and was not edited

**The two-directory decision, resolved on the coordination/presentation line** —
the split slot 3b used for the recent-documents surface.
`ui/window/local_history/` is the **canonical role home** and holds the facade,
all coordination, the single `policy.rs`, and the single `evidence.rs`.
`ui/editor_page/local_history.rs` is a **called surface** whose ownership is
recorded in its own module doc: it is per-tab presentation-adjacent capture,
watching one buffer's clean/modified transitions. It owns no policy and no
evidence — it **calls** the canonical `policy.rs` for both its freshness tickets,
which is what makes the one-policy-per-workflow rule honest here rather than
nominal. Two `policy.rs` files for one row was the alternative, and it is exactly
what the convention forbids.

**Its consumer surface was wider than the row recorded, and two corrections
matter.** The `Entry points` cell omitted the **sidebar context-menu path**
entirely, and it omitted that **migrated `WFR-DOCUMENT-SAVE` drives a capture on
every successful save**. An entry point missing from a census is how slot 3b
discovered an outright census gap; this is the second instance.

**One surface for a workflow spanning two directories.** `LocalHistoryEvidence`
lives in the canonical home and reads the editor page's capture state, folding in
**both** pre-convention typed observations. The disposed-widget rule was a **live
hazard here, not a formality**: the first cut of the surface called
`live_local_history_availability()`, which derefs the source-view template child,
and the required disposal proof test caught the panic. The fix split out
`live_local_history_availability_for_chars`, so an observer that already read the
buffer through `try_get()` does not read it again through the panicking accessor.

### WFR-DRAFT-RECOVERY

**Migrated by slot 4, last of the slot's four rows** — deliberately last: the
largest, the most inversions in the programme, and the only workflow whose cleanup
path deletes user content by design.

- facade: `crates/lushtext-core/src/ui/window/drafts/mod.rs` — **310** physical lines of the 370 budget, the programme's largest facade and its closest approach to the ceiling
- coordination: `crates/lushtext-core/src/ui/window/drafts/journal.rs`, `crates/lushtext-core/src/ui/window/drafts/admission.rs`, `crates/lushtext-core/src/ui/window/drafts/autosave_execution.rs`, `crates/lushtext-core/src/ui/window/drafts/restore_execution.rs`
- policy: `crates/lushtext-core/src/ui/window/drafts/policy.rs`
- evidence: `crates/lushtext-core/src/ui/window/drafts/evidence.rs`
- mutation parity: `openspec/changes/migrate-user-content-restore-workflow-readability/evidence/mutation-draft-recovery-policy.md` — **reports a gain from zero and one relocation separately**: the extracted decisions gained coverage, and the `DraftMutationOrder` epoch allocator relocated whole from the retired draft-ordering module with parity proved

**Four coordination modules, and `journal` absorbed the one the task list
expected to be `retirement`.** Orphan cleanup destroys payloads the workflow is
finished with, so `retirement` was checked against it and **rejected**:
`retirement` in this codebase means the disposal lane's off-GTK destruction of an
*in-memory* payload, while orphan cleanup reloads *this* manifest under *this*
record's write lock, is gated by *this* record's authority, and merges back into
*this* record. Applying the cohesion test — would a reader look for it under its
own name — the answer is that "what keeps the manifest consistent with the bodies
on disk" is a journal question. `DraftCleanupContinuation`'s manifest offset
therefore lives with the journal it protects, and is **reclassified** from a seam
value object to exactly that: an offset into the record.

**Two execution stage orders, both stage-order-qualified.** `autosave_execution`
and `restore_execution` are both new, so neither is a stable sibling renamed for
symmetry. The autosave and close-flush pipelines share one module because they
share their shape exactly and differ only in admission rule and terminal.

**This row also carries the slot's confirmed data-safety fix.** The autosave lane
never consulted `installation_incomplete`, so a cancelled load installation — which
empties the buffer and clears `modified` without clearing `draft_dirty` — let one
keystroke make a near-empty buffer look like an ordinary dirty candidate, and the
next pass wrote it over a draft holding real unsaved work. The guard is now
`policy::draft_candidate_is_eligible`'s own term, mutation-tested, and the
regression test is proven to fail without it.

**Shared-field owners a reader needs at this seam** (full table in the facade's
module doc): `close_safety_*` shared with `WFR-SESSION-RESTORE`;
`imp().load.installation_incomplete` read through migrated
`WFR-DOCUMENT-LOAD`'s `has_incomplete_load_installation()`; `collect_session`
called from `WFR-SESSION-RESTORE`; the local-history baseline seeded through
`WFR-LOCAL-HISTORY`'s named operation; and `ui/window/startup_data.rs`, which
calls `start_autosave_timer` and is owned by neither restore row.

### WFR-BUFFER-REPLACEMENT

**Migrated by slot 4, first of the slot's four rows** — deliberately first,
because the other two in-slot restore paths and two out-of-slot workflows all
drive their bytes through it, so its boundary had to be settled before anything
was built on it.

- facade: `crates/lushtext-core/src/ui/editor_page/buffer_replacement/mod.rs` — **167** physical lines of the 370 budget, the programme's smallest facade so far
- coordination: `crates/lushtext-core/src/ui/editor_page/buffer_replacement/execution.rs`
- policy: `crates/lushtext-core/src/ui/editor_page/buffer_replacement/policy.rs`
- evidence: `crates/lushtext-core/src/ui/editor_page/buffer_replacement/evidence.rs`
- mutation parity: `openspec/changes/migrate-user-content-restore-workflow-readability/evidence/mutation-buffer-replacement.md` — **records a gain from zero, not a relocation parity**, because `model/buffer_replacement.rs` stays cross-cutting and was not edited

**One coordination module, and the cohesion test says one is right.** The
workflow has a single stage order and a single deferred mechanism. `journal` does
not apply — nothing durable is written. A separate `admission` was rejected:
supersession is the facade's entry decision plus `execution`'s session ownership,
and splitting them would put two halves of one question in two files.

**The `policy: none` probe found policy, so this row declares one.** The proposal
expected this to be the programme's first `policy: none` row; the probe required
by the spec delta found five separable pure decisions in the GTK adapter, four of
them determining whether a partially mutated buffer can be seen or whether a
caller learns the truth about its terminal. Gain from zero: **19 mutants
generated, 15 killed, 0 missed, 4 unviable** (the four are
`-> Enum with Default::default()` on return types that deliberately implement no
`Default`). Amendment (a) therefore ships **stated but not exercised**; see the
slot-4 friction section.

**The cross-cutting module stays and is called, never copied.**
`model/buffer_replacement.rs` owns the direct/sliced threshold, the clear slice
budget, and `next_replacement_boundary` — the **paragraph-boundary contract** that
keeps recovering a 33 MB single-line draft linear instead of quadratic. Three
owning workflows consume it directly (`WFR-BUFFER-REPLACEMENT`,
`WFR-DOCUMENT-LOAD` via `load/policy.rs` and the `next_install_boundary` synonym,
and `WFR-LOCAL-HISTORY`'s preview installer), and none duplicates it. Slot 4 kept
the one-line `model::file_load::next_install_boundary` synonym and strengthened
its doc comment to name the owner and the contract; see the change's task 2.1.

**Evidence surface, with all three obligations discharged.**
`BufferReplacementEvidence` via `buffer_replacement_evidence()` folds in all four
retired inspection seams. Tight-borrow: every derived scalar is computed and every
`Ref` dropped before the struct literal. **Disposed-widget: this workflow's whole
subject is the source view's buffer**, so `buffer_char_count` reads through
`TemplateChild::try_get()` and answers `None` — not zero — for a disposed page,
proved by `editor_page::test_buffer_replacement_evidence_reads_survive_widget_disposal`.
Reentrancy: `editor_page::test_buffer_replacement_evidence_reads_stay_side_effect_free_across_replacement_mutation`
drives the workflow through session install, per-turn mutation, pending-request
parking, the terminal, and disposal, reading **after** each and asserting repeated
reads of unchanged state are identical.

**One deliberate production change, called out because it is not purely
structural.** `BufferReplacementTerminalDiagnostic` and the editor's
`last_terminal` slot were `#[cfg(feature = "test-utils")]` and are now always
compiled, because an evidence surface must be readable in a production build. The
cost is one `Copy` struct of scalars per editor page — ticket, optional cancel
reason, four metrics, two release flags — and it carries **no document content**,
which the row's redaction contract requires. Nothing else about the workflow's
behavior changed; two behavior-preservation slips made during the migration were
caught and are recorded in the mutation evidence.

**Shared-field owners a reader needs at this seam** (full table in the facade's
module doc): the guard suspends and exactly restores `imp().minimap.tracking_suspended`,
`imp().local_history.automatic_capture_suppressed`, `imp().monitor.file_monitor`,
and the search bar's context, all owned elsewhere; `imp().load.projection_suspended`
is never written here, and `load_projection_suspended` reads both flags because a
projection must stand down for either workflow's suspension.

### WFR-COMMAND-PALETTE

- facade: `ui/command_palette/mod.rs`
- coordination: `ui/command_palette/query_execution.rs`, `ui/command_palette/index_admission.rs`, `ui/command_palette/index_execution.rs`, `ui/command_palette/retirement.rs`
- policy: `ui/command_palette/policy.rs`
- evidence: `ui/command_palette/evidence.rs`
- mutation parity: `openspec/changes/archive/2026-08-25-migrate-command-palette-workflow-readability/evidence/mutation-parity-palette-policy.md`

Notes on this row, which is the second migration and the first to exercise the
stage-order qualification rule:

- `runtime.rs` is gone. It was the census's clearest example of the name the
  convention rejects: 59 lines holding a seam value object, a worker entry
  point, and a test-only delay static. The value object and the worker entry
  moved to `query_execution.rs`; the static moved to `test_policy.rs`.
- **The palette owns two `execution` modules, qualified by stage order.** The
  query flight and the incremental file-index mutation are separate ordered stage
  orders and each has a submit/dispatch/arbitrate shape, so they are
  `query_execution.rs` and `index_execution.rs`. `index_admission.rs` is the
  mutation queue's bounded retention, its 75 ms debounce, its disposal-capacity
  retry, and its flush gate; `retirement.rs` needs no qualifier because only one
  stage order retires anything. The recommended pre-implementation mapping put
  the mutation worker inside `admission.rs`; that was rejected because it would
  overload a bounded role name to avoid the collision, which is exactly what the
  qualification rule exists to prevent.
- `imp.rs` keeps the template children, the list factory, the accessible-state
  projection, and the source-installation helpers. It is adapter detail, not a
  coordination role.
- `test_policy.rs` holds `CommandPaletteTestPolicy`, the workflow's single
  test-only timing value, behind `#[cfg(feature = "test-utils")]`. It replaced
  two independent statics with two public setters that sat ahead of the
  workflow's logic in `mod.rs` and `runtime.rs`.
- `policy.rs` is `pub` for the same reason the exemplar's is: GTK-free pure types
  addressed directly from outside the workflow's private module tree. Nothing
  outside the workflow mutates through it.
- Two test-serving `Cell` fields (`observed_search_cancellations`,
  `last_cancelled_search_examined`) were unconditional production fields before
  this change. They are now behind the workflow's test feature, so a
  default-feature build compiles no storage for them.
- The three process-global retirement counters stay lifecycle probes rather than
  evidence fields. They are monotonic process accumulators answering "did this
  process ever observe a last-owned at-cap retirement", which a per-widget
  evidence field cannot express; folding them in would silently change their
  meaning. They moved into `retirement.rs` beside the classification policy they
  instrument.
- `FileIndexBuildCoordinator` in `services/palette/index.rs` is now a
  palette-named alias over the shared `SingleFlightCoordinator`, the way
  `services/palette/runtime.rs` already aliases `PaletteSearchCoordinator`. Its
  snapshot gains the shared type's two high-water fields; the only readers were
  `services/palette/tests.rs` and `benches/benchmarks.rs`, and both read only
  `active`, `pending`, and `started`.
- Mutation coverage is a **gain, not a relocation**: `ui/command_palette/**` was
  outside `examine_globs` before this change, so the pre-move baseline is zero by
  construction. The evidence file states that asymmetry rather than claiming
  "0 → 0, parity holds".

## Completion Rule

A workflow may be marked `migrated` only when all of the following hold, and
`make check-policy` enforces the mechanical parts:

- The row names the facade, coordination, policy, and evidence roles that exist.
- Any pure policy the workflow owns lives in a `policy.rs` inside the
  workflow's directory, contains no `gtk4`, `glib`, `gio`, `libadwaita`, or
  `sourceview5` import, and is reachable by the mutation scope.
- Mutation coverage parity evidence exists for every relocated policy module.
- The seam value object named in the row exists, is constructed once at the
  workflow entry point, and is validated as a unit.
- The workflow's inspection seams are readable from one evidence surface, the
  retired per-field functions have no remaining callers, and the project test
  count has not decreased.
- Any automation snapshot field for this workflow projects from the evidence
  surface, with the exported D-Bus fields, names, and semantics unchanged.

### Slot 2b amendment re-check

**Slot 2b's amendment, and the per-row re-check it owed.** Slot 2b added
`journal` to the bounded coordination role set. Adding a role name cannot
invalidate an existing correct name, so the obligation was a confirmation rather
than a rename, and each already-migrated row was checked explicitly:

| Row | Declared coordination names | Verdict under the amended set |
| --- | --- | --- |
| `WFR-SEARCH-REPLACE` | `execution`, `retirement`, `replace_execution`, `journal` | correct. `journal` is the newly added name and is the accurate one for that module; the other three are unchanged and still accurate |
| `WFR-COMMAND-PALETTE` | `query_execution`, `index_admission`, `index_execution`, `retirement` | correct and unchanged. None of the four is a durable generation-guarded record a later stage reads back, so none should become `journal`. `retirement.rs` there is index-retirement accounting, which is destruction, not preservation |

No rename was required, and no second generation of the convention exists in the
tree. The other settled conventions were re-checked too and are unchanged by this
amendment: the facade budget stays 370 (both migrated facades measured against
it — search 369, palette 335), the seam value-object shape is unchanged, and the
evidence-surface visibility rule is unchanged.

**Two things about the amendment a future adopter must not have to rediscover.**

- **The role list is now closed, where it used to read as open.** The
  pre-amendment spec sentence said the coordination role is named "from a bounded
  set of role names that state the job the module performs, **such as** admission,
  execution, retirement, or watch". Slot 2b's delta replaces that with a closed
  enumeration: "`admission`, `execution`, `retirement`, `watch`, and `journal`".
  That is a deliberate tightening, disclosed here because it is easy to miss in a
  diff that reads like it only appends a fifth name. It changes nothing
  operationally — the same requirement already said a job no existing name
  describes "MUST be added to the bounded set by amending this specification", so
  an off-list name always required an amendment — and it matches how the
  convention is written everywhere else it is normative (`.agents/rules/rust.md`'s
  bounded set and the programme record both enumerate rather than exemplify). Any
  future off-list name still requires amending
  `openspec/specs/gtk-adapter-module-boundaries/spec.md`.
- **`journal` includes its admission gate; it is not a persistence-only role.**
  The role covers the mutual-exclusion gate that serializes the workflow's apply
  and undo transactions, and the disposal reservation those transactions take,
  as well as the generation-guarded install and clear, the worker-side write and
  delete, startup recovery with stale-record cleanup, and the hand-back. Slot 2b
  considered splitting the gate and the reservation into a separate
  `undo_admission.rs` and **rejected it**: two small jobs whose whole purpose is
  to protect one durable record do not justify a third module plus the facade
  narration churn of a sixth stage. The next `journal` adopter should copy that
  boundary rather than re-derive it — a `journal` module is expected to own the
  admission of the thing it journals.

### Evidence pointer form

`make check-workflow-boundaries` resolves a **live-form** pointer under the live changes directory
against the matching archived change directory,
but it does **not** resolve an archive-prefixed claim before the change is
actually archived. The tolerance runs one way only, which has a practical
consequence worth recording once:

- **A live change records its own evidence pointers in live form.** An
  archive-prefixed pointer fails the gate immediately, because the archive
  directory does not exist yet.
- **An archived change's pointers are rewritten to archive form**, so a human
  following the path finds the file. The gate accepts both at that point, but
  only the archive form is a real path on disk.

Slot 3a fixed the two already-archived pointers that were still in live form
(slot 1's search-policy parity reference and the palette row's parity pointer)
and left its own in live form, which is the only form that can pass while the
change is live. Rewriting it is part of archiving.

### Slot 3a amendment re-check

**Slot 3a's amendment, and the per-row re-check it owed.** Slot 3a amended
`gtk-adapter-module-boundaries` to add the **per-workflow subdirectory** as a
second permitted role home, because `ui/editor_page/` hosts eight workflows and
the fixed `policy.rs` / `evidence.rs` names cannot be shared. Adding a permitted
*location* cannot invalidate a correct existing location, so the obligation was
again a confirmation rather than a rename, and each already-migrated row was
checked explicitly:

| Row | Declared role home | Verdict under the amended text |
| --- | --- | --- |
| `WFR-SEARCH-REPLACE` | dedicated directory `ui/search_panel/` with `mod.rs` as the facade | correct and unchanged. The directory hosts exactly one workflow, so its flat `policy.rs` / `evidence.rs` / bounded coordination names collide with nothing and the amendment leaves them untouched. No rename |
| `WFR-COMMAND-PALETTE` | dedicated directory `ui/command_palette/` with `mod.rs` as the facade | correct and unchanged, for the same reason. Its two stage-order-qualified execution modules are a separate rule (slot 2a's) that this amendment does not touch. No rename |

Zero renames, and no second generation of the convention exists in the tree. The
other settled conventions were re-checked and are unchanged by this amendment:
**the facade budget stays 370** with both migrated facades measured against it
(search 369, palette 335) and the new save facade measured against the same
number; the bounded coordination role set is unchanged at `admission`,
`execution`, `retirement`, `watch`, `journal`; the seam value-object shape
(Ticket + Facts + one predicate, or a coordinator that owns the generation) is
unchanged; and the evidence-surface visibility rule is unchanged.

**One word of preserved scenario text moved, disclosed so it is not read as a
silent rewrite.** The existing scenario "One directory hosting several workflows
keeps flat role names" ended "migration does not require restructuring the
directory into one subdirectory per workflow"; it now reads "restructuring the
**whole** directory into one subdirectory per workflow". Without that word the
sentence can be read as forbidding the per-workflow subdirectory the amendment
permits, when what it actually rules out is a wholesale directory restructuring.
The scenario's force is unchanged.

**Cost note for the next amendment.** Three migrated rows now exist, so the next
convention amendment owes three per-row re-checks rather than two.

### Slot 3b amendment re-check

**Slot 3b's amendment.** Slot 3b amended `workflow-evidence-surfaces` to promote
the **evidence-surface reentrancy constraint** from a per-workflow module note on
the exemplar into stated convention, with a required proof test. Slot 2b handed
this promotion forward explicitly, having obeyed the constraint while adding ten
fields and recorded that it "should become a stated convention, not a
per-workflow module note".

**Unlike the previous two amendments, this one was not a confirmation.** The
amended requirement's content *is* a proof obligation, so "the constraint already
held" is not enough — each migrated row owed the test, in the right shape. Three
rows were migrated when this change started; each was checked individually, and
the verdict is recorded per row:

| Row | Proof test | Verdict |
| --- | --- | --- |
| `WFR-SEARCH-REPLACE` | `search_panel::test_evidence_reads_stay_side_effect_free_across_journal_mutation` | **existed**, written by slot 2b as the reference implementation. Re-read against the amended text: it drives the workflow through each operation taking a mutable borrow of the state the accessor reads (query, preview enter/exit, undo-backup install, undo-backup clear), reads after each, and asserts repeated reads of unchanged state are identical. Correct shape, no change needed |
| `WFR-DOCUMENT-SAVE` | `editor_page::test_save_evidence_reads_stay_side_effect_free_across_save_mutation` | **existed**, written by slot 3a. Verified for *both* halves of the obligation: it reads after the queue stage, after the admitted ticket is installed under a `borrow_mut()`, and after the terminal clears it, and it asserts `first_read == second_read`. It does **not** read the surface while a borrow is held. Correct shape, no change needed |
| `WFR-COMMAND-PALETTE` | `command_palette::test_evidence_reads_stay_side_effect_free_across_palette_mutation` | **missing — written by slot 3b.** The palette had only a teardown-observation test (`test_evidence_reads_survive_widget_disposal`), which proves a different property. The new test drives the workflow through source installation, mode change, query submission into the single-flight coordinator, and three bounded index mutations, reading after each, then asserts repeated reads of query, mode, result count, searching, source counts, queue depth, queue bytes, and coordinator counters are identical |

This is the amendment the retroactive rule was written for: one row genuinely
lacked the proof, and leaving it would have meant two generations of the
convention coexisting — one where the constraint is proved and one where it is
merely true.

**The other settled conventions were re-checked and are unchanged by this
amendment**: the facade budget stays **370** with all four migrated facades
measured against it (search 369, palette 335, save 223, load 273); the bounded
coordination role set is unchanged at `admission`, `execution`, `retirement`,
`watch`, `journal`; the seam value-object shape (Ticket + Facts + one predicate,
the `is_current(&editor)` variant, or a coordinator that owns the generation) is
unchanged; the evidence-surface **visibility** rule is unchanged; and slot 3a's
per-workflow subdirectory role home is unchanged — slot 3b is its second adopter
and changed nothing about it.

**Cost note for the next amendment.** Four migrated rows now exist, so the next
convention amendment owes four per-row re-checks. Slot 4 migrates four more rows,
which would take it to eight. The record's warning that the cheapest moment to
correct a convention is early is now measurably true: this amendment cost one
test to write; the same amendment after slot 4 would have cost up to five.

### Slot 4 amendment re-check

**Slot 4's two amendments.** Slot 4 amended `workflow-readability-boundaries`
twice, both closing adjacencies the convention already sanctioned:

- **(a) A migrated workflow whose only pure policy is cross-cutting is a complete
  row.** `scripts/check-workflow-boundaries.py` already lists `policy` in
  `OPTIONAL_ROLE_VALUES`, so `policy: none` was gate-tolerated, but **no spec
  scenario said such a row is complete** — the permission read as gate tolerance
  rather than convention. Confirmed from the code and the main spec before
  amending.
- **(b) A migration re-derives its row's measured cells rather than inheriting
  them.** Confirmed from three friction-section instructions: "**Re-derive, and
  expect the answer to move in either direction**" (slot 3b), "**Re-derive
  row-scoped counts before sizing evidence work**" (slot 3a), and "**Write the
  narration from the code, every time**" (slot 3a, on the adjacent stage-trace
  case).

**(a) is a confirmation for all four rows.** None of `WFR-SEARCH-REPLACE`,
`WFR-COMMAND-PALETTE`, `WFR-DOCUMENT-SAVE`, or `WFR-DOCUMENT-LOAD` declares
`policy: none`; each declares a relocated or extracted `policy.rs`. Nothing to
fill.

**(b) is not a confirmation. Two of the four rows had gaps, and slot 4 filled
them.** This is the second consecutive amendment where "it must already hold" was
not a discharge.

| Row | Verdict against (b) | Action |
| --- | --- | --- |
| `WFR-DOCUMENT-SAVE` | **compliant.** Slot 3a re-derived `5 files, 1,855 production lines`, counted non-`#[cfg(test)]` lines only, stated the old cell was "wrong in both directions", and named the pooled population (`editor_io.rs` 3,035 and `durable_write.rs` 1,228) with the rows that share it | none |
| `WFR-DOCUMENT-LOAD` | **compliant.** Slot 3b re-derived `7 files, 2,375 production lines`, named the pooled `editor_io.rs` and the window files the row only calls, and named the rows sharing them | none |
| `WFR-SEARCH-REPLACE` | **gap.** The size cell still carried the census figure `19 files, 13,686 lines (ui 5,422 / model 2,369 / services 5,895)` with `exemplar scope 4,762` appended. The `services 5,895` subtotal was never row-scoped and the pooled population was never named. The `Seams` cell *was* re-derived | size cell re-derived below |
| `WFR-COMMAND-PALETTE` | **partial gap.** The size cell re-derived the `ui` subtotal and named one census misattribution (`ui/window/focus_indexing.rs`, 856, which "stays window code"), but kept `services 7,897` pooled with no sharing rows named | size cell completed below |

#### Filled: `WFR-SEARCH-REPLACE` row-scoped size

**14 files, 5,527 production lines** in `ui/search_panel/**`, non-`#[cfg(test)]`
only: `mod.rs` 369, `imp.rs` 828, `journal.rs` 692, `execution.rs` 537,
`replace_execution.rs` 530, `evidence.rs` 515, `list_factory.rs` 503,
`retirement.rs` 375, `policy.rs` 349 (of 728; 379 are the module's co-located
unit tests), `history.rs` 212, `results.rs` 194, `item.rs` 164,
`test_policy.rs` 155, `accessibility.rs` 104.

Pooled population the old `services 5,895` had shared, now named:
`services/content_search/**` (1,978 production lines: `replace.rs` 1,533 of
3,104, `search.rs` 414 of 1,336, `mod.rs` 31) shared with `WFR-EDITOR-FIND` and
the fault-injection lane; `services/search_backup.rs` (1,073 of 1,825), which
slot 2b decided stays in services; and `services/saved_searches.rs` (68 of 175),
shared with the palette's saved-search source. The census figure counted whole
files including their co-located tests.

#### Filled: `WFR-COMMAND-PALETTE` row-scoped size

**10 files, 2,534 production lines** in `ui/command_palette/**`,
non-`#[cfg(test)]` only: `imp.rs` 705, `policy.rs` 419 (of 860; 441 are
co-located unit tests), `mod.rs` 335, `evidence.rs` 210, `item.rs` 185,
`query_execution.rs` 186, `index_execution.rs` 176, `index_admission.rs` 119,
`test_policy.rs` 100, `retirement.rs` 99.

Pooled population the old `services 7,897` had shared, now named:
`services/palette/**` is 4,829 production lines once
`services/palette/tests.rs` (1,223) is excluded — it is a separate file declared
`#[cfg(test)] mod tests;`, so a naive per-file scan counts it as production. Of
that, `notes.rs` (2,163 of 3,428) is shared with **`WFR-NOTES-BOOKMARKS`**
(slot 5) and must not be re-derived from that side; `index.rs` (1,288 of 1,523),
`commands.rs` (507 of 570), `fuzzy.rs` (358 of 467), `grouped.rs` (290),
`runtime.rs` (86 of 130), `charge_scope.rs` (77), and `mod.rs` (60 of 62) are the
palette's own service half and stay in services.

**The other settled conventions were re-checked and are unchanged by these
amendments**: the facade budget stays **370**; the bounded coordination role set
is unchanged at `admission`, `execution`, `retirement`, `watch`, `journal` —
slot 4's `journal` verdict used the existing name for all three of its durable
rows and needed no addition; the seam value-object shape is unchanged; the
evidence-surface visibility rule and slot 3b's reentrancy constraint are
unchanged; cross-cutting eligibility is unchanged and amendment (a) only states
its already-implied consequence; the evidence pointer form is unchanged; and slot
3a's per-workflow subdirectory role home is unchanged, with slot 4 as its third
through sixth adopters.

**Cost note.** This re-check cost two size-cell re-derivations across two rows.
After slot 4 the count of migrated rows is eight, so the next amendment owes
**eight** per-row re-checks.

#### Re-derive at the end, not when the role file is written

Slot 4's own four size cells were re-measured once every edit was final, and
**three of the four had drifted during the change itself**:

| Row | Cell when written | Cell at final measurement |
| --- | --- | --- |
| `WFR-BUFFER-REPLACEMENT` | 1,455 (execution 897 of 948, policy 220 of 359) | **1,462** (execution 912 of 964, policy 212 of 339) |
| `WFR-SESSION-RESTORE` | 1,744 (policy 541 of 766) | **1,756** (policy 553 of 1,172) |
| `WFR-LOCAL-HISTORY` | 2,995 (policy 313 of 558) | **3,001** (policy 319 of 650) |
| `WFR-DRAFT-RECOVERY` | 3,106 | **3,106** — unchanged |

The cause is mundane and will recur in every slot: the size cell gets written when
the role file is finished, and then **mutation-survivor triage adds tests and
occasionally moves a line of production code**. Session restore's policy file grew
from 766 raw lines to 1,172 that way — its co-located test module more than
doubled while its production half moved by only 12 lines.

So amendment (b) needs a *timing* rider, and this is it: **re-derive the measured
cells as the last documentation step, after the final test and mutation runs**, not
when the module is written. A cell measured mid-change is stale by construction if
any survivor is closed afterwards, and closing survivors is a required step of
every migration. Note also which direction the drift ran — production sizes moved
by 6 to 12 lines while raw totals moved by up to 406, so a reviewer eyeballing raw
`wc -l` would badly misjudge whether production code had grown.

### Retroactive amendment

A change that amends the convention MUST re-migrate every row already marked
`migrated` in the same change. Two generations of the convention MUST NOT
coexist in the tree.

This applies to the role names, the facade budget number, the seam value-object
shape, the evidence-surface visibility rule, and anything else recorded in
[Settled Conventions](#settled-conventions) or the two capability specs. A later
migration therefore cannot fork the convention: either it follows what is
recorded here, or it pays to re-migrate every earlier workflow. The practical
consequence is that the cheapest moment to correct a convention is while exactly
one workflow is migrated, which is slot 2. The same rule is recorded in
`docs/next/workflow-readability.md` and `AGENTS.md`.
