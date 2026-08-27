# Current ordered stages and real inversion counts (task 0.4)

Written **from the code**, not from the matrix trace. Census inversion counts are
floors; four consecutive slots found more in the code than the trace recorded.

Two kinds are counted separately, because the convention only asks the facade to
name the first:

- **Deferred inversion** — control leaves through a timer, idle source, capacity
  wakeup, or worker handoff and resumes at a named point later. The facade must
  name each one and its resume point.
- **Ownership hand-out** — a callback the workflow fires synchronously to give a
  caller a terminal or a payload. Control does not come back; these are terminal
  ownership transfers, narrated as such rather than as inversions.

---

## WFR-BUFFER-REPLACEMENT — one stage order

### Stages

1. **Accept or supersede** (`replace_buffer_bounded`). With an active session:
   cancel it as `Superseded`; if that cancellation reached its terminal
   synchronously the new request starts immediately, otherwise it is parked as
   `pending` and any request it displaces has its terminal fired as `Superseded`.
2. **Begin** (`start_buffer_replacement`). `BufferReplacementPlan::for_sizes`
   classifies `Direct` vs `Sliced` from both the existing char count and the
   incoming byte length; `begin_guard` captures and suspends editability, cursor
   visibility, syntax highlighting, minimap tracking, local-history capture,
   projection, and the file monitor; `begin_irreversible_action`; the session is
   published as active and the slice count reset.
3. **Direct** (`run_direct`): revalidate editor and freshness, `set_text`,
   record metrics, finish — all in one turn.
4. **Clear in slices** (`run_clear_slice`): `next_clear_char_count`, then
   `delete_one_slice`, which extends the deletion end to the next line start so
   GTK re-lays-out each paragraph exactly once. Reschedule until the buffer is
   empty; an empty body finishes here, otherwise the phase becomes `Installing`.
5. **Install in slices** (`run_insert_slice`): `next_replacement_boundary` picks a
   paragraph-aligned end, `body[start..end]` is appended, and the phase
   reschedules until `end == body.len()`.
6. **Cancel** (`cancel_session`): the uninstalled body is returned to its owner
   and the pending source removed. Disposal or a not-yet-started mutation
   finishes immediately; a started mutation enters `ClearingCancelled` and clears
   the partial buffer in slices first, because a half-installed buffer must not
   be left visible.
7. **Terminal** (`finish_session`): mark terminal, take source/guard/body/
   callback, `end_irreversible_action`, build the outcome, trace it, record the
   terminal diagnostic, restore the guard unless disposing, fire the caller's
   callback exactly once, publish the slice count, then hand ownership to any
   pending request.

### Inversions

| # | Kind | Mechanism | Resumes in |
| --- | --- | --- | --- |
| 1 | deferred | `glib::timeout_add_local_once(1 ms)` (`schedule_slice`) | `run_slice`, which dispatches to `run_clear_slice`, `run_insert_slice`, or `run_cancelled_clear_slice` by phase |

**One deferred mechanism, three phase resume points.** Plus four ownership
hand-outs: the terminal `TerminalCallback`; the guarded `on_cancel` body return;
the guarded `on_complete` body return; and the synchronous eviction of a
displaced pending request from stage 1. The matrix's "1 already-reified
inversion" is the mechanism and is **correct** — this is the one row of the four
whose census inversion count does not move.

---

## WFR-SESSION-RESTORE — two stage orders

### Stage order A: persistence

`collect_session` → 500 ms debounce → re-check restoring/descriptors-pending →
`collect_session` again → worker `session_service::save_ordered` →
`clear_session_save_failure` or `record_session_save_failure`.

Two additional entry points share the same later stages:
`save_session_sync` (deliberately blocking, close-request path) and
`save_session_for_close_async`. Both switch to `collect_session_for_close`, which
layers not-yet-admitted restore descriptors over the mounted pages, unless
startup descriptors are still pending — in which case the persisted file is
loaded and merged so preservation authority is not discarded.

### Stage order B: restore

