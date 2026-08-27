# Widget-test reach-through migration (tasks 0.7, 4.8, 6.3)

## The population, re-derived and scoped

Authoring counted 190 ungated `.imp().` sites across this slot's widget tests.
Re-derived, the counts match exactly:

| File | Sites | Distinct fields |
| --- | --- | --- |
| `crates/lushtext/tests/widget/sidebar.rs` | 32 | 8 |
| `crates/lushtext/tests/widget/workspace_section.rs` | **158** | **23** |
| `crates/lushtext/tests/widget/file_tree_item.rs` | **0** | 0 |
| `crates/lushtext/tests/widget/window.rs` (notes-side subset) | 21 `startup_data_flow` + 6 `notes_menu_button` | 2 |

**Not all of it is in scope, and saying which is part of the task.** The
convention's target is *state observation*, not widget access.

Out of scope in `workspace_section.rs` — **113 of the 158** are `TemplateChild`
widget handles: `refresh_button` 35, `file_tree_view` 30, `add_folder_button` 12,
`collapse_button` 10, `inner_scrolled_window` 8, `header_box` 8,
`empty_folder_set_label` 3, `context_menu` 3, `peek_widgets` 2, `header_label` 1,
`drilldown_back_button` 1. A test that clicks a button or scrolls a list is
reaching for a *widget*, and an evidence surface is the wrong home for a widget
handle. Recorded so a later slot does not read the omission as an oversight.

In scope in `workspace_section.rs` — the remaining **45** private runtime reads:
`watch_runtime.watcher` / `.poll_source_id` 12, `top_level_store` 10,
`tree_model` 8, `drilldown_stack` 5, `refresh_runtime.pending_full_reload` /
`.pending_paths` 2, `original_folders` 2, `is_new_item` 2,
`workspace_folder_ids` 1, and the three callback slots. In `sidebar.rs` the
in-scope population is **2** (`sections`, the per-workspace child collection).

## What this change migrated

| Population | Before | After | Disposition |
| --- | --- | --- | --- |
| Notes-side typed inspection seams (`window.rs`) | 4 functions across 4 call surfaces | **0** | Retired into `NotesEvidence`; the two parameterized tuple-returning seams became **one named operation** returning `OpenEditorNoteCaptureEvidence` |
| `notes_browser_runtime_snapshot_for_test()` call sites | 20 | 0 | now `notes_evidence().browser` |
| `note_save_snapshot_count_for_test()` call sites | 1 | 0 | now `notes_evidence().active_note_save_captures` — and the retired getter **pruned** as a side effect of being read, which the surface does not |
| `open_editor_note_snapshot_*_for_test()` call sites | 2 (two different functions, tuple returns) | 0 | now `capture_open_editor_note_evidence*` with named fields |
| `ui/automation.rs` production `.imp()` reach-through owned by this slot's rows | 2 | **2, unchanged** | See below |
| Tree-side 45 in-scope runtime reads | 45 | **45, unchanged** | Moves to slot 5b with the row |
| `window.rs` `startup_data_flow.completed` reads | 21 | **21, unchanged** | See below |

## Two populations deliberately left, with the reason

**The sidebar's two `ui/automation.rs` reads** (`:766` readiness blocker, `:927`
workspace snapshot, both reading
`imp.sidebar.imp().workspace_filter_animation_active`) are
`WFR-WORKSPACE-TREE`'s. Task 6.1 requires them to become a named accessor or an
evidence projection *on that row's surface* — and that surface is slot 5b's. Adding
an accessor now, without the row's evidence surface to project from, would be the
"fixing one from outside is how a row acquires a change nobody planned" hazard task
6.1 records for the other six. **Left unchanged and handed to 5b.**

**The 21 `startup_data_flow.completed` reads** are not this row's either: task 2.2
decided `ui/window/startup_data.rs` is **cross-cutting, owned by neither row**, and
a cross-cutting module owns no evidence surface. The planned migration — one named
window operation `startup_data_flow_completed()` — is recorded in
`shared-ownership-decisions.md` §2.2 and belongs with whoever migrates that
module (slot 7). **Left unchanged**, because moving them to a *notes* accessor
would encode the ownership answer task 2.2 explicitly rejected.

## The six out-of-scope production reach-throughs, with owners (task 6.1)

`ui/automation.rs` holds exactly **8** `.imp().` sites; two are this slot's rows'
(above) and six belong elsewhere:

| Site | Reads | Owning row |
| --- | --- | --- |
| `:518`, `:519` | `window.imp().tab_view` | `WFR-SHELL-LAYOUT` / the tab workflow, slot 7 |
| `:1137` | `editor.imp().scrolled_window` | `WFR-MINIMAP`, slot 6 |
| `:1144` | `editor.imp().minimap_overlay` | `WFR-MINIMAP`, slot 6 |
| `:1162` | `editor.imp().minimap.source_map` | `WFR-MINIMAP`, slot 6 |
| `:1224` | `editor.imp().minimap.marker_strip` | `WFR-MINIMAP`, slot 6 |

**None was touched.** Each is a projection decision for the row that owns it.

A sweep for the same shape across `ui/` found no other cross-workflow-boundary
reach: every other `\.imp\(\)\.` hit is a widget reaching into its own module
family's imp (`editor_page/minimap.rs` → `editor_page` imp, and so on), which is
ordinary private access rather than a boundary crossing.

## Harness configuration (task 6.2)

The widget harness's notes-side configuration setters are unchanged in **name and
timing**: `set_notes_browser_source_entry_limit_for_test` and
`set_bookmark_excerpt_preview_delay_for_test` now route through
`notes/test_policy.rs` instead of two separate module statics, and
`set_note_source_delay_for_test` / `set_notes_browser_query_delay_for_test` still
come from `services/palette/notes.rs`, which owns the behavior they change and
shares it with the migrated palette row. No wait helper was added, copied, or
changed: `wait_until`, `flush_events`, `flush_after_delay`, and `present_window`
are still imported from `crates/lushtext/tests/widget/common.rs`.

## Seams added

**Two**, both on the tree row, both counted and justified individually in
`ui/sidebar/test_policy.rs`:
`set_workspace_rename_worker_delay_for_test` and
`set_workspace_placeholder_cleanup_delay_for_test`. Without them, two
data-safety regression tests for confirmed data-destruction defects **passed
against the broken code as well as the fixed code** — the fixed and unfixed
worker both won the race a headless test could set up. A test that cannot fail is
not coverage. Both are entirely behind `#[cfg(feature = "test-utils")]`, and both
are documented in the module with why they were necessary.

**Zero** seams added on the notes row.
