# Sidebar

This folder owns the multi-workspace sidebar adapter and its workspace-section subtree.

## Responsibilities

- Keep the top-level sidebar responsible for workspace orchestration, persistence, and callback forwarding.
- Keep the top selector row responsible for editing the shared current workspace scope, not just local visibility.
- Keep per-workspace header and tree behavior inside `workspace_section/`.
- Keep dialog helpers and callback plumbing in sibling modules instead of re-inlining them into `mod.rs`.
- **`mod.rs` is the workflow's narrative facade.** It narrates the twelve stage orders and delegates every one; it must own no timer, no admission bookkeeping, no generation counter, and no widget mutation. Its normative ceiling is **370 physical lines**.
- Keep this workflow's **pure decisions** in `policy.rs` and its **seam value
  objects** in `seams.rs`, both at this directory (the workflow's canonical role
  home). `policy.rs` must import no `gtk4`, `glib`, `gio`, `libadwaita`, or
  `sourceview5` — that purity is what keeps it inside the default `cargo-mutants`
  `ui/**/policy.rs` scope, and this workflow's decisions are the ones that most need
  that coverage because they **rename and delete the user's own documents**. Test-only
  configuration lives in `test_policy.rs`, entirely behind
  `#[cfg(feature = "test-utils")]`.
- `width_preset.rs` is **not this workflow's**. `WorkspaceSidebarWidthPreset` is the
  `workspace-sidebar-width-policy` capability's value, owned by `WFR-SHELL-LAYOUT`
  and consumed by Preferences and the window shell. It lives here only because it
  names a sidebar dimension.

## Role Map

This directory is the workspace tree workflow's **canonical role home**, and the role
home is **nested**: `workspace_section/` holds the per-section coordination roles and
its own module doc lists them.

| Here | Role |
| --- | --- |
| `mod.rs` | narrative facade |
| `policy.rs` | pure policy (the only one) — **must import no `gtk4`, `glib`, `gio`, `libadwaita`, or `sourceview5`**, which is what keeps it in the default mutation scope |
| `evidence.rs` | evidence (the only one) |
| `seams.rs` | seam value objects |
| `test_policy.rs` | **not a role and not a presentation surface** — the workflow's one test policy value, entirely behind `#[cfg(feature = "test-utils")]`. It is not a second `policy.rs` |
| `list_execution.rs` | coordination `execution`: workspace-list load, add/rename/unlist |
| `membership_execution.rs` | coordination `execution`: folder add/remove/reorder |
| `filter_execution.rs` | coordination `execution`: scope filter and its fade |
| `persist_execution.rs` | coordination `execution`: the `workspaces.json` pipeline |
| `callbacks.rs`, `dialogs.rs`, `imp.rs` | **called presentation surfaces** — no role, no `policy.rs`, no `evidence.rs` |
| `width_preset.rs` | **not this workflow's** — `WFR-SHELL-LAYOUT` owns it |
| `file_tree_item.rs` | outside this workflow — no coordination tier |

`workspace_section/watch_targets.rs` is likewise **neither** a role nor a presentation
surface: it is a plain incremental data structure the `watch` role owns, with no GTK
import and no stage of its own. Both classifications are stated in the modules' own docs
and in the workflow's matrix row, so neither absence reads as an oversight.

Do **not** rename `workspace_section/watch.rs`: it already carries a correct bounded
role name, and renaming a stable correct module for symmetry is churn a reader must
diff to understand.

## Local Contracts