`load_session_and_drafts` → publish a cancel token → reserve progress-disposal
capacity for the startup preload graph (or arm a capacity wakeup and retry) →
worker `draft_service::load_restore_state_cancellable` +
`fit_startup_preloads_to_reservation` → GTK: install manifest, manifest
authority, and guarded preloads; `start_session_restore` →
`SessionRestorePolicy::new(generation, tabs, active_ordinal)` →
`schedule_session_restore_turn` → `plan_turn()` bounded by pages-per-turn and
file-plan permits → per admission `create_session_restore_page` →
`open_document_from_session_restore(path, on_terminal)` → each terminal reaches
`release_session_restore_plan_permit` → re-arm the next turn while
`needs_next_turn()`, else `finish_session_restore`.

### Inversions

| # | Kind | Mechanism | Resumes in |
| --- | --- | --- | --- |
| 1 | deferred | `Debounce::schedule` (500 ms) | the debounce closure in `save_session_debounced` |
| 2 | deferred | `spawn_blocking_then(save_ordered)` | that closure's completion |
| 3 | deferred | `spawn_blocking_then` in `save_session_for_close_async` | its completion, which calls the caller's `on_done` |
| 4 | deferred | `ProgressDisposalCapacityWakeup::arm` | `start_session_and_drafts_load`, retried against the same cancel token |
| 5 | deferred | `spawn_blocking_then(load_restore_state_cancellable)` | the GTK completion that installs the manifest and starts restore |
| 6 | deferred | `glib::idle_add_local_once` per bounded turn | `run_scheduled_session_restore_turn` |
| 7 | deferred | the load workflow's planning terminal, carried by `load_file_async_with_planning_terminal` | `release_session_restore_plan_permit` |

**7 deferred inversions where the matrix records 1.** Correction of +6. One
ownership hand-out: `save_session_for_close_async`'s `on_done`.

---

## WFR-LOCAL-HISTORY — two stage orders, in two directories

### Stage order A: capture (`ui/editor_page/local_history.rs`)

Baseline: buffer becomes modified → availability and path checks →
`capture_local_history_baseline` → acquire the process-wide
`AutomaticHistoryCapturePermit` or enqueue a weak waiter → take the clean
baseline text → build `BaselineCaptureTicket` → worker
`capture_snapshot_for_path(Baseline, DeduplicateLatest)` → on failure only,
revalidate with `baseline_capture_is_current` and restore the text, spending one
retry budget unit.

Periodic: `schedule_local_history_periodic_capture` advances
`periodic_generation` and arms a `SupersedingTimer` →
`run_local_history_periodic_capture` → generation/modified/path/availability
checks → permit → chunked or direct buffer snapshot → build
`PeriodicCaptureTicket` → `persist_periodic_snapshot_if_current` revalidates
against live `PeriodicCaptureFacts` → worker capture → reschedule.

### Stage order B: browse, preview, restore, undo (`ui/window/local_history.rs`)

Listing: `show_local_history_dialog` (active editor) or
`show_local_history_for_path` (sidebar/explicit path) → worker
`list_snapshots_for_path_recovering` → `present_local_history_browser`, which
filters legacy empty baselines and either shows the empty status dialog or builds
the browser.

Preview: `load_preview_for_index` → cancel any install, retire the loaded
snapshot → `LocalHistoryPreviewCoordinator::submit` (one-active/one-latest) →
`start_preview_load` → reserve disposal capacity or arm a capacity wakeup →
worker `load_snapshot_for_path_cancellable` + on-worker guarding →
`finish_preview_load` (accept only the current generation) →
`begin_preview_install` → direct below 1 MiB, otherwise sliced through
`next_replacement_boundary` → `finish_preview_install`.

Restore: `restore_local_history_snapshot` → admit a progress snapshot or arm a
wakeup and retain the compact intent → capture the undo body (chunked or direct)
→ worker `capture_snapshot_for_path(RestoreSafety, PreserveDuplicate)` →
revalidate `LocalHistoryReplacementTicket` → `replace_buffer_bounded` →
replacement terminal publishes the undo body, notifications, and status.

Undo: `undo_local_history_restore` → take the undo body →
`replace_buffer_bounded` → terminal.

