# LushText

A minimalist, fast text editor targeting Libadwaita. Looks similar to GNOME Text Editor but with a left-side file tree pane and workspace support.

## Tech Stack

- **Language:** Rust (MSRV: 1.94.1, Edition 2024)
- **GUI:** GTK4 (0.11) + Libadwaita (0.9) + GtkSourceView 5 (0.11)
- **Config:** GSettings (`data/dev.cominotti.lushtext.gschema.xml`)
- **Build:** Cargo workspace + Makefile (dev), Meson (Flatpak/installed)
- **App ID:** `dev.cominotti.lushtext`
- **License:** GPL-3.0-or-later

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
│   ├── workspace.rs    # WorkspaceId, WorkspaceEntry, WorkspaceConfig, WorkspacesFile
│   ├── session.rs      # SessionTab, SessionData — global session for tab restore
│   ├── palette.rs      # IndexedFile, CommandDef, CommandCategory, SearchMode, ScoredResult
│   ├── draft.rs        # DraftEntry, DraftManifest — draft persistence metadata
│   └── formatting_overrides.rs  # FormattingOverrides — per-file EditorConfig overrides
├── services/           # Business logic
│   ├── async_task.rs   # spawn_blocking_then, MAX_CONCURRENT_SPAWNS, concurrency guard
│   ├── json_store.rs   # Generic JSON load/save + data_dir()
│   ├── workspace_manager.rs
│   ├── session_service.rs
│   ├── file_tree.rs    # Directory scanning (pure I/O, bounded/cancellable helpers for sidebar)
│   ├── file_limits.rs  # File size thresholds for graceful degradation
│   ├── palette.rs      # Fuzzy matching (nucleo SIMD), file indexing, command registry
│   ├── draft_service.rs # Draft persistence: save/load/delete draft files and manifest
│   └── editorconfig.rs  # .editorconfig file discovery and parsing (pure I/O, no GTK)
├── benches/
│   └── benchmarks.rs   # Criterion benchmarks for all performance-sensitive services
└── ui/                 # GTK4/Libadwaita widgets (each has mod.rs + imp.rs)
    ├── window/          # Main window: HeaderBar, TabBar, Paned, Stack, StatusBar
    │   ├── dialogs.rs   # File dialogs: open file, open folder, save as
    │   └── preview.rs   # Markdown preview pane: side-by-side + Alt+P toggle modes
    ├── editor_page/     # GtkSourceView + search bar revealer
    ├── sidebar/         # Multi-workspace sidebar orchestrator
    │   ├── file_tree_item.rs       # GObject wrapper for tree entries
    │   └── workspace_section/      # Per-workspace section widget (header + file tree)
    ├── markdown_preview/ # Read-only Markdown preview (pulldown-cmark → TextTags)
    ├── info_bar/        # Contextual warning/error bars (GtkInfoBar) above editor
    ├── command_palette/  # Ctrl+P fuzzy search: files + commands
    │   └── item.rs      # PaletteItem GObject wrapper for ListStore
    ├── search_bar/      # Find/replace widget
    ├── status_bar/      # Bottom bar: feedback messages + file metadata
    └── preferences/     # AdwPreferencesDialog
