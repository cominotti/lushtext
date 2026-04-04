# LushText

A fast, minimalist text editor for GNOME built with Rust, GTK4, and Libadwaita. Similar in spirit to GNOME Text Editor, but with an always-visible file tree sidebar and workspace support.

## Features

- **File tree sidebar** -- always-visible left pane with directory navigation
- **Workspaces** -- named collections of root directories, persisted across sessions
- **Syntax highlighting** -- via GtkSourceView for common file types (Rust, Python, JSON, TOML, YAML, Markdown, and more)
- **EditorConfig support** -- per-file formatting overrides from `.editorconfig` files (`indent_style`, `tab_width`, `indent_size`); toggle in Preferences
- **Session persistence** -- tabs, cursor positions, and scroll offsets restored on restart
- **Draft recovery** -- unsaved changes auto-saved to disk and recovered after crash
- **Print** -- native GTK print dialog with syntax highlighting and editor settings preserved
- **Find and replace** -- per-tab search bar with match highlighting
- **Command palette** -- Ctrl+P fuzzy search for files and commands (SIMD-accelerated via nucleo)
- **Large file handling** -- graceful degradation: >1MB toast, >10MB disable syntax, >50MB disable undo, >500MB refuse
- **Buffer eviction** -- background tabs evicted when total memory exceeds 256MB, transparently reloaded on focus
- **Dark mode** -- automatic GtkSourceView scheme switching via Libadwaita StyleManager
- **Customizable font** -- system monospace or custom font, applied via CSS provider
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
```

The Makefile auto-detects [mold](https://github.com/rui314/mold) for faster linking and [cargo-nextest](https://nexte.st/) for parallel test execution. Both are optional.

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
  model/             Domain types (no GTK deps)
    workspace.rs     Workspace persistence model
    session.rs       Tab session model
    palette.rs       Command palette types
    draft.rs         Draft persistence metadata
    formatting_overrides.rs   Per-file EditorConfig overrides
  services/          Business logic (GTK-free where possible)
    editorconfig.rs  .editorconfig resolution
    palette.rs       SIMD fuzzy search + file indexing
    file_tree.rs     Directory scanning
    draft_service.rs Draft autosave
    session_service.rs  Session load/save
    workspace_manager.rs  Workspace CRUD
    async_task.rs    spawn_blocking_then concurrency guard
  ui/                GTK4/Libadwaita widgets
    window/          Main window + dialogs
    editor_page/     GtkSourceView tab
    sidebar/         Multi-workspace file tree
    command_palette/ Ctrl+P fuzzy search
    search_bar/      Find/replace
    status_bar/      Bottom bar
    info_bar/        Contextual warnings
    preferences/     Settings dialog
```

## Testing

```sh
make test        # All tests (301 total)
make test-unit   # Unit tests only
make test-int    # Integration tests only
make test-widget # Widget tests (requires display server)
```

Widget tests can run headless with `xvfb-run make test-widget`.

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