Rename migration: `migrate_local_history_after_rename` → worker
`record_pending` + `run_tracked_kind(move_path_tree)` → completion warns on
failure.

### Inversions

| # | Kind | Mechanism | Resumes in |
| --- | --- | --- | --- |
| 1 | deferred | `spawn_blocking_then` baseline capture | the failure-only completion |
| 2 | deferred | `MainContext::invoke` from `AutomaticHistoryCapturePermit::drop` | `drain_next_baseline_capture_waiter` |
| 3 | deferred | `SupersedingTimer` periodic interval | `run_local_history_periodic_capture` |
| 4 | deferred | chunked periodic buffer snapshot | the `finish` closure |
| 5 | deferred | `spawn_blocking_then` periodic persist | `reschedule_local_history_after_capture` |
| 6 | deferred | `spawn_blocking_then` listing for the active editor | `present_local_history_browser` |
| 7 | deferred | `spawn_blocking_then` listing for an explicit path | `open_document` + `present_local_history_browser` |
| 8 | deferred | `DisposalCapacityWakeup::arm` for a deferred preview | `retry_preview_admission` |
| 9 | deferred | `spawn_blocking_then` preview body load | `finish_preview_load` |
| 10 | deferred | `idle_add_local_once` / `timeout_add_local_once` preview install slice | `run_preview_install_slice` |
| 11 | deferred | `ProgressDisposalCapacityWakeup::arm` for a deferred restore | the wakeup closure re-entering `restore_local_history_snapshot` |
| 12 | deferred | chunked undo-body capture | the `run_restore` closure |
| 13 | deferred | `spawn_blocking_then` restore-safety capture | its completion, which starts the replacement |
| 14 | deferred | buffer-replacement terminal (restore) | the terminal closure |
| 15 | deferred | buffer-replacement terminal (undo) | the terminal closure |
| 16 | deferred | `spawn_blocking_then` rename migration | its completion |

**16 deferred inversions where the matrix records 6.** Correction of +10.

---

## WFR-DRAFT-RECOVERY — three stage orders

### Stage order A: autosave

First dirty edit → `schedule_first_dirty_draft_autosave` (750 ms
`SupersedingTimer`) **or** the global 5 s repeating timer → `autosave_tick` →
in-flight gate: if autosave, mutation, or cleanup owns the lane, set
`autosave_pending` and return (it does **not** queue) →
`collect_dirty_draft_candidates` (modified, draft-dirty, not evicted, **and now
not mid-incomplete-installation**), each assigned a `DraftMutationIntent` before
any document-sized work → `drive_dirty_draft_pipeline` snapshots and writes **one
candidate at a time** → per candidate: chunked or direct snapshot, post-snapshot
identity/generation recheck, worker `write_draft` → `commit_dirty_draft_pipeline`
worker `update_manifest` → accept per completion under matching generation and
`mutation_order.is_current`, clearing `draft_dirty` and the recovery-limit warning
→ `finish_autosave_pipeline` releases the lane and drains pending mutations.

### Stage order B: restore

Startup: the session workflow's worker delivers the manifest and the guarded
preload map. Per tab: `check_draft_by_id` (untitled) or `check_draft_on_open`
(file-backed) → `take_preloaded_draft` moves one eager body under a replacement
disposal reservation, or demotes every eager body to a compact lazy marker when
headroom is unavailable → eager bodies go straight to `apply_draft`; compact
markers go to `queue_lazy_draft_restore` → `drive_lazy_draft_restore_queue`
admits **one** lazy body at a time under a disposal reservation (or arms a
capacity wakeup) → worker `resolve_draft_restore` → `finish_draft_restore`
validates `draft_restore_is_current(ticket, facts)` → `apply_draft` drives the
bounded buffer replacement → its terminal calls `finish_applied_draft`, which
seeds the local-history baseline, marks the buffer modified, and emits the
restored-draft inline alert.

### Stage order C: orphan cleanup, deletes, and close flush

