# Editor Page

This folder owns one open editor tab: buffer state, file I/O choreography, external file monitoring, and in-tab search.

## Responsibilities

- Keep `mod.rs` as the small public facade for `LushtextEditorPage`.
- Keep file load/save flows and bounded GTK installation in `load_save.rs`, process-wide weak/scalar load admission in `load_runtime.rs`, Focus Mode presentation in `focus_mode.rs`, minimap behavior in `minimap.rs`, dynamic editor overscroll in `overscroll.rs`, bookmark projection in `bookmarks.rs`, external monitor behavior in `monitor.rs`, and in-tab search-bar behavior in `search.rs`.
- Keep `imp.rs` focused on template/state wiring and helper routines shared by those workflows.

## Local Contracts

- Preserve the data-safety boundary around save, draft recovery, file monitoring, and load cancellation. Changes in one of those areas usually need a check across the others.
- Keep blocking file I/O off the main thread.
- Keep file-load planning payload-free and queued ownership weak/scalar. A payload worker must own a process-wide byte permit before bounded ingestion begins, and decoded text must retain that permit through direct or chunked GTK installation. Cancellation, stale completion, disposal, worker failure, and success must all converge on exact-once permit release.
- While chunked load installation clears old text, inserts new text, or cleans up cancellation, suppress in-editor search, draft, preview, local-history, minimap, title, memory-policy, and similar document-amplifying projections. Publish metadata and one final memory-policy update only after exact text has been installed; never expose partial text as `Loaded` or saveable state. Disposal may discard the buffer directly, but a live page must clear document-sized text in bounded main-loop turns.
- Keep `last_known_mtime`, eviction state, draft flags, bookmark projection state, and EditorConfig override state explicit and well-named; these are coupled safety/restore signals, not incidental fields.
- Keep minimap state tab-local. Marker projection, availability gating, and gesture-driven navigation belong here with the editor, not in `window/` or `services/`.
- Keep workspace-wide search behavior out of this folder; this subtree only owns the per-tab search bar and editor-local flows.
- Keep note persistence out of this folder. `EditorPage` owns live marks, anchors, and highlights; the window layer owns sidecar loading, saving, workspace browse flows, and export commands.
- Automatic local-history capture must classify the current live buffer, acquire shared admission before moving or copying document-sized text, and revalidate editor/path/timer/edit generations before worker persistence. Chunked capture stays byte-budgeted and cancellable; contended baselines wait as weak/scalar state rather than queued text payloads.

## Editing Rules

- When adding a new tab-local workflow, prefer a new sibling module over expanding `mod.rs` or `imp.rs` into another mixed file.
- Re-read the data-safety guidance in the root docs and skills when touching persistence or close/save behavior.
