# Data-safety passes and the four candidates (tasks 0.6, 7.1–7.5)

Two passes are required: one over the **intended** diff before implementing, one
over the **finished** diff. Pass 1 is recorded below. Pass 2 is recorded at the
bottom.

`.agents/rules/preexisting-blockers.md` is binding: a confirmed finding is fixed
in this work stream with a regression test proven to fail without the fix. Four
consecutive prior slots each found at least one confirmed pre-existing defect in
this class of code; **this slot found more than any of them**, which is what
auditing the workspace tree's file operations and the notes sidecar family
together produces.

## Pass 1 — before implementing (task 0.6)

Mode: `data-safety` explicit, full five-domain deterministic audit, read-only.
Scope: 49 files — `ui/sidebar/**`, `ui/window/notes/**`,
`ui/window/startup_data.rs`, the eleven services those call, and
`model/workspace_persistence.rs` / `model/workspace_scan.rs`.

**No domain came back clean.**

### Verified-clean guards, recorded so they are not re-litigated

The durable-write contract itself (`durable_write.rs:213-256` — temp `sync_all`
*after* metadata mutation, parent sync *after* rename, unique temp in the final
directory); **zero** raw-filesystem escapes across all 49 scope files; workspace
persistence's generation/retry/close-flush state machine; DnD folder reorder
(stable-id, no filesystem mutation, guarded persist); apply-time expansion-set
reads at `tree_loading.rs:1252-1262`; the migration ledger's ordering, merge, and
never-delete-while-incomplete properties; new-before-old sidecar write ordering
(`bookmark_service.rs:213-216`); `file_peek` and `file_tree` being read-only;
workspace unlist and file delete preserving sidecars, matching the dialog copy.

## The four named candidates (tasks 7.1–7.4)

### 7.1 `pending_activation_paths` — **not a data-safety defect**; bounded-work gap recorded

`ui/window/imp.rs:290`, fed at `startup_data.rs:92-95`, drained at `:98-103`.

**Verdict: no, with evidence.** The queue holds `PathBuf`s only — no user
content — and losing an entry would at worst fail to open a file the user can
reopen. The drain is correct: `completed` is set at `startup_data.rs:74`
**before** `flush_pending_activation_opens()` runs, so
`queue_activation_open_if_startup_pending` returns `false` during the drain and
cannot re-enter, and duplicates collapse because
`open_document_with_intent_and_planning_terminal` returns the existing page
(`documents.rs:190`). Ordering against restore is also correct:
`begin_session_restore` observes the already-created activation tabs through
`preserve_existing_selection` (`session_restore/admission.rs:75`).

The unboundedness is real but is a **bounded-work** question, not a safety one:
the pending window is unbounded in *time* (the format-upgrade dialog holds the
gate open until the user answers), the feeder is an external interface, there is
no queue-depth budget, no dedup at enqueue, and the drain opens every path in one
main-loop turn with no tab cap. **Owner: cross-cutting `startup_data.rs`
(task 2.2), so it is handed to slot 7 rather than absorbed here** — deciding a
cap for a queue this change does not own would be exactly the "fixing one from
outside is how a migrated row acquires a change nobody planned" hazard task 6.1
records for the automation reach-throughs.

### 7.2 Format-upgrade retry loop — **not a defect** on all three questions

- **Each retry re-scans and re-plans.** `startup_data.rs:180-185` re-runs `scan`
  and `build_plan` *inside the worker*, and the dialog is re-presented with that
  fresh plan (`:232`, `:239`) — never the dialog's snapshot.
- **A failed apply cannot leave app data misclassified.** Every write is
  `atomic_replace`, so each file is wholly old or wholly new, and the rescan
  re-derives classification from actual bytes. Already-converted items classify
  `Current` and are skipped; unconverted items convert again idempotently.
  `format-upgrade-backups/` is **not** in the scan set (fixed list at
  `inventory.rs:380-412`, proven by `scan_and_plan_do_not_write_app_data` at
  `:985`), so backups cannot reappear as actionable legacy data and make the loop
  non-terminating.
