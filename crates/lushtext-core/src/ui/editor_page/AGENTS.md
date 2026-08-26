# Editor Page

This folder owns one open editor tab: buffer state, file I/O choreography, external file monitoring, and in-tab search.

## Responsibilities

- Keep `mod.rs` as the small public facade for `LushtextEditorPage`.
- The two document workflows own **per-workflow role home subdirectories**, each
  with a narrative facade `mod.rs`, coordination modules named from the bounded
  set, `policy.rs`, and `evidence.rs`. Read the facade first; it narrates the
  stage order and names every point where control leaves and comes back.
  - `load/` — open, reopen-with-encoding, recent documents, session restore:
    `admission.rs` (identity rotation, planning probe, the process-wide
    coordinator, queue, drain, charge release), `execution.rs` (acceptance and
    the four-phase bounded install state machine), `retirement.rs` (cancellation
    and disposal: payload, charge, partial buffer, identity).
  - `save/` — Ctrl+S, Save As, close-with-changes: `admission.rs` and
    `execution.rs`.
- Cross-cutting editor-page state that neither document workflow owns lives
  beside them: shared document identity and metadata in `document_identity.rs`,
  and the deferred cursor/scroll group in `restore_position.rs` (five owning
  workflows). Both are reached through named operations, not by reaching into
  another workflow's state.
- Keep Focus Mode presentation in `focus_mode.rs`, minimap behavior in
  `minimap.rs`, dynamic editor overscroll in `overscroll.rs`, bookmark projection
  in `bookmarks.rs`, external monitor behavior in `monitor.rs`, and in-tab
  search-bar behavior in `search.rs`.
- Keep `imp.rs` focused on template/state wiring and helper routines shared by those workflows.

## Local Contracts

- Preserve the data-safety boundary around save, draft recovery, file monitoring, and load cancellation. Changes in one of those areas usually need a check across the others.
- Keep queued saves weak/scalar until admission. The typed save permit must span snapshot, transformations, encoding, and durable-write consumption; every pre-admission and terminal path revalidates editor, save generation, destination, and close-session identity before changing buffer/path/draft state.
- Large save snapshots remain independently allocated UTF-8 chunks until a
  worker coalesces/transforms them. The same typed permit covers capture,
  worker handoff, durable write, stale/rejected disposal, and exact-once
  release; no whole-body `String` may return to GTK.
- Keep blocking file I/O off the main thread.
- Keep file-load planning payload-free and queued ownership weak/scalar. A payload worker must own a process-wide byte permit before bounded ingestion begins, and decoded text must retain that permit through direct or chunked GTK installation. Cancellation, stale completion, disposal, worker failure, and success must all converge on exact-once permit release.
- While chunked load installation clears old text, inserts new text, or cleans up cancellation, suppress in-editor search, draft, preview, local-history, minimap, title, memory-policy, and similar document-amplifying projections. Publish metadata and one final memory-policy update only after exact text has been installed; never expose partial text as `Loaded` or saveable state. Disposal may discard the buffer directly, but a live page must clear document-sized text in bounded main-loop turns.
- Whole-buffer replacement must mark mutation ownership before every non-empty GTK delete or insert and release mutable session borrows across those signal-emitting calls. Synchronous `changed` handlers may supersede the generation; the continuation must revalidate phase/identity, clear the old partial body exactly once, and restore editability, saveability, and projection state only at the terminal boundary.
- Keep `last_known_mtime`, eviction state, draft flags, bookmark projection state, and EditorConfig override state explicit and well-named; these are coupled safety/restore signals, not incidental fields.
- Keep minimap state tab-local. Marker projection, availability gating, and gesture-driven navigation belong here with the editor, not in `window/` or `services/`. Wrapped-layout and long-line evidence must come from the current accepted `model::minimap_analysis` cache or generation/lifetime-bound GTK cursor slices; edits, wrap/marker changes, file replacement, eviction/reload suspension, and teardown invalidate the active cursor and source before stale work can continue or publish.
- Keep workspace-wide search behavior out of this folder; this subtree only owns the per-tab search bar and editor-local flows.
- Keep note persistence out of this folder. `EditorPage` owns live marks, anchors, and highlights; the window layer owns sidecar loading, saving, workspace browse flows, and export commands.
- Automatic local-history capture must classify the current live buffer, acquire shared admission before moving or copying document-sized text, and revalidate editor/path/timer/edit generations before worker persistence. Chunked capture stays byte-budgeted and cancellable; contended baselines wait as weak/scalar state rather than queued text payloads.
- Local History Restore and Undo Restore must reserve current-buffer ownership
  before capture, persist the safety snapshot before mutation, and keep the
  returned prior body inside its disposal guard through replacement,
  supersession, save/clean transitions, eviction, and teardown. Under pressure,
  retain only compact latest intent and start no capture or mutation.

## Editing Rules

- When adding a new tab-local workflow, prefer a new sibling module over expanding `mod.rs` or `imp.rs` into another mixed file.
- Re-read the data-safety guidance in the root docs and skills when touching persistence or close/save behavior.
