# LushText

A fast, minimalist text editor for GNOME built with Rust, GTK4, and Libadwaita. Similar in spirit to GNOME Text Editor, but with a persistent workspace sidebar, an optional properties sidebar, and workspace support.

## Features

- **Dual sidebars** -- persistent left workspace tree plus optional right properties panel for document metadata and editor formatting controls
- **Workspaces** -- named collections of root directories, persisted across sessions
- **File peek** -- press `Space` on a selected sidebar file to inspect a bounded read-only preview in a floating card with the absolute file path, then `Enter` or `Open` to promote it into a real tab
- **Syntax highlighting** -- via GtkSourceView for common file types (Rust, Python, JSON, TOML, YAML, Markdown, and more)
- **EditorConfig support** -- per-file formatting overrides from `.editorconfig` files (`indent_style`, `tab_width`, `indent_size`); toggle in Preferences
- **Session persistence** -- tabs, cursor positions, and scroll offsets restored on restart
- **Draft recovery** -- unsaved changes auto-saved to disk and recovered after crash
- **Print** -- native GTK print dialog with syntax highlighting and editor settings preserved
- **Workspace content search** -- Ctrl+Shift+F parallel grep across all workspace files with streaming results, regex/literal/whole-word modes, .gitignore toggle, glob file filter, F4/Shift+F4 match navigation, progress reporting, search history with full state recall, and named saved searches
- **Multi-file Replace All** -- preview proposed changes with per-match checkboxes, atomic file writes, skip-modified-tabs safety, and full undo support
- **Find and replace** -- per-tab search bar with match highlighting
- **Command palette** -- Ctrl+P fuzzy search for files and commands (SIMD-accelerated via nucleo)
- **Large file handling** -- graceful degradation: >1MB toast, >10MB disable syntax, >50MB disable undo, >500MB refuse
- **Buffer eviction** -- background tabs evicted when total memory exceeds 256MB, transparently reloaded on focus
- **Dark mode** -- automatic GtkSourceView scheme switching via Libadwaita StyleManager
- **Customizable font** -- system monospace or custom font, applied via CSS provider
- **Markdown preview** -- side-by-side or full-width preview pane with native TextTag rendering (headings, bold, italic, code, links, lists, blockquotes); Alt+P toggles full-width preview
- **File monitoring** -- detects external changes and offers reload

## Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust (Edition 2024, MSRV 1.94.1) |
| GUI | GTK4 0.11 + Libadwaita 0.9 + GtkSourceView 5 0.11 |
| Config | GSettings |
| Build | Cargo workspace + Makefile (dev), Meson (Flatpak/installed) |
| Packaging | Flatpak (org.gnome.Platform 49) |
| License | GPL-3.0-or-later |

## Building from Source

### Dependencies

- Rust 1.94.1+
- GTK4 development libraries
- Libadwaita development libraries
- GtkSourceView 5 development libraries
- GLib development tools (`glib-compile-schemas`)

On Fedora:

```sh
sudo dnf install gtk4-devel libadwaita-devel gtksourceview5-devel glib2-devel
```