- **The first attempt's backup survives a failed second attempt.**
  `BackupSession::create` reserves a fresh `{ts}-{action}-{NN}` directory with
  `AlreadyExists` retry (`backup.rs:92-119`), so attempt 2 never reuses attempt
  1's directory. Last recovery write is `apply.rs:235`/`:333`, first destructive
  write is `:248`/`:344`, with all-or-nothing abort gates at
  `:205-214`/`:319-331`.

Three adjacent gaps in `services/format_upgrade/**` are recorded below as
**M-5**, **M-6**, and **M-7**. That subtree is owned by the
`format-upgrade-workflow` capability and reached only through cross-cutting
`startup_data.rs`; see "Findings handed on" for why they are recorded rather than
fixed here.

### 7.3 Detached cleanup thread on inline-create cancel — **CONFIRMED DEFECT**

`ui/sidebar/workspace_section/actions.rs:510-525`, reached from `:330`
(failed-rename recovery) and `:360` (cancel).

**Can it race a later create?** No — `create_unique` uses `O_EXCL` and retries to
`New File 2` (`:487-493`, `sys.rs:435-447`).

**Can it race a later rename? Yes.** The delete is purely path-based
(`fs_mutate::remove_file_if_exists`) with no inode recheck, no
`TargetWriteGuard`, and it bypasses the eight-slot worker admission entirely, so
it is unordered against every `spawn_blocking_then` worker. Combined with **C-1**
below (`rename_durable` silently replaces its destination), a user who cancels a
`New File` placeholder and then renames a real file onto that same name can have
the detached thread unlink the user's real file. The multi-process variant is
also live: the placeholder is a real on-disk file another program can write to
before the cancel lands.

**Should it be on the guarded path? Yes**, and the repository has already written
the rule down for the analogous case: draft orphan-body cleanup "records the
candidate inode … then rechecks inode before deletion. Never delete a planned
orphan body using only its path" (`.agents/rules/rust.md`, implemented at
`draft_service.rs:1909`). The `is_dir` branch is at least conservative
(`remove_dir_if_exists`, empty-only, never `remove_dir_all`).

**Fixed in this change.** See "Fixes landed".

### 7.4 File-operation ordering against the watcher and the sidecars

Three separate questions, three different answers.

- **Can it lose a sidecar? No.** `migration_ledger::record_pending` is durable
  *before* the first move (`notes/mod.rs:441-450` → `migration_ledger.rs:119-131`,
  under the process-wide `ledger_lock()` at `:105`); each kind runs tracked; a
  ledger-write failure aborts with `?` before mutating anything;
  `remove_completed` retains any entry with an incomplete kind
  (`model/migration_ledger.rs:182-184`); and startup `reconcile_pending` replays
  pending kinds in generation order. Integration tests already cover the
  interrupted-retry path (`tests/integration/notes.rs:281,311,375,405`).
  **The ledger does cover the single-rename window.** It does *not* cover two
  overlapping renames — see **H-1**.
- **Can it resurrect a stale watch target? Yes — CONFIRMED DEFECT.** Every
  sidebar index update inside `confirm_rename` (`rename_expanded_subtree`,
  `clear_dir_state`, the `dir_rows` insert, `set_path`,
  `refresh_workspace_watch_row`, `rename_cached_item`) sits inside
  `if let Some(ref target) = *imp.context_target.borrow() && …` at `:279-306`,
  while `is_new_item` and the **rename callback fire unconditionally at
  `:307-324`**. `context_target` is replaced by any right-click
  (`context_menus.rs:381`), by a new-item bind (`row_factory.rs:313`), or cleared
  on row recycling (`row_factory.rs:428-434`). When it changes mid-flight the
  watch mirror keeps the old path and there is **no automatic repair** — the
  code's own comment at `:294-295` notes that `set_path` emits no splice, and
  `watch_contribution` is re-read only through the explicit `update_row` call.
  Consequence: LushText silently stops watching the renamed file, the "File Has
  Changed on Disk" warning never fires, and the next save overwrites another
  program's changes. Worse variant at the same site: if `context_target` points
  at a *different* row, `:293` writes the renamed path onto that unrelated row's
  `FileTreeItem` and `:291` inserts it into `dir_rows`, so a later right-click
  Delete on that row targets the wrong file. The happy path is tested
  (`test_inline_rename_refreshes_expanded_directory_watch_target`), which is why
  the gap survived. **Fixed in this change**, and the fix is exactly the seam
  value object this slot owes: capture the target at confirm time, validate it at
  completion.
