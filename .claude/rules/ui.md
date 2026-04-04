---
description: UI and GTK widget design rules
globs: "**/*.{rs,ui,css}"
---

# UI Design Rules

## Visual Target

LushText should look and feel like GNOME Text Editor, with these differences:
- Always-visible left sidebar with file tree (GNOME Text Editor has a right-side properties panel)
- Workspace concept with multiple roots

## Widget Hierarchy

```
LushtextWindow (AdwApplicationWindow)
├── AdwHeaderBar
├── AdwTabBar → bound to AdwTabView
├── GtkRevealer [palette_revealer] → LushtextCommandPalette (Ctrl+P)
├── GtkPaned (horizontal)
│   ├── [start] LushtextSidebar (always visible)
│   │   ├── GtkScrolledWindow (outer, vexpand)
│   │   │   └── GtkBox [sections_box]
│   │   │       └── LushtextWorkspaceSection (per workspace)
│   │   │           ├── GtkSeparator
│   │   │           ├── GtkBox [header: label + add_folder_button]
│   │   │           └── GtkScrolledWindow (inner, propagate-natural-height=true)
│   │   │               └── GtkListView + TreeListModel
│   │   ├── GtkSeparator
│   │   └── GtkBox [footer: "New Workspace" label + button]
│   └── [end] GtkStack
│       ├── "tabs": AdwTabView → LushtextEditorPage per tab
│       └── "empty": AdwStatusPage
└── LushtextStatusBar (always visible, full width)
    ├── GtkToggleButton [sidebar_toggle_button] — toggle sidebar (action: win.toggle-sidebar)
    ├── GtkLabel [message_label] — feedback messages (left, hexpand)
    └── GtkBox [metadata_box] — encoding + file size (right, hidden when no tabs)
```

## Libadwaita Widgets to Use

- `AdwHeaderBar` (not `GtkHeaderBar`)
- `AdwTabView` + `AdwTabBar` for document tabs
- `AdwPreferencesDialog` with `AdwComboRow`, `AdwSwitchRow`, `AdwSpinRow`
- `AdwStatusPage` for empty states
- `AdwAboutDialog` for the about dialog
- `AdwWindowTitle` for header title/subtitle

## File Tree

- Use `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` (modern GTK4 pattern).
- Never use deprecated `GtkTreeView`.
- Sort: directories first, then alphabetical (case-insensitive).
- Skip hidden files (starting with `.`).
- **Disable TreeExpander's gesture for file rows**: `GtkTreeExpander` installs an internal `GtkGestureClick` (BUBBLE phase) that intercepts click events for ALL rows — even non-expandable files. This prevents `GtkListView`'s built-in double-click activation from firing. The fix: in `connect_bind`, use `expander.observe_controllers()` to find the `GtkGestureClick` and set `propagation_phase` to `None` for file rows (disabling it) and `Bubble` for directory rows (preserving expand/collapse). This runs on every bind (including ListItem recycling). Do NOT use `single-click-activate=true` (changes UX) or CAPTURE-phase gestures (fragile, fails for first file due to `SingleSelection::selected()` timing).

## Multi-Workspace Sidebar

- `LushtextSidebar` is an orchestrator: manages workspace sections, the "New Workspace" footer, and persistence (`workspaces.json`).
- `LushtextWorkspaceSection` encapsulates per-workspace state: file tree, file context menu, header context menu.
- **Inner ScrolledWindow pattern**: Each section wraps its `GtkListView` in `GtkScrolledWindow(propagate-natural-height=true, vscrollbar-policy=never)`. This provides the vadjustment that ListView requires. The outer ScrolledWindow handles all scrolling.
- **Footer always visible**: The "New Workspace" footer (GtkSeparator + label + button) sits below the outer ScrolledWindow, outside the scrollable area.
- **Callback forwarding**: Sections emit file callbacks (activated, renamed, deleted, created) and workspace callbacks (add-folder, rename, unlist). The sidebar forwards file callbacks to the window and handles workspace callbacks itself.
- **Persistence**: Sidebar owns `WorkspacesFile` in a `RefCell`. Every mutation saves to disk via `workspace_manager::save()`.

## File Context Menu (per WorkspaceSection)

