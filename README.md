# LushText

A fast, minimalist text editor for GNOME built with Rust, GTK4, and Libadwaita. Similar in spirit to GNOME Text Editor, but with a persistent workspace sidebar, an optional properties sidebar, and workspace support.

## Features

- **Dual sidebars** -- persistent left workspace tree plus optional right properties panel for document metadata and editor formatting controls
- **Workspaces** -- named collections of root directories, persisted across sessions
- **Workspace auto-refresh** -- external file and folder changes refresh the sidebar's currently materialized root rows and expanded directories automatically, with access-noise filtering plus in-place reconciliation for both subtree and manual root refreshes to avoid visible flashing, and a per-section `Refresh` button for deterministic broader reloads
- **File peek** -- press `Space` on a selected sidebar file to inspect a bounded read-only preview in a floating card with the absolute file path, then `Enter` or `Open` to promote it into a real tab
- **Focus Folder** -- re-root a workspace section into a deep subfolder so the sidebar can drill into nested trees without wasting width on clipped ancestors
- **Syntax highlighting** -- via GtkSourceView for common file types (Rust, Python, JSON, TOML, YAML, Markdown, and more)
- **EditorConfig support** -- per-file formatting overrides from `.editorconfig` files (`indent_style`, `tab_width`, `indent_size`); toggle in Preferences
- **Bookmarks and annotations** -- saved-file bookmark gutter marks with labels and F2 navigation, plus sidecar line-range annotations with searchable workspace browse/export workflows
- **Minimap** -- toggleable right-edge document overview with semantic markers for bookmarks, active in-tab search matches, modified-since-save regions, and long-line warnings on supported files
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

## Bookmarks and Annotations

LushText includes non-destructive per-file notes for saved documents:

- **Bookmarks** live in the GtkSourceView gutter, can carry an optional label, and support next/previous navigation with `F2` / `Shift+F2`.
- **Annotations** store note text plus a presentation style for one or more lines without modifying the file on disk.
- **Browse and export** flows operate on the currently selected workspace scope, so the bookmark browser, annotation browser, and markdown export stay aligned with the sidebar filter.

### Shortcuts

| Workflow | Shortcut |
|----------|----------|
| Toggle bookmark | `Ctrl+F2` |
| Edit bookmark label | `Ctrl+Shift+F2` |
| Next / previous bookmark | `F2` / `Shift+F2` |
| Browse bookmarks | `Ctrl+Alt+B` |
| Add annotation | `Ctrl+Alt+N` |
| Edit annotation at cursor | `Ctrl+Alt+M` |
| Browse annotations | `Ctrl+Alt+A` |
| Export annotations | `Ctrl+Alt+Shift+A` |

### Manual test checklist

Use this checklist to exercise the full shipped bookmark and annotation flow:

1. Start the app with `make run`.
2. Add a workspace folder and open a saved text file from the sidebar.
3. Press `Ctrl+F2` on the current line.
   Expected: a bookmark appears in the gutter and the file content does not change.
4. Press `Ctrl+Shift+F2` on that bookmarked line and add a label.
   Expected: the label saves and later appears in bookmark browse surfaces.
5. Add a second bookmark on another line, then use `F2` and `Shift+F2`.
   Expected: the cursor jumps forward and backward through bookmarks in the active file.
6. Press `Ctrl+Alt+B`.
   Expected: the bookmark browser opens for the current workspace scope, supports search, and clicking a row opens or focuses the bookmarked file and jumps to its line.
7. Select one or more lines and press `Ctrl+Alt+N`.
   Expected: the annotation dialog opens, lets you choose a style and note text, and saving it does not modify the file bytes.
8. Move the cursor onto the annotated range and press `Ctrl+Alt+M`.
   Expected: the existing annotation opens for editing, and Delete removes it cleanly.
9. Press `Ctrl+Alt+A`.
   Expected: the annotation browser opens for the current workspace scope, supports search, and clicking a row jumps to the file and reopens the annotation.
10. Insert and delete lines above an annotation while the file stays open.
    Expected: the annotation range follows the content; deleting the whole range removes the annotation.
11. Toggle **Preferences > Show Bookmark Gutter**.
    Expected: bookmark gutter indicators hide and reappear without losing stored bookmarks.