- **Can the expansion set describe a nonexistent path?** Transiently, yes — but
  it is in-memory only (`workspace_section/imp.rs:263`; nothing reaches
  `workspace_manager::save`, whose payload is workspaces plus scope),
  `find_dir_row` returns `None` harmlessly, and `save_expanded_paths` re-derives
  from the model (`tree_index.rs:59-63`). **Not data loss.**

## Fixes landed in this change

Each fix has a regression test proven to fail without it. The evidence file
`tree-behavior-equivalence.md` / `notes-behavior-equivalence.md` records the
failing-before/passing-after runs.

| Id | Defect | Row | Site |
| --- | --- | --- | --- |
| C-1 | Inline rename **silently destroys an existing file**: the only validation is empty-or-unchanged (`actions.rs:244`), no existence check (`:253`), and `rename_durable` → `rustix::fs::renameat` is plain `rename(2)`, which replaces a regular destination without asking (`durable_write.rs:392`, `sys.rs:137`). Reachable in ordinary use: New File → type an existing name → Enter; or rename `draft.md` to `final.md`. The destination's contents are permanently destroyed with no prompt, no warning, no undo. No widget test covered a rename collision (8 rename tests, none for an existing destination). | `WFR-WORKSPACE-TREE` | `ui/sidebar/workspace_section/actions.rs:244-272` |
| M-1 | Rename completion re-reads `context_target` instead of the target it was issued for: stale watch mirror, and a wrong-row `set_path`/`dir_rows` write. Candidate 7.4 above. | `WFR-WORKSPACE-TREE` | `actions.rs:275-324` |
| M-2 | Detached path-only placeholder delete. Candidate 7.3 above. | `WFR-WORKSPACE-TREE` | `actions.rs:510-525` |
| H-5 | Sidebar rename and delete bypass the `TargetWriteGuard` an editor save holds. Ctrl+S on `foo.txt`, then rename it: the save worker guards and writes its temp file, the unguarded rename moves `foo.txt` → `bar.txt`, and the save's `rename()` **re-creates `foo.txt`** with the buffer bytes. `bar.txt` — which the tab now points at — stays stale while the UI reports a successful save. The complete guard population today is `editor_io.rs:1561`, `draft_service.rs:1437,1909`, `content_search/replace.rs:509,1298,1476`; every sidebar writer is absent. | `WFR-WORKSPACE-TREE` | `actions.rs:272`, `:401-403` |
| H-4a | Bookmark persistence has **no failure retry**: `save_dirty` is cleared *before* the write (`bookmarks.rs:315-336`) and the `Err` arm restores nothing and schedules nothing, so a transient failure leaves the bookmarks in memory indefinitely. | `WFR-NOTES-BOOKMARKS` | `ui/window/notes/bookmarks.rs` |
| H-4b | Bookmark persistence has **no close flush**: `Debounce::schedule` captures the target weakly (`gtk-lush/settle/src/lib.rs:145-148`) and neither `handle_tab_detached` nor the close chain touches `bookmarks.persistence`. Workspace persistence does all of this correctly, which is the asymmetry that proves this is a gap rather than a design choice. | `WFR-NOTES-BOOKMARKS` | `notes/bookmarks.rs`, `ui/window/tabs.rs` |
| H-6 | Note editor dialog is destroyed **before the write result is known**: `AdwAlertDialog::choose` fires after the dialog has closed, so the only copy of the typed prose was gone before the `Err` arm published a transient status message with no retry and no draft. Reachable without exotic timing — `note_storage::resolve_document_identity` returns `Err` if the file was renamed or deleted by another program while the dialog was open. **Fixed:** the worker now returns a named `NoteWriteOutcome` instead of a `Result`, so the `Failed` variant carries the owned text back off the worker; the completion re-presents the editor pre-filled with it in Edit mode and says so (`"… could not be saved — reopened with your text"`). Both editors. A `Result<_, anyhow::Error>` could not express this, which is why the type changed rather than the error arm. | `WFR-NOTES-BOOKMARKS` | `ui/window/notes/editor_execution.rs` |
| H-2 | One ambiguous note conflict **permanently abandons every sidecar sorted after it**: `merge_document_note_target(...)?` aborts the whole bulk loop, `scan_directory` sorts by filename so every retry hits the same poisoned sidecar at the same position, and `MAX_MIGRATION_ATTEMPTS = 3` then skips the kind forever. Unlike Start Fresh's removal loop (`apply.rs:337-350`) there is no per-item isolation. | `WFR-NOTES-BOOKMARKS` | `document_note_service.rs:193-198`, `folder_note_service.rs:286-291`, `bookmark_service.rs:212-215` |
| H-1 | Two overlapping renames **strand a sidecar while both ledger entries complete**: `operation()` runs *outside* `ledger_lock()`, `scan_directory` returns a snapshot, chained renames create independent entries, and a `rebase_identity_paths` miss returns `Ok(0)`, which `mark_kind_completed` treats as done and removes. Rename A→B then B→C before the first finishes: the file lives at C, the only sidecar is `S_B`, both entries are retired, and `reconcile_pending` has nothing to retry. Serialization, **not** supersession — a superseding coordinator would drop the first hop and strand the sidecar at A. | `WFR-NOTES-BOOKMARKS` reaching `WFR-MIGRATION-LEDGER` | `services/migration_ledger.rs:184-188` |
| H-3 | The migration worker **races restored-tab sidecar read/write**: `reconcile_pending_migrations_on_startup` dispatches a worker and returns, and nothing awaits it before `load_session_and_drafts` opens restored tabs. With a pending A→B migration, the tab for B reads a sidecar that does not exist yet and shows **0 bookmarks**; nothing re-resolves after the migration completes; and the user's next bookmark toggle whole-file-replaces the sidecar with the stale empty set — which `save_document` turns into a **delete** when the set is empty (`bookmark_service.rs:131-137`). The startup gate's own doc says consumers "must wait until app-owned metadata is known to be current"; reconciliation is what makes it current, and it was not awaited. | `WFR-NOTES-BOOKMARKS` reaching cross-cutting `startup_data.rs` | `ui/window/startup_data.rs:78-82` (the unawaited dispatch) and, post-migration, `ui/window/notes/journal.rs` (`reconcile_pending_migrations_on_startup`) plus `ui/window/notes/bookmark_execution.rs` (`resolve_notes_for_editor`, `persist_bookmarks_now`) — the code moved out of `notes/mod.rs` during this change |

