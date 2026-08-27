# Ordered stages and real inversion counts, two workflows (task 0.4)

Read from the code, not from the census. Two units are counted separately and
never mixed: a **deferral primitive** (`spawn_blocking_then`,
`idle_add_local_once`, `timeout_add_local(_once)`, a `Debounce` instance, a
`SupersedingTimer` instance) and a **non-primitive callback resumption** (a
dialog response, a `FileDialog`/folder chooser, a drag-lifetime callback, a
selection/activation callback, or the process boundary itself).

## `WFR-WORKSPACE-TREE`

Matrix records **five** inversions. Measured primitives in `ui/sidebar/**`:
**11 `spawn_blocking_then`, 8 `idle_add_local_once`, 4
`timeout_add_local(_once)`, 3 `Debounce` instances, 1 `SupersedingTimer` = 27**,
confirming authoring's total exactly. Each is attributed to **exactly one**
stage order below, so the subtotals sum to 27 rather than to authoring's
unreconciled 32.

`Debounce` instances: `sidebar/imp.rs::persist_debounce`,
`workspace_section/imp.rs::RefreshRuntimeState::debounce`,
`workspace_section/imp.rs::WatchRuntimeState::restart_debounce`.
`SupersedingTimer`: `sidebar/imp.rs::workspace_filter_settle_timer`.

### Eleven stage orders

