# LushText

> **Load order:** Agents must always load [SOUL.md](./SOUL.md) first and immediately, before reading the rest of [AGENTS.md](./AGENTS.md), so it can inform how the agent should behave while applying the project guidance below.

A minimalist, fast text editor targeting Libadwaita. Looks similar to GNOME Text Editor but with a left workspace pane, an optional right properties pane, and workspace support.

## Tech Stack

- **Language:** Rust (MSRV: 1.95.0, Edition 2024)
- **GUI:** GTK4 (0.11) + Libadwaita (0.9) + GtkSourceView 5 (0.11)
- **Config:** GSettings (`data/dev.cominotti.lushtext.gschema.xml`)
- **Build:** Cargo workspace + Makefile (dev), Meson (Flatpak/installed)
- **App ID:** `dev.cominotti.lushtext`
- **License:** GPL-3.0-or-later

## Rules Index / Sync Map

Keep this index in sync with `.agents/rules/*.md`. When a new rule file is added or an existing rule is materially changed, update this section in the same change.

- `build.md` — build, dependency, and test command rules
- `documentation.md` — documentation maintenance requirements
- `git.md` — git workflow and commit conventions
- `preexisting-blockers.md` — mandatory no-exceptions rule: fix pre-existing blockers in the same work stream
- `rust.md` — Rust language, module-splitting, and state-grouping conventions
- `ui.md` — UI, theming, and GTK paned geometry/animation conventions
- `widget-wiring.md` — GTK widget composition, signal wiring, live paned-validation, and allocation-frame animation rules

## Architecture

Two-crate workspace (plus workspace-hack for cargo-hakari):

- `crates/lushtext-core` — all application logic: data models, services, GTK widgets
- `crates/lushtext` — thin binary entry point + integration tests

### Module Layout (lushtext-core)

```
src/
├── app.rs              # LushtextApplication (AdwApplication subclass)
├── config.rs           # Compile-time constants (APP_ID, VERSION, PKGDATADIR)
├── lib.rs              # Entry point: GResource registration, CSS loading, GSettings schema dir, app.run()
├── model/              # Domain types (no GTK deps)
│   ├── workspace.rs    # WorkspaceId, WorkspaceScope, WorkspaceEntry (legacy/tree-local), WorkspaceConfig, WorkspacesFile
│   ├── session.rs      # SessionTab, SessionData — global session for tab restore
│   ├── palette.rs      # IndexedFile, CommandDef, CommandCategory, SearchMode, ScoredResult
│   ├── draft.rs        # DraftEntry, DraftManifest — draft persistence metadata
│   ├── note.rs         # RichNoteBody, NoteViewMode — shared note-body primitives
│   ├── bookmark.rs     # BookmarkId, BookmarkRecord, BookmarkDocument — saved-file line bookmarks
│   ├── annotation.rs   # AnnotationId, AnnotationRecord, AnnotationStyle, AnnotationDocument
│   ├── document_note.rs # DocumentNoteDocument — one rich note per saved file
│   ├── local_history.rs # LocalHistoryDocument, LocalHistorySnapshotMeta — per-file snapshot metadata
│   ├── content_search.rs # SearchMatch, ContentSearchOptions, SearchEvent, SearchHistoryEntry — content search types
│   ├── encoding.rs     # DocumentEncodingState, LineEnding, FileHealthFinding, InvisibleCharactersMode
│   ├── sidecar_identity.rs # DocumentSidecarIdentity — canonical-path sidecar keys for notes and history
│   ├── workspace_note.rs # WorkspaceRootIdentity, WorkspaceNoteDocument — one rich note per workspace root
│   └── formatting_overrides.rs  # FormattingOverrides — per-file EditorConfig overrides
├── services/           # Business logic
│   ├── async_task.rs   # spawn_blocking_then, MAX_CONCURRENT_SPAWNS, concurrency guard
│   ├── annotation_service.rs # Annotation sidecar load/save/move/export helpers
│   ├── bookmark_service.rs # Bookmark sidecar load/save/move/list helpers
│   ├── document_note_service.rs # Saved-file document-note load/save/move/list helpers
│   ├── local_history_service.rs # Snapshot capture/list/load/prune/move helpers
│   ├── note_storage.rs # Shared sidecar identity/load/filter helpers for note workflows
│   ├── content_search/ # Workspace-wide grep: streaming search + replace/undo helpers
│   ├── palette/        # Command registry, fuzzy matching, and file indexing
│   ├── draft_service.rs # Draft persistence: save/load/delete draft files and manifest
│   ├── durable_write.rs # Directory fsync helpers for crash-durable atomic writes
│   ├── editor_io.rs    # Encoding-aware text file load/save helpers, health analysis, mtimes
│   ├── editorconfig.rs # .editorconfig file discovery and parsing (pure I/O, no GTK)
│   ├── file_peek.rs    # Bounded read-only snapshots for sidebar file peek
│   ├── file_limits.rs  # File size thresholds for graceful degradation
│   ├── file_tree.rs    # Directory scanning (pure I/O, bounded/cancellable helpers for sidebar)
│   ├── json_store.rs   # Generic JSON load/save + data_dir()
│   ├── notifications.rs # Window-scoped status and inline notification store
│   ├── saved_searches.rs # Named saved search persistence: load/save/add/remove (permanent)
│   ├── search_backup.rs # Replace All undo backup persistence within the current session
│   ├── search_history.rs # Search history persistence: load/save/dedup (capped at 20)
│   ├── session_service.rs
│   ├── workspace_note_service.rs # Workspace-root note load/save/move/list helpers
│   ├── workspace_manager.rs
│   └── workspace_watch.rs # Materialized-scope filesystem watch service for sidebar auto-refresh
├── benches/
│   └── benchmarks.rs   # Criterion benchmarks for all performance-sensitive services
└── ui/                 # GTK4/Libadwaita widgets (each folder keeps mod.rs + imp.rs)
    ├── window/          # Main window shell plus workflow modules for actions, documents, drafts, encoding, Focus Mode, local-history, notes, search, preview, print, session persistence, tab management, workspace scope, and zoom
    ├── editor_page/     # Per-tab editor adapter plus Focus Mode presentation, local-history capture, minimap, overscroll, invisible-character rendering, bookmark/annotation projection, load/save, monitor, and in-tab search helpers
    ├── sidebar/         # Multi-workspace sidebar orchestrator plus dialogs, callbacks, and per-workspace sections
    ├── search_panel/    # Workspace-wide content search panel plus history, list factory, replace, results, and runtime flows
    ├── command_palette/ # Ctrl+P fuzzy search: files + commands
    ├── properties_panel/ # Right-side document metadata + formatting controls
    ├── markdown_preview/ # Read-only Markdown preview (pulldown-cmark → TextTags)
    ├── info_bar/        # Contextual warning/error bars (GtkInfoBar) above editor
    ├── search_bar/      # Find/replace widget
    ├── status_bar/      # Bottom bar: feedback messages + file metadata
    └── preferences/     # AdwPreferencesDialog
```