## User-visible strings the fixes add, recorded against the non-goals

The proposal's non-goals say "no ... user-visible string" change. The data-safety
fixes require **two new** ones, and neither alters an existing string:

| String | Surface | Why it is required |
| --- | --- | --- |
| `"A file named 'X' already exists in this folder"` | status bar, `NotificationSeverity::Warning`, from the sidebar section's message callback | The C-1 fix **refuses** the rename rather than performing it. A silent refusal would be worse than the bug it replaces: the user would see the old name still in the tree with no explanation. |
| `"Bookmarks could not be saved before closing"` | status bar, `MessageKind::Warning` | The H-4b close flush is synchronous and can fail. Failing silently at close is the shape of the defect being fixed. |

Both go through the existing status-bar notification lane, so no new
accessibility surface, anchor, or announcement lane is introduced and
`docs/accessibility-matrix.md` needs no new row. **A Replace-confirmation dialog
was deliberately not added**: GNOME Files offers one, and it would be the better
long-term product answer, but it is a feature rather than a fix, and refusing is
strictly safer than the status quo. Recorded as a follow-up product decision
rather than shipped half-done.

## Findings recorded and handed on, with the reason

**Nine findings**, not four: H-7, M-3, M-5, M-6, M-7, M-8, M-9, M-10, and M-11.
(H-4c is a *verdict of not-a-defect*, not a handed-on finding, and is listed
separately below.)

