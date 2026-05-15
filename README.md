# LushText

A fast, minimalist text editor for GNOME built with Rust, GTK4, and Libadwaita. Similar in spirit to GNOME Text Editor, but with a persistent workspace sidebar, an optional properties sidebar, and workspace support.

## Features

- **Document properties surface** -- persistent left workspace tree plus an adaptive document-properties surface that appears as a right pane on spacious windows and a bottom sheet on compact ones, keeping document metadata, file-health details, and formatting-source explanation out of the bottom bar
- **Focus Mode** -- `Ctrl+Shift+F11` enters a reversible fullscreen writing shell with chrome suppressed, readable editor/Markdown columns, a subtle source text-origin guide, optional typewriter scrolling, and `Alt+P` preview-only support
- **Adaptive workspace sidebar width** -- choose `Small`, `Comfy`, or `Large` in `Preferences > Workspace`; each preset stays comfortable on large displays by clamping to a bounded desktop width
- **Tab content transparency** -- adjust `Transparency` in `Preferences > Editor > Appearance` to soften editor and Markdown preview backgrounds while keeping the header, side panels, status/search chrome, and minimap opaque
- **Workspaces** -- named single-root directories with a shared current workspace scope, persisted across sessions
- **Workspace auto-refresh** -- external file and folder changes refresh the sidebar's currently materialized root rows and expanded directories automatically, with access-noise filtering plus in-place reconciliation for both subtree and manual root refreshes to avoid visible flashing, and a per-section `Refresh` button for deterministic broader reloads
- **File peek** -- press `Space` on a selected sidebar file to inspect a bounded read-only preview in a floating card with the absolute file path, then `Enter` or `Open` to promote it into a real tab
- **Focus Folder** -- re-root a workspace section into a deep subfolder so the sidebar can drill into nested trees without wasting width on clipped ancestors
- **Syntax highlighting** -- via GtkSourceView for common file types (Rust, Python, JSON, TOML, YAML, Markdown, and more)
- **EditorConfig support** -- per-file formatting overrides from `.editorconfig` files (`indent_style`, `tab_width`, `indent_size`); toggle in Preferences
- **Bookmarks and rich notes** -- saved-file bookmark gutter marks with labels and F2 navigation, plus markdown-capable range notes, document notes, workspace notes, and a unified notes browser
- **Local history** -- saved-file snapshot browser with automatic baseline, periodic, and save-boundary restore points, an adaptive Adwaita browse/preview UI, restore safety snapshots, and one-click undo of a restore
- **Minimap** -- toggleable right-edge document overview with semantic markers for bookmarks, active in-tab search matches, modified-since-save regions, and long-line warnings on supported files
- **Session persistence** -- tabs, pinned state, cursor positions, and scroll offsets restored on restart
- **Draft recovery** -- unsaved changes auto-saved to disk and recovered after crash
- **Print** -- native GTK print dialog with syntax highlighting and editor settings preserved
- **Workspace content search** -- Ctrl+Shift+F parallel grep across the current workspace scope (`All workspaces` or one selected workspace) with streaming results, regex/literal/whole-word modes, .gitignore toggle, glob file filter, F4/Shift+F4 match navigation, progress reporting, search history with full state recall, and named saved searches
- **Multi-file Replace All** -- preview proposed changes with per-match checkboxes, atomic file writes, skip-modified-tabs safety, and full undo support
- **Find and replace** -- per-tab search bar with match highlighting
- **Command palette** -- Ctrl+P fuzzy search for files and commands, scoped to the current workspace selection unless `All workspaces` is active (SIMD-accelerated via nucleo)
- **Large file handling** -- graceful degradation: >1MB toast, >10MB disable syntax, >50MB disable undo, >500MB refuse
- **Buffer eviction** -- background tabs evicted when total memory exceeds 256MB, transparently reloaded on focus
- **Dark mode** -- automatic GtkSourceView scheme switching via Libadwaita StyleManager
- **Customizable font** -- system monospace or custom font, applied via CSS provider
- **Markdown preview** -- side-by-side or full-width preview pane with native GTK rendering for headings, emphasis, code, links, ordered and unordered lists, task lists, blockquotes, GitHub alert callouts, footnotes, and Markdown tables; Alt+P toggles full-width preview
- **File monitoring** -- detects external changes and offers reload

## Installation and Running

LushText is packaged as a GNOME Flatpak and can also be run directly from a
source checkout for development.

