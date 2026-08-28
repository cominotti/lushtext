# Widget-test reach-through, catalogued by field — `WFR-WORKSPACE-TREE` (task 0.7)

**Method, stated before any figure.** A site is one occurrence of
`.imp().<identifier>` in a widget-test file. Counting is done on the file with
newline-plus-indentation collapsed, because a rustfmt-wrapped chain such as

```rust
section
    .imp()
    .watch_runtime
    .poll_source_id
    .borrow()
```

is **one** site that a per-line grep cannot see. Matches are counted
non-overlapping and non-consuming, so a nested `window.imp().sidebar.imp().sections`
counts as two sites (`sidebar` on the window imp, `sections` on the sidebar imp)
rather than one.

A site is **row-owned** when the identifier it reaches is a field of
`LushtextSidebar`'s imp (`ui/sidebar/imp.rs:38`) or `LushtextWorkspaceSection`'s
imp (`ui/sidebar/workspace_section/imp.rs:218`). Everything else in the same file
— `window.imp().tab_view`, `window.imp().sidebar` as an access path, the notes and
drafts fields — belongs to another row and is excluded, not merely deprioritised.

## The archive's figures were an undercount; this change does not inherit them

Task 0.7 carries forward slot 5a's **190 ungated `.imp().` sites**, of which
**113 of 158** were called `TemplateChild` handles and **45** in-scope tree-side
runtime reads. Re-derived against the current tree, none of the three numbers
holds, and the reasons are separable:

| 5a figure | Re-derived | Why it differs |
| --- | --- | --- |
| `workspace_section.rs` = 158 sites | **229** | A same-line-only grep of that file returns **161** today and returned 158 at 5a's own commit; the normalized count at that same commit (`ceaa58b^`) was **227**. The 158 was a per-line grep that missed ~69 rustfmt-wrapped multi-line chains. Growth since is real but small: 227 → 229 across the archived change itself. |
| `sidebar.rs` = 32 sites | **38** (23 row-owned) | Same cause: same-line-only grep returns 36 today. 15 of the 38 are `window.imp().sidebar` / `window.imp().tab_view` — window template children, not this row's. |
| `window.rs` tree-side | **not counted at all** | 5a scoped `window.rs` to its *notes* subset (`startup_data_flow`, `notes_menu_button`). The **43** window-side sites reaching sidebar/section imp fields were never in the 190. |
| in-scope runtime reads = 45 | **79** | 62 in `workspace_section.rs` + 2 in `sidebar.rs` + 15 in `window.rs`. The 45 was 5a's undercounted `workspace_section.rs` slice only. |

A second correction, of *classification* rather than arithmetic: 5a's
`TemplateChild` bucket pooled `peek_widgets` (2), `context_menu` (3) and
`drilldown_back_button` (1) together with real template children.
`peek_widgets`, `context_menu`, `context_menu_box`, `header_context_menu` and
`header_context_menu_box` are **not** `#[template_child]` — they are
`RefCell<Option<gtk4::Widget…>>` slots populated at first use
(`workspace_section/imp.rs:61-82`, `:270-279`). They are still widget handles and
still out of scope, but for a slightly different reason, so they are counted as
their own bucket below rather than silently folded in. A later slot reading "113
TemplateChild" would otherwise look for template children that do not exist.

## Row-owned population: 295 sites in three buckets

| Bucket | Sites | Disposition |
| --- | --- | --- |
| A. `TemplateChild` widget handles | **179** | Out of scope — widget access, not workflow state |
| B. Lazily-populated runtime widget handles | **37** | Out of scope — same reason, different mechanism |
| C. **In-scope runtime state** | **79** | Migrates to `WorkspaceTreeEvidence` or to a real drive |
| **Total row-owned** | **295** | |

Per file: `workspace_section.rs` 229 row-owned of 229 total; `sidebar.rs` 23 of
38; `window.rs` 43 of 662; `file_tree_item.rs` **0** of 0; `accessibility.rs`
**0** of 0. The last two are named explicitly because task 0.7 lists them as
files of interest and a reader must not take the silence for "not checked":
neither file contains a single `.imp()` occurrence.

# BEFORE — per file, per field

## `crates/lushtext/tests/widget/workspace_section.rs` — 229 sites, 26 fields

Owning struct for every row: `LushtextWorkspaceSection` imp,
`crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs`.

### Bucket A — `TemplateChild` (137 sites, 9 fields)