- Single `GtkPopoverMenu` (from `gio::Menu`) attached to the `GtkListView`, not per-row popovers.
- Right-click detection: `GtkGestureClick(button=3)` on the ListView. Use `Widget::pick(x, y)` + `find_ancestor_expander()` to locate the `TreeExpander` → `list_row()` → `FileTreeItem` at click position.
- Actions are in a `section` action group (`insert_action_group`) with `new-file`, `new-dir`, `rename`, and `delete`.
- Inline rename: dynamically append a `GtkEntry` to the row's content box, hide the label. Guard against double-fire from focus-out after confirm/cancel using `entry.parent().is_none()`.
- `connect_bind` cleanup: remove any lingering rename `GtkEntry` from row recycling and restore label visibility.
- Window integration via callback pattern: `connect_file_renamed(Fn(&Path, &Path))` and `connect_file_deleted(Fn(&Path))`, consistent with `connect_file_activated`.
- Directory operations must use `Path::starts_with` prefix matching for tab path updates and closures (not just exact equality).

## Workspace Header Context Menu (per WorkspaceSection)

- `GtkPopoverMenu` attached to `header_box` with "Rename Workspace" and "Unlist Workspace" actions.
- Actions are in a `ws-header` action group on the section widget.
- Rename shows `AdwAlertDialog` with `extra_child` text entry pre-filled with current name.
- Unlist shows `AdwAlertDialog` confirmation. Files are NOT deleted from disk.

## UI Templates

- Composite templates in `resources/ui/*.ui` (GTK XML format).
- GResource XML at `resources/dev.cominotti.lushtext.gresource.xml`.
- Compiled by `glib-build-tools` in `build.rs` for dev builds.
- Template `class` attribute must exactly match `ObjectSubclass::NAME`.

## Status Bar

- Per-window, below `GtkPaned`, always visible regardless of tab count.
- `metadata_box` (encoding + file size) is hidden via `set_visible(false)` when no tabs are open; the message area remains available.
- Messages use Adwaita semantic color tokens: `@accent_color` (Info), `@warning_color` (Warning), `@error_color` (Error). These adapt to light/dark mode automatically — no Rust-side dark mode handling needed.
- Background uses `@headerbar_bg_color` to visually distinguish from the editor area.
- Use the `caption` Adwaita CSS class for status bar text (small font, standard GNOME HIG for secondary UI).

## GSettings Bindings

Editor preferences use GSettings (`dev.cominotti.lushtext` schema) with `gio::Settings::bind()`:

- **Direct bindings** for properties with matching types: `show-line-numbers`, `highlight-current-line`, `tab-width`, `insert-spaces-instead-of-tabs`. Preferences dialog uses two-way (`DEFAULT`), editor pages use one-way (`GET`).
- **Manual mapping** for `word-wrap` (bool → `GtkWrapMode`): use `connect_changed()` to convert.
- **Color scheme**: stored as base ID string, dark variant appended automatically. Combo row uses manual wiring (position ↔ string ID).
- **Font customization**: display-wide CSS provider in `load_css()` targeting `.monospace` widgets. Updated reactively via `connect_changed()` on `use-system-font` and `custom-font` keys.
- **Word wrap default**: enabled (`true`). Maps to `WrapMode::Word`.

## Window State Persistence

Window geometry and sidebar position are persisted via GSettings (not JSON session files):

- **Keys**: `window-width` (i), `window-height` (i), `window-maximized` (b), `sidebar-position` (i)
- **Restore**: in `window/imp.rs` `constructed()` via `set_default_size()` + `maximize()` + `set_position()`, all before `present()`
- **Persist**: via `connect_notify_local` on `default-width`, `default-height`, `maximized` properties. Width/height only persisted when `!is_maximized()` to avoid overwriting normal dimensions with maximized size.
- **Sidebar clamp**: `clamp_sidebar_position()` function in `window/imp.rs` enforces `position <= width / 3`. Called from two places: (1) `WidgetImpl::size_allocate()` — uses the definitive allocated width parameter, catches all resize/maximize/unmaximize transitions; (2) `notify::position` on the paned — catches user drag. **Do not use property notifications for clamping** — `notify::default-width`/`notify::maximized` fire before the new allocation is applied, so `window.width()` returns the old stale value.

## Syntax Highlighting

Supported via GtkSourceView built-in language specs: JSON, TOML, YAML, Markdown.
JSONC is deferred (requires custom `.lang` file — see `docs/next/jsonc-support.md`).