**Each now has a durable home outside this change directory**, because a change
directory is archived and these outlive it:

| Finding | Durable home |
| --- | --- |
| H-7 external rename strands sidecars | `docs/next/bookmarks.md`, "Good Next Steps" item 1 |
| M-3 premature teardown survives a refused close | `docs/next/persistent-format-hardening.md` |
| M-5 startup gate fails open on scan failure | `docs/next/persistent-format-hardening.md` |
| M-6 partial Convert loses its backup pointer | `docs/next/persistent-format-hardening.md` |
| M-7 Start Fresh deletes uncopied bytes | `docs/next/persistent-format-hardening.md` |
| M-8 transient read failure quarantines `workspaces.json` | `docs/next/persistent-format-hardening.md` |
| M-9 no retry route past the attempt cap | `docs/next/persistent-format-hardening.md` |
| M-10 `Guarded` group doc over-promises | `docs/next/persistent-format-hardening.md` |
| M-11 note editor Escape discards prose | `docs/next/persistent-format-hardening.md` |

These are confirmed or needs-decision findings **outside the two rows this change
migrates**. Fixing a defect in a row this change does not own is the hazard task
6.1 records for the production `.imp()` reach-throughs: it gives a migrated or
future row a change nobody planned, and it would be reviewed against this
change's diff rather than against that row's contracts. Each is recorded here
with its severity, site, and owning row so the receiving slot inherits a verdict
rather than a hunt.