### Flatpak from this checkout

```sh
make flatpak-install
flatpak run dev.cominotti.lushtext
```

The Flatpak uses `org.gnome.Platform` 50 and requires the matching GNOME SDK.
`make flatpak-install` idempotently adds the user Flathub remote when needed
and installs missing runtime, SDK, and SDK-extension dependencies before
building. Use `make flatpak` when you only want to build the Flatpak without
installing it.
If dependencies change, regenerate the vendored Cargo sources before building:

```sh
make cargo-sources
```

### Development run

```sh
make run
```

`make run` builds the debug binary and temporarily stages a GNOME desktop entry
and app icon so the running development copy appears correctly in GNOME Shell.

## First Run

1. Open a file with `Ctrl+O`, from the header-bar open button, or by launching
   `lushtext PATH`.
2. Add a workspace folder from the left sidebar to browse a project directory.
3. Use the workspace selector to choose `All workspaces` or one specific root.
4. Open the command palette with `Ctrl+Shift+P` to search files and commands.
5. Open the main menu and choose **Keyboard Shortcuts** for the complete
   shortcut reference shipped with the app.

LushText restores open tabs, pinned tabs, cursor positions, scroll positions,
workspaces, search state, and recoverable drafts on restart.

## Preferences

Preferences are stored with GSettings under `dev.cominotti.lushtext`.

### Editor

- **Color Scheme** selects the base GtkSourceView style scheme; dark variants
  are chosen automatically when GNOME is in dark mode.
- **Use System Monospace Font** and **Custom Font** control editor and sidebar
  monospace text.
- **Transparency** adjusts editor and Markdown preview document backgrounds
  without making window chrome or side panels transparent.
- **Focus Mode** preferences set the target column width and optional typewriter
  scrolling.
- **Use EditorConfig**, **Word Wrap**, **Tab Width**, **Insert Spaces Instead of
  Tabs**, **Show Line Numbers**, **Highlight Current Line**, **Show Minimap**,
  **Show Bookmark Gutter**, and **Show Annotation Highlights** control editing
  behavior and editor decorations.

### Workspace

- **Sidebar Width** chooses the `Small`, `Comfy`, or `Large` workspace sidebar
  preset.
- **Auto-Collapse Workspaces** collapses other workspace sections when focusing
  a folder.
- **Empty Folder Lookahead Cap** controls how many subdirectories LushText peeks
  into when deciding whether a folder should be marked `(Empty)`.

Advanced users can inspect or reset settings with:

```sh
gsettings list-recursively dev.cominotti.lushtext
gsettings reset-recursively dev.cominotti.lushtext
```

For Flatpak installs, run those commands inside the sandbox:

```sh
flatpak run --command=gsettings dev.cominotti.lushtext list-recursively dev.cominotti.lushtext
```

## Data, Privacy, and Reset

LushText keeps application state under `$XDG_DATA_HOME/lushtext` for source and
host installs. On typical systems this is `~/.local/share/lushtext`. Flatpak
installs keep the same app data inside the sandbox, normally under
`~/.var/app/dev.cominotti.lushtext/data/lushtext`.

Stored state can include document text:

| Path | Contains |
|------|----------|
| `session.json` | Open tabs, pinned state, cursor positions, and scroll offsets |
| `workspaces.json` | Saved workspace roots and names |
| `drafts/` | Plain-text autosaved drafts for unsaved changes |
| `bookmarks/` | Saved-file bookmark metadata |
| `annotations/` | Range-note metadata and note text |
| `document-notes/` | Per-file document notes |
| `workspace-notes/` | Per-workspace-root notes |
| `local-history/` | Local-history snapshots for saved files |
| `search-history.json` | Recent workspace search queries and options |
| `saved-searches.json` | Named saved searches |
| `replace-backup.json` | Temporary undo data for multi-file Replace All |

To fully reset LushText state, close the app and remove that app-data directory.
For Flatpak installs, also reset the sandboxed GSettings if you want preferences
back at defaults:

```sh
flatpak run --command=gsettings dev.cominotti.lushtext reset-recursively dev.cominotti.lushtext
```

## Flatpak Permissions

The Flatpak manifest grants host filesystem access because LushText is a local
workspace text editor that must open, save, search, rename, delete, and
event-monitor user-selected files and workspace folders across local paths, not
only under the home directory. It does not request network access.