| Field | imp.rs | Sites | Kind |
| --- | --- | --- | --- |
| `file_tree_view` | `:248` | 39 | R + widget actuation (`grab_focus`, `scroll_to`, `emit_by_name`) |
| `refresh_button` | `:233` | 36 | R + `emit_clicked()` ×17 (a real drive) |
| `inner_scrolled_window` | `:245` | 17 | R, plus **2 property writes** (see writes list) |
| `add_folder_button` | `:230` | 14 | R + `emit_clicked()`, `grab_focus()` |
| `collapse_button` | `:224` | 10 | R |
| `empty_folder_set_label` | `:251` | 10 | R |
| `header_box` | `:221` | 9 | R + `observe_controllers()` |
| `header_label` | `:227` | 1 | R (`ellipsize()`) |
| `drilldown_back_button` | `:239` | 1 | R (`icon_name()`) |

### Bucket B — lazily-populated widget handles (30 sites, 5 fields)

| Field | imp.rs | Sites | Kind |
| --- | --- | --- | --- |
| `peek_widgets.*` | `:343` (`PeekWidgets`, `:61`) | 14 | R — `open_button` 6, `popover` 3, `text_buffer`/`text_view`/`body_stack`/`fallback_title_label`/`fallback_body_label` 1 each |
| `context_menu` | `:270` | 9 | R |
| `header_context_menu` | `:277` | 5 | R |
| `context_menu_box` | `:272` | 1 | R |
| `header_context_menu_box` | `:279` | 1 | R |

### Bucket C — in-scope runtime state (62 sites, 12 fields)

| Field | imp.rs | R | W | Representative sites |
| --- | --- | --- | --- | --- |
| `top_level_store` | `:289` | 12 | **4** | R `:3342`, `:4417`, `:4719`, `:4768-4835`, `:4994`, `:5020`, `:5649`, `:5671`; W `:2962`, `:2983`, `:2999`, `:3050` |
| `tree_model` | `:287` | 12 | **1** | R `:519`, `:545`, `:565`, `:578`, `:591`, `:5484`, `:5556`, `:5590`, `:5605`, `:5614`, `:5656`, `:5682`; W `:3022` |
| `watch_runtime.watcher` | `:177` (in `WatchRuntimeState`, `:175`) | 8 | 0 | `:3909`, `:4012`, `:4053`, `:4462`, `:4507`, `:4583`, `:4843`, `:5029` |
| `watch_runtime.poll_source_id` | `:191` | 5 | 0 | `:4054`, `:4463`, `:4584`, `:4844`, `:5030` |
| `drilldown_stack` | `:257` | 5 | 0 | `:377`, `:455`, `:1953`, `:2648`, `:2890` |
| `is_new_item` | `:284` | 0 | **4** | `:2833`, `:2849`, `:3708`, `:3753` |
| `original_folders` | `:259` | 2 | **1** | R `:1870`, `:2892`; W `:3372` |
| `workspace_folder_ids` | `:261` | 2 | 0 | `:1957`, `:2897` |
| `refresh_runtime.pending_full_reload` | `:105` (in `RefreshRuntimeState`, `:99`) | 1 | 0 | `:5010` |
| `refresh_runtime.pending_paths` | `:103` | 1 | 0 | `:5015` |
| `dir_stores` | `:293` | 0 | **1** | `:3055` |
| `rename_callback` | `:352` | 1 | 0 | `:3085` — borrows the slot and **invokes** the stored callback |
| `delete_callback` | `:354` | 1 | 0 | `:3112` — same shape |
| `create_callback` | `:356` | 1 | 0 | `:3135` — same shape |

The three callback rows are reads of the slot but *actuations* of the workflow:
the test calls `cb(path)` directly instead of driving the rename/delete/create
path that installs it. They are counted as reads because no `imp` state is
mutated by the test, and flagged here so the migration does not treat them as
ordinary observations.

## `crates/lushtext/tests/widget/sidebar.rs` — 38 sites, 23 row-owned

Owning struct: `LushtextSidebar` imp, `crates/lushtext-core/src/ui/sidebar/imp.rs`.

### Bucket A — `TemplateChild` (21 sites)

| Field | imp.rs | Sites | Kind |
| --- | --- | --- | --- |
| `new_workspace_button` | `:57` | 7 | R (`icon_name`, `valign`, margins, `parent()`) |
| `workspace_filter_dropdown` | `:50` | 4 | R (`model()`, `selected()`) + `set_selected()` real drive |
| `workspace_list_revealer` | `:53` | 4 | R (`transition_type`, `transition_duration`, `reveals_child`, `as_ptr`) |
| `new_workspace_box` | `:47` | 3 | R (`height()` ×2, `margin_start()`) |
| `outer_scrolled_window` | `:41` | 3 | R (`as_ptr`, `hscrollbar_policy`, `propagates_natural_width`) |

