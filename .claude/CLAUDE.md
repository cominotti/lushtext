# LushText

A minimalist, fast text editor targeting Libadwaita. Looks similar to GNOME Text Editor but with a left-side file tree pane and workspace support.

## Tech Stack

- **Language:** Rust (MSRV: 1.83+)
- **GUI:** GTK4 (0.11) + Libadwaita (0.9) + GtkSourceView 5 (0.11)
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
├── lib.rs              # Entry point: GResource registration, CSS loading, app.run()
├── model/              # Domain types (no GTK deps)
│   ├── workspace.rs    # WorkspaceId, WorkspaceEntry, WorkspaceConfig, WorkspacesFile
│   ├── document.rs     # DocumentId
│   └── session.rs      # SessionTab, SessionData
├── services/           # Business logic
│   ├── json_store.rs   # Generic JSON load/save + data_dir()
│   ├── workspace_manager.rs
│   ├── session_service.rs
│   └── file_tree.rs    # Builds GListModel hierarchy for sidebar
└── ui/                 # GTK4/Libadwaita widgets (each has mod.rs + imp.rs)
    ├── window/          # Main window: HeaderBar, TabBar, Paned, Stack, StatusBar
    ├── editor_page/     # GtkSourceView + search bar revealer
    ├── sidebar/         # File tree: ListView + TreeListModel + TreeExpander
    │   └── file_tree_item.rs  # GObject wrapper for tree entries
    ├── search_bar/      # Find/replace widget
    ├── status_bar/      # Bottom bar: feedback messages + file metadata
    └── preferences/     # AdwPreferencesDialog
```

### Key Design Decisions

- **GtkSourceView owns editing**: language detection, syntax highlighting, style schemes, and undo/redo are all delegated to GtkSourceView, not reimplemented.
- **Dark mode**: GtkSourceView has its own style scheme system separate from GTK CSS. We query `libadwaita::StyleManager::is_dark()` and react to `connect_dark_notify()` to switch between `"Adwaita"` and `"Adwaita-dark"` schemes.
- **File tree uses modern GTK4 model**: `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` (NOT the deprecated `GtkTreeView`).
- **Workspace concept**: a named collection of root directories/files, persisted to `$XDG_DATA_HOME/lushtext/workspaces.json`.
- **Session persistence**: open tabs per workspace, persisted to `$XDG_DATA_HOME/lushtext/session-{id}.json`.
- **Status bar**: per-window bottom bar below `GtkPaned`, always visible. Three sections: feedback message area (left), encoding label (right), file size label (right). The window orchestrates all updates via `refresh_status_bar()`, called from `new_tab()`, `open_document()`, `close-tab` action, `save` action, and the `selected-page` notify handler.
- **Status bar auto-dismiss**: messages auto-dismiss after 5 seconds using a generation counter (`Cell<u32>`). Each `push_message` increments the counter; the timer closure captures the value and no-ops if the counter has advanced (a newer message replaced the old one). This avoids storing/cancelling `glib::SourceId` handles entirely.
- **File metadata on EditorPage**: `file_size: Cell<Option<u64>>` is populated during async load (from `fs::metadata`) and updated on save (from written byte count). The window pulls this on tab switch via `editor.file_size()`.

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