12. Toggle **Preferences > Show Annotation Highlights**.
    Expected: annotation highlighting hides and reappears without losing stored annotations.
13. Close and reopen the file, then restart the app and open it again.
    Expected: bookmarks and annotations restore automatically.
14. Rename the file from the LushText sidebar.
    Expected: reopening the renamed file keeps the same bookmarks and annotations.
15. Use **Save As** to write the file to a new path.
    Expected: the new file opens without copied bookmarks or annotations, while the original file keeps its existing notes.
16. Press `Ctrl+Alt+Shift+A`.
    Expected: the export dialog writes a markdown report grouped by file, including line ranges, note text, and source excerpts.
17. Try steps 3 and 7 in an untitled tab.
    Expected: LushText refuses to create bookmarks or annotations and shows clear feedback that a saved file is required.

### Persistence rules

- Bookmarks and annotations require a **saved file path**. Untitled tabs show feedback instead of creating note state.
- Sidecars live under `$XDG_DATA_HOME/lushtext/bookmarks/` and `$XDG_DATA_HOME/lushtext/annotations/`.
- **Save As** creates a new note identity and does not copy the old file's bookmarks or annotations by default.
- **Sidebar rename inside LushText** migrates sidecars to the new path automatically.

### First-release limitations

- Path-based identity does not automatically follow **external** filesystem moves or copies performed outside LushText.
- Annotation indicators are currently **highlight-based**, not clickable gutter popovers or inline rendered note blocks.
- Annotation export is **markdown only** in the first release.

## Preview and Sidebar Helpers

### Markdown preview

LushText can render Markdown files in a read-only preview pane instead of just
showing the raw source text.

- `Alt+P` toggles **preview-only mode**, where the editor hides and the rendered
  Markdown takes the full content area.
- A separate side-by-side preview pane is also available through the existing
  preview action surfaces, giving you editor text on the left and rendered
  output on the right.
- The renderer uses native GTK text styling for headings, emphasis, code,
  links, lists, and blockquotes.
- Non-Markdown files show a placeholder instead of trying to render arbitrary
  text as Markdown.

### Focus Folder

When deep directory nesting makes a folder hard to browse comfortably in the
workspace tree, the sidebar provides a **Focus Folder** action.

- Open the context menu on a directory in the sidebar and choose **Focus Folder**.
- The selected directory becomes the effective root for that workspace section,
  so the tree can drill into that area without wasting width on all of its
  ancestors.
- If **Auto-Collapse Workspaces** is enabled, focusing a folder can collapse
  other workspace sections to keep attention on the active subtree.
- Folders detected as effectively empty are marked `(Empty)` and do not offer
  the Focus Folder action.

### File peek

The sidebar includes a lightweight file peek flow for checking a file before
opening a real editor tab.

- Select a sidebar file row and press `Space` to open a bounded read-only
  preview popover.
- The preview shows the file name, absolute path, size, modified timestamp, and
  a short text sample or an explicit unsupported/error state.
- Pressing `Space` again on the same file, pressing `Escape`, clicking away, or
  moving selection to a non-file row closes the preview.
- Press `Enter` or use the **Open** button in the popover to promote the file
  through the normal open-tab flow.
- The preview is intentionally lightweight and does not create editor, draft,
  monitor, or undo state.

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
    bookmark.rs      Bookmark sidecar model
    annotation.rs    Annotation sidecar model and styles
    content_search.rs  Content search types (SearchMatch, SearchEvent, etc.)
    sidecar_identity.rs  Canonical-path sidecar identity helpers
    formatting_overrides.rs   Per-file EditorConfig overrides
  services/          Business logic (GTK-free where possible)
    bookmark_service.rs  Bookmark sidecar load/save/move/list helpers
    annotation_service.rs  Annotation sidecar load/save/move/export helpers
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
    workspace_watch.rs  Materialized-scope filesystem watch service for sidebar auto-refresh
    async_task.rs    spawn_blocking_then concurrency guard
  ui/                GTK4/Libadwaita widgets
    window/          Main window shell plus actions, documents, drafts, notes, search, preview, session persistence, print, and zoom wiring
    editor_page/     GtkSourceView tab plus minimap, bookmark/annotation projection, load/save, monitor, and in-tab search helpers
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