```

### Key Design Decisions

- **GtkSourceView owns editing**: language detection, syntax highlighting, style schemes, and undo/redo are all delegated to GtkSourceView, not reimplemented.
- **GSettings for preferences**: Editor settings (word wrap, tab width, line numbers, etc.) are stored in GSettings (`dev.cominotti.lushtext` schema). `gio::Settings::bind()` creates two-way bindings between settings keys and widget properties. Each `EditorPage` binds its own source view to GSettings in `constructed()` — when a setting changes, all editors update automatically with no manual iteration.
- **Dark mode**: GtkSourceView has its own style scheme system separate from GTK CSS. The base scheme ID is read from GSettings (`style-scheme` key) and the dark variant (e.g., `"Adwaita-dark"`) is selected automatically based on `libadwaita::StyleManager::is_dark()`, with live switching via `connect_dark_notify()`.
- **Multi-workspace sidebar**: The sidebar is a two-level orchestrator. `LushtextSidebar` manages a scrollable list of `LushtextWorkspaceSection` widgets plus a fixed "New Workspace" footer. Each section has its own `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` file tree. The footer is always visible below the scroll area. Clicking footer's add-folder button creates a new workspace (auto-named from folder). Each section has its own add-folder button for adding more roots. Right-click on workspace header shows Rename/Unlist context menu.
- **Inner ScrolledWindow pattern**: Each `WorkspaceSection` wraps its `GtkListView` in a `GtkScrolledWindow(propagate-natural-height=true, vscrollbar-policy=never)`. This provides the vadjustment that ListView requires while propagating natural height so the outer `GtkScrolledWindow` on the sidebar handles all scrolling. Without this, GtkListView may crash or emit warnings.
- **Workspace persistence**: `WorkspacesFile` (model) is loaded from `workspaces.json` via `workspace_manager::load()` on window construction. Sidebar mutations mark persistence dirty, debounce for 150ms, and save via `spawn_blocking_then`, with in-flight serialization so older snapshots cannot overwrite newer ones. `WorkspacesFile` derives `Clone` to enable cloning out of `RefCell` for background save work. The sidebar owns the `WorkspacesFile` state.
- **File tree uses modern GTK4 model**: `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` (NOT the deprecated `GtkTreeView`). File tree labels use the `.monospace` CSS class, sharing the editor's font customization provider.
- **File context menu**: Per-`WorkspaceSection`. Right-click on a file or directory shows a `GtkPopoverMenu` with New File, New Folder, Rename, and Delete actions. Uses `Widget::pick()` + ancestor traversal to find the `TreeExpander` → `TreeListRow` → `FileTreeItem`. Actions are in a `section` action group on each section widget.
- **Workspace header context menu**: Per-`WorkspaceSection`. Right-click on the workspace header shows a `GtkPopoverMenu` with Rename Workspace and Unlist Workspace. Rename shows an `AdwAlertDialog` with text entry. Unlist shows a confirmation dialog. Actions are in a `ws-header` action group.
- **Inline rename**: Rename swaps the row's `GtkLabel` for a `GtkEntry` dynamically. Enter confirms (removes entry immediately, then `std::fs::rename` runs on a background thread via `spawn_blocking_then`; on success the `FileTreeItem` path and label are updated on the main thread), Escape and focus-out cancel. A guard (`entry.parent().is_none()`) prevents double-fire from focus-out after confirm/cancel removes the entry. The window is notified via `connect_file_renamed` callback for tab path updates.
- **Delete with confirmation**: Delete shows an `AdwAlertDialog` with a destructive "Delete" response. On confirm, `std::fs::remove_file` or `std::fs::remove_dir_all` runs on a background thread via `spawn_blocking_then`; on success the item is removed from the parent `ListStore` and the window is notified via `connect_file_deleted` callback to close affected tabs. Directory operations use `Path::starts_with` for prefix matching (closing all tabs inside deleted/renamed directories).
- **New File / New Folder**: Context menu "New File" and "New Folder" actions use `spawn_blocking_then` for the `create_unique` filesystem call (avoids blocking the UI on slow filesystems or name collision retries), then add a `FileTreeItem` with `pending_rename = true` to the target `ListStore`. `connect_bind` detects the flag to automatically trigger inline rename. On confirm, the temp file is renamed to the user's chosen name and `connect_file_created` opens files in tabs. On cancel, the temp file is deleted via fire-and-forget `std::thread::spawn` and removed from the model. The `build_children_model` callback deduplicates items already in the store to prevent duplicates when an expanded directory's async scan finds the temp file.
- **Workspace concept**: a named collection of root directories/files, persisted to `$XDG_DATA_HOME/lushtext/workspaces.json`. Methods: `add_workspace()`, `remove_workspace()`, `rename_workspace()`, `add_entry()`, `remove_entry()`.
- **Session persistence**: All open tabs (with cursor position and scroll offset) are saved to a single global `$XDG_DATA_HOME/lushtext/session.json`. Tabs are not workspace-scoped in the UI — they all share one `AdwTabView` — so a single session file captures everything. Session save is debounced at 500ms (generation-counter pattern) and triggered by tab open, close, switch, and detach. A synchronous save runs on `close_request` as a safety net. On startup, `load_session_and_drafts` combines draft manifest + session loading in one background task (via `spawn_blocking_then`), then `restore_tabs` opens file-backed tabs via `open_document` and untitled tabs via `new_tab` with draft recovery. Cursor/scroll positions are deferred via `set_restore_position` → `apply_restore_position` (called in `load_file_async`'s success callback after content is loaded). The `restoring_session` flag suppresses redundant session saves during restore. CLI file arguments (`ApplicationImpl::open`) take priority over session's active tab selection.
- **Status bar**: per-window bottom bar below `GtkPaned`, always visible. Four sections: sidebar toggle button (far left), feedback message area (left, hexpand), encoding label (right), file size label (right). The window orchestrates all updates via `refresh_status_bar()`, called from `new_tab()`, `open_document()`, `close-tab` action, `save` action, and the `selected-page` notify handler.
- **Sidebar toggle**: `GtkToggleButton` in the status bar (icon: `sidebar-show-symbolic`) with `action-name=win.toggle-sidebar` (stateful boolean action via `SimpleAction::new_stateful` + `connect_change_state`). F9 keyboard shortcut. Uses `animate_sidebar(bool)` for a smooth slide animation via `AdwTimedAnimation` + `CallbackAnimationTarget` on the paned's position property (250ms, `EaseOutCubic`). Hide animates position to 1px (not 0 — zero-width allocations trigger pixman "Invalid rectangle" warnings), then `connect_done` calls `sidebar.set_visible(false)` for the final snap. Show calls `sidebar.set_visible(true)` then animates from current position to `saved_sidebar_pos`. `shrink-start-child` is temporarily set to `true` during animation (so the divider can pass the sidebar's minimum width) and restored to `false` in `connect_done`. Rapid toggle is handled by pausing the in-flight animation and starting fresh from `paned.position()`. `saved_sidebar_pos: Cell<i32>` stores the pre-hide position (only overwritten when no animation is running, to avoid saving intermediate values). Visibility persisted via GSettings `sidebar-visible` key (default: `true`). A cached `sidebar_visible: Cell<bool>` on the window imp struct avoids GObject property lookups in the `clamp_sidebar_position` hot path (~60Hz during resize). The clamp function early-returns when the cache is `false` to prevent persisting stale position values.
- **Status bar auto-dismiss**: messages auto-dismiss after 5 seconds using a generation counter (`Cell<u32>`). Each `push_message` increments the counter; the timer closure captures the value and no-ops if the counter has advanced (a newer message replaced the old one). This avoids storing/cancelling `glib::SourceId` handles entirely.
- **File metadata on EditorPage**: `file_size: Cell<Option<u64>>` is populated during async load (from `fs::metadata`) and updated on save (from written byte count). The window pulls this on tab switch via `editor.file_size()`.
- **Window state persistence**: Window width, height, maximized state, sidebar position, and sidebar visibility are persisted via GSettings (`window-width`, `window-height`, `window-maximized`, `sidebar-position`, `sidebar-visible` keys). Width/height/maximized use `connect_notify_local` on their respective properties for incremental persistence — no `close_request` override needed. On construction, `set_default_size()` and `maximize()` restore from GSettings before `present()`.
- **Sidebar position constraint**: The GtkPaned sidebar position is clamped to `min(window_width / 3, window_width - stack_min - 16)` via two mechanisms: (1) `WidgetImpl::size_allocate()` override clamps on every allocation — this is the primary constraint, using the definitive allocated width parameter (not a potentially stale `window.width()` read); (2) `notify::position` handler clamps when the user drags the sidebar. The `stack_min` term queries `content_stack.measure(Horizontal, -1)` to prevent the sidebar from squeezing the content stack below its minimum (driven by `AdwStatusPage` at ~415px when `hhomogeneous=true`). The 16px buffer covers the GtkPaned handle/separator with margin for theme variance. **Critical ordering:** the `size_allocate` clamp runs BEFORE `parent_size_allocate` so the paned position is already correct when GTK measures the children — prevents "Trying to measure GtkStack for width of X, but it needs at least Y" warnings. A `width-request=640` on the window template prevents geometrically impossible layouts (`sidebar_min + handle + stack_min > window_width`). Guards: `window_width <= 0` for pre-realization safety, `sidebar_visible.get() == false` to skip clamping when hidden, `.max(0)` to prevent negative positions.
- **CLI file opening**: `ApplicationImpl::open()` is overridden in `app.rs` to handle `HANDLES_OPEN`. File arguments open as tabs via `open_document()`, with window reuse for single-instance behavior.
- **Save As dialog**: `show_save_as_dialog()` in `window/dialogs.rs` uses `FileDialog::save()`. After saving, `set_file_path()` updates the path and re-detects syntax language via `reapply_language()` (gated on `size_check.syntax_enabled()` to avoid re-highlighting large files), then the tab title and status bar are refreshed.
- **Large file handling**: `load_file_async` checks `fs::metadata` size before reading. Thresholds in `services/file_limits.rs`: >1MB toast, >10MB disable syntax, >50MB disable undo, >500MB refuse. `FileSizeCheck` enum classifies sizes and provides `syntax_enabled()` / `undo_enabled()` queries. Files >50MB keep `begin_irreversible_action()` permanently open (no `end_irreversible_action()`). Files >10MB use `simdutf8` for SIMD-accelerated UTF-8 validation (`std::fs::read` + `simdutf8::basic::from_utf8` + `String::from_utf8_unchecked`) to avoid redundant scalar validation in `read_to_string`.
- **Async save**: `save_file_async` uses atomic write (temp file + `rename`) on a background thread via `spawn_blocking_then`, matching the `json_store::save` pattern. Small/normal buffers snapshot synchronously; very large buffers (>=10MB) are snapshotted in 64k-char GTK main-loop slices with the view temporarily made read-only so the saved text stays consistent without a long single-frame pause. The modified dot is cleared right before the background write; on write failure it is restored via `set_modified(true)`. Callers pass a callback for success/error handling. This prevents UI freezes on slow filesystems (NFS, USB) and data loss on crash.
- **GSettings handler disconnect**: `EditorPage` stores `SignalHandlerId` for `connect_changed` handlers on the application-global `gio::Settings` (word wrap, style scheme). These are disconnected in `Drop`, matching the existing `dark_handler_id` pattern for `StyleManager`. Without this, handlers accumulate on tab open/close cycles, keeping stale `View`/`Buffer` refs alive.
- **Tab duplicate detection**: `LushtextWindow` maintains an `open_paths: RefCell<HashSet<PathBuf>>` for O(1) duplicate file detection in `open_document()`. Updated on tab open, close, rename, and save-as. Eliminates O(n²) during session restore with many tabs.
- **Load cancellation**: `EditorPage` stores an `Arc<AtomicBool>` cancel token. `cancel_load()` sets it; the background work closure checks it before and after `read_to_string`. Both `close-tab` and `close_tab_for_path` call `cancel_load()` before `close_page()`.
- **Focus restoration on overlay close**: When an overlay widget steals focus (command palette, search bar), focus must be explicitly saved before opening and restored after closing. The command palette saves `window.focus()` into a `RefCell<Option<glib::WeakRef<gtk4::Widget>>>` on `LushtextWindow` before `open()`, and `close_command_palette()` calls `restore_saved_focus()` which tries: (1) the saved widget via `WeakRef::upgrade()`, (2) the active editor's `source_view`, (3) `set_focus(None)` (empty state). The search bar always restores to its own editor's `source_view` (no saved state needed). Without explicit restoration, GTK4's default focus traversal after `GtkRevealer.set_reveal_child(false)` walks the widget tree to the first focusable widget, which is typically a sidebar button.
- **Search debounce**: Command palette uses a 150ms generation-counter debounce in `setup_search`. Each keystroke increments a `Cell<u32>`, schedules a `timeout_add_local_once`, and the callback no-ops if the counter advanced. Same pattern at 300ms for `rebuild_file_index` to coalesce rapid workspace mutations.
- **SIMD fuzzy matching**: `fuzzy_score` and `search_items` use `nucleo-matcher` (SIMD-accelerated via AVX2/NEON) instead of hand-rolled scalar scoring. `search_items` reuses a single `Matcher` and char buffer across all candidates. Top-N results use `collect` + `sort_unstable_by` + `truncate(max)` for simplicity (max=50 is fixed and small).
- **Info bar**: Per-`EditorPage` `LushtextInfoBar` widget containing two `GtkInfoBar` sub-bars (access error: red, discard/draft: yellow), all starting `revealed=false`. Placed as the first child in the EditorPage vertical box, above the editor overlay. `GtkInfoBar` is deprecated since GTK 4.10 but has no multi-button replacement — GNOME Text Editor still uses it. A type alias `GtkInfoBar = gtk4::InfoBar` with `#[allow(deprecated)]` suppresses warnings. Communication uses direct method calls (`show_*()` / `dismiss_all()`) and callback connectors (`connect_save()` / `connect_discard()` / `connect_retry()`), following the StatusBar pattern.
- **File monitoring**: Each `EditorPage` with a file path gets a `gio::FileMonitor` (started in `open_document`, cancelled on tab close/dispose). The `changed` signal is debounced with a 500ms generation counter (same pattern as sidebar persist). After debounce, the file's mtime (stored as `Cell<Option<u64>>` epoch seconds) is compared against `last_known_mtime` — only shows the "File Has Changed on Disk" bar when mtime actually differs. Mtime is updated on load success and save success to prevent our own writes from triggering the bar.
- **Draft persistence**: Unsaved buffer content is periodically written to `$XDG_DATA_HOME/lushtext/drafts/` as plain UTF-8 text files. A JSON manifest (`manifest.json`) maps draft IDs to original paths and metadata (mtime, saved_at). Draft IDs for path-backed files use `std::hash::DefaultHasher` (SipHash) → 16-char hex string; untitled tabs use `"untitled-{counter}"`. A global 30-second `glib::timeout_add_local` timer on the window scans all tabs, writing drafts only for those where `is_modified() && draft_dirty && !is_evicted()`. The `draft_dirty` flag is set in `wire_modified_indicator` and cleared after each draft write. On file open, the manifest is checked for an existing draft — if found, draft content replaces the buffer and the "Draft Changes Restored" bar is shown. Draft cleanup: deleted on explicit save, on discard, and on clean tab close; preserved on crash for recovery. Orphan cleanup runs at startup.
- **Save-changes dialog**: `AdwAlertDialog` matching GNOME Text Editor's design, shown on tab close (via `AdwTabView::connect_close_page`) and window close (via `WindowImpl::close_request`). Heading: "Save Changes?", body explains permanent loss. Buttons: Cancel (neutral), Discard/Discard All (destructive), Save (suggested). For multiple unsaved files, an `AdwPreferencesGroup` with `AdwActionRow` + `GtkCheckButton` per file lets the user select which to save. Unchecked files are not saved but their drafts are still deleted (same as discard). On Save, all checked files are saved via `save_file_async` with a pending-count to fire the completion callback after the last save finishes. On Discard, all drafts are deleted immediately. Tab close uses `close_page_finish(confirmed)` to complete or cancel the Adwaita close-page flow. Window close returns `Propagation::Stop` to inhibit and calls `window.destroy()` after confirmation. The `close-tab` action delegates entirely to `tab_view.close_page()` — cleanup (open_paths, monitor, memory) is handled by the `page_detached` handler.
- **Buffer eviction**: When total estimated buffer memory exceeds 256MB (`BUFFER_MEMORY_BUDGET`), unmodified background tabs are evicted on tab switch. Memory estimation uses `file_size * 2` to account for GtkTextBuffer overhead (B-tree + line index + undo stack). `EditorPage::evict()` sets `evicted=true` first (to prevent `modified-changed` signal flash), then clears buffer text via irreversible action. `reload_if_evicted()` transparently reloads evicted tabs when re-focused. The eviction loop computes a running total inline (O(n)) instead of rescanning all tabs after each eviction.
- **ListStore splice**: Both command palette results and file tree children use `gio::ListStore::splice()` for batch updates (single `items-changed` signal) instead of per-item `append()` loops.
- **Directory entry cap**: `build_children_model` caps entries at 10,000 per directory, appends rows in 256-item batches, and shows a placeholder row when truncation occurs so very large folders do not stall the GTK thread or silently clip results.
- **File index cap**: `FileIndex::rebuild` skips well-known build/dependency directories during scanning (`IGNORED_INDEX_DIRS`: `node_modules`, `target`, `__pycache__`, `venv`, `vendor`) and truncates at 100,000 files with a warning log as a safety net. The skip list applies only to the palette file index, not the sidebar file tree.
- **Arc workspace_root**: `IndexedFile.workspace_root` uses `Arc<PathBuf>` — files in the same workspace share one allocation instead of cloning per file.
- **EditorConfig support**: Per-file formatting overrides via `.editorconfig` files. The service (`services/editorconfig.rs`) walks the directory tree from the file's parent upward, parses each `.editorconfig` with the `editorconfig-parser` crate (pure Rust, zero deps), and returns a `FormattingOverrides` struct (model layer). Resolution runs on a background thread via `spawn_blocking_then`. The `EditorPage` stores overrides in `Cell<FormattingOverrides>` and uses `apply_formatting_settings()` to resolve EditorConfig vs GSettings: override wins when `Some`, GSettings fallback when `None`. This replaces the previous `Settings::bind(GET)` for `tab-width` and `insert-spaces-instead-of-tabs` with manual `connect_changed` handlers. A `use-editorconfig` GSettings toggle (default: `true`) enables/disables the feature. The status bar shows an "EditorConfig" label when overrides are active. Supported properties: `indent_style`, `tab_width`, `indent_size`. Deferred properties documented in `docs/next/editorconfig-future.md`.
- **Benchmark framework**: Criterion.rs benchmarks in `crates/lushtext-core/benches/benchmarks.rs` cover all performance-sensitive service code (fuzzy search, file indexing, directory scanning, JSON persistence). All benchmarked functions are GTK-free. `FileIndex::from(Vec<IndexedFile>)` enables synthetic index construction without filesystem I/O. `scripts/bench-report.sh` parses Criterion JSON output into markdown for GitHub release assets. CI compile-checks benchmarks on every PR; full benchmark runs happen on release tags.
- **Markdown preview**: `LushtextMarkdownPreview` widget uses `pulldown-cmark` (CommonMark parser) → `GtkTextTag` rendering on a read-only `GtkTextView`. The preview lives as the end-child of `preview_paned` (a `GtkPaned` inside the "tabs" stack page). Three states: editor-only (default, preview hidden), side-by-side (`toggle-preview-pane` action, clamped to max 1/3 window width), and preview-only (Alt+P `toggle-preview-mode`, editor hidden). Animation follows the sidebar pattern: `AdwTimedAnimation` + `EaseOutCubic`, 250ms, 1px minimum. Markdown detection uses GtkSourceView language ID (`"markdown"`). Preview refreshes on tab switch and on buffer changes (300ms debounce, generation counter). TextTags use Adwaita-matching color constants (`#1c71d8`/`#78aeed` accent, `#f6f5f4`/`#3d3846` code bg, `#5e5c64`/`#9a9996` dim), switched by `StyleManager::connect_dark_notify()`. GSettings keys: `preview-pane-position` (i), `preview-pane-visible` (b). Preview logic extracted to `window/preview.rs` to stay under the 1000-line file limit.