| # | Stage order | Primitives | Sites |
| --- | --- | --- | --- |
| 1 | Workspace-list load | 1 | `workspaces.rs:38` spawn |
| 2 | Directory scan and expansion | 6 | `tree_loading.rs:345` spawn (child scan), `:429` timeout (admission retry), `:1137` timeout_once (reconcile batch), `:1252` timeout_once (child-state restore), `folders.rs:471` idle (model-state restore), `row_factory.rs:316` idle (factory realization) |
| 3 | Watcher install, mirror, mailbox drain | 5 | `watch.rs:175` idle (expanded hook), `:258` spawn (install), `:351` timeout (poll source), `:581` spawn (retire), `restart_debounce` |
| 4 | Targeted in-place refresh | 2 | `refresh.rs:375` idle (dispatch batch), `RefreshRuntimeState::debounce` |
| 5 | Folder-reorder DnD | 1 | `tree_loading.rs:143` idle (shield's no-scan child-model fallback) |
| 6 | Workspace persistence | 3 | `workspaces.rs:715` spawn (persist worker), `:804` idle (flush completion), `persist_debounce` |
| 7 | File create / rename / delete | 5 | `actions.rs:43` spawn (create), `:76` idle (create), `:269` spawn (rename), `:397` spawn (delete), `context_menus.rs:236` idle (popover action route) |
| 8 | `Space` peek | 1 | `peek.rs:387` spawn |
| 9 | Workspace add / rename / unlist | 1 | `workspaces.rs:298` spawn (add folder) |
| 10 | Workspace scope filter fade | 1 | `workspace_filter_settle_timer` |
| 11 | Focused-folder drilldown | 1 | `folders.rs:621` spawn (folder-empty probe) |
| | **total** | **27** | |

**Shared path named once, not twice.** `folders.rs:471`
`restore_folder_model_state` is reached both by the scan bootstrap and by
drilldown enter/leave. It is attributed to stage order 2 (expansion restore) and
recorded here as *shared with* stage order 11, not counted in both.

### Non-primitive callback resumptions (11, counted separately)

- 4 dialog responses: delete confirmation, workspace rename, workspace unlist,
  and the add-folder `FileDialog`.
- 2 inline-rename resumptions: `GtkEntry::activate` and the focus-out
  double-fire-guarded path.
- 3 DnD lifetime callbacks: drag prepare, drag begin/end, and drop.
- 2 selection/activation callbacks: `GtkListView::activate` (row open) and the
  selection change that re-targets peek.

**Reconciled total: 27 primitives + 11 callback resumptions = 38 resumption
points across 11 stage orders.** The census cell records 5. **The floor is off
by roughly 7.6×, the widest in the programme.**

### Two stage orders authoring's first pass missed, both confirmed

1. **Workspace scope filter** (#10). `ui/sidebar/workspaces.rs:166`
   `animate_workspace_filter_change` runs a revealer fade and settles through
   `workspace_filter_settle_timer` — the row's single `SupersedingTimer`, whose
   *primitive* was already inside the count of 27 while its *stage order* was
   unnamed. This row also projects `filter_animation_active` and the
   `workspace-filter-animation` readiness blocker from it, so leaving it unnamed
   would narrate a projected field with no stage behind it.
2. **Focused-folder drilldown** (#11). `workspace_section/folders.rs:247`
   `focus_folder` plus `drilldown_stack`, entered from `row_factory.rs:107`,
   with four DnD gates keyed on drilldown emptiness. `workspace-sidebar-shell`
   names focused-folder mode as a required state extreme.

### The inversion that most needs naming

**Deferred expansion restore.** `schedule_child_state_restore`
(`tree_loading.rs:1252`) and `restore_folder_model_state`
(`folders.rs:471`) resume after the model has been replaced, and they MUST read
the live `expanded_paths` set **at apply time**. A snapshot cloned at schedule
time resurrects a collapse the user performed in between.

## `WFR-NOTES-BOOKMARKS`

Matrix records **four** inversions. Measured primitives in
`ui/window/notes/**` plus the window-imp cells this row owns: **15
`spawn_blocking_then`, 2 `idle_add_local_once`, 4 `Debounce` instances = 21**.

`Debounce` instances owned by this row: `browser.rs::NotesBrowserState::search_debounce`,
`editors.rs::NoteSaveQueue::debounce`, `ui/window/imp.rs::save_debounce` (bookmark
persistence), `ui/window/imp.rs::command_palette_notes_refresh_debounce`.

### Five stage orders

| # | Stage order | Primitives | Sites |
| --- | --- | --- | --- |
| 1 | Notes browser source build + query | 4 | `browser.rs:990` spawn (source load), `:1503` spawn (query), `:372` spawn (palette source refresh), `search_debounce` |
| 2 | Bookmark lifecycle (toggle / edit / persist) | 4 | `bookmarks.rs:321` spawn (persist), `:394` spawn (closed-file excerpt preview), `save_debounce`, `command_palette_notes_refresh_debounce` |
| 3 | Note editors (document + folder) | 9 | `editors.rs:152`, `:248`, `:341`, `:344`, `:383`, `:488`, `:491`, `:529` spawn, `NoteSaveQueue::debounce` |
| 4 | Sidecar migration on rename | 2 | `mod.rs:437` spawn (record pending + run tracked kinds), `mod.rs:513` spawn (startup reconcile) |
| 5 | Editor note resolution on load / Save As | 2 | `mod.rs:379` spawn (`resolve_notes_for_editor`), `mod.rs:876` idle (`focus_after_present`) |
| | **total** | **21** | |

### Non-primitive callback resumptions (7, counted separately)

- 4 folder-chooser / dialog responses in `editors.rs` (folder note target choice,
  document and folder note dialog commit/discard).
- 2 `connect_closed` dialog-teardown resumptions (`browser.rs`, `editors.rs`).
- **1 process boundary** — see below.

**Reconciled total: 21 primitives + 7 callback resumptions = 28 resumption
points across 5 stage orders.** The census cell records 4; the floor is off by
7×.

### The inversion that most needs naming

**The cross-process resumption.** `migrate_note_sidecars_after_rename`
(`mod.rs:431`) calls `migration_ledger::record_pending` **before** any sidecar
moves, then runs the three kinds under `run_tracked_kind`. If the process dies
mid-run, control resumes in `reconcile_pending_migrations_on_startup`
(`mod.rs:511`) **on a later app launch**, bounded by `MAX_MIGRATION_ATTEMPTS`.
This is the longest-lived inversion in the codebase and the reason the migration
ledger is a `journal` (task 2.7).

## Format-upgrade startup gate — **not** attributed to either row

`ui/window/startup_data.rs` holds 2 `spawn_blocking_then`, 1 dialog
`connect_response`, and a user-driven retry loop. Task 2.2 decides that module
**cross-cutting**, so its 4 resumption points are attributed to neither row's
trace and are recorded in the matrix's Cross-Cutting Coordination section
instead. Authoring's notes-side count of "startup format gate 4 plus a
user-driven retry loop" is therefore **removed** from this row rather than
re-derived: it is an ownership correction, not a count correction.
