# Shared-ownership decisions (section 2), role homes (A.11), and the `journal` verdict (A.8)

Each of these is a boundary two or more of this slot's rows would otherwise
decide twice, from two sides. All are decided **before** the workflow that would
absorb them is touched.

## 2.1 `next_install_boundary` — the alias stays, as a named domain synonym

The arithmetic is one function,
`model::buffer_replacement::next_replacement_boundary`.
`model/file_load.rs:276` is a **one-line delegation**, not a copy:

```rust
pub fn next_install_boundary(text: &str, start: usize) -> usize {
    super::buffer_replacement::next_replacement_boundary(text, start)
}
```

**Decision: the alias stays.** Reasons, in order of weight:

1. It has **no body of its own**, so it cannot drift. The amendment's
   shared-arithmetic scenario ("both call the cross-cutting owner; neither copies
   it") is satisfied — there is exactly one implementation and one caller chain
   to it.
2. Removing it would edit a **migrated** workflow's narration: four call sites
   (`benches/benchmarks.rs`, `tests/properties/file_load.rs` ×3,
   `ui/editor_page/load/execution.rs`) plus two rustdoc intra-doc links in
   `load/policy.rs` and `load/execution.rs`. Task 3.7 says a migrated caller's
   structure is not this change's to rework, and the edit buys no behavior, no
   coverage, and no removed duplication.
3. The load workflow narrates in load vocabulary — "install slice" — and the
   synonym keeps that narration honest at its own boundary.

The cost of two names for one function is that a reader of `load/execution.rs`
does not immediately see they are calling the buffer-replacement
paragraph-boundary contract. That is paid down by **strengthening the alias's own
doc comment** to name the cross-cutting owner and the contract, so following the
one-line delegation is a two-second step rather than a discovery. That doc edit
is the only change this decision makes.

Not done, deliberately: the `tests/properties/file_load.rs` sites are behind
`required-features = ["property-tests"]`, so no default lane would have caught a
broken import. Leaving the name alone removes that exposure entirely, which is
why task 10.8 has nothing to run for this decision.

## 2.2 `ui/window/startup_data.rs` — **neither slot-4 row owns it**

The task asked "shared or owned?", assuming it is one of the two. The code says
it is **neither**. `startup_data.rs` is the **startup app-data format-upgrade
gate**: it scans app data with `services::format_upgrade`, presents the
compatibility dialog, applies Convert or Start Fresh, and only then releases the
gate. Its own state group is `StartupDataFlowState` (`completed`, `running`,
`pending_activation_paths`) and it shares none of `DraftState` or `SessionState`.

Its relationship to both slot-4 rows is **calling**, in one function:

```rust
fn continue_startup_data_flow(&self) {
    ... self.load_session_and_drafts();   // WFR-SESSION-RESTORE entry point
        self.start_autosave_timer();      // WFR-DRAFT-RECOVERY entry point
}
```

The matrix's census home for format upgrade is `WFR-NOTES-BOOKMARKS`, whose row
title is literally "Notes, bookmarks, sidecar migration, **format upgrade**" —
slot 5. **Decision: `startup_data.rs` stays exactly where it is, unchanged**,
recorded in both slot-4 facades' "State this workflow shares with others" tables
as a **caller** and not as shared state, and handed to slot 5 in B.2 as the file
whose ownership its row already implies.

The one field both rows *read* from it is
`startup_data_flow.completed`, consulted by
`startup_session_descriptors_pending()`. That read stays a read; the session row
projects it into its evidence surface rather than claiming the field.

## 2.3 `services/recovery_metadata.rs` — stays in services, shared by three rows

1,162 production lines of 1,636. Consumed by all three durable rows: draft
recovery, session restore, and local history each publish `RecoveryDiagnostic`
values from it (`ui/window/drafts.rs`, `ui/window/session_persistence.rs`,
`ui/window/local_history.rs`).

**Decision: unchanged, and not split.** A `services -> ui` dependency inversion is
forbidden outright — the way 2b settled `workspace_search.rs` and 3b settled
`file_load.rs` — and the module is the shared *vocabulary* of recovery
diagnostics (`RecoveryMetadataClass`, `RecoveryProblem`, `RecoveryPreservation`),
not one row's decision logic. Its seams are shared and stay in the service.

**But it does have a caller-side pure-policy neighbour, and that one moves.**
`startup_recovery_status_message(&[RecoveryDiagnostic]) -> String` in
`session_persistence.rs` (lines 918–972) is pure classification over diagnostics
with no GTK dependency, and it is the session workflow's own decision about how
to summarise them. It is a **gain-from-zero** extraction into the session row's
`policy.rs`, not a relocation from `services/`. Recorded here so a later slot does
not re-derive it as `recovery_metadata`'s.

## 2.4 `session.save_failed` — the session row owns it, and 3a's correction stands

Slot 3a's planning named `window.imp().session.save_failed` as a document-save
priority site, then found it is **session-file** save failure: written only by
`record_session_save_failure` and cleared only by `clear_session_save_failure`,
both in `ui/window/session_persistence.rs`, and both keyed on the session
`save_debounce` generation. No save-workflow code touches it.

**Carried forward verbatim, because it is the reusable lesson: a field whose name
contains "save" is not thereby save-workflow state.**

Its three widget-test read sites (`window.rs` lines 6676, 13211, 13226) become
`SessionRestoreEvidence` reads, together with `failed_generation` and
`failure_detail`.

## 2.4a The three shared imp state groups, split deliberately

### `SessionState` (`ui/window/imp.rs:134`) — three-way split

| Field | Owner |
| --- | --- |
| `save_debounce`, `restoring`, `restore_cancel`, `restore_capacity_wakeup`, `restore_runtime`, `next_restore_generation`, `last_restore_evidence`, `selection_generation`, `applying_restore_selection`, `save_failed`, `failed_generation`, `failure_detail` | **`WFR-SESSION-RESTORE` owns.** 12 fields |
| `tab_projection_publications` | **shared, stays.** It is the *window's* aggregate tab-projection counter; session restore begins and ends a batch but the tab workflow owns the projection. Session evidence **projects** it, and does not claim it |
| `next_close_save_identity`, `active_close_save_identity` | **migrated `WFR-DOCUMENT-SAVE` owns**, driven end to end by `ui/window/dialogs.rs`. See below |
| `close_safety_inflight`, `close_safety_bypass` | **genuinely shared** between the draft and session rows. See below |

**The close-save identity pair.** These belong to a migrated workflow and are
read by save's `SaveCompletionTicket`. Decision: **they stay in `SessionState`
with a recorded owner rather than moving.** Moving them would edit 3a's migrated
structure for a naming benefit, and 3a already chose to leave them here — its
`expire_close_save_session_for_test` seam was added precisely so the close-save
session could be driven without reaching the fields. Recording the owner in the
session facade's shared-state table gives a reader the same information at no
structural cost. The field names already say "close_save", so the readability gap
is the *group* they sit in, not the names.

**`close_safety_inflight` / `close_safety_bypass`.** Task 4.5 hypothesised the
session row owns both. **The hypothesis is wrong and the doc comments were
right**: these gate the combined draft-and-session close-safety pass —
`flush_dirty_drafts_async` (draft) and `save_session_for_close_async` (session)
run under one flag, and `close_safety_bypass` is the one-shot that lets the final
close proceed after *both* succeed. Decision: **shared between
`WFR-DRAFT-RECOVERY` and `WFR-SESSION-RESTORE`, recorded as shared in both
facades' tables.** The session evidence surface projects them (it is the surface
the existing widget reads use); the draft evidence surface projects them too,
because a draft-side reader needs to know the close pass is running. Two
projections of one shared field, both read-only, is not a second source of truth —
neither surface *owns* it.