## Build Commands

```
make build       # Release build
make build-debug # Debug build
make run         # Debug build + run
make test        # All tests (unit + integration + widget)
make test-unit   # Unit tests only
make test-int    # Integration tests only
make test-widget # Widget tests only (requires display server)
make check       # clippy + fmt check
make clean       # Remove build artifacts

# Benchmarks
make bench             # Run Criterion benchmarks
make bench-report      # Run + generate markdown report (short)
make bench-report-full # Run + generate markdown report (full sampling)
make bench-baseline    # Save current results as baseline
make bench-compare     # Compare against saved baseline

# Packaging
make meson-build     # Meson release build (installed layout)
make flatpak         # Build Flatpak (needs flatpak-builder)
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
- **cargo-nextest** auto-detected for parallel test execution

## Testing

- Unit tests: `#[cfg(test)]` modules inside `workspace_manager.rs` and `session_service.rs`
- Integration tests: `crates/lushtext/tests/integration.rs` with `#[path]` split binary pattern
- `TestContext` struct: isolated tempdir with simulated XDG data directory
- Tests exercise services only (no display server needed)

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
- **CI**: `.github/workflows/ci.yml` (Cargo check/test) and `.github/workflows/flatpak.yml` (Flatpak build).

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

**Atomic JSON writes:** `json_store::save` uses write-to-temp + `rename` for crash-safe persistence. The file is either fully old or fully new, never partially written.

**Key constraint:** GTK objects are NOT `Send`/`Sync` (raw pointers inside). Never pass them directly across threads. Always use `ThreadGuard` or `SendWeakRef`.

**TreeListModel caveat:** Never set `autoexpand = true` on `TreeListModel` — it recursively calls the child-model callback for every directory, which with background I/O spawns unbounded threads, and with synchronous I/O freezes the UI.