Cleanup: `schedule_orphan_cleanup(cleanup_allowed)` → start-delay
`SupersedingTimer` → release eager preloads → refuse outright if the manifest is
untrusted → `run_orphan_cleanup_pass(offset)` → worker
`inspect_orphan_cleanup_from` + `execute_orphan_cleanup` (manifest lock, manifest
reload, target guard, inode recheck, delete) → GTK merges only exact fingerprints
→ `orphan_cleanup_follow_up` decides stop or a backoff-scheduled follow-up.

Deletes: `delete_draft_by_id` advances intent, installs a tombstone, removes the
in-memory entry, appends to the pending queue (moving a superseded same-ID
command to the tail) → `drive_pending_draft_mutations` runs **one** at a time →
worker deletes the body first, then the manifest entry only if the body is gone.

Close flush: `flush_dirty_drafts_async` polls until the lane drains →
`collect_close_draft_candidates` → `drive_close_draft_pipeline` →
`commit_close_draft_pipeline` → `wait_for_draft_mutations_then(on_done)`. The
synchronous `flush_dirty_drafts` remains as the deliberate blocking variant for
process exit, but **no production path reaches it** — `ui/window/dialogs.rs`
closes through the async flush, and the synchronous entry point is currently
exercised only by widget tests.

### Inversions

| # | Kind | Mechanism | Resumes in |
| --- | --- | --- | --- |
| 1 | deferred | first-dirty `SupersedingTimer` (750 ms) | `autosave_tick` |
| 2 | deferred | global `timeout_add_local` (5 s, repeating) | `autosave_tick` |
| 3 | deferred | chunked autosave buffer snapshot | the `finish_snapshot` closure |
| 4 | deferred | `spawn_blocking_then` autosave body write | its completion, which admits the next candidate |
| 5 | deferred | `spawn_blocking_then` autosave manifest commit | its completion, which accepts matching generations |
| 6 | deferred | `timeout_add_local_once` lane-drain poll in `flush_dirty_drafts_async` | itself, until the lane is free |
| 7 | deferred | chunked close-flush buffer snapshot | the `finish_snapshot` closure |
| 8 | deferred | `spawn_blocking_then` close body write | its completion |
| 9 | deferred | `spawn_blocking_then` close manifest commit | its completion |
| 10 | deferred | `timeout_add_local_once` poll in `wait_for_draft_mutations_then` | itself, then the close continuation |
| 11 | deferred | `ProgressDisposalCapacityWakeup::arm` for a lazy restore | `drive_lazy_draft_restore_queue` |
| 12 | deferred | `spawn_blocking_then` lazy body resolve | `finish_draft_restore` |
| 13 | deferred | buffer-replacement terminal for `apply_draft` | the terminal closure → `finish_applied_draft` |
| 14 | deferred | `spawn_blocking_then` compact delete | its completion |
| 15 | deferred | orphan-cleanup start `SupersedingTimer` | `run_orphan_cleanup_pass` |
| 16 | deferred | orphan-cleanup follow-up `SupersedingTimer` (backoff) | `run_orphan_cleanup_pass` |
| 17 | deferred | `spawn_blocking_then` orphan-cleanup inspect+execute | `finish_orphan_cleanup_pass` |

**17 deferred inversions where the matrix records 7 worker handoffs.** The census
counted only `spawn_blocking_then` sites; the 10 it missed are timers, polls,
capacity wakeups, chunked snapshots, and the replacement terminal. Two ownership
hand-outs: `flush_dirty_drafts_async`'s `on_done` and the guarded body's
`on_complete`.

---

## Summary of corrections owed to `Workflow Stage Traces` (task 9.7)

| Row | Census inversions | Real deferred inversions | Delta |
| --- | --- | --- | --- |
| `WFR-BUFFER-REPLACEMENT` | 1 | **1** (3 phase resume points, 4 hand-outs) | 0 — the only row of the four the census got right |
| `WFR-SESSION-RESTORE` | 1 | **7** | +6 |
| `WFR-LOCAL-HISTORY` | 6 | **16** | +10 |
| `WFR-DRAFT-RECOVERY` | 7 | **17** | +10 |