| Id | Severity | Finding | Owning row / capability |
| --- | --- | --- | --- |
| H-7 | HIGH, **needs-decision** | External rename strands sidecars permanently. Sidecars are keyed only by an FNV hash of the canonical path (`model/sidecar_identity.rs:34-39`) and migration runs from exactly one place — the in-app sidebar rename callback. The watcher classifies external renames for tree refresh only and calls no `move_path_tree`, **even though it already carries both paths** (`workspace_watch.rs:73`, `:481-483`). A `mv`, `git mv`, or file-manager move makes the note unreachable from every UI surface, silently. Path-keying is itself the *safer* choice — inode-keying would lose notes on every external atomic save — so this is a product decision, not a mechanical bug. Candidate fix: correlate `RenameMode::Both` into the same ledger-tracked migration, or add an orphaned-notes recovery affordance. | `WFR-NOTES-BOOKMARKS` product decision + `external-file-monitor-coverage`. Deliberately **not** taken silently: it changes what a rename means. |
| H-4c | MEDIUM, **not a defect** | A bookmark toggle never sets `modified`, so the close gate at `tabs.rs:52` closes the tab with no prompt. **Verdict: correct as-is.** Bookmarks are sidecar metadata, not document content; marking the *document* modified because a bookmark moved would make Ctrl+S rewrite the file and would make the close prompt lie about unsaved *text*. The real gap was the missing flush, fixed as H-4b. | — |
| M-3 | MEDIUM | Premature teardown survives a refused close: `documents.rs:1107-1109` runs `cancel_load()` / `stop_file_monitor()` / `untrack_editor_memory()` *before* `close_page`, which `handle_tab_close_request` can refuse. `start_file_monitor` is only re-armed by a load or buffer-replacement completion, so a tab surviving a cancelled close permanently loses external-change detection. `handle_tab_detached` already does this cleanup on real detach, so the fix is to delete the three eager calls. The adjacent `open_paths.remove` at `:1100-1104` is **not** a defect — `reconcile_open_paths_from_tabs` repairs it. | tabs/close workflow — `WFR-SHELL-LAYOUT`, slot 7 |
| M-4 | MEDIUM | `load_workspaces` clobbers a pre-load mutation: `build_sections_from_file` unconditionally does `*imp.workspaces_file.borrow_mut() = workspaces_file;` with no load generation, while "New Workspace" is live from window present. `persist()` already calls `state.request_mutation()`, so the fix is to skip `build_sections_from_file` when the requested generation advanced since dispatch. | `WFR-WORKSPACE-TREE` — **in this row.** Recorded as a fifth in-row candidate; see the note below. |
| M-5 | MEDIUM | The startup gate **fails open on scan failure**: `FormatPlan` has only `groups` (`plan.rs:16-19`), so `build_plan` structurally discards `inventory.diagnostics`, and `requires_startup_decision()` walks only `groups`. If `bookmarks/` is a file, or unreadable, `scan_json_directory` records a diagnostic and contributes zero items; the plan is empty, startup continues silently, and every record in that directory is invisible with no warning. | `format-upgrade-workflow` via cross-cutting `startup_data.rs` |
| M-6 | MEDIUM | Mid-loop `?` at `apply.rs:246-262` discards `failures`, `converted_count`, and the backup-manifest handle. Durability is fine and retry is idempotent, but backup items use hashed leaf names and the manifest path exists only in a `tracing::info!`, so after a partial Convert the user sees a bare I/O error with no indication a recoverable backup exists or where. | `format-upgrade-workflow` |
| M-7 | MEDIUM | No write guard plus second-granularity mtime on the Start Fresh delete (`backup.rs:262-272`). `ensure_regular_file_unchanged` *is* called before the copy, the read, and the removal, but no `TargetWriteGuard` spans check→write and `modified_at_secs` is whole seconds. Benign for Convert (old bytes are in the backup); for **Start Fresh** it deletes bytes that were never copied. | `format-upgrade-workflow` |
| M-8 | MEDIUM | A transient read failure renames the live `workspaces.json` away: `RecoveryProblem::Unreadable` is classified identically to structural corruption (`recovery_metadata.rs:579-587`, `:825-845`) and triggers `preserve_original`, which quarantines the live file and returns default state with `replacement_allowed = true`; the sidebar then persists an empty configuration. An `EMFILE`/`ENOMEM`/`EIO` blip at startup empties the user's workspace list on disk. | `recovery-metadata-integrity`, cross-cutting |
| M-9 | MEDIUM, needs-decision | No retry route for a `record_pending` failure, and after `MAX_MIGRATION_ATTEMPTS = 3` the kind is skipped forever with only a warning. | `WFR-MIGRATION-LEDGER` |
| M-10 | LOW | `FormatPlanGroupKind::Guarded`'s doc over-promises: group atomicity is enforced only in the backup phase, not in the write loop, so a mid-loop `?` can split a Guarded group across two format versions. Contained — the gate re-fires next launch and both halves have backups. | `format-upgrade-workflow` |
| M-11 | LOW, needs-decision | Note editor Escape / parent close discards typed prose (`editors.rs:310`, `:451` set `RESPONSE_CANCEL` as the close response with no unsaved guard). Escape-as-Cancel is conventional, so this is a product decision rather than a defect. | `WFR-NOTES-BOOKMARKS` product decision |

**M-4 is in this row and is fixed here.** `load_workspaces` now captures
`requested_generation()` **before** dispatching the load and skips
`build_sections_from_file` when a mutation superseded it — because
`persist()` has already scheduled that mutation for disk, so adopting the loaded
file would revert an in-memory workspace the user just created while its write was
pending. That mismatch is what makes it data loss rather than a stale view. The
guard logs the skip and still fires the structure/scope notifications, so no
consumer is left un-refreshed.

## Pass 2 — over the finished diff (task 7.5)

Mode: `data-safety` explicit, scoped to the uncommitted diff, read-only, plus
this change's own review of each fix's control flow. Aimed specifically at
hazards the **fixes themselves** could introduce, which is the point of a second
pass over a change whose first pass produced eleven findings — and it earned its
place immediately, by finding one.

### Seven defects found in this change's own fixes, all fixed

The second pass earned its place. **Every one of these was introduced by a
fix landed earlier in this same change**, which is the argument for the pass
existing: a fix written against a confirmed defect is written under pressure, and
the pressure is what produces the next defect.