### `DraftState` (`ui/window/imp.rs:188`) — all draft-owned except one read

All 24 fields are draft-owned. The only cross-workflow touch is
`ui/window/session_persistence.rs`'s startup completion, which writes
`drafts.manifest`, `drafts.manifest_authority`, and `drafts.preloaded` from the
session restore worker's result. **Decision: that write becomes a named draft
operation** (`adopt_startup_draft_records`) that the session workflow calls,
rather than three field reaches from another workflow's file. The fields stay
draft-owned.

### `LocalHistoryState` (`ui/editor_page/imp.rs`) — local history owns, two migrated readers keep named operations

`ui/editor_page/save/mod.rs` already documents this group as slot 4's to own, so
this is an inherited obligation. Migrated save reads it as its
`SaveCompletionTicket` freshness identity, and migrated load reads it at two
sites.

**Decision: `WFR-LOCAL-HISTORY` owns the group, and save and load keep reading it
through named operations rather than field reaches.** The operations already
exist and are already intent-first — `prepare_local_history_for_save`,
`complete_local_history_after_save_success`,
`complete_local_history_after_save_failure`,
`seed_local_history_from_guarded_loaded_content`,
`advance_local_history_path_generation`,
`release_local_history_residency_for_eviction`. The remaining raw reads are
`automatic_capture_suppressed` (load's `restore_load_installation_state` and
buffer replacement's guard save/restore) and `editor_generation` (save's
completion ticket). Those two become accessors on the local-history facade;
neither migrated workflow needs a structural change.