### Bucket C — in-scope runtime state (2 sites)

| Field | imp.rs | R | W | Sites |
| --- | --- | --- | --- | --- |
| `sections` | `:63` | 2 | 0 | `:54` (`is_empty()` on a fresh sidebar), `:127` (`len() == 3` after restore) |

### Not row-owned (15 sites)

`window.imp().sidebar` 9 and `window.imp().tab_view` 6 are `LushtextWindow`
template children. `sidebar` is only the *access path* to this row; `tab_view`
belongs to `WFR-SHELL-LAYOUT` (slot 7).

## `crates/lushtext/tests/widget/window.rs` — 662 sites, 43 row-owned

The other 619 belong to other rows (`tab_view` 105, `sidebar`-as-path 69,
`notification_bus` 39, `drafts` 35, `status_bar` 34, `markdown_preview` 28,
`startup_data_flow` 18, `settings` 18, `preview_mode` 18, `properties_panel` 17,
and a long tail). None is touched here.

### Bucket A — `TemplateChild` (21 sites)

| Field | Owner | Sites | Notes |
| --- | --- | --- | --- |
| `empty_folder_set_label` | section imp `:251` | 6 | `:10383`, `:10414`, `:10519`, `:10607`, `:11809`, `:11847` |
| `file_tree_view` | section imp `:248` | 5 | `:3113` (`scroll_to`), `:3206` (`model()`), `:3228`, `:3318`, `:3361` |
| `workspace_filter_dropdown` | sidebar imp `:50` | 5 | `:10151`, `:10519`, `:10750`, `:10783`, `:11410` — all `set_selected()`, a real drive |
| `header_label` | section imp `:227` | 3 | `:10378`, `:10406`, `:11843` |
| `inner_scrolled_window` | section imp `:245` | 1 | `:10416` |
| `add_folder_button` | section imp `:230` | 1 | `:10423` |

### Bucket B — lazily-populated widget handles (7 sites)

`peek_widgets.open_button` 2 (`:3326`, `:3372`), `context_menu` 2 (`:15832`,
`:15846`), `context_menu_box` 1 (`:15837`), `header_context_menu` 1 (`:15849`),
`header_context_menu_box` 1 (`:15852`).

### Bucket C — in-scope runtime state (15 sites, all reads)

| Field | Owner | Sites | Lines |
| --- | --- | --- | --- |
| `sections` | sidebar imp `:63` | 10 | `:546`, `:3137`, `:3207`, `:3251`, `:10171`, `:11404`, `:11450`, `:11464`, `:11518`, `:15391` |
| `persistence` | sidebar imp `:106` | 3 | `:10612`, `:10678` (`has_pending_work()`), `:10717` (`in_flight_generation()`) |
| `tree_model` | section imp `:287` | 1 | `:3097` |
| `top_level_store` | section imp `:289` | 1 | `:10082` |

`window.imp().sidebar.current_scope()`, `.workspaces_file()`,
`.all_workspace_folder_paths()` and the `*_for_test` calls are **facade method
calls**, not imp reach-through, and are excluded from all three buckets.

# Out-of-scope record, so a later slot does not read this as an oversight

**Bucket A, 179 `TemplateChild` sites.** A test that calls `emit_clicked()`,
reads `is_visible()`, or asserts `margin_top()` is reaching for a *widget*, and
an evidence surface is the wrong home for a widget handle: the surface reports
workflow state, and duplicating GTK property getters into it would make the
surface grow with every template change while proving nothing about the workflow.
Two sub-populations inside A are worth naming because they look like defects and
are not:

- `refresh_button.emit_clicked()` (17 sites) and
  `workspace_filter_dropdown.set_selected(n)` (6 sites across `window.rs` and
  `sidebar.rs`) are **real drives** through the production signal path
  (`connect_selected_notify`, `imp.rs:174`), not state pokes. They stay.
- `file_tree_view.grab_focus()` / `scroll_to()` and
  `header_box.observe_controllers()` are widget actuation and introspection that
  no evidence field can replace.

**Bucket B, 37 lazily-populated widget-handle sites.** Same conclusion, different
mechanism: these are `RefCell<Option<Widget>>` slots, not template children, so
`try_get()` does not apply and there is nothing for the disposed-widget rule to
say. A test asserting the peek popover's `open_button` sensitivity or the context
menu's action rows is asserting rendered chrome. Where the *presence* of a menu
or popover is a workflow fact rather than a widget fact — for example "a
keyboard-opened context menu exists for the focused row" — the migration plan
below routes it through an evidence field and leaves the widget assertions where
they are.

