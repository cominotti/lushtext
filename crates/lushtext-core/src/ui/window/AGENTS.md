# Window Shell

This folder owns the top-level application shell adapter.

## Responsibilities

- Keep `mod.rs` as the small public facade for `LushtextWindow`.
- Keep workflow-specific logic in sibling modules such as `actions`, `documents`, `drafts`, `focus_indexing`, `notifications`, `preview`, `print`, `search`, `session_persistence`, and `zoom`.
- Keep `imp.rs` focused on template children, state, and setup glue rather than long workflow implementations.

## Local Contracts

- Treat draft lifecycle and session snapshot persistence as separate workflows. Coordinate them explicitly; do not collapse them back into one catch-all module.
- Keep status-bar refresh and properties-panel refresh behavior aligned when window-level document state changes.
- Keep search-panel shell integration here, but keep search-panel internal list/history/replace/runtime mechanics in `ui/search_panel/`.
- When split-view geometry changes, preserve the total-window width contracts and the mirrored status-bar toggle behavior described in the root `AGENTS.md` and `.agents/rules/ui.md`.

## Editing Rules

- Prefer a new sibling workflow module over growing `mod.rs` or `imp.rs` into another large mixed-responsibility file.
- Update the root `AGENTS.md` and `README.md` module map when this folder's workflow files materially change.