- The top workspace-selector row stays fixed outside the scroller. Do not let it scroll away.
- Each persisted workspace owns one ordered folder set and therefore one workspace section. Keep one top-level tree entry per configured folder, in stored order, and preserve empty workspaces as real sections.
- Width presets are selected from `Preferences > Workspace` and keep their `Small=20%`, `Comfy=30%`, `Large=40%` identities while the window layer clamps their visible width on large displays. Do not reinterpret them as local paned fractions.
- Preserve the no-horizontal-scrollbar contract. Prefer tooltips, focused folders, or explicit drill-down behavior over widening the sidebar or clipping silently.
- Keep workspace-section async tree loading off the main thread and preserve deduplication/placeholder behavior for large directories.
- Give each materialized child store one `policy::WorkspaceScanFlight` (this workflow's own pure policy, relocated out of `model/` with mutation parity): one admitted scan and one replaceable latest weak/scalar request. Capture strong store ownership and the current mirror only at worker admission, and require lifetime, store, target-generation, and scan-generation agreement before reconciliation or empty-folder evidence may publish.
- Keep workspace watcher targets as an incremental mirror of flattened-row splices and expansion state; do not restore full `GtkTreeListModel` scans on restart. Watcher creation, registration, replacement teardown, and stale-handle disposal stay off the GTK thread, with at most one lifecycle worker per section and one latest-generation handoff.
- Keep watcher event delivery in the per-handle GTK-free coalescing mailbox. Backend callbacks own tree-change filtering and bounded path deduplication; mailbox and refresh planning share the 1,024-path cap, raw normalization stops when overflow becomes one full refresh, and each non-blocking GTK poll consumes at most one notice. A pending full refresh releases and dominates targeted paths until it runs; targeted planning indexes expanded stores once per pass.
- In workspace-section rows, workspace-folder reorder DnD hover belongs to the transparent row-level shield. `GtkTreeExpander` owns normal disclosure behavior, and any idle collapse after drag-hover child-model creation is defensive only, not the intended reorder path.
- Keep recycled row setup/bind/unbind ownership in
  `workspace_section/row_factory.rs`, row metadata and expanded-hook symmetry in
  `row_accessibility.rs`, and file/header popup construction plus targeting in
  `context_menus.rs`. `workspace_section/imp.rs` retains subclass state,
  template children, construction, and disposal glue.

- **`evidence.rs` has one derivation, not two.** `workspace_snapshot_evidence` derives the
  ten scalars the exported `window.workspace` snapshot serializes, and
  `workspace_tree_evidence` builds itself **from** that value rather than repeating it —
  so a polled D-Bus read never allocates the per-section collections, and the surface and
  the snapshot cannot drift. Do not hand-copy the derivation back into both.
- **A workspace load's adoption bit is written only by the load-adoption path.**
  `imp.load_adopted` feeds `policy::superseded_load_action`, whose parameter is named
  `any_load_adopted`; setting it from `build_sections_from_file` — which every mutation
  reaches through `rebuild_sections_from_state` — made that guard inert and destroyed the
  stored workspace list. Only `adopt_loaded_workspaces` may set it.
- **Reading `evidence.rs` must not make the toolkit do work.** This workflow's model is a lazily materialized `GtkTreeListModel`, so several innocuous-looking accessors *create* state: `build_children_model` is the create function itself; `find_store_for_dir` calls `row.children()` **and inserts into** the `dir_stores` cache; `find_dir_row` **evicts** from `dir_rows`; `visible_child_stores` calls `row.children()` with **no `is_expanded()` filter**; `derive_expanded_paths_from_model` advances the very capture counters the surface reports; and `set_expanded(true)` materializes children **and** queues a watcher restart. Derive from `expanded_paths` instead, and keep the inertness proof passing.
- **Aggregates over the section set must stay bounded and honest.** Answer correctly with zero workspaces, and read only `Cell`/`RefCell` imp state — the surface deliberately reads **no** `TemplateChild`, which is what makes it disposal-safe. The section's `dispose()` does **not** call `dispose_template()`; if that ever changes, add a `try_get()` guard **together with** a test that drives the state.
- Widget tests read the evidence surface. Do **not** add a new per-field `*_for_test` getter for something it does not expose — extend the surface instead.

## Editing Rules

- If a change only affects one workspace section's header/tree workflow, keep it in `workspace_section/` rather than pulling it up into the sidebar orchestrator.
- Update the root `AGENTS.md` and `README.md` module map when this folder's structure materially changes.
