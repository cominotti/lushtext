## Why

`add-sidebar-visual-proof-scenario` pinned GTK axiom A22, and its side clause records that `TreeListRow::is_expandable()` runs the `GtkTreeListModel` create function and throws the model away. LushText's create function, `scan_execution::build_children_model` (reached from the closure in `ui/sidebar/workspace_section/folder_execution.rs`), does real work on every call. It registers the new store in `child_store_paths`, `child_store_refs`, `dir_stores` and `child_scan_flights`, and it submits a directory scan through the process-wide scan admission.

Reading the GTK 4.22.5 source shows the cost is larger than "once per bound row":

- `gtk_tree_expander_update_for_list_row` (`gtk/gtktreeexpander.c:189`) evaluates `gtk_tree_list_row_is_expandable (self->list_row) && !self->hide_expander`. `is_expandable` comes first, so `hide-expander` does not prevent the call. The update runs on every `set_list_row` (`:904`), so every bind and every recycled rebind while the sidebar scrolls triggers it. It also runs on every row notify other than `expanded` and `item` (`:309`), including the `children` notify that `set_expanded` emits.
- `gtk_tree_list_row_is_expandable` (`gtk/gtktreelistmodel.c:1255-1283`) caches only a negative answer (`node->empty`). A positive answer is never cached. Every call on a collapsed, non-empty node runs the create function again and unrefs the result.
- Collapsing clears `node->model` (`gtk_tree_list_model_clear_node_children`, `:399-410`). The `children` notify that follows makes the bound expander probe again, so collapsing a directory starts a scan of the directory the user just closed.

So when the sidebar binds, scrolls or collapses a directory row, LushText likely starts a scan of a directory nobody opened. That is filesystem I/O, a scan admission permit and a `spawn_blocking_then` slot taken away from real expansions and from work that protects the user's data. When admission is free, the scan's completion state holds a strong reference to the discarded store. That keeps an orphan store alive and listed in `dir_stores` until the scan finishes, so `find_store_for_dir` can return a store no row shows. The responsiveness requirement is absolute: LushText stays instantly responsive with gigantic inputs, and a workspace folder with thousands of subdirectories is one. This change verifies the defect, measures it before fixing anything, and fixes it.

This change is not part of the long-line stream. It is the formal-verification / GTK-axiom follow-up the maintainer approved.

## What Changes