| Id | Defect introduced by a fix in this change | Fix |
| --- | --- | --- |
| P2-1 | **`rename_target_guarded` could deadlock a worker forever.** `TargetWriteGuard` keys on the **resolved** identity — symlinks canonicalize to their target — while the code sorted the **raw** paths. Renaming the symlink `link` (→ `target`) to the name `target` resolved *both* paths to one key, so the second acquire blocked on the first for the process's lifetime, holding one of eight worker slots. A few of those exhaust the pool and **all** background I/O stalls, including draft autosave. The doc comment's deadlock-freedom claim was simply false. | Resolve **first**, compare the resolved identities, refuse when they are equal (which is also the correct user-facing answer), then acquire in **resolved** order via `from_identity`. |
| P2-2 | **The close-time flush bypassed the unread-sidecar guard**, so it was the one path that could write an empty set over a sidecar the editor had never read — and because `save_document` deletes on empty, that is a **deletion**. | The flush applies the same guard and leaves `save_dirty` set. |
| P2-3 | **The deferral had no retry path.** Nothing consults `save_dirty` outside a live flight's completion, so a deferred write waited for the user's next edit — or for the flush in P2-2. | `resolve_notes_for_editor`'s completion re-drives a pending write the moment the fact it waits on becomes true. |
| P2-4 | **The synchronous flush raced the in-flight async write with no coordination at all**, and the doc comment asserted a guard that did not exist: `bookmark_service::save_document` never took one. Toggle A → worker writes `{A}` → toggle B → close → flush writes `{A,B}` → the worker's older `rename()` lands last → the sidecar is `{A}`. **The flush lost the bookmark it existed to save.** | `save_document` now takes the target write guard, which orders *all* sidecar writers. An unresolvable target (the first write to a fresh profile, before the directory exists) proceeds without it, because at that point there is no existing sidecar to lose — and refusing to save a bookmark because a coordination key could not be computed would be worse than the race. |
| P2-5 | **`sidecar_resolved` was never reset on Save As**, leaving the flag `true` from the old identity and disarming the guard for the new path's resolve window. | `reset_notes_after_save_as` clears it before re-resolving. |
| P2-6 | **The success re-save path was routed through the debounce**, adding a fresh 200 ms in which the newest set was off disk — widening the exact window the flush was added to close. Not on the declared change list. | A newer in-flight edit re-persists **immediately**, as it always did; only failures go through the debounce. |
| P2-7 | **The flush read through a panicking `TemplateChild`.** `bookmark_records()` → `source_view()` panics once GTK4 has cleared template children, and the flush is a new read at a teardown boundary. | Early return when `source_view.try_get()` is `None`. |

Two lower-severity gaps in the same fixes were also closed: a **refused** rename
of a new item left its placeholder on disk and in the tree (the I/O-failure arm
cleaned up, the refusal arm did not), and the placeholder cleanup silently dropped
a guard-resolution failure that its own doc comment promised to log.

### One defect found in this change's own fix, and fixed

**P2-0: unbounded retry on a failing bookmark write.** The H-4a fix re-armed the dirty
flag and rescheduled through the debounce on failure — which, for a sidecar that
stays unwritable (read-only directory, full disk, permissions), meant a write
attempt **every 200 ms forever**, each one publishing `"Bookmark save failed"` to
the status bar. That is worse than the defect it replaced: the original at least
gave up silently.

Caught by reading the completion path's control flow rather than by a test, and
fixed before the lanes ran: `NotesPersistenceState` gained a
`save_failure_streak` counter, `policy::MAX_BOOKMARK_SAVE_ATTEMPTS` is pinned at
**3** with the reason beside it, the warning is published **once per streak**
rather than once per retry, and past the cap nothing reschedules on its own. The
`save_dirty` flag is still set on failure — so the state honestly says "not on
disk" and the **close-time flush tries once more** — while `reschedule` is a
separate decision. Separating those two was the actual fix: conflating them is
what made the loop unbounded.

This is the same shape as workspace persistence, which the audit named as the
asymmetry proving H-4 was a gap: bounded backoff, then **await explicit intent**.
Here explicit intent is the user's next bookmark edit or the close flush.

### The coverage gap this leaves, stated rather than papered over