| Permission | Why it is used |
|------------|----------------|
| `--filesystem=host` | Open, save, search, rename, delete, and event-monitor user-selected local files and workspace folders across host paths |
| `--socket=wayland` | Native Wayland display support |
| `--socket=fallback-x11` and `--share=ipc` | X11 fallback support |
| `--device=dri` | GTK hardware-accelerated rendering |

## Common Shortcuts

The full shortcut list is available in **Main Menu > Keyboard Shortcuts**.

| Workflow | Shortcut |
|----------|----------|
| New tab | `Ctrl+T` |
| Open file | `Ctrl+O` |
| Save / Save As | `Ctrl+S` / `Ctrl+Shift+S` |
| Close tab | `Ctrl+W` |
| Print | `Ctrl+P` |
| Find / Find and Replace | `Ctrl+F` / `Ctrl+H` |
| Next / previous find match | `Ctrl+G` / `Ctrl+Shift+G` |
| Command palette | `Ctrl+Shift+P` |
| Workspace search | `Ctrl+Shift+F` |
| Workspace search next / previous match | `F4` / `Shift+F4` |
| Toggle minimap | `Ctrl+Shift+M` |
| Cycle invisible characters | `Ctrl+Shift+I` |
| Document properties | `F9` |
| Fullscreen | `F11` |
| Focus Mode | `Ctrl+Shift+F11` |
| Markdown preview-only mode | `Alt+P` |
| Zoom in / out / reset | `Ctrl++` / `Ctrl+-` / `Ctrl+0` |

## Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust (Edition 2024, MSRV 1.95.0) |
| GUI | GTK4 0.11 + Libadwaita 0.9 + GtkSourceView 5 0.11 |
| Config | GSettings |
| Build | Cargo workspace + Makefile (dev), Meson (Flatpak/installed) |
| Packaging | Flatpak (org.gnome.Platform 50) |
| License | GPL-3.0-or-later |

## Building from Source

### Dependencies

- Rust 1.95.0+
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
make run         # Debug build + run with temporary GNOME desktop staging for dock icon matching
make refresh-dock-icon # Regenerate app icon assets + force a fresh GNOME Shell dock icon reload
make test        # All tests (unit + integration + widget)
make check       # clippy + fmt check
make pre-commit  # repo pre-commit gate (fmt + clippy)
make install-git-hooks
```

LushText ships repo-managed Git hooks in `.githooks/`. Run `make install-git-hooks` once per checkout to configure `core.hooksPath`; after that, each commit runs the same rustfmt + Clippy gate locally before Git creates the commit.

The Makefile auto-detects [cargo-nextest](https://nexte.st/) for parallel non-widget execution (optional), but it always runs widget tests explicitly through the shared `scripts/run-widget-tests.sh` runner so `make test` still means the full suite. Rust 1.90+ uses [rust-lld](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable/) as the default linker on Linux for fast linking.

On GNOME Shell, `make run` temporarily stages a user-local desktop entry plus `hicolor` app icons while the debug binary is running. The staged desktop entry points at a content-addressed absolute icon file so Shell reloads icon changes reliably during development instead of reusing a stale themed-icon cache entry. The launcher also repairs any stale user-local LushText desktop entry whose absolute `Icon=` path no longer exists. If you changed the app icon artwork, use `make refresh-dock-icon`: it regenerates the shipped PNG fallbacks from `data/icons/dev.cominotti.lushtext.svg`, then restarts the current dev instance against a fresh file-backed icon so the dock updates immediately.

### Flatpak

```sh
make flatpak-deps    # Install Flatpak runtime/SDK deps into the user installation
make flatpak         # Build Flatpak (sets up missing runtime/SDK deps)
make flatpak-install # Build and install Flatpak into the user installation
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

LushText includes non-destructive notes for saved files and explicit workspace roots:

- **Bookmarks** live in the GtkSourceView gutter, can carry an optional label, and support next/previous navigation with `F2` / `Shift+F2`.
- **Range notes** store markdown-capable note text plus a presentation style for one or more lines without modifying the file on disk.
- **Document notes** store one markdown-capable note for a saved file as a whole.
- **Workspace notes** store one markdown-capable note for each workspace root.
- **Browse and export** flows operate on the currently selected workspace scope, so the bookmark browser, unified notes browser, and range-note markdown export stay aligned with the sidebar filter.

### Shortcuts

