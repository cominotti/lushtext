# Call-Site Audit (Task 3.6)

Audited every `set_expanded(true)` call site and every caller of `find_store_for_dir`, `dir_stores`, and `child_store_paths` under `crates/lushtext-core/src/ui/sidebar/` for correctness under deferred child-store materialization.

## `set_expanded(true)` Call Sites

1. **`ui/sidebar/reveal_execution.rs:344` (`advance_expansion`)**
   - Calls `row.set_expanded(true)` and reads `row.is_expanded()` synchronously (ledger A22) to tell non-empty/unknown directories from known-empty/non-directory rows, returning `RevealStep::Wait`.
   - On the next poll, `advance_expansion` checks `section.child_scans_pending()` (`scan_execution::child_scan_blocks_readiness`), which now returns `true` via `materialization_execution::has_live_pending_materializations(section)` while the installed store waits in `pending_materializations`.
   - **Verdict:** Safe; reveal cannot classify an expanded-but-unscanned directory as `NotFound`.

2. **`ui/sidebar/workspace_section/folder_execution.rs:399` (`expand_folders`), `:409` (`toggle_folders`), `:636` (`restore_folder_model_state`), and `focus_folder` (`:286` → `load_folder_model(auto_expand = true)`)**
   - Calls `row.set_expanded(true)` on top-level folder rows. GTK installs the child `gio::ListStore` synchronously into `node->model`, `build_children_model` enqueues it, and the `Priority::HIGH` drain runs `populate_child_store` in the same main-loop iteration before default-priority sources and before the next frame.
   - `has_live_pending_materializations` keeps `child_scan_blocks_readiness` true during the interval before the drain runs.
   - **Verdict:** Safe.

3. **`ui/sidebar/workspace_section/scan_execution.rs:1677` (`schedule_child_state_restore`)**
   - Runs from a 1 ms `timeout_add_local_once`, reads `expanded_paths` at apply time, and calls `row.set_expanded(true)` on matching directory rows.
   - **Verdict:** Safe; each expanded store stays alive and is materialized by the `Priority::HIGH` drain.

4. **`ui/sidebar/workspace_section/file_execution.rs:120` (`create_new_item`)**
   - Expands a collapsed target directory via `row.set_expanded(true)` and schedules `glib::idle_add_local_once` (`Priority::DEFAULT_IDLE`) to look up `section.find_store_for_dir(&target_dir)` and call `append_item_preserving_placeholder`.
   - Because `materialization_execution` drains at `Priority::HIGH`, it runs before `DEFAULT_IDLE`, registers `dir_stores[target_dir]`, and submits the scan. Even if a caller reached `find_store_for_dir` before the drain, `find_store_for_dir` falls back to `row.children()`, returns the installed store, and leaves `child_store_refs` untouched so the drain still materializes it once.
   - **Verdict:** Safe.

## `find_store_for_dir`, `dir_stores`, and `child_store_paths` Call Sites

1. **`ui/sidebar/workspace_section/file_execution.rs:133, 142` (`create_new_item`)**
   - Calls `section.find_store_for_dir(&target_dir)`. Uses `dir_stores` or falls back to `find_dir_row(&target_dir)?.children()`, returning only an installed store.
   - **Verdict:** Safe.

2. **`ui/sidebar/workspace_section/file_execution.rs:367, 393` (`confirm_rename` on a directory) and `scan_execution.rs:2204` (`remove_from_model`)**
   - Call `scan_execution::clear_dir_state(section, &old_path)` → `clear_dir_states`, which now calls `materialization_execution::retire_materializations_under_roots(section, roots)`. Any queued entry under the renamed/removed directory is marked `retired = true` and dropped via `MaterializationDisposition::Retired`.
   - **Verdict:** Safe; stale pre-rename/delete entries cannot populate a store under the old path.

3. **`ui/sidebar/workspace_section/scan_execution.rs:2121` (`rename_cached_item`) and `:2235` (`remove_from_model`)**
   - Call `self.find_store_for_dir(parent_dir)`, which returns the live installed store from `dir_stores` or falls back to `row.children()`.
   - **Verdict:** Safe.

4. **`ui/sidebar/workspace_section/refresh_execution.rs:287, 373` (`expanded_store_index`)**
   - Walks `tree_model` for rows where `row.is_expanded()` is true, reads `row.children()`, and submits `populate_child_store`.
   - If a refresh reaches a newly expanded directory in the same main-loop turn before the `Priority::HIGH` drain runs, `populate_child_store` registers the store in `child_store_refs`; when the drain runs afterwards, `child_store_identity_matches` is `true` and `materialization_disposition` returns `MaterializationDisposition::AlreadyMaterialized`, preventing a double scan.
   - **Verdict:** Safe.

5. **`ui/sidebar/workspace_section/reorder_execution.rs:770` (`empty_children_model_for_drag_hover`)**
   - Checked first in `build_children_model` before `enqueue_child_store_materialization`. Drag hover still creates no pending materialization entry and no scan.
   - **Verdict:** Safe.

6. **`ui/sidebar/workspace_section/scan_execution.rs:1818` (`retain_shown_child_store`)**
   - Called only from `record_row_expansion_transition` when `row.is_expanded()` is true; reads `row.children()` and keeps a strong clone in `retained_child_stores` for slice-based collapse retirement.
   - **Verdict:** Safe.

7. **`ui/sidebar/workspace_section/scan_execution.rs` (`clear_dir_states`, `clear_all_dir_state`, `retire_child_store`, `start_child_scan`, `child_scan_is_active`, `ensure_child_store_identity`, `splice_child_store`, `child_store_is_live`) & `ui/sidebar/evidence.rs`**
   - Only read or retire registered installed stores; never trigger bind-time child scans. `evidence.rs` reads `dir_stores` and `child_store_paths` via `RefCell` only.
   - **Verdict:** Safe.
