---
description: UI and GTK widget design rules
globs: "**/*.{rs,ui,css}"
---

# UI Design Rules

## Visual Target

LushText should look and feel like GNOME Text Editor, with these differences:
- Persistent left workspace sidebar on desktop, plus an optional right-side properties panel
- Workspace concept with multiple roots

## Widget Hierarchy

```
LushtextWindow (AdwApplicationWindow)
├── AdwHeaderBar
├── AdwTabBar → bound to AdwTabView
├── GtkRevealer [palette_revealer] → LushtextCommandPalette (Ctrl+P)
├── AdwOverlaySplitView [workspace_split_view]
│   ├── [sidebar/start] LushtextSidebar
│   │   ├── GtkBox ["New Workspace" label + button]
│   │   ├── GtkSeparator
│   │   └── GtkScrolledWindow (outer, vexpand, horizontal scroll when needed)
│   │       └── GtkBox [sections_box]
│   │           └── LushtextWorkspaceSection (per workspace)
│   │               ├── GtkSeparator
│   │               ├── GtkBox [header: label + add_folder_button]
│   │               └── GtkScrolledWindow (inner, propagate-natural-height=true, propagate-natural-width=true)
│   │                   └── GtkListView + TreeListModel
│   └── [content] AdwOverlaySplitView [properties_split_view]
│       ├── [content] GtkBox [content_box] (vertical)
│       │   ├── GtkStack [content_stack] (vexpand)
│       │   │   ├── "tabs": GtkPaned [preview_paned]
│       │   │   │   ├── [start] GtkBox [editor_box] → AdwTabView → LushtextEditorPage per tab
│       │   │   │   └── [end] LushtextMarkdownPreview (starts hidden)
│       │   │   └── "empty": AdwStatusPage
│       │   └── GtkRevealer [search_panel_revealer] (slide-up, 250ms)
│       │       └── LushtextSearchPanel (Ctrl+Shift+F workspace search)
│       └── [sidebar/end] LushtextPropertiesPanel
└── LushtextStatusBar (always visible, full width)
    ├── GtkToggleButton [sidebar_toggle_button] — toggle sidebar (action: win.toggle-sidebar)
    ├── GtkLabel [message_label] — feedback messages (left, hexpand)
    ├── GtkBox [metadata_box] — EditorConfig + file size + encoding (right, hidden when no tabs)
    └── GtkToggleButton [properties_toggle_button] — toggle properties (action: win.toggle-properties)
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

- `LushtextSidebar` is an orchestrator: manages the fixed top "New Workspace" affordance, workspace sections, and persistence (`workspaces.json`).
- `LushtextWorkspaceSection` encapsulates per-workspace state: file tree, file context menu, header context menu.
- **Inner ScrolledWindow pattern**: Each section wraps its `GtkListView` in `GtkScrolledWindow(propagate-natural-height=true, propagate-natural-width=true, vscrollbar-policy=never, hscrollbar-policy=never)`. This provides the vadjustment that ListView requires while letting natural width bubble up to the outer sidebar scroller.
- **Top affordance always visible**: The "New Workspace" affordance (label + button) sits above the outer ScrolledWindow, outside the scrollable area.
- **Long sidebar content must not be ellipsized**: workspace headers and file-tree labels stay fully rendered; the outer sidebar scroller is responsible for horizontal overflow.
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

- Per-window, below the split-view shell, always visible regardless of tab count.
- The left workspace toggle stays at the far left and the properties toggle stays at the far right so both pane controls remain visually mirrored in the bottom bar.
- `metadata_box` (EditorConfig + file size + encoding) is hidden via `set_visible(false)` when no tabs are open; the message area and both pane toggles remain available.
- Messages use Adwaita semantic color tokens: `@accent_color` (Info), `@warning_color` (Warning), `@error_color` (Error). These adapt to light/dark mode automatically — no Rust-side dark mode handling needed.
- Background uses `@headerbar_bg_color` to visually distinguish from the editor area.
- Use the `caption` Adwaita CSS class for status bar text (small font, standard GNOME HIG for secondary UI).

## Info Bars

- `LushtextInfoBar` remains the inline notification surface above each editor page; do not replace it with a global status message for restored-file or access-error actions.
- Titles, subtitles, and infobar action labels must stay readable on narrow windows by wrapping instead of disappearing or being truncated.
- Save/Discard action widths should stay visually balanced so restored-document banners do not jitter while the window is being resized.

## GSettings Bindings

Editor preferences use GSettings (`dev.cominotti.lushtext` schema) with `gio::Settings::bind()`:

- **Direct bindings** for properties with matching types: `show-line-numbers`, `highlight-current-line`, `tab-width`, `insert-spaces-instead-of-tabs`. Preferences dialog uses two-way (`DEFAULT`), editor pages use one-way (`GET`).
- **Manual mapping** for `word-wrap` (bool → `GtkWrapMode`): use `connect_changed()` to convert.
- **Color scheme**: stored as base ID string, dark variant appended automatically. Combo row uses manual wiring (position ↔ string ID).
- **Font customization**: display-wide CSS provider in `load_css()` targeting `.monospace` widgets. Updated reactively via `connect_changed()` on `use-system-font` and `custom-font` keys.
- **Word wrap default**: enabled (`true`). Maps to `WrapMode::Word`.

## Window State Persistence

Window geometry and split-view state are persisted via GSettings (not JSON session files):

- **Keys**: `window-width` (i), `window-height` (i), `window-maximized` (b), `workspace-sidebar-visible` (b), `workspace-sidebar-width-fraction` (d), `properties-sidebar-visible` (b), `properties-sidebar-width-fraction` (d)
- **Restore**: in `window/imp.rs` `constructed()` via `set_default_size()`, `maximize()`, split-view property restoration, and breakpoint installation before `present()`
- **Persist**: width/height/maximized still use `connect_notify_local`; split-view booleans persist from `notify::show-sidebar` handlers and width fractions persist from `notify::sidebar-width-fraction`
- **Migration**: legacy `sidebar-position` / `sidebar-visible` keys remain only as one-shot migration inputs for existing installs; fresh installs keep the split-view defaults directly

## Split-View Rules

- Use nested `AdwOverlaySplitView`s for the window shell instead of an outer `GtkPaned`.
- `workspace_split_view` owns the left workspace pane and stays bound to `win.toggle-sidebar` in the status bar.
- `properties_split_view` owns the right properties pane and stays bound to `win.toggle-properties` in the status bar.
- Both side panes normalize to a quarter-width fraction whenever they are shown.
- Breakpoints collapse the properties pane before the workspace pane so medium-width windows keep the file tree visible longer.
- The properties-pane breakpoint should be tuned to protect the center editor width, especially for restored-document infobars and other editor chrome, rather than only mirroring split-view math.
- When a utility pane closes, return focus to the active editor rather than leaving focus stranded on a toggle button.

## Entry Width Symmetry in Toggle Layouts (CRITICAL)

When a GtkGrid layout has toggle-visible rows sharing columns (e.g., Find/Replace bar), **all entries across rows must have identical widths at all times**, regardless of which rows are currently visible. Toggling a row on or off must not change any column width.

**Root cause of violations:** `set_visible(false)` removes widgets from GtkGrid column sizing. When toggle-visible text buttons (e.g., "Replace", "Replace All") are wider than their counterpart icon buttons, showing them widens those columns, which steals width from the entry column.

**Required pattern:** Wrap toggle-visible widgets in `GtkRevealer` (not `set_visible(false)`) within their grid cells. GtkRevealer with a vertical transition (slide-down, slide-up, crossfade, or none) always reports the child's **full natural width** to the grid — only height is animated. This means column widths are always computed considering both rows' widgets, even when a row is collapsed to zero height.

**Implementation rules:**
- Set `row-spacing=0` on the grid; use `margin-top` on revealed children for inter-row spacing (the margin is included in the revealer's animated height, so it appears only when revealed).
- All revealers in the same row must use the same `transition-type` and `transition-duration` for synchronized animation.
- Never use `set_visible(false)` on individual grid cells if their width affects other rows' layout.
- The replace row uses `slide-down` / `150ms` for the reveal animation.

## Syntax Highlighting

Supported via GtkSourceView built-in language specs: JSON, TOML, YAML, Markdown.
JSONC is deferred (requires custom `.lang` file — see `docs/next/jsonc-support.md`).