On Ubuntu/Debian:

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev libglib2.0-dev
```

### Dev Builds

```sh
make build       # Release build
make build-debug # Debug build
make run         # Debug build + run
make test        # All tests (unit + integration + widget)
make check       # clippy + fmt check
make pre-commit  # repo pre-commit gate (fmt + clippy)
make install-git-hooks
```

LushText ships repo-managed Git hooks in `.githooks/`. Run `make install-git-hooks` once per checkout to configure `core.hooksPath`; after that, each commit runs the same rustfmt + Clippy gate locally before Git creates the commit.

The Makefile auto-detects [cargo-nextest](https://nexte.st/) for parallel non-widget execution (optional), but it always runs widget tests explicitly through the shared `scripts/run-widget-tests.sh` runner so `make test` still means the full suite. Rust 1.90+ uses [rust-lld](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable/) as the default linker on Linux for fast linking.

Critical rule: pre-existing blockers discovered while implementing or verifying a change must be fixed in the same work stream rather than worked around, deferred, or called out as "pre-existing". No exceptions.

### Flatpak

```sh
make flatpak         # Build Flatpak (requires flatpak-builder)
make cargo-sources   # Regenerate cargo-sources.json after dependency changes
```

## EditorConfig

LushText reads `.editorconfig` files from the directory tree and applies per-file formatting overrides. This is the same [EditorConfig](https://editorconfig.org/) standard supported by most editors.

### Supported properties

| Property | Maps to |
|----------|---------|
| `indent_style` | `insert-spaces-instead-of-tabs` |
| `tab_width` | `tab-width` (clamped 1-12) |
| `indent_size` | `indent-width` (clamped 1-12) |

### How it works

1. When a file is opened (or saved-as to a new path), the service walks from the file's parent directory upward, collecting `.editorconfig` files
2. Closer files take priority over farther ones
3. `root = true` stops the directory walk
4. Overrides are applied on the main thread; GSettings values are used as fallback for any property not specified in `.editorconfig`

The feature can be toggled in **Preferences > Use EditorConfig** (enabled by default). The status bar shows an "EditorConfig" indicator when overrides are active for the current tab.

### Deferred properties

`end_of_line`, `charset`, `trim_trailing_whitespace`, `insert_final_newline`, and `max_line_length` are not yet supported. See `docs/next/editorconfig-future.md` for details and implementation priorities.

## Architecture

Two-crate Cargo workspace:

- **`crates/lushtext-core`** -- all application logic: domain models, services, GTK widgets
- **`crates/lushtext`** -- thin binary entry point + integration tests

### Module layout

```
lushtext-core/src/
  app.rs             Application entry (AdwApplication subclass)
  config.rs          Compile-time constants
  lib.rs             Resource registration, CSS loading, and app bootstrap
  model/             Domain types (no GTK deps)
    workspace.rs     Workspace persistence model
    session.rs       Tab session model
    palette.rs       Command palette types
    draft.rs         Draft persistence metadata
    content_search.rs  Content search types (SearchMatch, SearchEvent, etc.)
    formatting_overrides.rs   Per-file EditorConfig overrides
  services/          Business logic (GTK-free where possible)
    content_search/  Parallel workspace grep plus replace/undo helpers
    palette/         Command registry, SIMD fuzzy search, and file indexing
    editor_io.rs     Text file load/save helpers and mtimes
    editorconfig.rs  .editorconfig resolution
    file_peek.rs     Bounded read-only snapshots for sidebar file peek
    notifications.rs Window-scoped status and inline notification store
    file_tree.rs     Directory scanning
    draft_service.rs Draft autosave
    search_backup.rs Replace All undo backup persistence for the active session
    search_history.rs  Search history persistence
    saved_searches.rs  Named saved search persistence
    session_service.rs  Session load/save
    workspace_manager.rs  Workspace CRUD
    async_task.rs    spawn_blocking_then concurrency guard
  ui/                GTK4/Libadwaita widgets
    window/          Main window shell plus actions, documents, drafts, search, preview, session persistence, print, and zoom wiring
    editor_page/     GtkSourceView tab plus load/save, monitor, and in-tab search helpers
    sidebar/         Multi-workspace file tree, dialogs, callbacks, per-section async child-tree loading, and file peek
    properties_panel/ Right-side metadata + formatting controls
    search_panel/    Ctrl+Shift+F workspace content search plus history, list factory, replace, results, and runtime flows
    command_palette/ Ctrl+P fuzzy search
    search_bar/      Find/replace
    status_bar/      Bottom bar
    info_bar/        Contextual warnings
    preferences/     Settings dialog
```

## Testing

```sh
make test        # All tests
make test-unit   # Unit tests only
make test-int    # Integration tests only
make test-widget # Widget tests with shared native/headless runner
make test-widget-headless # Widget tests with the CI mutter/dbus setup
```

Widget tests require a display server. `make test` and `make test-widget-headless` use the CI-style `mutter --headless` path for deterministic full-suite runs. `make test-widget` uses `scripts/run-widget-tests.sh`, which runs against the current display session when one is available and otherwise falls back to headless mode if the required tools are installed.

GTK widget tests run through the custom harness in [`crates/lushtext/tests/widget.rs`](./crates/lushtext/tests/widget.rs), which executes each widget test in its own process so GTK objects stay on a real main thread and test state cannot leak across cases. Because that binary is not owned by nextest, the shared runner keeps the native and headless `cargo test --test widget` paths aligned in one place.

## Benchmarks

Performance-sensitive code (fuzzy search, file indexing, directory scanning) is benchmarked with Criterion.rs:

```sh
make bench              # Run benchmarks
make bench-baseline     # Save as baseline
make bench-compare      # Compare against baseline
make bench-report       # Generate markdown report
```

## License

LushText is licensed under the [GNU General Public License v3.0 or later](https://www.gnu.org/licenses/gpl-3.0.html).