## 2.5 Closed boundaries, confirmed not re-opened

Recorded so a reader does not think any of these is an open question:

| Module | Status |
| --- | --- |
| `ui/editor_page/restore_position.rs` (93) | cross-cutting with **five** owning workflows, one of them this slot's session row. MUST NOT move — closed and recorded in the matrix. **Called, not absorbed** |
| `ui/plain_disposal.rs` | cross-cutting, 21 files / 10 workflows. Unchanged |
| `ui/buffer_snapshot.rs` | `WFR-BUFFER-SNAPSHOT`, slot 7. Unchanged, including its chunked threshold, which this slot must not duplicate |
| `model/editor_memory.rs` | exempt, no slot. Unchanged |
| `ui/editor_page/document_identity.rs` (102) | owned by **neither** document workflow. Unchanged |

## 2.6 Excluded by scope: `WFR-NOTES-BOOKMARKS`

`NoteSourceRefreshCoordinator`, `services/palette/notes.rs`, and the notes
browser surfaces are slot 5's. The adjacency is real and will look inviting:
notes are sidecars like local history, both go through `NoteSourceAdmission`, and
`ui/window/local_history.rs` already calls `resolve_notes_for_editor` from two
restore terminals. Those calls stay calls. `startup_data.rs`'s format-upgrade
gate (2.2) is also slot 5's census home.

## 2.7 The `journal` role name — checked once, for all three durable rows

Slot 3a reserved `journal` for this slot after rejecting it for document save.
The test is 3a's and 3b re-applied it: **does a later stage of the same workflow
restore from the record**, not *does it touch the disk*.

| Record | Later stage that reads it back | Verdict |
| --- | --- | --- |
| draft manifest + bodies | startup recovery: `load_restore_state_cancellable` → `check_draft_by_id` / `check_draft_on_open` → `apply_draft` | **PASSES** |
| the session file | the next startup's `load_session_and_drafts` → `start_session_restore`; and within one run, `load_and_merge_persisted_session_for_close` reads it back to preserve not-yet-admitted descriptors | **PASSES** |
| local-history sidecars | `list_snapshots_for_path_recovering` → preview → restore | **PASSES** |

All three pass outright. **`gtk-adapter-module-boundaries` needs no amendment.**

Per slot 2b's definition, each row's **mutual-exclusion gate and the byte
reservation its writes take live inside the journal**, not in a separate
`admission`: for drafts that is `mutation_inflight`, `pending_deletes`,
`pending_delete_ids`, `delete_tombstones`, and `mutation_order`; for the session
it is the `save_debounce` generation.

### Orphan cleanup is `journal`, not `retirement`

Task 6.2 asked to check `retirement` against orphan cleanup because it "destroys
payloads the workflow is finished with". Applying the cohesion test — *would a
reader look for this under its own name* — the answer is **no**:

- `retirement` in this codebase means the **disposal lane**: deferred, off-GTK
  destruction of a large *in-memory* payload through `DisposalOwned`. Orphan
  cleanup deletes files.
- Orphan cleanup is **journal maintenance**. It reloads the same manifest under
  the same `manifest_write_lock` the journal's writes take, is gated by the same
  `manifest_authority`, and merges its result back into the same in-memory
  record. A reader asking "what keeps the draft manifest consistent with the
  bodies on disk" looks in `journal`.
- `DraftCleanupContinuation`, the bounded resumable loop, therefore belongs
  **with the journal it protects**: its `manifest_offset` is an offset *into that
  record*.