# Every ungated `.imp()` WRITE — 13 sites

Per slot 3a's finding, an ungated `imp()` write is usually a real drive in
disguise. Each row below names the **existing** configuration seam that can set
the same precondition, so no new counted actuation seam is needed. The seam
inventory re-derived for this purpose is **59** `*_for_test` function definitions
under `crates/lushtext-core/src/ui/sidebar/` (task 0.7 says 60; 59 is the
distinct-name count in the current tree).

| # | Site | Write | Existing seam + real drive to use instead |
| --- | --- | --- | --- |
| 1 | `workspace_section.rs:2833` | `is_new_item.set(true)` | `set_context_target_for_test(path, is_dir, id)` then `activate_action("section.new-file")` — production sets the flag at `workspace_section/actions.rs:76`. The test already calls `prepare_context_menu_for_path` on the line above, so the precondition is one action away. |
| 2 | `workspace_section.rs:2849` | `is_new_item.set(false)` | No seam needed: production clears it at `actions.rs:342`/`:367`/`:378`/`:399`. This is a teardown poke that the real commit/cancel path performs. |
| 3 | `workspace_section.rs:3708` | `is_new_item.set(true)` | As #1. This is the placeholder-cleanup data-safety test, which already drives the real cancel path (`entry.emit_by_name("activate")`) — only the *entry* into new-item mode is faked. |
| 4 | `workspace_section.rs:3753` | `is_new_item.set(false)` | As #2. |
| 5 | `workspace_section.rs:2962` | `*top_level_store.borrow_mut() = Some(store)` | `load_folders(&[FolderTreeEntry::File{..}])` (production loader) — installs a real top-level store. Test is `test_remove_from_model_*`. |
| 6 | `workspace_section.rs:2983` | same | as #5 |
| 7 | `workspace_section.rs:2999` | same | as #5 |
| 8 | `workspace_section.rs:3050` | same | as #5 |
| 9 | `workspace_section.rs:3022` | `*tree_model.borrow_mut() = Some(model)` | `load_folders(&[Directory{..}])` + `expand_folders()` installs the real `GtkTreeListModel` including the production create-closure (`folders.rs:96-107`). The hand-built closure in the test bypasses `build_children_model` entirely. |
| 10 | `workspace_section.rs:3055` | `dir_stores.borrow_mut().insert(parent, weak)` | `expand_folders()` + `wait_until(tree_contains_path(..))`; production populates `dir_stores` through `find_store_for_dir` (`tree_index.rs:404-407`). |
| 11 | `workspace_section.rs:3372` | `*original_folders.borrow_mut() = vec![..]` | Re-call `load_workspace_folders(&[second, first, third])` in the new order, then `refresh_button.emit_clicked()`. The test's intent is "the configured order changed under a refresh", which the production loader expresses directly. |
| 12 | `workspace_section.rs:5715` | `inner_scrolled_window.set_propagate_natural_height(false)` | Bucket A (widget property), not workflow state. Deliberate and commented: it bounds the fixture's rendered height. Candidate for a fixture helper in `tests/widget/common.rs`, **not** for a production seam. |
| 13 | `workspace_section.rs:5719` | `inner_scrolled_window.set_max_content_height(400)` | as #12 |

**No writes exist in `sidebar.rs` or in `window.rs`'s row-owned population.** All
25 of those sites are reads or facade calls. The five `set_selected()` and one
`dropdown.set_selected()` sites are TemplateChild actuation through a connected
production signal, which is a drive rather than a write.

