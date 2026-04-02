# LushText

A minimalist, fast text editor targeting Libadwaita. Looks similar to GNOME Text Editor but with a left-side file tree pane and workspace support.

## Tech Stack

- **Language:** Rust (MSRV: 1.83+)
- **GUI:** GTK4 (0.11) + Libadwaita (0.9) + GtkSourceView 5 (0.11)
- **Config:** GSettings (`data/dev.cominotti.lushtext.gschema.xml`)
- **Build:** Cargo workspace + Makefile (dev), Meson (Flatpak — planned)
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
├── config.rs           # Compile-time constants (APP_ID, VERSION)
├── lib.rs              # Entry point: GResource registration, CSS loading, GSettings schema dir, app.run()
├── model/              # Domain types (no GTK deps)
│   ├── workspace.rs    # WorkspaceId, WorkspaceEntry, WorkspaceConfig, WorkspacesFile
│   └── session.rs      # SessionTab, SessionData
├── services/           # Business logic
│   ├── json_store.rs   # Generic JSON load/save + data_dir()
│   ├── workspace_manager.rs
│   ├── session_service.rs
│   └── file_tree.rs    # Directory scanning (pure I/O, returns Vec<(PathBuf, bool)>)
└── ui/                 # GTK4/Libadwaita widgets (each has mod.rs + imp.rs)
    ├── window/          # Main window: HeaderBar, TabBar, Paned, Stack, StatusBar
    │   └── dialogs.rs   # File dialogs: open file, open folder, save as
    ├── editor_page/     # GtkSourceView + search bar revealer
    ├── sidebar/         # Multi-workspace sidebar orchestrator
    │   ├── file_tree_item.rs       # GObject wrapper for tree entries
    │   └── workspace_section/      # Per-workspace section widget (header + file tree)
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
- **Workspace persistence**: `WorkspacesFile` (model) is loaded from `workspaces.json` via `workspace_manager::load()` on window construction. Every mutation (add workspace, add folder, rename, unlist) saves immediately via `workspace_manager::save()`. The sidebar owns the `WorkspacesFile` state.
- **File tree uses modern GTK4 model**: `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` (NOT the deprecated `GtkTreeView`). File tree labels use the `.monospace` CSS class, sharing the editor's font customization provider.
- **File context menu**: Per-`WorkspaceSection`. Right-click on a file or directory shows a `GtkPopoverMenu` with New File, New Folder, Rename, and Delete actions. Uses `Widget::pick()` + ancestor traversal to find the `TreeExpander` → `TreeListRow` → `FileTreeItem`. Actions are in a `section` action group on each section widget.
- **Workspace header context menu**: Per-`WorkspaceSection`. Right-click on the workspace header shows a `GtkPopoverMenu` with Rename Workspace and Unlist Workspace. Rename shows an `AdwAlertDialog` with text entry. Unlist shows a confirmation dialog. Actions are in a `ws-header` action group.
- **Inline rename**: Rename swaps the row's `GtkLabel` for a `GtkEntry` dynamically. Enter confirms (removes entry immediately, then `std::fs::rename` runs on a background thread via `spawn_blocking_then`; on success the `FileTreeItem` path and label are updated on the main thread), Escape and focus-out cancel. A guard (`entry.parent().is_none()`) prevents double-fire from focus-out after confirm/cancel removes the entry. The window is notified via `connect_file_renamed` callback for tab path updates.
- **Delete with confirmation**: Delete shows an `AdwAlertDialog` with a destructive "Delete" response. On confirm, `std::fs::remove_file` or `std::fs::remove_dir_all` runs on a background thread via `spawn_blocking_then`; on success the item is removed from the parent `ListStore` and the window is notified via `connect_file_deleted` callback to close affected tabs. Directory operations use `Path::starts_with` for prefix matching (closing all tabs inside deleted/renamed directories).
- **New File / New Folder**: Context menu "New File" and "New Folder" actions create a temp-named item on disk, add a `FileTreeItem` with `pending_rename = true` to the target `ListStore`, and `connect_bind` detects the flag to automatically trigger inline rename. On confirm, the temp file is renamed to the user's chosen name and `connect_file_created` opens files in tabs. On cancel, the temp file is deleted and removed from the model. The `build_children_model` callback deduplicates items already in the store to prevent duplicates when an expanded directory's async scan finds the temp file.
- **Workspace concept**: a named collection of root directories/files, persisted to `$XDG_DATA_HOME/lushtext/workspaces.json`. Methods: `add_workspace()`, `remove_workspace()`, `rename_workspace()`, `add_entry()`, `remove_entry()`.
- **Session persistence**: open tabs per workspace, persisted to `$XDG_DATA_HOME/lushtext/session-{id}.json`.
- **Status bar**: per-window bottom bar below `GtkPaned`, always visible. Three sections: feedback message area (left), encoding label (right), file size label (right). The window orchestrates all updates via `refresh_status_bar()`, called from `new_tab()`, `open_document()`, `close-tab` action, `save` action, and the `selected-page` notify handler.
- **Status bar auto-dismiss**: messages auto-dismiss after 5 seconds using a generation counter (`Cell<u32>`). Each `push_message` increments the counter; the timer closure captures the value and no-ops if the counter has advanced (a newer message replaced the old one). This avoids storing/cancelling `glib::SourceId` handles entirely.
- **File metadata on EditorPage**: `file_size: Cell<Option<u64>>` is populated during async load (from `fs::metadata`) and updated on save (from written byte count). The window pulls this on tab switch via `editor.file_size()`.
- **Window state persistence**: Window width, height, maximized state, and sidebar position are persisted via GSettings (`window-width`, `window-height`, `window-maximized`, `sidebar-position` keys). Width/height/maximized use `connect_notify_local` on their respective properties for incremental persistence — no `close_request` override needed. On construction, `set_default_size()` and `maximize()` restore from GSettings before `present()`.
- **Sidebar 1/3 max width constraint**: The GtkPaned sidebar position is clamped to `window_width / 3` via two mechanisms: (1) `WidgetImpl::size_allocate()` override clamps on every allocation — this is the primary constraint, using the definitive allocated width parameter (not a potentially stale `window.width()` read); (2) `notify::position` handler clamps when the user drags the sidebar. This dual approach avoids timing bugs where property notifications (`notify::default-width`, `notify::maximized`) fire before the new allocation is applied. The `clamp_sidebar_position` free function guards with `window_width <= 0` for pre-realization safety.
- **CLI file opening**: `ApplicationImpl::open()` is overridden in `app.rs` to handle `HANDLES_OPEN`. File arguments open as tabs via `open_document()`, with window reuse for single-instance behavior.
- **Save As dialog**: `show_save_as_dialog()` in `window/dialogs.rs` uses `FileDialog::save()`. After saving, `set_file_path()` updates the path and re-detects syntax language via `reapply_language()`, then the tab title and status bar are refreshed.