| Workflow | Shortcut |
|----------|----------|
| Toggle bookmark | `Ctrl+F2` |
| Edit bookmark label | `Ctrl+Shift+F2` |
| Next / previous bookmark | `F2` / `Shift+F2` |
| Browse bookmarks | `Ctrl+Alt+B` |
| Add range note | `Ctrl+Alt+N` |
| Edit range note at cursor | `Ctrl+Alt+M` |
| Browse notes | `Ctrl+Alt+A` |
| Export range notes | `Ctrl+Alt+Shift+A` |

### Manual test checklist

Use this checklist to exercise the full shipped bookmark and rich-note flow:

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
   Expected: the range-note dialog opens, lets you choose a style and note text, and saving it does not modify the file bytes.
8. Move the cursor onto the annotated range and press `Ctrl+Alt+M`.
   Expected: the existing range note opens for editing, supports Edit/Render switching, and Delete removes it cleanly.
9. Press `Ctrl+Alt+A`.
   Expected: the unified notes browser opens for the current workspace scope, previews workspace/document/range notes, and clicking Open on a row routes to the right note surface.
10. Insert and delete lines above an annotation while the file stays open.
    Expected: the range-note span follows the content; deleting the whole range removes the range note.
11. Open **Document Note…** for the active saved file.
    Expected: the file-level note opens, supports Edit/Render switching, and Save persists it without changing the file bytes.
12. Select one concrete workspace and open **Workspace Note…**.
    Expected: the workspace-level note opens for that root; in `All workspaces`, the single-workspace action stays disabled and the unified browser remains available.
13. Toggle **Preferences > Show Bookmark Gutter**.
    Expected: bookmark gutter indicators hide and reappear without losing stored bookmarks.
14. Toggle **Preferences > Show Annotation Highlights**.
    Expected: range-note highlighting hides and reappears without losing stored range notes.
15. Close and reopen the file, then restart the app and open it again.
    Expected: bookmarks, range notes, and document notes restore automatically; workspace notes return when the same workspace root is restored.
16. Rename the file from the LushText sidebar.
    Expected: reopening the renamed file keeps the same bookmarks, range notes, and document note.
17. Use **Save As** to write the file to a new path.
    Expected: the new file opens without copied range notes or document notes, while the original file keeps its existing notes.
18. Press `Ctrl+Alt+Shift+A`.
    Expected: the export dialog writes a markdown report grouped by file, including range-note line ranges, note text, and source excerpts.
19. Try steps 3, 7, and 11 in an untitled tab.
    Expected: LushText refuses to create bookmarks, range notes, or document notes and shows clear feedback that a saved file is required.

### Persistence rules

- Bookmarks, range notes, and document notes require a **saved file path**. Untitled tabs show feedback instead of creating note state.
- Workspace notes require a **concrete workspace root**. `All workspaces` keeps the browser available, but the single-workspace note action stays disabled until one workspace is selected.
- Sidecars live under `$XDG_DATA_HOME/lushtext/bookmarks/`, `$XDG_DATA_HOME/lushtext/annotations/`, `$XDG_DATA_HOME/lushtext/document-notes/`, and `$XDG_DATA_HOME/lushtext/workspace-notes/`.
- **Save As** creates a new file-backed note identity and does not copy the old file's bookmarks, range notes, or document notes by default.
- **Sidebar rename inside LushText** migrates file-backed and workspace-root note sidecars to the new path automatically.

### First-release limitations

- Path-based identity does not automatically follow **external** filesystem moves or copies performed outside LushText.
- Range-note indicators are currently **highlight-based**, not clickable gutter popovers or inline rendered note blocks.
- Note export is limited to **range notes only** in the first release.

## Local History

LushText includes a focused local-history MVP for saved documents.

- Open **Local History** from the main menu, the command palette, `Ctrl+Alt+L`, the sidebar file context menu, or the editor content context menu while a saved file is active.
- The browser opens in an adaptive Libadwaita dialog with newest-first snapshots and a read-only preview.
- On wide windows, the dialog expands into a large viewer-first surface that uses most of the parent window while staying parent-bounded, with an Adwaita snapshot rail beside the preview.
- Empty historical snapshots are explained explicitly in the browser so an empty file state does not look like a broken preview.
- Legacy stale-disk empty baseline rows from older history can be hidden from the browser while the stored history on disk remains unchanged.
- Restoring a snapshot writes it into the editor buffer, marks the document modified, and immediately offers **Undo Restore** without writing to disk.
- **Save As** starts a fresh history lineage for the new path, while sidebar renames inside LushText migrate the existing lineage to the renamed path.

