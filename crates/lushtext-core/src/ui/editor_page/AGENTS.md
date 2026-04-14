# Editor Page

This folder owns one open editor tab: buffer state, file I/O choreography, external file monitoring, and in-tab search.

## Responsibilities

- Keep `mod.rs` as the small public facade for `LushtextEditorPage`.
- Keep file load/save flows in `load_save.rs`, bookmark projection in `bookmarks.rs`, annotation projection in `annotations.rs`, external monitor behavior in `monitor.rs`, and in-tab search-bar behavior in `search.rs`.
- Keep `imp.rs` focused on template/state wiring and helper routines shared by those workflows.

## Local Contracts

- Preserve the data-safety boundary around save, draft recovery, file monitoring, and load cancellation. Changes in one of those areas usually need a check across the others.
- Keep blocking file I/O off the main thread.
- Keep `last_known_mtime`, eviction state, draft flags, bookmark/annotation projection state, and EditorConfig override state explicit and well-named; these are coupled safety/restore signals, not incidental fields.
- Keep workspace-wide search behavior out of this folder; this subtree only owns the per-tab search bar and editor-local flows.
- Keep note persistence out of this folder. `EditorPage` owns live marks, anchors, and highlights; the window layer owns sidecar loading, saving, workspace browse flows, and export commands.

## Editing Rules

- When adding a new tab-local workflow, prefer a new sibling module over expanding `mod.rs` or `imp.rs` into another mixed file.
- Re-read the data-safety guidance in the root docs and skills when touching persistence or close/save behavior.
