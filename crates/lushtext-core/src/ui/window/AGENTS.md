# Window Shell

This folder owns the top-level application shell adapter.

## Responsibilities

- Keep `mod.rs` as the small public facade for `LushtextWindow`.
- Keep workflow-specific logic in sibling modules such as `actions`, `documents`, `drafts`, `focus_indexing`, `focus_mode`, `notes`, `notifications`, `preview`, `print`, `search`, `session_persistence`, `tabs`, `transient_surfaces`, and `zoom`.
- Keep `imp.rs` focused on template children, state, and setup glue rather than long workflow implementations.

## Local Contracts

- Treat draft lifecycle and session snapshot persistence as separate workflows. Coordinate them explicitly; do not collapse them back into one catch-all module.
- Keep per-draft intent allocation and wrap-safe freshness in `draft_ordering.rs`. `drafts.rs` remains the GTK driving adapter and owns the single-flight body/manifest/delete choreography; commands waiting behind document-sized work must stay compact and GTK-free.
- Keep bookmark and note workflows in the private `notes/` module. `mod.rs`
  owns shared callbacks, migration coordination, and menu availability;
  `bookmarks.rs`, `editors.rs`, and `browser.rs` own their named workflows.
  Keep these GTK workflows out of `documents.rs`, `imp.rs`, and services.
- Keep status-bar refresh and properties-panel refresh behavior aligned when window-level document state changes.
- Keep search-panel shell integration here, but keep search-panel internal list/history/replace/runtime mechanics in `ui/search_panel/`.
- Keep window-level transient dismissal in `transient_surfaces.rs`: Escape closes one topmost dismissible shell surface before Focus Mode exit, and command-palette click-away routes through `close_command_palette()` so focus restoration stays centralized.
- Keep automation-friendly target-state actions in `actions.rs` thin and routed
  through the same production workflows as visible toggles. After adding or
  changing any externally observable action, update `services::action_catalog`
  plus `docs/automation-reference.md` and run `make check-automation-docs`.
- When split-view geometry changes, preserve the total-window width contracts and the mirrored status-bar toggle behavior described in the root `AGENTS.md` and `.agents/rules/ui.md`.
- Keep split-view allocation paths cheap and runtime-only. `size_allocate()` may clamp live sidebar fractions and update cached breakpoint thresholds when the actual allocated width changes, but it must not persist GSettings, rebuild/reparse `AdwBreakpoint` conditions, or rehost secondary surfaces on every animation frame.
- Keep plain split-view width, fraction, breakpoint, and compact-surface
  decisions in `adaptive_shell.rs`. `imp.rs` owns the live Libadwaita objects,
  settings reads and writes, focus restoration, signal setup, and application
  of those decisions.

## Editing Rules

- Prefer a new sibling workflow module over growing `mod.rs` or `imp.rs` into another large mixed-responsibility file.
- Update the root `AGENTS.md` and `README.md` module map when this folder's workflow files materially change.
