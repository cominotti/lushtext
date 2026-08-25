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
| WFR-SEARCH-REPLACE | Workspace search and Replace All | 19 files, 13,686 lines (ui 5,422 / model 2,369 / services 5,895); exemplar scope 4,762 | `win.begin-search`, `Ctrl+Shift+F`, search entry changed, Replace All button, Undo button | relocated to `crates/lushtext-core/src/ui/search_panel/policy.rs`: slot 1 moved the two `model/` search policy modules named in the [Policy Module Census](#policy-module-census); slot 2b added the Replace All durable half's decisions (preview reservation and shrink-to weights, the saturating retained-byte cast, the undo-capacity admission plan, the journal generation predicate, `ReplaceApplyCounts`). Mutation parity proved for both, and slot 2b's move was out-of-scope-to-in-scope, so it *gained* 11 mutants with zero survivors. Domain `model/workspace_search.rs` stays | both halves migrated: slot 1 retired 8 inspection fns into `evidence.rs` and collapsed 5 configuration setters plus 6 override statics into `SearchPanelTestPolicy`; slot 2b added 10 evidence fields and needed no new inspection fn, no new override, and no new actuation seam. 5 actuation seams remain on the replace/undo transaction (`clear_undo_backup_for_test`, `reserve_undo_backup_generation_for_test`, `set_persisted_undo_backup_for_generation_for_test`, `begin_replace_transaction_for_test`, `finish_replace_transaction_for_test`) plus 2 accessibility probes, all deferred at programme level. Write-side seams in `services/content_search/replace.rs` are classified under [Migrated Workflow Roles](#wfr-search-replace) | **3 seams reified** (the programme's primary unit; long signatures shortened remains 0 for this row, and no function in it has 6 or more non-receiver parameters, so neither the receiver-counted 88 nor the strict 43 signature figure applies here). exists: `WorkspaceSearchRequest` + `WorkspaceSearchStart` (search side), `ReplacePreviewTicket` + `ReplacePreviewFacts` (preview-freshness side). new: `UndoRestoreClaim` (the window's undo-restore claim seam, naming the transaction-busy and capacity-deferred refusals the panel must restore the affordance for) | exists: `SearchPanelEvidence` via `evidence()`, extended by slot 2b with the apply transaction's own pending flag, all three generations, the preview capacity-retry state, the installed journal's entry count and retained weight, in-flight and cumulative journal disk-job counters, and the last apply's counts; automation `window.content_search` projects from it and slot 2b widened nothing | tier-2 (search/preview half), tier-3 (Replace All write half) — **both now covered** | 1 (search/preview half) + 2b (replace/undo half) | migrated |
| WFR-COMMAND-PALETTE | Command palette, file index, notes browse modes | 16 files, 11,179 lines (ui 2,528 / model 754 / services 7,897); the `ui` subtotal is `ui/command_palette/**` (1,672) plus `ui/window/focus_indexing.rs` (856), which the census attributed to this row but which stays window code | `win.toggle-command-palette` (`Ctrl+Shift+P`), palette mode dropdown and Tab cycling, sidebar file create/delete/rename and watcher reconciliation, `win.notes-show-notes` (the census cell said `Ctrl+P` / `Ctrl+K`; the real palette accelerator is `Ctrl+Shift+P` and `Ctrl+K` belongs to the recent-Open popover) | relocated to `crates/lushtext-core/src/ui/command_palette/policy.rs` (queue admission byte/cap math, batch-kind selection and flush guard, mutation-generation arbitration and replay, retirement-cap classification, header-skipping navigation, presentation decisions); mutation coverage gained, not relocated — see [Migrated Workflow Roles](#wfr-command-palette). Domain `model/palette.rs` (17 consumers) stays | palette half migrated: 5 inspection fns retired into `evidence.rs`, 2 configuration setters plus 2 override statics collapsed into `CommandPaletteTestPolicy`; 3 process-global retirement counters retained as lifecycle probes (per-widget folding would change their meaning from "this process observed a last-owned at-cap retirement" to a count no test asks for); 3 actuation seams deferred. The pre-migration cell read `15/10/2/0 = 27 fns, 40 sites, 4 override statics`, which was row-scoped: `ui/command_palette/**` held 12 of those functions and 22 of the gate-attribute sites, and the other 15 live in `services/palette/**` and `ui/window/notes/**` shared with `WFR-NOTES-BOOKMARKS` (slot 5) | exists: `PaletteSearchCoordinator` generation identity + `is_current` (query seam; the convention accepts a coordinator that owns the generation as the seam value object). new: `FileIndexMutationTicket` + `FileIndexMutationFacts` + `arbitrate` (file-index mutation seam) | exists: `CommandPaletteEvidence` via `evidence()`; automation `window.command_palette` and both palette readiness blockers project from it | tier-2 | 2a | migrated |
| WFR-DOCUMENT-SAVE | Save, Save As, save formatting, durability | 7 files, 6,672 lines (ui 2,132 / model 991 / services 3,549) | `win.save`, `win.save-as`, `Ctrl+S`, close-with-changes dialog, autosave-on-close | `model/save_admission.rs` (2 consumer files, 1 workflow → single-consumer, relocates) | 10/11/9/4 = 34 fns, 44 sites, 5 override statics | exists: `SaveCompletionTicket` (completion seam). required: `QueuedSaveTicket` + `QueuedSaveFacts` (admission seam; carries the renamed field) | partial: `SaveAdmissionSnapshot` | tier-3 | 3 | pending |
| WFR-DOCUMENT-LOAD | Open document, reopen with encoding, recent documents | 10 files, 5,301 lines (ui 3,265 / model 661 / services 1,375) | `win.open-file`, `win.open-recent`, `Ctrl+O`, `Ctrl+K`, sidebar row activation, session restore | `model/file_load.rs` (4 consumers → domain-shaped, stays pending review in slot 3) | 23/7/3/1 = 34 fns, 55 sites, 3 override statics | required: `LoadRequestTicket` (carries `{load_generation, cancel_token}`, exploded at 2 call sites today) | partial: `FileLoadAdmissionSnapshot`, `OpenPopoverRowLayoutSnapshot` | tier-3 | 3 | pending |
| WFR-DRAFT-RECOVERY | Draft autosave, crash recovery, orphan cleanup | 6 files, 8,930 lines (ui 2,578 / model 442 / services 5,910) | first-dirty autosave timer, startup recovery scan, restored-draft inline alert, `Discard...` / `Save...` | `model/draft.rs` (9 consumers → domain, stays) | 7/18/3/0 = 28 fns, 53 sites, 14 override statics | exists: `DraftRestoreTicket` + `DraftRestoreFacts`; `DraftMutationIntent`; `DraftCleanupContinuation` | partial: `OrphanCleanupRuntimeSnapshot` | tier-3 | 4 | partially-conforming |
| WFR-SESSION-RESTORE | Session persistence and bounded restore | 5 files, 2,599 lines (ui 1,962 / model 300 / services 337) | app startup, window close, tab mutation persistence | `model/session.rs` (8 consumers → domain, stays) | 2/0/2/0 = 4 fns, 5 sites | exists: `SessionRestorePlanPermit` + `SessionRestoreAdmission` | exists: `SessionRestoreEvidence` via `evidence()` — the only canonical accessor in the tree | tier-3 | 4 | partially-conforming |
| WFR-LOCAL-HISTORY | Local history capture, preview, restore | 4 files, 5,536 lines (ui 2,586 / model 173 / services 2,777) | `win.show-local-history`, baseline capture on first edit, periodic capture timer, restore action | `model/local_history.rs` (6 consumers → domain, stays) | 9/11/4/0 = 24 fns, 33 sites, 4 override statics | exists: `BaselineCaptureTicket` + `BaselineCaptureFacts`; `PeriodicCaptureTicket` + `PeriodicCaptureFacts`; `LocalHistoryReplacementTicket` | partial: `LocalHistoryPreviewCoordinatorSnapshot`, `LocalHistoryPreviewInstallSnapshot` | tier-3 | 4 | partially-conforming |
| WFR-BUFFER-REPLACEMENT | Bounded buffer install and clear slices | 2 files, 1,215 lines (ui 1,029 / model 186) | local-history restore, Replace All undo, draft restore install | `model/buffer_replacement.rs` (2 consumer files, 2 workflows → cross-cutting between local-history and Replace All undo; stays) | 4/0/4/0 = 8 fns, 26 sites | exists: `BufferReplacementTicket` + `BufferReplacementSession` | none | tier-3 | 4 | partially-conforming |
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
| WFR-AUTOMATION-SPINE | Read-only D-Bus automation and action catalog | 5 files, 6,897 lines (ui 2,146 / model 2,195 / services 2,556) | D-Bus method calls, `scripts/lushtext-automation.py` | `model/action_catalog.rs` (3 consumers → domain, stays) | 0/0/2/0 = 2 fns, 2 sites | none required — the exported contract is the value object | exists: 18 `Automation*Snapshot` types; these become projections of workflow evidence as each workflow migrates. Two projections exist: `window.content_search` from `SearchPanelEvidence` (slot 1) and `window.command_palette` plus both palette readiness blockers from `CommandPaletteEvidence` (slot 2a). `make check-automation-docs` gates both against the `Evidence Projection Map` in `docs/automation-reference.md` | tier-1 | 2a onward, incrementally per migrated workflow | pending |
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

`win.save` → editor save entry → `SaveSubmission` → `save_runtime::submit` →
`SaveAdmissionPolicy::queue` → `schedule_drain()` ⇢ `glib::idle_add_once` drain,
resuming in `begin_admitted_save` → `queued_save_is_current` freshness gate →
`SaveCompletionTicket::capture` → buffer capture, either
`snapshot_buffer_text_direct` or ⇢ chunked async capture resuming per chunk →
`spawn_blocking_then` ⇢ worker write through `editor_io` and
`filesystem::write::atomic_replace`, resuming in the completion closure →
`SaveCompletionTicket::is_current(editor)` → save-formatting acceptance and
buffer mirror-back → `SavePayloadPermit` drop ⇢ `idle_add_once` release,
resuming in the permit release path → notifications, draft cleanup,
accessibility refresh.

Four inversions across six files. This is the workflow the programme cites as
requiring 13 hops to answer "what happens on Ctrl+S", and the census confirms
the hop count and the file spread.

### WFR-DOCUMENT-LOAD

`win.open-file` or sidebar activation → `open_document(path)` →
`open_document_with_intent` → tab creation → `load_file_async(path)` →
`load_runtime::cancel_for_editor` → load plan → `load_runtime::submit` ⇢
admission drain, resuming in the admitted load → `spawn_blocking_then` ⇢ worker
read and decode, resuming in the completion closure →
`load_request_is_current(generation, &cancel)` → `install_loaded_direct` ⇢
bounded install slices ending on paragraph boundaries, resuming per slice →
`finish_load_finalization(permit)`.

Four inversions. The freshness check is the unreified seam.

### WFR-DRAFT-RECOVERY

Autosave: first dirty edit → `SupersedingTimer` ⇢ timer fires in `autosave_tick`
→ in-flight gate that sets `autosave_pending` instead of queueing →
`collect_dirty_draft_candidates` → `drive_dirty_draft_pipeline` ⇢ a staged
worker pipeline, resuming once per stage, ending in a durable manifest write.

Restore: startup scan → candidate queue → pop candidate →
`spawn_blocking_then(resolve_draft_restore)` ⇢ worker resolves the body under a
disposal reservation, resuming in the completion closure →
`draft_restore_is_current(ticket, facts)` → `apply_draft` ⇢ bounded buffer
install → orphan-body cleanup, which re-reads the trusted manifest, reacquires
the same `TargetWriteGuard`, and rechecks inode before deleting.

The file contains seven distinct worker handoffs, the highest inversion count of
any workflow.

### WFR-SESSION-RESTORE

App startup → `startup_data` → restore plan → `plan_turn()` →
`SessionRestoreAdmission` per descriptor → `open_document_from_session_restore`
per tab ⇢ one bounded turn per GTK turn, re-armed while `needs_next_turn()` is
true → `release_permit` → `evidence()`.

One inversion, already expressed as an explicit bounded-turn policy. This is the
closest workflow in the tree to the target shape.

### WFR-LOCAL-HISTORY

Capture: first edit → `BaselineCaptureTicket` ⇢ worker capture, resuming against
`baseline_capture_is_current(ticket, facts)` → sidecar write. Periodic timer →
`PeriodicCaptureTicket` ⇢ same shape against `periodic_capture_is_current`.

Browse and restore: `win.show-local-history` → browser →
`LocalHistoryPreviewCoordinator` generation ⇢ worker preview read →
`LocalHistoryPreviewInstallSession` ⇢ bounded install slices → restore →
`LocalHistoryReplacementTicket::is_current(editor)` → `BufferReplacementTicket`
⇢ bounded buffer install.

Six inversions, all already ticket-guarded.

### WFR-BUFFER-REPLACEMENT

Caller supplies replacement text → `BufferReplacementTicket` →
`BufferReplacementSession` ⇢ install or clear slices resuming per turn, each
ending on a paragraph boundary → projection-suspension release → caller
completion.

One inversion, already reified. Called by local-history restore, Replace All
undo, and draft restore, which is why it is cross-cutting.

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
| Argument-count suppressions | `#[expect(clippy::too_many_arguments)]` in the workspace | 2 |
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
| `save_admission.rs` | 405 | `ui/editor_page/save_runtime.rs`, `ui/editor_page/load_save.rs` | 1 (`WFR-DOCUMENT-SAVE`) | single-consumer → relocates to `ui/editor_page/policy.rs` |
| `search_flight.rs` | 191 | `ui/search_panel/imp.rs`, `runtime.rs` (both in `ui/search_panel/`) | 1 (`WFR-SEARCH-REPLACE`) | single-consumer → relocates to `ui/search_panel/policy.rs` |
| `search_retirement.rs` | 80 | `runtime.rs` (in `ui/search_panel/`) | 1 (`WFR-SEARCH-REPLACE`) | single-consumer → relocates to `ui/search_panel/policy.rs` |
| `minimap_analysis.rs` | 186 | `ui/editor_page/minimap.rs` | 1 (`WFR-MINIMAP`) | single-consumer → relocates to `ui/editor_page/minimap/policy.rs` |
| `plain_disposal.rs` | 692 | `ui/plain_disposal.rs` | its own adapter, serving 10 workflows | cross-cutting → stays |
| `buffer_replacement.rs` | 186 | `ui/window/local_history.rs`, `ui/editor_page/buffer_replacement.rs` | 2 (`WFR-LOCAL-HISTORY`, Replace All undo) | cross-cutting → stays |
| `editor_memory.rs` | 469 | `ui/window/focus_indexing.rs`, `ui/window/imp.rs`, `ui/editor_page/mod.rs`, `ui/editor_page/save_runtime.rs`, `ui/editor_page/load_runtime.rs` | 3 | cross-cutting → stays, exempt |
| `migration_ledger.rs` | 225 | `ui/window/notes/mod.rs`, `ui/window/local_history.rs`, `services/migration_ledger.rs` | 2 plus a service | cross-cutting → stays |

**Post-migration note.** This table is the census snapshot, kept as the record
of what each classification was decided from; its line counts and consumer
names are as-censused and are deliberately not rewritten. Two rows have since
been acted on. `search_flight.rs` and `search_retirement.rs` are gone from
`model/`: both relocated into `ui/search_panel/policy.rs`, with mutation-coverage
parity recorded in
`openspec/changes/normalize-workflow-readability-boundaries/evidence/mutation-parity-search-policy.md`.
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

`model/file_load.rs` (4 consumers) is domain-shaped but sits close to the
boundary; `WFR-DOCUMENT-LOAD`'s migration in slot 3 must decide it explicitly
rather than inheriting this row.

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

Four seams remain unreified. Each is named here so its migration change does
not have to rediscover it.

### required: `QueuedSaveTicket` + `QueuedSaveFacts` (`WFR-DOCUMENT-SAVE`)

Carries `{save_generation, path, explicit_destination, required_modified,
close_session_identity}`. Today those five fields live as loose parameters
threaded through `SaveSubmission` → `QueuedSave` → `begin_admitted_save` →
`queued_save_is_current`, and `begin_admitted_save` carries the programme's only
non-catalog `#[expect(clippy::too_many_arguments)]`.

This is the seam holding the archetype defect: `begin_admitted_save` passes its
`cancel_pending_load` argument positionally into `queued_save_is_current`'s
`explicit_destination` parameter
(`ui/editor_page/load_save.rs:1390` into `:1344`). The two names denote the same
value today, so no test can see the drift. **The value object MUST use
`explicit_destination`**, which names the user's intent, and MUST NOT use
`cancel_pending_load`, which names only the consequence.

### required: `LoadRequestTicket` (`WFR-DOCUMENT-LOAD`)

Carries `{load_generation, cancel_token}` with an `is_current(&editor)`
predicate matching `SaveCompletionTicket`'s shape. The pair is already grouped
inside `load_runtime`'s request type but is exploded back into loose parameters
at both call sites (`ui/editor_page/load_runtime.rs:209`,
`ui/editor_page/load_save.rs:553`) against
`ui/editor_page/load_save.rs:1012`.

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
| 3 | Save and load | `WFR-DOCUMENT-SAVE`, `WFR-DOCUMENT-LOAD` | tier-3 |
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

### Argument-count suppressions

The residual sweep asserts **zero** `#[expect(clippy::too_many_arguments)]` in
workflow adapter and coordination code, with no allowlist. Reachable because
Clippy's threshold is 7, only two functions in the crate have 8 or more parameters,
and the only one in workflow code is the save seam that `QueuedSaveTicket` removes.
Domain catalog construction in `model/` is outside the workflow-seam rule.

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

### WFR-COMMAND-PALETTE

- facade: `ui/command_palette/mod.rs`
- coordination: `ui/command_palette/query_execution.rs`, `ui/command_palette/index_admission.rs`, `ui/command_palette/index_execution.rs`, `ui/command_palette/retirement.rs`
- policy: `ui/command_palette/policy.rs`
- evidence: `ui/command_palette/evidence.rs`
- mutation parity: `openspec/changes/migrate-command-palette-workflow-readability/evidence/mutation-parity-palette-policy.md`

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