## Coordination role mapping, per workflow (A.8)

| Workflow | Roles | Notes |
| --- | --- | --- |
| `WFR-BUFFER-REPLACEMENT` | `execution` | One stage order, one deferred mechanism. No durable record, so no `journal`; supersession is the facade's entry decision plus execution's session ownership, so no separate `admission` |
| `WFR-SESSION-RESTORE` | `admission`, `execution`, `journal` | `admission` = the bounded-turn policy runtime, permits, and the startup capacity wakeup — reserve then settle, exactly. `execution` = page creation, selection restore, terminal publication. `journal` = the session file |
| `WFR-LOCAL-HISTORY` | `capture_execution`, `preview_execution`, `journal` | Two stage orders of the same shape, so the stage-order qualifier applies to **both** new modules (neither is a stable sibling being renamed for symmetry). `journal` = the sidecars |
| `WFR-DRAFT-RECOVERY` | `admission`, `autosave_execution`, `restore_execution`, `journal` | `admission` = preload demotion, the one-at-a-time lazy queue, disposal reservations, restore in-flight accounting. Two execution stage orders of the same shape take the qualifier. `journal` = manifest, bodies, deletes, tombstones, mutation serialization, **and orphan cleanup** |

No name outside the bounded set `{admission, execution, retirement, watch,
journal}` is used, and every qualifier is drawn from the workflow's own domain
vocabulary.

## Role homes, by collision analysis (A.11)

Derived, not asserted: every non-plumbing file in `ui/window/` mapped to the
matrix row that owns it. The 22 `WFR-*` ids are the full census list, taken from
`grep -oP '^\| WFR-[A-Z-]+' docs/workflow-readability-matrix.md`.

| `ui/window/` file | Owning row |
| --- | --- |
| `drafts.rs`, `draft_ordering.rs` | `WFR-DRAFT-RECOVERY` |
| `session_persistence.rs`, `session_restore.rs` | `WFR-SESSION-RESTORE` |
| `local_history.rs` | `WFR-LOCAL-HISTORY` |
| `focus_indexing.rs` | `WFR-COMMAND-PALETTE` |
| `notes/`, `startup_data.rs` | `WFR-NOTES-BOOKMARKS` (see 2.2 for `startup_data.rs`) |
| `preview.rs` | `WFR-MARKDOWN-PREVIEW` |
| `print.rs` | `WFR-PRINT` |
| `encoding.rs` | `WFR-ENCODING` |
| `search.rs` | `WFR-SEARCH-REPLACE` / `WFR-EDITOR-FIND` |
| `documents.rs`, `dialogs.rs`, `recent_open.rs` | `WFR-DOCUMENT-LOAD` / `WFR-DOCUMENT-SAVE` (both migrated; the matrix records these as files those rows **call**) |
| `notifications.rs` | `WFR-STATUS-NOTIFICATIONS` |
| `adaptive_shell.rs`, `focus_mode.rs`, `transient_surfaces.rs`, `tabs.rs`, `zoom.rs` | `WFR-SHELL-LAYOUT` |
| `workspace_scope.rs` | `WFR-WORKSPACE-TREE` |
| `actions.rs`, `mod.rs`, `imp.rs` | shell plumbing, no row |

That is **15 distinct rows with owned or called code in one directory**. The fixed
names `policy.rs` and `evidence.rs` are one each per workflow and cannot be shared
across 15, so every slot-4 row in this directory takes a **per-workflow
subdirectory**, the shape 3a established and 3b confirmed.

| Workflow | Home | Collision |
| --- | --- | --- |
| `WFR-BUFFER-REPLACEMENT` | `ui/editor_page/buffer_replacement/` | `ui/editor_page/` hosts 8 workflows; `save/` and `load/` already hold `policy.rs` + `evidence.rs`, so a third pair collides. A prefixed `buffer_replacement_policy.rs` is not available: it would leave the `ui/**/policy.rs` mutation glob |
| `WFR-SESSION-RESTORE` | `ui/window/session_restore/` | first adopter of the per-workflow subdirectory under `ui/window/` |
| `WFR-DRAFT-RECOVERY` | `ui/window/drafts/` | same |
| `WFR-LOCAL-HISTORY` | `ui/window/local_history/` — see 5.2 | the two-directory decision |
