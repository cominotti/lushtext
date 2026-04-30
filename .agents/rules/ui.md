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
│   │   └── GtkScrolledWindow (outer, vexpand, vertical scrolling only)
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
- `AdwSidebar` for shallow, sectioned dialog browse rails such as Notes and Local History where each item activates or previews one record
- `AdwAboutDialog` for the about dialog
- `AdwWindowTitle` for header title/subtitle

## File Tree

- Use `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` (modern GTK4 pattern).
- Do not replace the primary workspace file tree with `AdwSidebar`; it owns filesystem tree expansion, deep-folder focus, file operations, file peek, and watcher reconciliation rather than shallow navigation.
- Never use deprecated `GtkTreeView`.
- Sort: directories first, then alphabetical (case-insensitive).
- Skip hidden files (starting with `.`).
- **Disable TreeExpander's gesture for file rows**: `GtkTreeExpander` installs an internal `GtkGestureClick` (BUBBLE phase) that intercepts click events for ALL rows — even non-expandable files. This prevents `GtkListView`'s built-in double-click activation from firing. The fix: in `connect_bind`, use `expander.observe_controllers()` to find the `GtkGestureClick` and set `propagation_phase` to `None` for file rows (disabling it) and `Bubble` for directory rows (preserving expand/collapse). This runs on every bind (including ListItem recycling). Do NOT use `single-click-activate=true` (changes UX) or CAPTURE-phase gestures (fragile, fails for first file due to `SingleSelection::selected()` timing).
- **Inline row actions that survive deep nesting**: When adding a fixed interactive element (like a hover button) to a deeply nested tree row, DO NOT place it inside the `GtkTreeExpander`'s child box, as the expander's indentation will eventually push the element off-screen. Instead, wrap the `GtkTreeExpander` inside a `GtkOverlay`, and add the button as an overlay widget anchored to the right (`halign=End`). Ensure hover-only actions are also available in a right-click context menu to satisfy GNOME HIG accessibility requirements (since hover is inaccessible to keyboard-only and screen reader users).

## Multi-Workspace Sidebar

- `LushtextSidebar` is an orchestrator: manages the fixed top "New Workspace" affordance, workspace sections, and persistence (`workspaces.json`).
- `LushtextWorkspaceSection` encapsulates per-workspace state: file tree, file context menu, header context menu.
- **Inner ScrolledWindow pattern**: Each section wraps its `GtkListView` in `GtkScrolledWindow(propagate-natural-height=true, propagate-natural-width=false, vscrollbar-policy=never, hscrollbar-policy=never)`. `propagate-natural-width` MUST be `false` to prevent deep tree indentation from expanding the fixed-width sidebar container indefinitely. Labels inside the tree must use `EllipsizeMode::End` so their minimum width yields to the container constraint.
- **Pinned top row**: The "New Workspace" affordance sits above the outer ScrolledWindow and stays fixed while the workspace list scrolls.
- **No horizontal sidebar scrollbar**: workspace headers and file-tree labels still avoid ellipsizing, but the left sidebar must not expose a horizontal scrollbar. Overflow is clipped by the viewport instead of enabling sideways scrolling.
- **Width presets drive the shell**: `Preferences > Workspace` exposes compact `Small`, `Comfy`, and `Large` options that keep their `20%`, `30%`, and `40%` identities while clamping the visible sidebar width to a comfortable desktop range. The window layer owns the split-view math; the sidebar does not expose a duplicate width control.
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

## Dialog Text Surface Padding (CRITICAL)

Any `GtkTextView`, `GtkSourceView`, or similar document surface placed inside a `GtkScrolledWindow` within a dialog, browser, popover, or other secondary surface must provide explicit inner content padding. Outer shell margins do not pad the document itself.

- Use text-widget margins or an inner padded wrapper so text never renders flush against the frame edge.
- Treat this as a blocking readability issue, not a polish-only follow-up.
- When a repo-local example already solves it correctly, reuse that pattern instead of inventing a one-off layout.

## Dialog Edit/Render Geometry (CRITICAL)

Dialogs, popovers, and browsers that use `GtkStack`, `GtkStackSwitcher`, or another multiplexer for Edit/Render modes must keep the first user-visible mode switch geometry-stable. Hidden stack pages can still participate in measurement, and a Render page that starts as a placeholder can change the parent dialog's natural size by a few pixels when it first renders real content.

- For existing non-empty notes or similar records, pre-render the hidden Render page before presenting the dialog while keeping Edit as the visible mode. The first click on Render must reveal already-measured content, not swap placeholder geometry for content geometry.
- If pre-rendering is not appropriate, make the placeholder and rendered content advertise the same natural size contract.
- Do not rely only on `set_size_request()` on an outer scroller when the inner visible child changes from placeholder to content.
- Add widget coverage for the first Edit -> Render activation, comparing dialog/content natural sizes and text-surface padding before and after activation.

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
- The left pane restores one of the Preferences-driven presets (`20%`, `30%`, `40%`) whenever it is shown, then clamps that preset to the active desktop width before deriving the effective split fraction, while the right pane keeps its quarter-width target.
- Breakpoints collapse the properties pane before the workspace pane so medium-width windows keep the file tree visible longer.
- The properties-pane breakpoint should be tuned from the workspace pane's effective visible width when the workspace pane consumes width so the center editor width stays protected for restored-document infobars and other editor chrome.
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