## Nested AGENTS.md Files

Use nested `AGENTS.md` files only when a subtree has stable local contracts that would otherwise make this root file churn constantly.

Current nested files:

- `crates/lushtext-core/AGENTS.md` — crate-level layering and ownership for core app logic
- `crates/lushtext/AGENTS.md` — binary crate and test-harness boundaries
- `crates/lushtext-core/src/ui/AGENTS.md` — common GTK driving-adapter rules
- `crates/lushtext-core/src/ui/window/AGENTS.md` — shell/window workflow contracts
- `crates/lushtext-core/src/ui/sidebar/AGENTS.md` — workspace sidebar and section contracts
- `crates/lushtext-core/src/ui/editor_page/AGENTS.md` — per-tab editor, save/load, monitor, and draft-sensitive rules
- `crates/lushtext-core/src/ui/search_panel/AGENTS.md` — workspace search/replace panel contracts

If you add another nested `AGENTS.md`, keep it local, non-duplicative, and worth its maintenance cost. Update this list in the same change.

### Key Design Decisions

- **GtkSourceView owns editing**: language detection, syntax highlighting, style schemes, and undo/redo are all delegated to GtkSourceView, not reimplemented.
- **GSettings for preferences**: Editor settings (word wrap, tab width, line numbers, etc.) are stored in GSettings (`dev.cominotti.lushtext` schema). `gio::Settings::bind()` creates two-way bindings between settings keys and widget properties. Each `EditorPage` binds its own source view to GSettings in `constructed()` — when a setting changes, all editors update automatically with no manual iteration.
- **Dark mode**: GtkSourceView has its own style scheme system separate from GTK CSS. The base scheme ID is read from GSettings (`style-scheme` key) and the dark variant (e.g., `"Adwaita-dark"`) is selected automatically based on `libadwaita::StyleManager::is_dark()`, with live switching via `connect_dark_notify()`.
- **Tab content transparency**: `Preferences > Editor > Appearance` includes an always-visible `Transparency` control patterned after modern Fedora terminal settings. The selected opacity applies to the main editor document surface and Markdown preview background, while the header bar, tab bar chrome, side panels, status/search chrome, and minimap remain explicitly opaque.
- **Multi-workspace sidebar**: The sidebar is a two-level orchestrator. `LushtextSidebar` keeps a fixed workspace-selector row at the top and a scrollable list of `LushtextWorkspaceSection` widgets below it. The selector row uses a `GtkDropDown` with the aggregate scope `All workspaces` plus one item per workspace, alongside the existing add-folder button for creating a new workspace. Selecting `All workspaces` shows every workspace section and acts as the app-wide aggregate workspace scope; selecting a specific workspace narrows the shared workspace scope to that single workspace and hides the others in the sidebar. Each persisted workspace owns exactly one root directory and therefore one workspace section. The top selector row stays visible while the workspace list scrolls, and the sidebar no longer exposes a horizontal scrollbar when long workspace or file names exceed the visible width. Workspace sidebar width now lives in `Preferences > Workspace` as `Small`, `Comfy`, and `Large` presets that keep their `20% / 30% / 40%` identity while clamping to a comfortable desktop width on large windows. Clicking the top add-folder button creates a new workspace (auto-named from folder). Each section's header keeps `Refresh` plus `Replace Workspace Root`, and right-click on the workspace header shows Rename/Remove context menu. When no workspaces exist, the fixed top row remains visible and the section list stays intentionally empty.
- **Inner ScrolledWindow pattern**: Each `WorkspaceSection` wraps its `GtkListView` in a `GtkScrolledWindow(propagate-natural-height=true, propagate-natural-width=false, vscrollbar-policy=never, hscrollbar-policy=never)`. `propagate-natural-width` is set to false (and tree labels use `EllipsizeMode::End`) so that deep tree indentation yields to the sidebar width instead of expanding the paned handle horizontally. A drill-down "Focus Folder" affordance lets users re-root the tree into deeply nested paths that become clipped. When focused, other workspaces can be automatically collapsed (controllable via GSettings `workspace-auto-collapse`) and the sidebar scrolls to bring the focused header to the top. Without the inner scroller, GtkListView may crash or emit warnings.
- **Empty folder detection**: `scan_directory_bounded` peeks one level deep into subdirectories (up to a configurable `lookahead_cap`) to detect emptiness. Folders confirmed empty show an `(Empty)` label, hide their expansion arrow, and disable the "Focus Folder" action. This provides immediate visual feedback without the N+1 lookahead bottleneck on large trees.
- **Workspace persistence**: `WorkspacesFile` (model) is loaded from `workspaces.json` via `workspace_manager::load()` on window construction. Sidebar mutations mark persistence dirty, debounce for 150ms, and save via `spawn_blocking_then`, with in-flight serialization so older snapshots cannot overwrite newer ones. `WorkspacesFile` derives `Clone` to enable cloning out of `RefCell` for background save work. The sidebar owns the `WorkspacesFile` state.
- **Workspace auto-refresh**: Each `WorkspaceSection` watches the directories the sidebar has actually materialized, using non-recursive debounced watches for root rows plus expanded directories via `services/workspace_watch.rs`. Broad configured roots no longer force a recursive startup watch across every descendant; collapsed or not-yet-loaded areas still refresh on demand through the manual `Refresh` button or when their parent directory is expanded. Access-only watcher noise is filtered before it reaches the UI, refreshed child stores are reconciled with bounded `splice()` updates instead of blanking the whole subtree, and manual refresh now keeps the existing root `TreeListModel` mounted whenever the visible root-row set is still reconcilable.
- **File tree uses modern GTK4 model**: `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` (NOT the deprecated `GtkTreeView`). File tree labels use the `.monospace` CSS class, sharing the editor's font customization provider.
- **File context menu**: Per-`WorkspaceSection`. Right-click on a file or directory shows a `GtkPopoverMenu` with New File, New Folder, Rename, and Delete actions. Uses `Widget::pick()` + ancestor traversal to find the `TreeExpander` → `TreeListRow` → `FileTreeItem`. Actions are in a `section` action group on each section widget.
- **File peek**: Per-`WorkspaceSection`. Press `Space` on a selected sidebar file row to open a `GtkPopover` anchored beside that row without resizing the split layout. The popover renders file name, absolute file path, size, modified time, and either a bounded read-only text sample or an explicit fallback state. Peek stays read-only, updates in place as sidebar selection changes, dismisses on repeated `Space`, `Escape`, click-away, non-file selection, section rebuild, or workspace-filter hide, and promotes through the existing sidebar `file_activated` callback so `open_document()` remains the single duplicate-tab and editor-focus authority.
- **Workspace header context menu**: Per-`WorkspaceSection`. Right-click on the workspace header shows a `GtkPopoverMenu` with Rename Workspace and Remove Workspace. Rename shows an `AdwAlertDialog` with text entry. Remove shows a confirmation dialog. Actions are in a `ws-header` action group.
- **Inline rename**: Rename swaps the row's `GtkLabel` for a `GtkEntry` dynamically. Enter confirms (removes entry immediately, then `std::fs::rename` runs on a background thread via `spawn_blocking_then`; on success the `FileTreeItem` path and label are updated on the main thread), Escape and focus-out cancel. A guard (`entry.parent().is_none()`) prevents double-fire from focus-out after confirm/cancel removes the entry. The window is notified via `connect_file_renamed` callback for tab path updates.
- **Delete with confirmation**: Delete shows an `AdwAlertDialog` with a destructive "Delete" response. On confirm, `std::fs::remove_file` or `std::fs::remove_dir_all` runs on a background thread via `spawn_blocking_then`; on success the item is removed from the parent `ListStore` and the window is notified via `connect_file_deleted` callback to close affected tabs. Directory operations use `Path::starts_with` for prefix matching (closing all tabs inside deleted/renamed directories).
- **New File / New Folder**: Context menu "New File" and "New Folder" actions use `spawn_blocking_then` for the `create_unique` filesystem call (avoids blocking the UI on slow filesystems or name collision retries), then add a `FileTreeItem` with `pending_rename = true` to the target `ListStore`. `connect_bind` detects the flag to automatically trigger inline rename. On confirm, the temp file is renamed to the user's chosen name and `connect_file_created` opens files in tabs. On cancel, the temp file is deleted via fire-and-forget `std::thread::spawn` and removed from the model. The `workspace_section/tree_loading.rs` helper deduplicates items already in the store to prevent duplicates when an expanded directory's async scan finds the temp file.
- **Workspace concept**: a workspace is one named root directory persisted in `$XDG_DATA_HOME/lushtext/workspaces.json`, plus a shared current workspace scope that can target one workspace or the aggregate `All workspaces` view. Creating a workspace selects it immediately; removing the selected workspace falls back to `All workspaces` instead of silently rebasing to another concrete workspace.
- **Session persistence**: All open tabs (with cursor position and scroll offset) are saved to a single global `$XDG_DATA_HOME/lushtext/session.json`. Tabs are not workspace-scoped in the UI — they all share one `AdwTabView` — so a single session file captures everything. Session save is debounced at 500ms (generation-counter pattern) and triggered by tab open, close, switch, and detach. Async and synchronous saves use ordered generations so an older background snapshot cannot overwrite a newer accepted one. A synchronous save runs on `close_request` as a safety net. On startup, `load_session_and_drafts` combines draft manifest + session loading in one background task (via `spawn_blocking_then`), then `restore_tabs` opens file-backed tabs via `open_document` and untitled tabs via `new_tab` with draft recovery. Cursor/scroll positions are deferred via `set_restore_position` → `apply_restore_position` (called in `load_file_async`'s success callback after content is loaded). The `restoring_session` flag suppresses redundant session saves during restore. CLI file arguments (`ApplicationImpl::open`) take priority over session's active tab selection.
- **Status bar**: per-window bottom bar below the split-view shell, always visible. Three sections: workspace sidebar toggle button (far left), feedback message area (left, hexpand), and a compact metadata cluster containing the terse `EditorConfig` badge plus the active document's line-ending and encoding entry points. Slower document-inspection details such as file size, formatting source, statistics, and file-health review live in document properties instead of the bottom bar. The window orchestrates all updates via `refresh_status_bar()`, which also refreshes the document-properties rows.
- **Adaptive document-properties shell**: The outer window shell keeps `workspace_split_view` on the left, while `properties_layout_view` uses Libadwaita layout slots to present the same `LushtextPropertiesPanel` either as the wide `properties_split_view` right pane or the compact `properties_bottom_sheet`. The panel is not manually rehosted between containers. The left pane restores one of three preset identities (`Small=20%`, `Comfy=30%`, `Large=40%`) from `Preferences > Workspace`, then clamps the visible sidebar width to a comfortable desktop range before turning that width back into the effective split fraction. The document-properties toggle lives in the header bar with `info-outline-symbolic` and owns `F9`; the status bar keeps only the workspace toggle. Compact layouts render only one secondary surface at a time, but requested visibility for the workspace sidebar and document properties is preserved so wider layouts can restore both surfaces when appropriate. The properties breakpoint is recalculated from the active left pane's effective visible width whenever the workspace pane actually consumes width so the center editor column stays wide enough for restored-document infobars and other editor chrome before the document-properties surface becomes a bottom sheet. Split-view allocation sync is runtime-only: it caches the last allocated width and derived breakpoint threshold, does not rewrite GSettings from animation-frame allocation or notify paths, and only reparses/reinstalls `AdwBreakpoint` conditions when the threshold actually changes.
- **Status bar auto-dismiss**: messages auto-dismiss after 5 seconds using a generation counter (`Cell<u32>`). Each `push_message` increments the counter; the timer closure captures the value and no-ops if the counter has advanced (a newer message replaced the old one). This avoids storing/cancelling `glib::SourceId` handles entirely.
- **File metadata on EditorPage**: `file_size: Cell<Option<u64>>` is populated during async load (from `fs::metadata`) and updated on save (from written byte count). The window pulls this on tab switch via `editor.file_size()`.
- **Window state persistence**: Window width, height, maximized state, workspace sidebar visibility, workspace sidebar width fraction, document-properties visibility, and properties sidebar width fraction are persisted via GSettings. Width/height/maximized still use `connect_notify_local` on their respective properties for incremental persistence. The workspace sidebar width key stores the selected preset hint fraction (`20%`, `30%`, `40%`), which is snapped to the nearest supported preset on restore before adaptive clamping is applied. Visibility keys now store requested desktop intent, while compact layouts may temporarily render only one secondary surface at a time without overwriting that intent. Legacy `sidebar-position` and `sidebar-visible` keys remain only as one-shot migration inputs.
- **CLI file opening**: `ApplicationImpl::open()` is overridden in `app.rs` to handle `HANDLES_OPEN`. File arguments open as tabs via `open_document()`, with window reuse for single-instance behavior.
- **Save As dialog**: `show_save_as_dialog()` in `window/dialogs.rs` uses `FileDialog::save()`. After saving, `set_file_path()` updates the path and re-detects syntax language via `reapply_language()` (gated on `size_check.syntax_enabled()` to avoid re-highlighting large files), then the tab title and status bar are refreshed.
- **Large file handling**: `load_file_async` checks `fs::metadata` size before reading. Thresholds in `services/file_limits.rs`: >1MB toast, >10MB disable syntax, >50MB disable undo, >500MB refuse. `FileSizeCheck` enum classifies sizes and provides `syntax_enabled()` / `undo_enabled()` queries. Files >50MB keep `begin_irreversible_action()` permanently open (no `end_irreversible_action()`). Files >10MB use `simdutf8` for SIMD-accelerated UTF-8 validation (`std::fs::read` + `simdutf8::basic::from_utf8` + `String::from_utf8_unchecked`) to avoid redundant scalar validation in `read_to_string`.
- **Async save**: `save_file_async` uses atomic write (unique temp file + file `sync_all()` + `rename` + parent-directory sync) on a background thread via `spawn_blocking_then`, matching the `json_store::save` durability pattern. Small/normal buffers snapshot synchronously; very large buffers (>=10MB) are snapshotted in 64k-char GTK main-loop slices. The view stays read-only for the whole save, duplicate save requests return `SaveInProgress`, and close flows are blocked while any editor is saving. The modified dot is cleared only after the durable write succeeds; on failure the previous modified state is restored. Callers pass a callback for success/error handling. This prevents UI freezes on slow filesystems (NFS, USB) and data loss on crash.
- **GSettings handler disconnect**: `EditorPage` stores `SignalHandlerId` for `connect_changed` handlers on the application-global `gio::Settings` (word wrap, style scheme). These are disconnected in `Drop`, matching the existing `dark_handler_id` pattern for `StyleManager`. Without this, handlers accumulate on tab open/close cycles, keeping stale `View`/`Buffer` refs alive.
- **Tab duplicate detection**: `LushtextWindow` maintains an `open_paths: RefCell<HashSet<PathBuf>>` for O(1) duplicate file detection in `open_document()`. Updated on tab open, close, rename, and save-as. Eliminates O(n²) during session restore with many tabs.
- **GTK4 window resizing in tests**: In GTK4, `set_default_size` does not shrink a window that has already been presented. For widget tests that verify collapse/overlay logic at specific widths, instantiate the window at the target size directly or create a fresh window for the narrow state instead of attempting to resize an existing one.
- **Load cancellation**: `EditorPage` stores an `Arc<AtomicBool>` cancel token. `cancel_load()` sets it; the background work closure checks it before and after `read_to_string`. Both `close-tab` and `close_tab_for_path` call `cancel_load()` before `close_page()`.
- **Focus restoration on overlay close**: When an overlay widget steals focus (command palette, search bar), focus must be explicitly saved before opening and restored after closing. The command palette saves `window.focus()` into a `RefCell<Option<glib::WeakRef<gtk4::Widget>>>` on `LushtextWindow` before `open()`, and `close_command_palette()` calls `restore_saved_focus()` which tries: (1) the saved widget via `WeakRef::upgrade()`, (2) the active editor's `source_view`, (3) `set_focus(None)` (empty state). The search bar always restores to its own editor's `source_view` (no saved state needed). Without explicit restoration, GTK4's default focus traversal after `GtkRevealer.set_reveal_child(false)` walks the widget tree to the first focusable widget, which is typically a sidebar button.
- **Search debounce**: Command palette uses a 150ms generation-counter debounce in `setup_search`. Each keystroke increments a `Cell<u32>`, schedules a `timeout_add_local_once`, and the callback no-ops if the counter advanced. Same pattern at 300ms for `rebuild_file_index` to coalesce rapid workspace mutations.
- **SIMD fuzzy matching**: `fuzzy_score` and `search_items` use `nucleo-matcher` (SIMD-accelerated via AVX2/NEON) instead of hand-rolled scalar scoring. `search_items` reuses a single `Matcher` and char buffer across all candidates. Top-N results use `collect` + `sort_unstable_by` + `truncate(max)` for simplicity (max=50 is fixed and small).
- **Info bar**: Per-`EditorPage` `LushtextInfoBar` widget containing two `GtkInfoBar` sub-bars (access error: red, discard/draft: yellow), all starting `revealed=false`. Placed as the first child in the EditorPage vertical box, above the editor overlay. `GtkInfoBar` is deprecated since GTK 4.10 but has no multi-button replacement — GNOME Text Editor still uses it. A type alias `GtkInfoBar = gtk4::InfoBar` with `#[allow(deprecated)]` suppresses warnings. Communication uses direct method calls (`show_*()` / `dismiss_all()`) and callback connectors (`connect_save()` / `connect_discard()` / `connect_retry()`), following the StatusBar pattern. Titles, subtitles, and action labels must remain narrow-safe by wrapping instead of disappearing, and Save/Discard widths stay balanced so restored-document banners remain usable during horizontal resize.
- **File monitoring**: Each `EditorPage` with a file path gets a `gio::FileMonitor` (started in `open_document`, cancelled on tab close/dispose). The `changed` signal is debounced with a 500ms generation counter (same pattern as sidebar persist). After debounce, the file's mtime (stored as `Cell<Option<u64>>` epoch seconds) is compared against `last_known_mtime` — only shows the "File Has Changed on Disk" bar when mtime actually differs. Mtime is updated on load success and save success to prevent our own writes from triggering the bar.
- **Draft persistence**: Unsaved buffer content is periodically written to `$XDG_DATA_HOME/lushtext/drafts/` as plain UTF-8 text files. A JSON manifest (`manifest.json`) maps draft IDs to original paths and metadata (mtime, saved_at). Draft IDs for path-backed files use `std::hash::DefaultHasher` (SipHash) → 16-char hex string; untitled tabs use `"untitled-{counter}"`. A global 5-second `glib::timeout_add_local` timer on the window scans all tabs, writing drafts only for those where `is_modified() && draft_dirty && !is_evicted()`. The `draft_dirty` flag is set by `connect_changed` (fires on every text mutation) and cleared after each draft write. On file open, the manifest is checked for an existing draft — if found, draft content replaces the buffer and the "Draft Changes Restored" bar is shown. Draft cleanup: deleted on explicit save, on discard, and on clean tab close; preserved on crash for recovery. Orphan cleanup runs at startup.
- **Save-changes dialog**: `AdwAlertDialog` matching GNOME Text Editor's design, shown on tab close (via `AdwTabView::connect_close_page`) and window close (via `WindowImpl::close_request`). Heading: "Save Changes?", body explains permanent loss. Buttons: Cancel (neutral), Discard/Discard All (destructive), Save (suggested). For multiple unsaved files, an `AdwPreferencesGroup` with `AdwActionRow` + `GtkCheckButton` per file lets the user select which to save. Unchecked files are not saved but their drafts are still deleted (same as discard). On Save, all checked files are saved via `save_file_async` with a pending-count to fire the completion callback after the last save finishes. On Discard, all drafts are deleted immediately. Tab close uses `close_page_finish(confirmed)` to complete or cancel the Adwaita close-page flow, and close requests are cancelled while a save is already in progress. Window close returns `Propagation::Stop` to inhibit and calls `window.destroy()` after confirmation. The `close-tab` action delegates entirely to `tab_view.close_page()` — cleanup (open_paths, monitor, memory) is handled by the `page_detached` handler.
- **Buffer eviction**: When total estimated buffer memory exceeds 256MB (`BUFFER_MEMORY_BUDGET`), unmodified background tabs are evicted on tab switch. Memory estimation uses `file_size * 2` to account for GtkTextBuffer overhead (B-tree + line index + undo stack). `EditorPage::evict()` sets `evicted=true` first (to prevent `modified-changed` signal flash), then clears buffer text via irreversible action. `reload_if_evicted()` transparently reloads evicted tabs when re-focused. The eviction loop computes a running total inline (O(n)) instead of rescanning all tabs after each eviction.
- **ListStore splice**: Both command palette results and file tree children use `gio::ListStore::splice()` for batch updates (single `items-changed` signal) instead of per-item `append()` loops.
- **Directory entry cap**: `workspace_section/tree_loading.rs::build_children_model` caps entries at 10,000 per directory, appends rows in 256-item batches, and shows a placeholder row when truncation occurs so very large folders do not stall the GTK thread or silently clip results.
- **File index cap**: `FileIndex::rebuild` skips well-known build/dependency directories during scanning (`IGNORED_INDEX_DIRS`: `node_modules`, `target`, `__pycache__`, `venv`, `vendor`) and truncates at 100,000 files with a warning log as a safety net. The skip list applies only to the palette file index, not the sidebar file tree.
- **Arc workspace_root**: `IndexedFile.workspace_root` uses `Arc<PathBuf>` — files in the same workspace share one allocation instead of cloning per file.
- **EditorConfig support**: Per-file formatting overrides via `.editorconfig` files. The service (`services/editorconfig.rs`) walks the directory tree from the file's parent upward, parses each `.editorconfig` with the `editorconfig-parser` crate (pure Rust, zero deps), and returns a `FormattingOverrides` struct (model layer). Resolution runs on a background thread via `spawn_blocking_then`. The `EditorPage` stores overrides in `Cell<FormattingOverrides>` and uses `apply_formatting_settings()` to resolve EditorConfig vs GSettings: override wins when `Some`, GSettings fallback when `None`. This replaces the previous `Settings::bind(GET)` for `tab-width` and `insert-spaces-instead-of-tabs` with manual `connect_changed` handlers. A `use-editorconfig` GSettings toggle (default: `true`) enables/disables the feature. The status bar shows an "EditorConfig" label when overrides are active. Supported properties: `indent_style`, `tab_width`, `indent_size`. Deferred properties documented in `docs/next/editorconfig-future.md`.
- **Bookmarks and rich notes**: Notes are available only for saved files or explicit workspace roots and persist as sidecar JSON under the app data directory. `BookmarkDocument`, `AnnotationDocument`, and `DocumentNoteDocument` use `DocumentSidecarIdentity` with a hash of the canonical path, so Save As starts a new file-backed note identity while in-app sidebar renames migrate existing sidecars. Workspace notes use a canonical-root identity instead of a transient workspace slot ID, so removing and re-adding the same root restores the same workspace note while `Replace Workspace Root` starts fresh. Open editors project bookmarks as `GtkSourceMark` gutter icons and range notes as paired `GtkTextMark` anchors plus theme-aware highlight tags. Document, workspace, and range notes all share markdown-capable edit/render surfaces, and the unified notes browser uses `AdwSidebar` sections for workspace, document, and range notes while window-level browse/export flows operate on the shared current workspace scope. Untitled tabs surface explicit feedback instead of silently creating note state.
- **Local history**: Saved files keep a separate local-history lineage under `$XDG_DATA_HOME/lushtext/local-history/`, keyed by the same canonical-path identity pattern used by note sidecars. Automatic capture records the pre-edit baseline, periodic modified-session snapshots every five minutes, and successful saves on background threads; files above 10 MB fall back to save-boundary capture only, and files above 50 MB disable local history entirely. File-backed draft restore is treated as continuity of unsaved work, so reopening a file with restored draft content does not mint a fresh baseline row for stale on-disk contents. The browser lives in `window/local_history.rs` as an adaptive `AdwDialog` + `AdwNavigationSplitView` with an `AdwSidebar` snapshot rail, opens from the main menu, command palette, `Ctrl+Alt+L`, the sidebar file context menu, and the editor content context menu, and sizes itself from the parent window so wide layouts feel like a large viewer while the preview keeps the majority of the side-by-side width. Empty-state browsing stays compact, valid empty snapshots get their own explanatory preview state instead of a blank pane, and legacy stale-disk empty baseline rows from older history may be hidden from the browser while their stored data remains on disk. Explicit inner preview padding keeps text from rendering flush against the scroll frame. Save As starts a fresh lineage, sidebar renames migrate lineages, and restore always captures a safety snapshot before replacing the buffer plus surfacing an `Undo Restore` info-bar action.
- **Benchmark framework**: Criterion.rs benchmarks in `crates/lushtext-core/benches/benchmarks.rs` cover all performance-sensitive service code (fuzzy search, file indexing, directory scanning, JSON persistence). All benchmarked functions are GTK-free. `FileIndex::from(Vec<IndexedFile>)` enables synthetic index construction without filesystem I/O. `scripts/bench-report.sh` parses Criterion JSON output into markdown for GitHub release assets. CI compile-checks benchmarks on every PR; full benchmark runs happen on release tags.
- **Content search panel**: Ctrl+Shift+F toggles `LushtextSearchPanel` (open if closed, close if open), a workspace-wide grep panel below the content stack. It searches the shared current workspace scope: one selected workspace root or the aggregate `All workspaces` scope. Uses `GtkRevealer(slide-up, 250ms)` for animated show/hide. A top `GtkSeparator` (not CSS `border-top`) provides the visual divider so the separator animates cleanly with the revealer transition. The panel is **compact when empty**: `results_scroll` starts `visible=false` (no `vexpand`) and is shown when the first match arrives, hidden again on `clear_results()`. This keeps the panel a thin strip (header + footer only) until results populate it. A close button (`window-close-symbolic`, flat + circular) sits to the right of the save button in the header for explicit panel dismissal; Escape on the search entry also closes. Ctrl+F (begin-search) and Ctrl+H (begin-replace) close the search panel first (with 260ms delay for animation completion) before opening the in-editor Find/Replace bar. The service (`services/content_search/search.rs`) spawns a background thread with `ignore::WalkParallel` (same crate powering ripgrep) + `grep-searcher`/`grep-regex` for fast parallel file searching, while `services/content_search/replace.rs` owns replace/undo flows. Results stream to the UI via `crossbeam_channel` (bounded, 1024 items), polled by a 50ms `glib::timeout_add_local`. Results are grouped by file in a two-level `GtkTreeListModel` (file → matches). Search options: case-sensitive, regex, whole-word (toggle buttons in the header), plus .gitignore toggle and glob filter in an expandable options revealer. GSettings keys: `search-panel-visible` (b), `search-panel-position` (i), `search-case-sensitive` (b), `search-regex` (b), `search-whole-word` (b), `search-panel-options-expanded` (b), `search-gitignore` (b). Match highlighting uses Pango markup (`<b>` tags) with proper escaping. Line content is truncated at 500 chars with ellipsis. Result cap: 10,000 matches (approximate under parallel walkers). Search panel integration logic extracted to `window/search.rs`.
- **Match navigation**: F4 (`win.search-next-match`) and Shift+F4 (`win.search-prev-match`) cycle through search results across files. `match_positions: RefCell<Vec<(PathBuf, u32)>>` maintains a flat navigation index in match arrival order, separate from the hierarchical `TreeListModel` display model. `current_match_index: Cell<Option<usize>>` tracks position. Navigation triggers `navigate_callback` to open the file at the matching line (shared `open_file_at_line` helper in `window/search.rs`), and visually selects the corresponding row in the `SingleSelection` model via O(n) scan + `ListView::scroll_to`. Actions are disabled when: no tabs open, search panel not visible, or no results — controlled by `update_search_navigation_actions()`. Navigation resets on new search via `clear_results()`.
- **Search progress reporting**: `SearchEvent::Progress(usize)` is emitted every 100 files by the search service via an `Arc<AtomicUsize>` file counter shared across `WalkParallel` threads. Best-effort via `try_send` to avoid blocking match delivery. In `window/search.rs`, progress display uses a 500ms delay before the notification bus starts rendering `"Searching X files…"` messages in the status bar, and a 1-second heartbeat renews the progress lease until the search completes or is cancelled.
- **Search history**: Recent searches (capped at 20) are persisted to `$XDG_DATA_HOME/lushtext/search-history.json` via `json_store` atomic write. Each `SearchHistoryEntry` captures query, case_sensitive, regex, whole_word, gitignore, and glob. History is saved on `SearchEvent::Done` when `total_matches > 0` (no-result searches are not recorded). Deduplication moves identical entries to the top. A `GtkPopover` + `GtkListBox` dropdown appears on search entry focus (created programmatically, not via template, because `GtkPopover` needs `set_parent()` not box child semantics). Each `AdwActionRow` shows the query and a compact toggle summary (`"Aa .* *.rs"`). Selecting an entry restores all state (query, toggles, glob) and triggers immediate search, using a `restoring_history: Cell<bool>` guard to suppress the redundant searches that `set_text()` and toggle `set_active()` would otherwise trigger. History is loaded at startup via `spawn_blocking_then` in `window/search.rs::setup_search_panel()`. Missing/corrupt files gracefully return empty history.
- **Saved searches**: Named saved searches are persisted permanently to `$XDG_DATA_HOME/lushtext/saved-searches.json` via `json_store` atomic write (separate file from history per architecture Decision 6). Each `SavedSearch` captures a user-given name, query, and all toggle states (case, regex, word, gitignore, glob). No cap (permanent until explicitly deleted), no dedup (user-named entries may duplicate). Service: `services/saved_searches.rs` (load, save, add, remove). UI: "Save Search" button (bookmark icon) appears in the search panel header when results exist (hidden during preview mode). Save dialog: `AdwAlertDialog` with `GtkEntry` pre-filled with query text. Dropdown popover restructured from flat `ListBox` to two-section layout: "Saved Searches" section (with delete buttons) above "Recent" history section. Selection restores all state and triggers immediate search, reusing the `restoring_history` guard. Saved searches loaded at startup via parallel `spawn_blocking_then` alongside history.
- **Multi-file Replace All**: Replace UI lives inside the `options_revealer` (behind "More" toggle). Two-phase flow: (1) user clicks "Replace All" → `enter_preview_mode()` generates `Replacement` previews via `generate_replacement_preview()` (pure function in `model/content_search.rs`), results list switches to show before/after with per-match `GtkCheckButton`; (2) user clicks "Confirm Replace" → checked replacements sent via `replace_callback` to window, which filters out modified open tabs (`skip_paths`), then calls `apply_replacements()` via `spawn_blocking_then`. Service function groups by file, sorts replacements in reverse order (last line first, rightmost match first) to avoid offset shifts, writes atomically (unique temp file + file sync + rename + parent-directory sync). Pre-replacement file bytes backed up in `HashMap<PathBuf, Vec<u8>>` for undo. After replace: status bar shows summary, open non-modified tabs auto-reload via `load_file_async()` with `last_known_mtime` updated to suppress file monitor. Undo: `undo_replacements()` restores files from backup. Backup is cleared on new search, panel close, or app exit so stale undo data cannot outlive the current close boundary. Regex mode: `regex::RegexBuilder` compiles query, `Captures::expand()` handles `$1`/`$2` backreferences.
- **Markdown preview**: `LushtextMarkdownPreview` widget uses `pulldown-cmark` (CommonMark parser) → `GtkTextTag` rendering on a read-only `GtkTextView`, with `GtkTextChildAnchor` widgets for native tables and local image blocks. Supported preview behavior includes activatable links, nested ordered/unordered list indentation, task lists, blockquotes, GitHub alert callouts, footnotes, Markdown tables, and explicit fallback states for missing or remote Markdown images. The preview lives as the end-child of `preview_paned` (a `GtkPaned` inside the "tabs" stack page). Three states: editor-only (default, preview hidden), side-by-side (`toggle-preview-pane` action, clamped to max 1/3 window width), and preview-only (Alt+P `toggle-preview-mode`, editor hidden). Animation follows the sidebar pattern: `AdwTimedAnimation` + `EaseOutCubic`, 250ms, 1px minimum. Like the sidebar, preview animations keep geometry clamps live but suppress debounced `preview-pane-position` persistence while `preview_animation_active` is true, then persist the remembered side-by-side width once the animation settles. Markdown detection uses GtkSourceView language ID (`"markdown"`). Preview refreshes on tab switch and on buffer changes (300ms debounce, generation counter). TextTags use Adwaita-matching color constants (`#1c71d8`/`#78aeed` accent, `#f6f5f4`/`#3d3846` code bg, `#5e5c64`/`#9a9996` dim), switched by `StyleManager::connect_dark_notify()`. GSettings keys: `preview-pane-position` (i), `preview-pane-visible` (b). Preview logic lives in `window/preview.rs` so the main window module stays focused on shell orchestration. Repo-owned sample content lives under `samples/`; `samples/markdown-test.md` is the canonical showcase for the Markdown preview features currently supported and should be updated when shipped preview behavior changes.

## Build Commands

```
make build       # Release build
make build-debug # Debug build
make run         # Debug build + run with temporary GNOME desktop staging
make refresh-dock-icon # Regenerate app icon assets + force a fresh GNOME Shell dock icon reload
make test        # All tests (unit + integration + widget)
make test-unit   # Unit tests only
make test-int    # Integration tests only
make test-widget # Widget tests with shared native/headless runner
make test-widget-headless # Widget tests with the CI mutter/dbus path
make check       # clippy + fmt check
make pre-commit  # repo pre-commit gate (fmt + clippy)
make install-git-hooks
make clean       # Remove build artifacts

# Benchmarks
make bench             # Run Criterion benchmarks
make bench-report      # Run + generate markdown report (short)
make bench-report-full # Run + generate markdown report (full sampling)
make bench-baseline    # Save current results as baseline
make bench-compare     # Compare against saved baseline

# Packaging
make meson-build     # Meson release build (installed layout)
make flatpak-deps    # Install Flatpak runtime/SDK deps into the user installation
make flatpak         # Build Flatpak (sets up missing runtime/SDK deps)
make flatpak-install # Build and install Flatpak into the user installation
make cargo-sources   # Regenerate cargo-sources.json
```

## Build Optimizations

Replicated from invowk-rust:

- `[profile.dev] debug = "line-tables-only"` — smaller debug info, faster linking
- `[profile.dev.package."*"] opt-level = 2` — deps compiled at O2, cached
- `[profile.dev.build-override] opt-level = 3` — build scripts at full optimization
- `[profile.release] lto = "thin", strip = true, codegen-units = 1`
- **rust-lld linker** — default on x86_64-linux since Rust 1.90, ~10x faster than BFD, zero configuration
- **cargo-hakari** workspace-hack for unified dependency features
- **cargo-nextest** auto-detected for parallel non-widget execution across the workspace; `make test` drives widget coverage through the shared headless runner for deterministic CI parity, while `make test-widget` keeps the native/auto path available for local debugging.

## Testing

- Unit tests: `#[cfg(test)]` modules across models, services, and selected GTK-free UI helper modules
- Integration tests: `crates/lushtext/tests/integration.rs` with `#[path]` split binary pattern
- Widget tests: `crates/lushtext/tests/widget.rs` uses a custom single-threaded harness so GTK tests stay on one stable thread for the life of the process; run it via `scripts/run-widget-tests.sh`, `make test-widget`, or `make test-widget-headless`, not nextest. Presented widget tests do not reliably advance `AdwTimedAnimation` frame clocks, so animation-dependent assertions should use deterministic end-state checks or a narrow `LUSHTEXT_WIDGET_CHILD` immediate-completion path.
- `TestContext` struct: isolated tempdir with simulated XDG data directory
- Non-widget tests exercise models, services, and GTK-free helper code with no display server required; widget tests cover real GTK flows and require a display server plus the custom harness

## GObject Subclassing Pattern

Every custom widget follows the gtk4-rs two-module pattern:
- `mod.rs` — public wrapper type via `glib::wrapper!`, public API methods
- `imp.rs` — private struct with `#[derive(CompositeTemplate)]`, trait impls

Required trait chain for `AdwApplicationWindow`:
`ObjectSubclass` → `ObjectImpl` → `WidgetImpl` → `WindowImpl` → `ApplicationWindowImpl` → `AdwApplicationWindowImpl`

Child widget types must be registered via `ensure_type()` in `class_init()` before template parsing.

## Dependencies

All versions centralized in workspace root `Cargo.toml` under `[workspace.dependencies]`.

**Critical version alignment:** All gtk-rs crates must be from the same release series. For the 0.11 cycle: `gtk4 = 0.11`, `libadwaita = 0.9`, `sourceview5 = 0.11`, `glib/gio/pango = 0.22`, `glib-build-tools = 0.22`.

## Meson / Flatpak Build

Meson wraps Cargo for installed/Flatpak builds. `build-aux/cargo.sh` bridges Meson → Cargo.

- **GResource dual-path**: Meson compiles and installs `.gresource` to `$(pkgdatadir)/`. `cargo.sh` exports `LUSHTEXT_PKGDATADIR` env var. `config.rs` reads it via `option_env!()`. `lib.rs` loads from installed path first, falls back to `include_bytes!` (dev).
- **GSettings**: `data/meson.build` installs schema to system path. `gnome.post_install()` compiles schemas. `build.rs` skips schema compilation when `LUSHTEXT_PKGDATADIR` is set.
- **Flatpak manifest**: `build-aux/dev.cominotti.lushtext.Flatpak.json` for local builds. `cargo-sources.json` (same dir) vendors all Cargo dependencies for offline builds.
- **CI**: `.github/workflows/ci.yml` now covers rustfmt, Clippy, the rustdoc lint gate, non-widget tests, widget tests, benchmark compilation, and `cargo deny check advisories bans sources`; `.github/workflows/flatpak.yml` still owns Flatpak build validation.

## GTK Initialization Order

CSS and Display access require GTK to be initialized. Initialization happens during `app.run()` → `startup()`. GResource registration can happen before (in `run()`), but CSS loading must happen in the `startup()` callback.

## Async I/O Pattern

Background I/O uses `services::async_task::spawn_blocking_then(state, work, then)`:
1. `state` — non-Send GTK object, wrapped in `ThreadGuard` automatically
2. `work` — `FnOnce() -> T + Send`, runs on a background thread
3. `then` — `FnOnce(state, T)`, runs on the main thread via `glib::idle_add_once`

Both `state` and `then` are wrapped in `glib::thread_guard::ThreadGuard` to safely cross thread boundaries. `ThreadGuard` implements `Send` and asserts same-thread access on `.into_inner()`.

**Concurrency guard:** `spawn_blocking_then` limits concurrent threads to `MAX_CONCURRENT_SPAWNS = 8` via a global `AtomicUsize` counter. When at limit, work is deferred via `timeout_add_local_once(50ms)` (not `idle_add_local_once`, which would busy-wait spin). This prevents RAM spikes during session restore or rapid tree expansion.

**Fire-and-forget pattern:** For tiny non-critical cleanup with no main-thread callback (for example temp file cleanup after a failed inline-create flow), raw `std::thread::spawn` is acceptable. Persistent app state writes should still go through `spawn_blocking_then` so they respect the concurrency guard and serialize correctly.

**Atomic JSON writes:** `json_store::save` uses write-to-temp + temp-file sync + `rename` + parent-directory sync for crash-safe persistence on Linux filesystems. The file is either fully old or fully new, never partially written, and the renamed directory entry is durable before success is reported.

**Key constraint:** GTK objects are NOT `Send`/`Sync` (raw pointers inside). Never pass them directly across threads. Always use `ThreadGuard` or `SendWeakRef`.

**TreeListModel caveat:** Never set `autoexpand = true` on `TreeListModel` — it recursively calls the child-model callback for every directory, which with background I/O spawns unbounded threads, and with synchronous I/O freezes the UI.

## Critical Rule: Pre-existing Blockers

If implementation or verification reveals a pre-existing blocker, fix it in the same work stream instead of deferring around it or treating it as out of scope.

This rule is mandatory and has no exceptions.

## Active Technologies
- Rust 1.95.0 (Edition 2024) + GTK4 0.11, Libadwaita 0.9, GtkSourceView 5 0.11, gio/glib/pango 0.22, existing `spawn_blocking_then` background executor (001-file-peek)
- Local workspace files for read-only snapshot reads; transient in-memory peek state only; no new XDG, draft, session, or GSettings persistence (001-file-peek)

## Recent Changes
- 001-file-peek: Added Rust 1.95.0 (Edition 2024) + GTK4 0.11, Libadwaita 0.9, GtkSourceView 5 0.11, gio/glib/pango 0.22, existing `spawn_blocking_then` background executor
