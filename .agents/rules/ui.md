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
│   ├── [start] GtkRevealer [sidebar_revealer]
│   │   └── LushtextSidebar
│   │       ├── GtkScrolledWindow (outer, vexpand)
│   │       │   └── GtkBox [sections_box]
│   │       │       └── LushtextWorkspaceSection (per workspace)
│   │       │           ├── GtkSeparator
│   │       │           ├── GtkBox [header: label + add_folder_button]
│   │       │           └── GtkScrolledWindow (inner, propagate-natural-height=true)
│   │       │               └── GtkListView + TreeListModel
│   │       ├── GtkSeparator
│   │       └── GtkBox [footer: "New Workspace" label + button]
│   └── [end] GtkBox [content_box] (vertical)
│       ├── GtkStack [content_stack] (vexpand)
│       │   ├── "tabs": GtkPaned [preview_paned]
│       │   │   ├── [start] GtkBox [editor_box] → AdwTabView → LushtextEditorPage per tab
│       │   │   └── [end] LushtextMarkdownPreview (starts hidden)
│       │   └── "empty": AdwStatusPage
│       └── GtkRevealer [search_panel_revealer] (slide-up, 250ms)
│           └── LushtextSearchPanel (Ctrl+Shift+F workspace search)
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
- **Sidebar clamp**: `clamp_sidebar_position()` in `window/imp.rs` enforces `position <= min(width / 3, max_position)`. When `GtkPaned` is allocated, prefer `main_paned.max_position()` as the authoritative runtime budget and only fall back to `width - content_min - handle_overhead` before allocation. The floor comes from `content_box.measure(Horizontal, -1)`, not the inner stack, because the warning-prone constraint belongs to the actual `GtkBox` end-child GTK is measuring. `handle_overhead` still needs refresh from the live layout budget (`paned_min - sidebar_min - content_min`) after map/realization and after async sidebar mutations such as restored workspaces; do not trust construction-time measurements forever. Called from two places: (1) `WidgetImpl::size_allocate()` — BEFORE `parent_size_allocate` so the position is correct when GTK measures children; (2) `notify::position` on the paned — catches user drag. A `width-request=640` on the window template prevents geometrically impossible layouts. **Do not use property notifications for clamping** — `notify::default-width`/`notify::maximized` fire before the new allocation is applied, so `window.width()` returns the old stale value. **Do not let `notify::position` persist settings during timed paned animations** — keep the clamp live, but defer GSettings writes until the animation completion path so sidebar/preview animations stay smooth.

## Paned Sizing Defense (measure-before-allocate gap)

GTK4's layout cycle runs `measure()` BEFORE `size_allocate()`. During `measure()`, `GtkPaned` distributes width based on its current `position` property, which may be stale from a previous frame. This can cause "Trying to measure GtkBox for width of X, but needs at least Y" warnings even though `size_allocate` corrects the position immediately after.

**Five-layer defense pattern:**

1. **Pre-clamp at construction**: After restoring a paned position from GSettings in `constructed()`, immediately validate it against the restored window width and the content child's measured minimum. Store the original unclamped value in `saved_*_pos` so animations can target it at wider widths.

2. **Explicit `width-request` on the paned end-child**: Set `content_box.set_width_request(content_min)` so the paned's minimum constraint is explicit in the widget tree and visible to GTK's layout negotiation. Refresh that width-request when the realized minimum changes (for example after map or after async children are restored).

   **Measure the legal floor the same way GTK validates it**: when a warning says `Trying to measure ... for width of X, but it needs at least Y`, GTK is comparing `X` against the widget's horizontal minimum measured with `for_height = -1`. Do not budget only from a height-adjusted measurement such as `measure(Horizontal, current_height)`. Resolve the end-child floor as `max(measure(..., -1), measure(..., current_height))`, and set `width-request` from that resolved floor.

3. **Hidden-state restore matches hidden runtime state**: If a paned child starts hidden, restore the live `position` to the same collapsed endpoint the hide animation uses (for the sidebar: 0px), while keeping `saved_*_pos` as the preferred visible width. Do not leave the live paned position expanded while the child is invisible.

4. **Animation-write clamping**: Clamp show targets and per-frame animation writes against the current layout budget **before** calling `GtkPaned::set_position()`. Refresh the measured budget immediately before toggling if async child population may have changed it. `size_allocate` / `notify::position` are backup guards, not the first line of defense for invalid animation ticks.

   GTK source lesson: `GtkPaned` computes legal positions using the handle widget's measured natural size, while `GtkRevealer` scales and rounds its child size during transitions. One-pixel gaps are therefore common in live animations. If a paned animation still logs `GtkBox ... needs at least ...`, do not assume `max-position` alone is authoritative enough; inspect the real child minimum plus the live handle budget and validate with a real app run.

5. **Wrap fully hidden paned children in `GtkRevealer`**: If a pane must animate all the way to zero width, do not expose the raw complex widget tree directly as the `GtkPaned` child. Wrap it in a `GtkRevealer`, animate the paned against that wrapper, and hide the wrapper (`set_visible(false)`) once the pane reaches the collapsed endpoint. During hide, keep the wrapper **revealed** until the paned animation completes, then drop `reveal-child`/`visible` together in the completion path. This keeps the offstage child clipped, avoids shrinking the start-child budget too early, and prevents the paned from reserving handle width while fully hidden.

6. **Keep clamps active until the wrapper actually leaves layout**: Hide actions often flip a logical visibility flag before the `GtkRevealer` wrapper is removed from the paned. Budget refreshes and runtime clamps must keep running while the wrapper's own `visible` property is still true; do not gate them solely on the logical visibility cache.

7. **Animation-time persistence is a completion concern, not a tick concern**: `notify::position` / `size_allocate` may still clamp during a timed paned animation, but they must not enqueue debounced persistence work on every frame. Track an `*_animation_active` flag (or equivalent) so timed animations only write the remembered visible width once from `connect_done` (or the immediate-completion test path), not from every animation tick.

8. **`size_allocate` clamp**: Runs BEFORE `parent_size_allocate` on every layout pass with the definitive allocated width. This remains the runtime backstop for drags, live resize, and any other unexpected position changes.

9. **Heavy sidebar trees need live validation, not widget-only confidence**: For paned animations that wrap a large workspace/sidebar subtree, green widget tests are not enough. A fix can stay smooth in the harness yet still emit geometry warnings or hitch in the real app because restored workspaces change the measured child minimums at runtime. Always validate sidebar toggle changes through `make run` against the user's restored workspaces while watching stderr.

10. **Snapshotting can remove stutter but must not keep hidden live children in layout**: Replacing a heavy sidebar subtree with a frozen `GtkWidgetPaintable` snapshot during the animation can eliminate frame-by-frame relayout cost. But any container used for that swap (`GtkStack`, nested wrappers, etc.) must be verified in the live app to ensure the hidden live child no longer influences `GtkPaned` measurement. If it still affects layout, the geometry warning is not fixed even if the animation feels smoother.

**Rule for future paned widgets:** Any code that restores a `GtkPaned` position from persistent storage must pre-clamp it in the same scope, before the first layout pass. If the pane starts hidden, the live `position` must also be restored to the hidden endpoint used by the hide animation. Any paned with `shrink-end-child=false` should have `width-request` set on the end-child matching the child's measured minimum, and animated show paths must clamp targets before writing them.

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