Eleven of the thirteen writes touch in-scope runtime state (#1–#11); two are
widget properties (#12–#13). So of the 79 in-scope sites, **68 are reads that
become evidence reads** and **11 are writes that become real drives**.

# Migration plan

Target names are proposals against the `WorkspaceTreeEvidence` surface specified
in `evidence/evidence-surface-materialization.md`; each is derived from
authoritative workflow state and **must not** reach `children()`,
`find_store_for_dir`, `find_dir_row`, `visible_child_stores`,
`expanded_store_index`, or `derive_expanded_paths_from_model` (the six hazards).

| Field reached today | Target | Sites |
| --- | --- | --- |
| `top_level_store` (read) | `evidence.tree.top_level_store_installed: bool` + `tree.top_level_row_count: usize` | 13 (12 ws + 1 window) |
| `top_level_store` (write) | real drive: `load_folders` / `load_workspace_folders` | 4 |
| `tree_model` (read) | `evidence.tree.tree_model_installed: bool`, `tree.flattened_row_count: usize`; the two `as_ptr()` identity comparisons (`:5656`, `:5682`) become `tree.tree_model_identity_generation: u64` | 13 (12 ws + 1 window) |
| `tree_model` (write) | real drive: `load_folders` + `expand_folders` | 1 |
| `watch_runtime.watcher` | `evidence.watch.watcher_installed: bool` + `watch.watched_target_count: usize` (covers `:4012`'s `watched_target_count`) | 8 |
| `watch_runtime.poll_source_id` | `evidence.watch.poll_source_active: bool` | 5 |
| `drilldown_stack` | `evidence.navigation.drilldown_depth: usize` + `navigation.drilldown_leaf: Option<PathBuf>` (covers `.last()` and the `vec![nested]` equality asserts) | 5 |
| `is_new_item` | **no evidence field.** Real drive via existing `set_context_target_for_test` + `section.new-file` / production commit-cancel | 4 |
| `original_folders` (read) | `evidence.navigation.configured_folder_paths: Vec<PathBuf>` | 2 |
| `original_folders` (write) | real drive: `load_workspace_folders` in the new order | 1 |
| `workspace_folder_ids` | `evidence.navigation.configured_folder_ids: Vec<(PathBuf, WorkspaceFolderId)>` | 2 |
| `refresh_runtime.pending_full_reload` | `evidence.refresh.pending_full_reload: bool` | 1 |
| `refresh_runtime.pending_paths` | `evidence.refresh.pending_path_count: usize` | 1 |
| `dir_stores` (write) | real drive: `expand_folders` | 1 |
| `rename_callback` / `delete_callback` / `create_callback` | `evidence.callbacks.file_callbacks_installed: {rename, delete, create}` for the wiring assertion; the *forwarding* assertion becomes a real drive through the inline-rename / delete / create paths | 3 |
| `sections` (sidebar) | `evidence.sections.count: usize` + `sections.visible_count: usize` + `sections.workspace_ids: Vec<WorkspaceId>` | 12 (2 sidebar + 10 window) |
| `persistence` | `evidence.persistence.has_pending_work: bool` + `persistence.in_flight_generation: Option<u64>` | 3 |
| **In-scope subtotal** | | **79** |
| Bucket A — `TemplateChild` handles | stays as `TemplateChild` access | 179 |
| Bucket B — lazily-populated widget handles | stays as widget access | 37 |
| `inner_scrolled_window` fixture writes (#12–#13) | fixture helper in `tests/widget/common.rs`, no production seam | 2 (already inside the 179) |
| **Row-owned total** | | **295** |

Distinct in-scope field names, for the record: `top_level_store`, `tree_model`,
`watch_runtime.watcher`, `watch_runtime.poll_source_id`, `drilldown_stack`,
`is_new_item`, `original_folders`, `workspace_folder_ids`,
`refresh_runtime.pending_full_reload`, `refresh_runtime.pending_paths`,
`dir_stores`, `rename_callback`, `delete_callback`, `create_callback`, `sections`,
`persistence` — **16** field paths across **14** declared struct fields (the two
`watch_runtime` sub-fields and the two `refresh_runtime` sub-fields each share one
declared field).

## Not in this catalogue

`workspace_filter_animation_active` has **zero** widget-test sites. Its two
reach-throughs were *production* (`ui/automation.rs`'s readiness blocker and its
workspace snapshot) and were task **6.7**'s, not 0.7's. **Both are now retired**: the
blocker reads a facade accessor and the snapshot projects from
`ui/sidebar/evidence.rs`. Recorded here only so a reader comparing the field lists does
not conclude the field was missed.

---

## Reconciliation: the seam inventory is 60, not 59

This file's own pass reported **59** distinct `*_for_test` definitions under
`ui/sidebar/`. Re-checked against the census:

```
grep -rhoE 'fn [a-z_0-9]+_for_test[a-z_0-9]*' crates/lushtext-core/src/ui/sidebar/
  → 60 occurrences, 60 distinct names, 0 duplicates
```

**60 is correct**, matching the matrix cell byte-exactly and matching
`evidence/census-reverification.md` §2. The 59 was an undercount, most likely from a
rustfmt-wrapped multi-line signature — the same failure mode this file documents as
cause (1) for the archive's own wrong 158. Recorded rather than silently corrected,
because a seam census that drifts by one in the *measuring* pass is evidence that the
grep must be occurrence-based rather than line-based.
