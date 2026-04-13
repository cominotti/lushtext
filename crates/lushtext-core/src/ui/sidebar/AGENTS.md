# Sidebar

This folder owns the multi-workspace sidebar adapter and its workspace-section subtree.

## Responsibilities

- Keep the top-level sidebar responsible for workspace orchestration, persistence, width presets, and callback forwarding.
- Keep per-workspace header and tree behavior inside `workspace_section/`.
- Keep dialog helpers and callback plumbing in sibling modules instead of re-inlining them into `mod.rs`.

## Local Contracts

- The top "New Workspace" affordance and bottom width-preset footer are fixed rows outside the scroller. Do not let them scroll away.
- Width presets are total-window targets: `Small=20%`, `Comfy=30%`, `Large=40%`. Do not reinterpret them as local paned fractions.
- Preserve the no-horizontal-scrollbar contract. Prefer tooltips, focused roots, or explicit drill-down behavior over widening the sidebar or clipping silently.
- Keep workspace-section async tree loading off the main thread and preserve deduplication/placeholder behavior for large directories.

## Editing Rules

- If a change only affects one workspace section's header/tree workflow, keep it in `workspace_section/` rather than pulling it up into the sidebar orchestrator.
- Update the root `AGENTS.md` and `README.md` module map when this folder's structure materially changes.
