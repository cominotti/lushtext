# Sidebar

This folder owns the multi-workspace sidebar adapter and its workspace-section subtree.

## Responsibilities

- Keep the top-level sidebar responsible for workspace orchestration, persistence, and callback forwarding.
- Keep the top selector row responsible for editing the shared current workspace scope, not just local visibility.
- Keep per-workspace header and tree behavior inside `workspace_section/`.
- Keep dialog helpers and callback plumbing in sibling modules instead of re-inlining them into `mod.rs`.

## Local Contracts

- The top workspace-selector row stays fixed outside the scroller. Do not let it scroll away.
- Each persisted workspace owns one ordered folder set and therefore one workspace section. Keep one top-level tree entry per configured folder, in stored order, and preserve empty workspaces as real sections.
- Width presets are selected from `Preferences > Workspace` and keep their `Small=20%`, `Comfy=30%`, `Large=40%` identities while the window layer clamps their visible width on large displays. Do not reinterpret them as local paned fractions.
- Preserve the no-horizontal-scrollbar contract. Prefer tooltips, focused folders, or explicit drill-down behavior over widening the sidebar or clipping silently.
- Keep workspace-section async tree loading off the main thread and preserve deduplication/placeholder behavior for large directories.
- Keep workspace watcher targets as an incremental mirror of flattened-row splices and expansion state; do not restore full `GtkTreeListModel` scans on restart. Watcher creation, registration, replacement teardown, and stale-handle disposal stay off the GTK thread, with at most one lifecycle worker per section and one latest-generation handoff.
- In workspace-section rows, workspace-folder reorder DnD hover belongs to the transparent row-level shield. `GtkTreeExpander` owns normal disclosure behavior, and any idle collapse after drag-hover child-model creation is defensive only, not the intended reorder path.
- Keep recycled row setup/bind/unbind ownership in
  `workspace_section/row_factory.rs`, row metadata and expanded-hook symmetry in
  `row_accessibility.rs`, and file/header popup construction plus targeting in
  `context_menus.rs`. `workspace_section/imp.rs` retains subclass state,
  template children, construction, and disposal glue.

## Editing Rules

- If a change only affects one workspace section's header/tree workflow, keep it in `workspace_section/` rather than pulling it up into the sidebar orchestrator.
- Update the root `AGENTS.md` and `README.md` module map when this folder's structure materially changes.