## Build Commands

```
make build       # Release build
make build-debug # Debug build
make run         # Debug build + run
make test        # All tests (unit + integration)
make test-unit   # Unit tests only
make test-int    # Integration tests only
make check       # clippy + fmt check
```

## Build Optimizations

Replicated from invowk-rust:

- `[profile.dev] debug = "line-tables-only"` — smaller debug info, faster linking
- `[profile.dev.package."*"] opt-level = 2` — deps compiled at O2, cached
- `[profile.dev.build-override] opt-level = 3` — build scripts at full optimization
- `[profile.release] lto = "thin", strip = true, codegen-units = 1`
- **mold linker** via Makefile RUSTFLAGS (not .cargo/config.toml, so builds don't fail without mold)
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

## GTK Initialization Order

CSS and Display access require GTK to be initialized. Initialization happens during `app.run()` → `startup()`. GResource registration can happen before (in `run()`), but CSS loading must happen in the `startup()` callback.

## Async I/O Pattern

Background I/O uses `services::async_task::spawn_blocking_then(state, work, then)`:
1. `state` — non-Send GTK object, wrapped in `ThreadGuard` automatically
2. `work` — `FnOnce() -> T + Send`, runs on a background thread
3. `then` — `FnOnce(state, T)`, runs on the main thread via `glib::idle_add_once`

Both `state` and `then` are wrapped in `glib::thread_guard::ThreadGuard` to safely cross thread boundaries. `ThreadGuard` implements `Send` and asserts same-thread access on `.into_inner()`.

**Key constraint:** GTK objects are NOT `Send`/`Sync` (raw pointers inside). Never pass them directly across threads. Always use `ThreadGuard` or `SendWeakRef`.

**TreeListModel caveat:** Never set `autoexpand = true` on `TreeListModel` — it recursively calls the child-model callback for every directory, which with background I/O spawns unbounded threads, and with synchronous I/O freezes the UI.