- **Failing-first measurement.** Before any fix, a widget test counts child-model creations, scans submitted and scans started over a synthetic workspace of collapsed, non-empty sibling directories, across five steps: first bind, a scroll that recycles rows, scrolling back, collapsing an expanded directory, and expanding one. The test asserts the target behaviour (zero scans for directories nobody expanded) and is recorded failing on the current tree with the measured numbers. A second measurement checks for the orphan-store hazard: a `dir_stores` entry or a live child store for a path that was never expanded. New counters are added to the workspace section's evidence surface so the tests read them there, not through new `*_for_test` getters.
- **New GTK axiom (provisionally T1; the final id is the next free one when this change is applied, A23 if it lands before `probe-long-line-toolkit-costs`).** The ledger gains the behaviour this change depends on: `GtkTreeExpander` asks `is_expandable()` synchronously from `set_list_row` and from row notifies other than `expanded`/`item`, whatever `hide-expander` says. A positive answer is not cached. A model the create function returned to `is_expandable()` is finalized before the call returns when nothing else holds it. A model installed by `set_expanded(true)` stays alive, and collapsing finalizes it and leads to a fresh probe. The new axiom gets a `gtk-lush-axioms` probe, a catalogue entry and an interactive/`--check` sample. A22's "recorded beside it" clause cross-references the new id.
- **The fix: deferred materialization.** The create function stops doing I/O or registration. It returns an empty `gio::ListStore` and records only a weak handle plus the path and section lifetime in a per-section pending-materialization queue, then arms one bounded high-priority drain. The drain runs before the next frame and before any default-priority source. It drops entries whose store is gone (a discarded `is_expandable()` probe). For live stores, which only an installing `set_expanded(true)` can keep alive, it runs the existing `populate_child_store` path unchanged: registration, active-plus-latest flight, admission, generation-guarded reconciliation. The decision for each entry is a pure, exhaustively tested function in `ui/sidebar/policy.rs`.
- **Readiness stays honest.** A pending materialization of a store GTK installed (one that is still alive) counts as a pending child scan in `child_scan_blocks_readiness`, so `workspace-refresh-complete`, `child_scans_pending()` and the workspace path reveal's scan wait never mistake "expanded, not yet scanned" for "scanned and empty". Discarded probes never count, so scrolling no longer toggles the file tree's accessible `busy` state, which today flips with every probe scan.
- **No regressions.** The parent scan's existing empty-folder lookahead stays the cheap expandability hint (`is_empty == Some(true)` returns `None`, so the row is marked empty and probed no more). The `(Empty)` label, Focus Folder enablement, the expander affordance, auto-refresh watch targets, the attention-moment emptiness revalidation, the reorder drag-hover inert model, the reveal, and the `ViewportSliceBin` rendered-row contracts keep their existing tests green. Each one also gets a new assertion where the fix touches it.
- **Docs.** The ledger row, `docs/next/formal-verification.md`, the `WFR-WORKSPACE-TREE` matrix row (re-derived cells, new evidence fields), `.agents/rules/ui.md` (File Tree), the sidebar `AGENTS.md` hazard list, `AGENTS.md`'s design decision for the multi-workspace sidebar, and the accessibility docs where they describe when the file tree reports `busy`.

No persisted format, GSettings key, action, D-Bus member, automation snapshot field or readiness predicate changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-tree-refresh`: adds a requirement that binding, recycling or collapsing a directory row starts no directory I/O and registers no child store. A child scan starts only for a directory whose child model GTK actually installed by expanding it, and readiness covers the time between installation and scan start.
- `gtk-axiom-ledger`: adds a requirement that the tree-expander expandability-query behaviours the sidebar relies on are ledger axioms, each pinned by an isolated `gtk-lush-axioms` probe with a sample.

## Impact

- **Code:** `crates/lushtext-core/src/ui/sidebar/workspace_section/scan_execution.rs` (create function, new drain, readiness), `workspace_section/folder_execution.rs` (create closure), `workspace_section/imp.rs` (pending queue state), `workspace_section/mod.rs` (`WorkspaceScanPressureEvidence` fields), `ui/sidebar/policy.rs` (materialization disposition), `ui/sidebar/evidence.rs`. `row_factory.rs` is unchanged; its `GtkTreeExpander` gesture handling is not touched.
- **GTK Lush:** `crates/gtk-lush/axioms/` (new probe, catalogue entry, sample, README/CHANGELOG), probed by `gtk-lush-adoption-lab`'s `axiom_probes`.
- **Tests:** new widget tests in an existing sidebar widget module (no new shard entry needed unless a new module file is chosen), an exhaustive unit test of the disposition policy, and the existing `workspace_tree_virtualization`, `workspace_entry_visibility` and sidebar refresh/reveal tests kept green.
- **Gates:** `make check`, `make check-gtk-axioms`, `make gtk-axioms`, `make check-workflow-boundaries`, `make check-widget-shards`, the widget shards that own the sidebar modules, and `make check-visual-proof-policy` (plus `make visual-geometry-smoke` if the policy classifies the touched files as visual-sensitive).
- **Data integrity:** child scans only read directories; no path in this change writes, renames or deletes a file, and no persisted state changes.
- **Dependencies:** none. Independent of the long-line changes (`probe-long-line-toolkit-costs` and its successors). The bind-dispatch cost is checked against their working interaction budget, B_interact = 8 ms per dispatch, with its final value taken from C0 if C0 has landed.