### Shortcut

| Workflow | Shortcut |
|----------|----------|
| Local History | `Ctrl+Alt+L` |

### Capture policy

- A baseline snapshot is recorded when a clean saved document first becomes modified.
- If a file-backed draft is restored at open time, local history treats that restored working content as the baseline instead of adding a fresh row for stale on-disk file contents.
- Additional snapshots are captured no more than once every five minutes while the document stays modified.
- Every successful save records a save-boundary snapshot.
- Consecutive duplicate snapshot bodies are skipped so the browser stays readable.

### Large-file limits

- Up to `10 MB`: full capture cadence and browsing are available.
- Above `10 MB` and up to `50 MB`: local history captures only on save boundaries.
- Above `50 MB`: local history is unavailable.

## Preview and Sidebar Helpers

### Markdown preview

LushText can render Markdown files in a read-only preview pane instead of just
showing the raw source text.

- `Alt+P` toggles **preview-only mode**, where the editor hides and the rendered
  Markdown takes the full content area.
- A separate side-by-side preview pane is also available through the existing
  preview action surfaces, giving you editor text on the left and rendered
  output on the right.
- The renderer uses native GTK styling and widgets for headings, emphasis,
  code, activatable links, ordered and unordered lists, task lists, nested
  list indentation, blockquotes, GitHub alert callouts, footnotes, Markdown
  tables, and local Markdown images with explicit fallback states for
  unsupported or unresolved image targets.
- Non-Markdown files show a placeholder instead of trying to render arbitrary
  text as Markdown.
- Canonical preview sample content lives under `samples/`. The file
  `samples/markdown-test.md` is the canonical showcase for the Markdown preview
  features LushText currently supports.

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
    note.rs          Shared note-body primitives
    bookmark.rs      Bookmark sidecar model
    annotation.rs    Annotation sidecar model and styles
    document_note.rs Saved-file document-note model
    local_history.rs Local-history snapshot metadata
    content_search.rs  Content search types (SearchMatch, SearchEvent, etc.)
    encoding.rs      Document encoding, line endings, file health, and invisible-character modes
    sidecar_identity.rs  Canonical-path sidecar identity helpers for notes and history
    workspace_note.rs  Workspace-root note model
    formatting_overrides.rs   Per-file EditorConfig overrides
  services/          Business logic (GTK-free where possible)
    bookmark_service.rs  Bookmark sidecar load/save/move/list helpers
    annotation_service.rs  Annotation sidecar load/save/move/export helpers
    document_note_service.rs  Saved-file document-note load/save/move/list helpers
    local_history_service.rs  Local-history capture/list/load/prune/move helpers
    note_storage.rs  Shared sidecar identity/load/filter helpers for note workflows
    content_search/  Parallel workspace grep plus replace/undo helpers
    palette/         Command registry, SIMD fuzzy search, and file indexing
    durable_write.rs Parent-directory fsync helpers for crash-durable atomic writes
    editor_io.rs     Encoding-aware text file load/save helpers, health analysis, and mtimes
    editorconfig.rs  .editorconfig resolution
    file_peek.rs     Bounded read-only snapshots for sidebar file peek
    notifications.rs Window-scoped status and inline notification store
    file_tree.rs     Directory scanning
    draft_service.rs Draft autosave
    search_backup.rs Replace All undo backup persistence for the active session
    search_history.rs  Search history persistence
    saved_searches.rs  Named saved search persistence
    session_service.rs  Session load/save
    workspace_note_service.rs  Workspace-root note load/save/move/list helpers
    workspace_manager.rs  Workspace CRUD
    workspace_watch.rs  Materialized-scope filesystem watch service for sidebar auto-refresh
    async_task.rs    spawn_blocking_then concurrency guard
  ui/                GTK4/Libadwaita widgets
    window/          Main window shell plus actions, documents, drafts, encoding, Focus Mode, local-history, notes, search, preview, session persistence, tab management, print, and zoom wiring
    editor_page/     GtkSourceView tab plus Focus Mode presentation, local-history capture, minimap, overscroll, invisible-character rendering, bookmark/annotation projection, load/save, monitor, and in-tab search helpers
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