Four of the seven pass-2 defects (P2-2 through P2-5) live in the
`sidecar_resolved` deferral and the close-flush ordering, and **no test exercises
them**. `grep` finds no reference to `sidecar_resolved` outside
`bookmark_execution.rs`. The three new tree-side tests and the tab-detach flush
test cover the rename/cleanup hazards and a *non-empty* flush; the empty-set
deferral, the flush-versus-in-flight race, and the Save As reset are reasoned and
guarded but unproven. Recorded as the highest-value follow-up test work rather
than claimed as covered — the whole point of P2-2's discovery is that "the guard
is obviously applied everywhere" was wrong.

### Hazards checked and cleared

| Hazard | Verdict |
| --- | --- |
| **Lock inversion between `migration_operation_lock` and `ledger_lock`** | Clear. `run_tracked_kind` takes the operation lock, then `mark_kind_completed`/`mark_kind_failed` take the ledger lock inside it — always in that order. Nothing takes the operation lock while holding the ledger lock: `record_pending`, `mark_kind_completed`, and `mark_kind_failed` each take and release the ledger lock without calling out, and `reconcile_pending` takes the operation lock in a **scoped block** around `run_migration_kind` only, releasing it before `mark_kind_completed`. The two locks are distinct objects, asserted by a unit test, which matters because the operation's own completion re-enters the ledger lock and a single shared mutex would deadlock. |
| **Re-entrancy of `migration_operation_lock`** | Clear. `std::sync::Mutex` is not reentrant, so a nested `run_tracked_kind` would deadlock — and there is none: the three `run_tracked_kind` calls in `notes/journal.rs` are **sequential** inside one worker closure, and `reconcile_pending` calls `run_migration_kind` (not `run_tracked_kind`) inside its scoped guard. |
| **Two-guard deadlock in `rename_target_guarded`** | **Was not clear; this row's earlier text was wrong and is corrected.** The first version sorted the **raw** paths while `TargetWriteGuard` keys on the **resolved** identity, which orders nothing about the keys — and, worse, renaming a symlink onto the name of its own target resolves *both* paths to one key, so the second acquire blocked on the first for the process's lifetime. That is now fixed (P2-1): resolve first, refuse when the two identities are equal, then acquire in **resolved** order via `from_identity`. Regression test: `test_inline_rename_of_a_symlink_onto_its_target_refuses_without_hanging`, whose second ordinary rename is the worker-pool-exhaustion detector. |
| **`TargetWriteGuard` on the GTK thread** | One site, deliberate and documented: `flush_bookmarks_for_editor` on the teardown path. It cannot deadlock (a worker needs no GTK turn to release) and the payload is a small JSON sidecar, never document text. Documented at the function so it is not copied elsewhere. |
| **`sidecar_resolved` blocking a legitimate empty write forever** | Clear. Every file-backed editor reaches `resolve_notes_for_editor` — through `connect_file_loaded` on load and reload, and through `reset_notes_after_save_as` after Save As — and the flag is set in the completion **before** the result is matched, so it is set on both the success and the error arm. It is also set after any successful non-empty write. There is no path that reaches `persist_bookmarks_now` for a saved editor whose sidecar has never been resolved, except the exact startup window the guard exists for. |
| **Per-item isolation leaving the ledger inconsistent** | Clear. `anyhow::bail!` after a partial loop makes `run_tracked_kind` call `mark_kind_failed`, so the kind stays pending and retries — and the retry is idempotent for the already-moved sidecars, because their identity no longer rebases from `old_path` and `rebase_identity_paths` returns `None`, so they are skipped rather than re-moved. |
| **Evidence-surface reentrancy** | Clear, and proved by test. Every derived scalar in `notes_evidence()` is computed into a local with its `Ref` explicitly dropped before the struct literal; no field is read inside a mutable borrow. |
| **Statements lost or reordered by the module moves** | Clear. The moves were performed as literal slices of the original files, and `git diff HEAD` for the renamed pair (`bookmarks.rs` → `bookmark_execution.rs`, `editors.rs` → `editor_execution.rs`) shows only the intended edits. The `GtkTreeExpander` gesture-disable block and the peek controller's phase and gate are byte-identical. |

