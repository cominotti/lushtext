# `WFR-WORKSPACE-TREE` reconciled stage trace (task 0.4)

Re-derived from the code on a clean tree, not inherited. Slot 5a's trace
(`openspec/changes/archive/2026-08-27-migrate-workspace-tree-and-notes-workflow-readability/evidence/stage-traces.md`)
was read first and is treated as a claim to verify. It is **corrected in five
places**, all in the direction of more resumption points, and its two explicit
reconciliation moves are **confirmed**.

## Method, and the two units kept separate

Scope is the **24 `.rs` files** under `crates/lushtext-core/src/ui/sidebar/` and
`crates/lushtext-core/src/ui/sidebar/workspace_section/` — the 23 row files the
size re-derivation counts (task 0.3), plus `file_tree_item.rs`, which is excluded
from sizes as a surface with no coordination tier but is **included here** because
the `pending_rename` handoff is stored on it.

Two units, never mixed:

- A **deferral primitive** is one registered `gtk_lush_tasks::spawn_blocking_then`
  call, one `glib::idle_add_local_once` call, one `glib::timeout_add_local(_once)`
  call, one `gtk_lush_settle::Debounce` instance, one `SupersedingTimer` instance,
  or one `SettleBurst` instance. Instances are counted where the field is
  declared, not per `arm()` site.
- A **non-primitive callback resumption** is a point where an already-started
  stage order of this workflow resumes at a toolkit callback rather than at a
  direct call: a dialog or file-chooser response, a `GtkRevealer`
  animation-completion notify, a drag-lifetime callback, a selection or activation
  callback, a popover `connect_closed`, or a GTK factory bind that consumes a
  one-shot flag. A user gesture that **starts** an order is an entry point, not a
  resumption, and is excluded.

Grep set: `spawn_blocking_then`, `idle_add`, `timeout_add`, `Debounce`,
`SupersedingTimer`, `SettleBurst`, `connect_done`, `connect_response`,
`connect_closed`, `FileDialog`, `select_folder`, plus the animation-completion
and drag/selection/activation callbacks.

**No `SettleBurst` instance and no `AdwTimedAnimation::connect_done` exists in
this row.** The filter fade settles through a `GtkRevealer`
`notify::child-revealed` handler plus a `SupersedingTimer` safety net; that notify
handler is counted as a non-primitive resumption, not as a primitive.

Raw primitive census, by kind:

| Kind | Count | Delta vs slot 5a |
| --- | --- | --- |
| `spawn_blocking_then` | 12 | **+1** |
| `glib::idle_add_local_once` | 8 | 0 |
| `glib::timeout_add_local(_once)` | 4 | 0 |
| `Debounce` instances | 3 | 0 |
| `SupersedingTimer` instances | 1 | 0 |
| `SettleBurst` instances | 0 | 0 |
| **total primitives** | **28** | **+1** |

The three `Debounce` instances are `sidebar/imp.rs:103` (`persist_debounce`),
`workspace_section/imp.rs:101` (`RefreshRuntimeState::debounce`), and
`workspace_section/imp.rs:181` (`WatchRuntimeState::restart_debounce`). The one
`SupersedingTimer` is `sidebar/imp.rs:78` (`workspace_filter_settle_timer`, armed
at `workspaces.rs:199`).

## Twelve stage orders, with every primitive attributed exactly once

The eleven stage orders named in task 0.4, plus the twelfth decided below. Every
primitive appears in exactly one row, so the subtotals sum to 28. Where a
primitive is on a code path several orders reach, it is attributed to the order
that **owns** the path and the sharing is stated; it is never counted twice.

| # | Stage order | Prim. | Sites (file:line) |
| --- | --- | --- | --- |
| 1 | Directory scan and expansion | 5 | `workspace_section/tree_loading.rs:345` spawn (child scan worker); `:429` `timeout_add_local` (scan admission retry — **shared** with #11 via `retry_one_folder_empty_admission` at `:460`, counted here); `:1137` `timeout_add_local_once` (batched child reconcile); `:1252` `timeout_add_local_once` (`schedule_child_state_restore`); `workspace_section/folders.rs:471` idle (`restore_folder_model_state`, reached from `load_folder_model` — **shared** with #3, #10, #11, counted here) |
| 2 | Watcher install + mailbox reconcile | 5 | `workspace_section/watch.rs:175` idle (row watch refresh after a `notify::expanded` transition); `:268` spawn (install/retire worker); `:361` `timeout_add_local` (mailbox poll source); `:591` spawn (`retire_watcher`, off-GTK watcher drop); `workspace_section/imp.rs:181` `restart_debounce` |
| 3 | Targeted in-place refresh | 2 | `workspace_section/refresh.rs:375` idle (scan-dispatch batch continuation); `workspace_section/imp.rs:101` `RefreshRuntimeState::debounce` |
| 4 | Folder-reorder DnD | 1 | `workspace_section/tree_loading.rs:143` idle (`empty_children_model_for_drag_hover` collapse-back shield) |
| 5 | File create / rename / delete | 7 | `workspace_section/actions.rs:49` spawn (`create_unique`); `:82` idle (append into the child store after the parent row expands); `:283` spawn (guarded rename worker); `:446` spawn (guarded delete worker); `:657` spawn (`spawn_temp_item_cleanup`, inode-rechecked placeholder discard); `workspace_section/context_menus.rs:236` idle (popover popdown after an action routes — shared context-menu chrome, counted here); `workspace_section/row_factory.rs:316` idle (post-bind `begin_rename()` kick) |
| 6 | `Space` peek | 1 | `workspace_section/peek.rs:387` spawn (`file_peek::load_snapshot`) |
| 7 | Workspace list add / rename / unlist, with debounced persistence | 3 | `workspaces.rs:736` spawn (persistence worker); `:825` idle (close-time flush completion on the already-durable branch); `sidebar/imp.rs:103` `persist_debounce` |
| 8 | Workspace-list load | 1 | `workspaces.rs:45` spawn (`workspace_manager::load_recovering`, resuming against `requested_generation`) |
| 9 | Workspace scope filter fade + settle timer | 1 | `sidebar/imp.rs:78` `workspace_filter_settle_timer` |
| 10 | Focused-folder drilldown | 0 | none owned. `focus_folder` (`folders.rs:247`) and `navigate_back` (`:269`) call `load_folder_model` synchronously; their only inversion is the shared `folders.rs:471` restore counted in #1, and the shared popdown idle counted in #5 |
| 11 | Top-level folder-row / empty-probe order | 1 | `workspace_section/folders.rs:621` spawn (folder-empty probe over `top_level_store`). Shares `tree_loading.rs:429` (#1) and `folders.rs:471` (#1) |
| 12 | **Workspace folder add / remove** | 1 | `workspaces.rs:319` spawn (folder identity + existing-identity resolution off the GTK thread) |
| | **total** | **28** | sums to the raw census above |

Stage order 10 owning **zero** primitives is a real finding, not a gap: a stage
order is defined by having its own entry point, its own ordered stages, and its
own terminal, not by owning a deferral primitive. Drilldown enter/leave is a
synchronous model replacement whose deferred restore is the scan order's.

### Slot 5a's two named reconciliation moves — both confirmed

- **`tree_loading.rs:143` belongs to the DnD shield, not the scan order.**
  Confirmed. The site is inside `empty_children_model_for_drag_hover`, whose own
  comment states the contract: GTK may ask `GtkTreeListModel` for children when a
  row auto-expands during hover, so the function returns an empty temporary store
  and schedules `suppress_next_expanded_watch_for_drag` + `set_expanded(false)`
  without scanning or restarting watches. It is reorder-hover inertness
  machinery, and `.agents/rules/ui.md`'s "Workspace folder reorder hover must be
  inert" is the contract it implements.
- **`folders.rs:471` named once as shared.** Confirmed as a method, with one
  correction to its extent: `restore_folder_model_state` is called from exactly
  one place, `load_folder_model` (`folders.rs:120`), which is reached from
  `load_folders`/`load_workspace_folders` (#11), `focus_folder`/`navigate_back`
  (#10), and the full-reload refresh path (#3, `refresh.rs:299`) as well as the
  scan bootstrap (#1). It is shared across **four** orders, not two. Attribution
  stays with #1, because model installation with expansion restore is the scan
  order's own stage.

### Where slot 5a's attribution was wrong

1. **`actions.rs:657` was missing.** `spawn_temp_item_cleanup` is a twelfth
   `spawn_blocking_then`. Its own doc comment records that it replaced a detached
   `std::thread::spawn` doing a path-only `remove_file_if_exists` — that is one
   of slot 5a's seven data-safety fixes. Slot 5a landed the primitive and did not
   add it to its own count, so 27 was already stale when it was written.
   Primitives: 27 → **28**.
2. **`row_factory.rs:316` was attributed to the scan order as "factory
   realization".** It is not. The idle body is `s.begin_rename()`, reached only
   from the `is_pending_rename()` branch at `row_factory.rs:309-320`. It is the
   inline-rename order's post-bind kick. Reassigned #1 → **#5**.
3. **`folders.rs:621` was attributed to focused-folder drilldown.** The probe is
   scheduled by `schedule_folder_empty_check` from `load_folder_model` for
   **top-level** entries and carries `top_level_store` through the worker, with
   ownership rechecked against `imp.top_level_store` on completion. Reassigned
   drilldown → **#11, the top-level folder-row / empty-probe order**.
4. **The New Workspace name dialog response was missing** from the non-primitive
   list. `dialogs.rs:43` `connect_response` → `handle_new_workspace_name`.
   Dialog responses: 4 → **5**.
5. **The filter fade's animation-completion resumption was missing entirely.**
   `sidebar/imp.rs:209` `connect_child_revealed_notify` is the path that actually
   applies the filter and re-reveals the list; the `SupersedingTimer` is
   documented in `workspaces.rs:212` as a *safety net* for headless frame clocks.
   Slot 5a counted the safety net and not the primary resumption.
6. **Inline-rename resumptions were undercounted at 2.** There are three
   controller-based ones (`connect_activate` commit, Escape cancel, focus-out
   cancel) plus the `pending_rename` bind handoff.

## Non-primitive callback resumptions (16, counted separately)

| # | Site | Resumes | Order |
| --- | --- | --- | --- |
| 1 | `dialogs.rs:43` `AlertDialog::connect_response` | New Workspace name confirmed → `handle_new_workspace_name` | 7 |
| 2 | `dialogs.rs:86` `gtk4::FileDialog::select_folder` | folder chosen → `handle_add_folder_to_workspace` | 12 |
| 3 | `dialogs.rs:152` `connect_response` | workspace rename confirmed | 7 |
| 4 | `dialogs.rs:189` `connect_response` | workspace unlist confirmed | 7 |
| 5 | `workspace_section/actions.rs:437` `connect_response` | delete confirmed → guarded delete worker | 5 |
| 6 | `workspace_section/actions.rs:166` `entry.connect_activate` | inline rename committed | 5 |
| 7 | `workspace_section/actions.rs:182` `connect_key_pressed` (Escape) | inline rename cancelled; for a new item, routes to `cancel_new_item` | 5 |
| 8 | `workspace_section/actions.rs:210` `focus_ctl.connect_leave` | inline rename cancelled by focus-out (double-fire guarded) | 5 |
| 9 | **`pending_rename` one-shot handoff** — field `ui/sidebar/file_tree_item.rs:35`, set at `workspace_section/actions.rs:75`, consumed and cleared at `workspace_section/row_factory.rs:309-320` | the create order suspends after the placeholder item is appended and resumes **when GTK next binds the recycled row**; the bind clears the flag, re-points `context_target`, and queues the `begin_rename()` idle (primitive #5/`row_factory.rs:316`) | 5 |
| 10 | `workspace_section/dnd.rs:77` `connect_prepare` | drag content resolved from the pressed row | 4 |
| 11 | `workspace_section/dnd.rs:86` `connect_drag_end` | drag lifetime torn down, overlays reset | 4 |
| 12 | `workspace_section/dnd.rs:147` `connect_drop` | drop position validated → reorder request | 4 |
| 13 | `workspace_section/peek.rs:49` `selection.connect_selected_notify` | selection moved → `refresh_peek_for_selection` re-targets the open peek | 6 |
| 14 | `workspace_section/peek.rs:295` `popover.connect_closed` | peek popover dismissed → `clear_peek_state` | 6 |
| 15 | `ui/sidebar/imp.rs:209` `workspace_list_revealer.connect_child_revealed_notify` | fade-out complete → `apply_workspace_filter_visibility` + re-reveal; fade-in complete → clear `workspace_filter_animation_active` and re-enter if the scope changed again. **One registered callback carrying two ordered resumptions**; counted as 1 | 9 |
| 16 | `workspace_section/mod.rs:264` `file_tree_view.connect_activate` | a bound row is activated → `activate_file_at` → the row's file-activated callback, handing off to `WFR-DOCUMENT-LOAD`. This is the tree order's terminal and is not reachable by a call chain from the scan that produced the row | 11 |

Deliberately **excluded** as projection rather than resumption, and stated so the
next reader does not re-add them: `dnd.rs:107` `connect_motion` and `:136`
`connect_leave` (insertion-line hover cue inside one live drag);
`row_factory.rs:167`/`:177` hover enter/leave (row button visibility);
`peek.rs:331` `connect_key_pressed` and the `context_menus.rs` action/gesture
handlers (entry points that **start** their orders); `watch.rs:168`
`notify::expanded` (its idle body is primitive #2/`watch.rs:175`, and counting
both would double-count one inversion).

## The twelfth candidate: workspace folder add and remove — **verdict: its own stage order**

Criterion applied, as stated in task 0.4: a stage order has **its own entry
point**, **its own ordered stages**, and **its own terminal**. Sharing a terminal
with another order does not merge the two.

**Entry points — two, neither shared with the workspace-list order.** Add enters
from the workspace header context menu's `add_folder_action`
(`workspace_section/context_menus.rs:725`) → the section's `add_folder_callback`
→ `LushtextSidebar::show_add_folder_dialog` (`dialogs.rs:71`). Remove enters from
the folder row's `remove_folder_action` (`context_menus.rs:565`) → the section's
`remove_folder_callback`, registered at `workspace_section/mod.rs:315`
`connect_remove_folder_requested` → `handle_remove_folder_from_workspace`
(`workspaces.rs:397`). `.agents/rules/ui.md` names the "add-folder request" as a
section callback the sidebar handles itself, and `Add Folder` is already in this
row's matrix `Entry points` cell, so the order is documented from two directions.

**Ordered stages — including the only off-GTK stage and the only self-restarting
retry in the membership family.** `handle_add_folder_to_workspace`
(`workspaces.rs:305`) reads the workspace's current folder paths, refuses early
with `WorkspaceFolderAddError::WorkspaceNotFound`, then dispatches
`spawn_blocking_then` (`:319`) to resolve `folder_identity` and
`existing_identities` off the GTK thread, resuming in
`apply_add_folder_to_workspace` (`:353`). On
`Err(WorkspaceFolderAddError::StaleFolderSnapshot)` that method **re-enters its
own entry point**, re-dispatching the worker against a freshly read snapshot — a
bounded retry inversion no other order in this row has.

**Terminal — two-sided, and different from the list order's.** On success it
mutates the *section* (`section.add_workspace_folder(&folder_id, folder_path)` or
`remove_workspace_folder`, falling back to `rebuild_sections_from_state()`),
**then** `persist()`, **then** `notify_workspace_structure_changed()`; remove also
emits an Info notification. Failure has its own typed terminals,
`emit_add_folder_error` / `emit_remove_folder_error` over
`WorkspaceFolderAddError` / `WorkspaceFolderRemoveError`.

**Contrast with stage order 7.** Its members —
`handle_new_workspace_name` (`workspaces.rs:284`), workspace rename, and
workspace unlist — mutate the *set of workspaces* and are **entirely synchronous
on the GTK thread** before `persist()`. None owns a worker, none has a retry, and
none reaches into a `LushtextWorkspaceSection`'s tree model. What folder
add/remove shares with them is only the debounced persistence terminal, and a
shared terminal is exactly what the criterion says does not merge orders.

**Verdict: a twelfth stage order.** Consequently `workspaces.rs:319` moves out of
the workspace-list order into #12, keeping the subtotal sum intact.

### The three consequences this verdict changes

1. **The floor correction (task 9.7).** The matrix `Workflow Stage Traces` entry
   for this row must be rewritten from "Five inversions" to **44 resumption
   points across 12 stage orders**, giving a correction factor of **8.8x** — up
   from slot 5a's 7.6x, and still the widest in the programme. The stage-order
   list in that entry must gain the folder membership order; today the entry
   narrates only scan and watch.
2. **The facade's narration budget (tasks 2.1 / 9.2).** Slot 5a projected a
   facade of **351 of the 370-line budget** for 11 orders and 38 resumption
   points. This trace adds one stage order and six resumption points. At roughly
   one narration line per named resumption plus a stage-order heading, the
   projection moves to about **358-360 of 370**: still inside budget, but the
   margin falls from 19 lines to roughly 10. The facade must therefore delegate
   at least as hard as slot 5a assumed and cannot afford module-doc prose beyond
   the stage narration itself; if it lands over 370 the retroactive-amendment
   rule applies rather than a silent overrun.
3. **`list_execution.rs` does not own folder membership (task 5.1).**
   `list_execution.rs` keeps stage orders 7 and 8 — workspace-list load and
   add/rename/unlist, both synchronous-then-persisted. Folder membership needs
   its **own** coordination module (`execution` role, qualified by its stage
   order in this row's vocabulary, e.g. `membership_execution.rs`), because it
   owns an off-GTK identity-resolution stage, a stale-snapshot retry, and a
   two-sided section+persistence terminal that the list order has none of. That
   module is the named destination for both routes: the add route from
   `dialogs.rs:71` and the remove route from `workspace_section/mod.rs:315`.
   Side effect for task 4.x: `apply_add_folder_to_workspace`'s positional
   parameter bundle stops being a private within-module boundary and becomes a
   **cross-module** workflow boundary, which strengthens the case to reify it
   rather than "record why the three parts are better passed individually".

### `apply_add_folder_to_workspace` — signature quoted, and one correction

Verbatim, `crates/lushtext-core/src/ui/sidebar/workspaces.rs:353`:

```rust
fn apply_add_folder_to_workspace(
    &self,
    workspace_id: &WorkspaceId,
    folder_path: &Path,
    existing_paths: &[PathBuf],
    folder_identity: &workspace_manager::WorkspaceFolderIdentity,
    existing_identities: &[workspace_manager::WorkspaceFolderIdentity],
) {
```

**Confirmed: six parameters** — `&self` plus five named values — and all five are
forwarded straight into
`workspace_manager::add_folder_to_workspace_with_identities`, which takes them in
the same positional order.

**One correction to the task's description.** Only **two** of them are slices:
`existing_paths: &[PathBuf]` and `existing_identities:
&[workspace_manager::WorkspaceFolderIdentity]`, which must stay **index-parallel**
because the domain call pairs them by position. The third parallel positional
value, `folder_identity`, is a **scalar** that must correspond to `folder_path`.
So the hazard is not "three parallel positional slices" but **four positional
values forming two order-dependent pairs**: (`folder_path`, `folder_identity`) and
(`existing_paths`, `existing_identities`). Both pairs are produced together by the
one worker closure at `:319` and then re-spread across five positional arguments
at the resumption — the archetype "a value renamed while crossing a seam" shape,
since swapping either pair's members is a type-checking call with different
meaning for `existing_paths`/`existing_identities` only by luck of differing
element types, and the `folder_path`/`folder_identity` pair carries no compile-time
link at all.

## Reconciled counts

**12 stage orders / 28 deferral primitives / 16 non-primitive callback
resumptions = 44 resumption points.**

| Source | Stage orders | Primitives | Non-primitive | Total |
| --- | --- | --- | --- | --- |
| Matrix `Workflow Stage Traces` floor | not stated (scan + watch narrated) | — | — | **5** |
| Slot 5a reconciliation | 11 | 27 | 11 | 38 |
| **This re-derivation** | **12** | **28** | **16** | **44** |

Correction factor against the matrix floor: **44 / 5 = 8.8x** (slot 5a's figure
was 7.6x). This remains the widest floor correction in the programme. 44 is the
number task 9.1's facade narration must cover and the number task 9.7 writes into
`docs/workflow-readability-matrix.md`.
