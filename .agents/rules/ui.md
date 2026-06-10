---
description: UI and GTK widget design rules
globs: "**/*.{rs,ui,css}"
---

# UI Design Rules

## Visual Target

LushText should look and feel like GNOME Text Editor, with these differences:
- Persistent left workspace sidebar on desktop, plus an optional right-side properties panel
- Workspace concept with ordered workspace folders

## Widget Hierarchy

```
LushtextWindow (AdwApplicationWindow)
├── AdwHeaderBar
│   └── GtkToggleButton [document_properties_toggle_button] — toggle properties (action: win.toggle-properties)
├── AdwTabBar → bound to AdwTabView
├── GtkOverlay [window_overlay]
│   ├── LushtextShrinkableBin [window_content_clipper] (min-height 0, clips flexible center content)
│   │   └── AdwOverlaySplitView [workspace_split_view]
│   │       ├── [sidebar/start] LushtextSidebar
│   │       │   ├── GtkBox ["New Workspace" label + button]
│   │       │   ├── GtkSeparator
│   │       │   └── GtkScrolledWindow (outer, vexpand, vertical scrolling only)
│   │       │       └── GtkBox [sections_box]
│   │       │           └── LushtextWorkspaceSection (per workspace)
│   │       │               ├── GtkSeparator
│   │       │               ├── GtkBox [header: label + refresh_button]
│   │       │               └── GtkScrolledWindow (inner, propagate-natural-height=true, propagate-natural-width=false)
│   │       │                   └── GtkListView + TreeListModel
│   │       └── [content] AdwMultiLayoutView [properties_layout_view]
│   │           ├── [slot: primary] GtkBox [content_box] (vertical)
│   │           │   ├── GtkStack [content_stack] (vexpand)
│   │           │   │   ├── "tabs": AdwMultiLayoutView [preview_layout_view]
│   │           │   │   │   ├── [slot: editor] GtkBox [editor_box] → AdwTabView → LushtextEditorPage per tab
│   │           │   │   │   ├── [slot: preview] LushtextMarkdownPreview (starts hidden)
│   │           │   │   │   ├── [layout: editor] AdwOverlaySplitView [preview_split_view]
│   │           │   │   │   │   ├── [content] AdwLayoutSlot "editor"
│   │           │   │   │   │   └── [sidebar/end] AdwLayoutSlot "preview"
│   │           │   │   │   └── [layout: preview] AdwLayoutSlot "preview"
│   │           │   │   └── "empty": AdwStatusPage
│   │           │   └── GtkRevealer [search_panel_revealer] (slide-up, 250ms)
│   │           │       └── LushtextSearchPanel (Ctrl+Shift+F workspace search)
│   │           ├── [slot: properties] LushtextPropertiesPanel
│   │           ├── [layout: pane] AdwOverlaySplitView [properties_split_view]
│   │           │   ├── [content] AdwLayoutSlot "primary"
│   │           │   └── [sidebar/end] AdwLayoutSlot "properties"
│   │           └── [layout: sheet] AdwBottomSheet [properties_bottom_sheet]
│   │               ├── [content] AdwLayoutSlot "primary"
│   │               └── [sheet] AdwLayoutSlot "properties"
│   ├── GtkRevealer [palette_revealer] → LushtextCommandPalette (Ctrl+P)
│   └── GtkRevealer [focus_mode_revealer] → focus-mode affordance
└── LushtextStatusBar (always visible, full width)
    ├── GtkToggleButton [sidebar_toggle_button] — toggle sidebar (action: win.toggle-sidebar)
    ├── GtkBox [message_area_box] — full-width feedback message lane (left, hexpand; small non-flashing start gap; pulse background)
    │   └── GtkLabel [message_label] — feedback message text (caption, ellipsized)
    └── GtkBox [metadata_box] — EditorConfig + line ending + encoding (right, hidden when no tabs)
```

## Libadwaita Widgets to Use

- `AdwHeaderBar` (not `GtkHeaderBar`)
- `AdwTabView` + `AdwTabBar` for document tabs
- `AdwPreferencesDialog` with `AdwComboRow`, `AdwSwitchRow`, `AdwSpinRow`
- `AdwStatusPage` for empty states
- `AdwSidebar` for shallow, sectioned dialog browse rails such as Notes and Local History where each item activates or previews one record
- `AdwAboutDialog` for the about dialog
- `AdwWindowTitle` for header title/subtitle
- `AdwMultiLayoutView` + `AdwLayoutSlot` for adaptive secondary surfaces that need the same child in multiple presentations
- `AdwOverlaySplitView` for explicit secondary surfaces such as workspace, document-properties, and side-by-side Markdown preview panes
- `AdwBottomSheet` for compact utility surfaces such as document properties on narrow windows

## State Extremes And Visibility Matrix (CRITICAL)

Every new or changed UI surface that presents a collection, picker, browser,
search result list, sidebar section, tab-dependent header control, status page,
or dialog must be checked against its real state extremes before the change is
called done.

At minimum, reason through and verify:

- **No items / no context**: no tabs, no workspaces, no notes, no history
  snapshots, no search results, or the relevant empty backing store. Independent
  commands must still be reachable, empty states must be readable, and the UI
  must not create fake rows or require unrelated context just to expose an
  available action.
- **Representative items**: one or a few normal records with realistic labels,
  paths, metadata, action buttons, and selection state.
- **Many or awkward items**: enough rows to require scrolling, plus long names,
  deep paths, mixed item types, or capped result sets when that surface can
  encounter them.
- **Constrained geometry**: the narrow or short layout where the surface is
  still expected to be usable, including adaptive breakpoints and compact
  dialogs when relevant.

Acceptance for these states:

- Empty `AdwStatusPage`-style surfaces must fit their icon, title,
  description, margins, and close affordance without absurdly narrow columns,
  overlapping text, or gratuitous vertical scrollbars. A scrollbar in an empty
  status-only dialog is a failed geometry contract unless the available viewport
  is genuinely smaller than the documented minimum.
- Dense states must scroll only the item/results region. Header controls,
  close buttons, search/filter controls, primary actions, and selection context
  must remain visible and usable.
- Long labels should wrap or ellipsize according to the surface's purpose; they
  must not expand fixed shells, create horizontal scrollbars, or push critical
  controls out of view.
- If the bug or feature is visual, geometry-related, adaptive, or user-reported
  from a screenshot, include widget-level allocation/overflow assertions when
  the invariant is purely geometric. If the invariant is pixel-visible, crosses
  adaptive layout states, or depends on what should remain unchanged while
  another surface moves, add a same-session visual geometry proof with protected
  zero-difference regions and explicit allowed-changing regions. A green action
  test is not enough when the question is "can a human actually see and read
  it?"
- Pixel-visible effects must have screenshot-derived anchors whenever GTK's
  native rendering, CSS, or app drawing could drift independently of our
  computed rectangles. App-owned geometry may bound safe crops, readiness, and
  diagnostics, but it must not be the only proof for a rendered edge,
  highlight, marker, or overlay. Add named visual geometry `pixel_anchors` and,
  when two visible rows/edges must stay aligned, a `relative_pixel_anchors`
  invariant so sidebar, properties, or overlay changes cannot move the effect by
  a few pixels without failing smoke. If the issue only appears at a user's live
  window size, run `scripts/lushtext-automation.py visual-geometry-capture ...`
  with explicit overrides for unknown theme/wrap/fixture fields, then replay the
  generated scenario; generic 720p, 1080p, 1440p, or "maximized-like" passes do
  not cover an intermediate threshold unless that exact class is in the matrix.
- Native `GtkSourceMap` minimap drift is an animation-frame invariant, not only
  a final-settle invariant. If sidebar/properties/editor-width work can reflow
  the active editor while the minimap is visible, capture stream frames with
  native viewport pixel anchors. A product fix may
  temporarily freeze already-rendered native minimap pixels during a detected
  width burst, but it must not draw, recolor, or restyle a replacement
  highlight. The freeze cover must be opaque if the live source map is allowed
  to repaint underneath, or transparent snapshot pixels can leak a stale native
  slider frame. It must reveal the live native map after the settle repair and
  quiet repaint window. Capture the freeze from the user action that is about
  to start the shell transition; passive scroll-adjustment or allocation
  observers should only schedule the settled repair, because they can fire after
  GTK has already invalidated or partially realized the native map.

## Adaptive Dialog Navigation

- When an `AdwSidebar` selection drives the content page of an `AdwNavigationSplitView`, user-selected rows should call `set_show_content(true)` regardless of the split view's current `is_collapsed()` value. `show-content` only affects the visible page while collapsed, but setting it before the adaptive layout settles preserves the user's navigation intent during resize transitions and widget-test collapse simulations. Back buttons can still call `set_show_content(false)` to return to the list page.

## Transient Shell Surfaces

- Window-level overlays and secondary shell surfaces must have a clear dismissal contract. Escape closes one topmost visible dismissible surface, then stops; it must not cascade through every open surface in one press.
- Child-owned popups, dropdowns, dialogs, and focused entries get first chance to handle Escape. The window shell should handle Escape only after child propagation reaches it.
- Command-palette click-away must close the palette from outside the palette geometry even if keyboard focus has moved elsewhere. Inside clicks on the search entry, mode selector, result rows, scrollbars, or child popup roots must keep the palette open and allow the child interaction to continue.
- Focus restoration is part of the close contract. Close command-palette overlays through `close_command_palette()` rather than hiding the revealer directly.
- Test no-context, representative, dense, and constrained states for overlay dismissal, especially when the surface has result lists or can appear above another transient surface such as the search panel or Focus Mode affordance.

## File Tree

- Use `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` (modern GTK4 pattern).
- Do not replace the primary workspace file tree with `AdwSidebar`; it owns filesystem tree expansion, deep-folder focus, file operations, file peek, and watcher reconciliation rather than shallow navigation.
- Never use deprecated `GtkTreeView`.
- Sort: directories first, then alphabetical (case-insensitive).
- Skip hidden files (starting with `.`).
- **Disable TreeExpander's gesture for file rows**: `GtkTreeExpander` installs an internal `GtkGestureClick` (BUBBLE phase) that intercepts click events for ALL rows — even non-expandable files. This prevents `GtkListView`'s built-in double-click activation from firing. The fix: in `connect_bind`, use `expander.observe_controllers()` to find the `GtkGestureClick` and set `propagation_phase` to `None` for file rows (disabling it) and `Bubble` for directory rows (preserving expand/collapse). This runs on every bind (including ListItem recycling). Do NOT use `single-click-activate=true` (changes UX) or CAPTURE-phase gestures (fragile, fails for first file due to `SingleSelection::selected()` timing).
- **Workspace folder reorder hover must be inert**: DnD reorder targets share row overlays with `GtkTreeExpander`, and GTK may ask for child models when a hovered folder auto-expands. During a workspace-folder reorder drag, the inert row-surface drop target must accept/own hover for every file-tree row while only showing or applying drops for valid top-level same-workspace reorder positions. Keep `TreeExpander` targetability stable; do not flip expander state as a hover workaround. Guard activation/focus paths, keep the no-scan child-model fallback defensive only, neutralize GTK `:drop(active)` paint on row/target surfaces, and render only a transparent target plus one fixed-height insertion-line child. Do not let hover expand folders, materialize descendants, restart watches, flicker the expander icon, or paint a filled drop rectangle.
- **Inline row actions that survive deep nesting**: When adding a fixed interactive element (like a hover button) to a deeply nested tree row, DO NOT place it inside the `GtkTreeExpander`'s child box, as the expander's indentation will eventually push the element off-screen. Instead, wrap the `GtkTreeExpander` inside a `GtkOverlay`, and add the button as an overlay widget anchored to the right (`halign=End`). Ensure hover-only actions are also available in a right-click context menu to satisfy GNOME HIG accessibility requirements (since hover is inaccessible to keyboard-only and screen reader users).

## Multi-Workspace Sidebar

- `LushtextSidebar` is an orchestrator: manages the fixed top `New Workspace` affordance, workspace sections, and persistence (`workspaces.json`).
- `LushtextWorkspaceSection` encapsulates per-workspace state: file tree, file context menu, header context menu.
- **Inner ScrolledWindow pattern**: Each section wraps its `GtkListView` in `GtkScrolledWindow(propagate-natural-height=true, propagate-natural-width=false, vscrollbar-policy=never, hscrollbar-policy=never)`. `propagate-natural-width` MUST be `false` to prevent deep tree indentation from expanding the fixed-width sidebar container indefinitely. Labels inside the tree must use `EllipsizeMode::End` so their minimum width yields to the container constraint.
- **Pinned top row**: The "New Workspace" affordance sits above the outer ScrolledWindow and stays fixed while the workspace list scrolls.
- **No horizontal sidebar scrollbar**: workspace headers and file-tree labels still avoid ellipsizing, but the left sidebar must not expose a horizontal scrollbar. Overflow is clipped by the viewport instead of enabling sideways scrolling.
- **Width presets drive the shell**: `Preferences > Workspace` exposes compact `Small`, `Comfy`, and `Large` options that keep their `20%`, `30%`, and `40%` identities while clamping the visible sidebar width to a comfortable desktop range. The window layer owns the split-view math; the sidebar does not expose a duplicate width control.
- **Callback forwarding**: Sections emit file callbacks (activated, renamed, deleted, created) and workspace callbacks (add-folder request, rename, unlist). The sidebar forwards file callbacks to the window and handles workspace callbacks itself.
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

- `GtkPopoverMenu` attached to `header_box` with "Add Folder...", "Rename Workspace", and "Unlist Workspace" actions.
- Actions are in a `ws-header` action group on the section widget.
- Rename shows `AdwAlertDialog` with `extra_child` text entry pre-filled with current name.
- Unlist shows `AdwAlertDialog` confirmation. Files are NOT deleted from disk.

## UI Templates

- Composite templates are authored in `resources/ui/*.blp` (Blueprint). The
  matching `resources/ui/*.ui` files are generated GtkBuilder XML and remain
  committed runtime GResource inputs.
- GResource XML at `resources/dev.cominotti.lushtext.gresource.xml`.
- Compiled by `glib-build-tools` in `build.rs` for dev builds.
- Template `class` attribute must exactly match `ObjectSubclass::NAME`.
- Do not hand-edit generated `.ui` files. Edit the `.blp`, run
  `make blueprint-generate`, then run `make check-blueprint`. For
  geometry-sensitive edits, also run the relevant widget and visual smoke lanes.
  Blueprint drift checks prove generated XML/source fidelity; they do not
  replace widget allocation assertions or same-session visual invariant proof
  when a template edit changes screenshot-visible geometry.
- Blueprint compile warnings are blocking unless they match the documented
  `resources/ui/shortcuts.blp` GTK shortcuts deprecation policy. Blueprint lint
  is a curated advisory gate; use `make lint-blueprint` to keep promoted
  diagnostics clean, bound accepted findings by rule and file, and classify
  geometry-sensitive suggestions before changing layout.
- For UI source-format migrations, preserve a pre-change baseline and compare it
  to the migrated checkout through the same headless Mutter state matrix with
  shared fixtures. Pixel diffs must be zero, or every nonzero difference must be
  intentionally explained and accepted before the change can claim 1:1 UI/UX.
  Use `scripts/compare-blueprint-visuals.sh --baseline-ref <ref>` for this
  reusable proof path; its screenshots and raw logs stay under ignored `build/`
  artifact directories.
- `gtk4-builder-tool validate` is useful for GTK-only templates, but it does not load Libadwaita widget types such as `AdwWrapBox` by itself. When a touched template contains Libadwaita-only types, record the expected standalone limitation and validate the template through the widget harness, which initializes Libadwaita before constructing widgets.

## Status Bar

- Per-window, below the split-view shell, always visible regardless of tab count.
- The left workspace toggle stays at the far left. The document-properties toggle lives in the header bar with the `win.toggle-properties` action and `F9` accelerator.
- The flexible center shell must yield before this bar at tiny heights; keep the editor/sidebar surface inside `LushtextShrinkableBin` so the root window can allocate the status bar inside the visible height.
- `metadata_box` (EditorConfig + line ending + encoding) is hidden via `set_visible(false)` when no tabs are open; the message area and workspace toggle remain available.
- Messages use Adwaita semantic color tokens: `@accent_color` (Info), `@warning_color` (Warning), `@error_color` (Error). These adapt to light/dark mode automatically — no Rust-side dark mode handling needed.
- Repeated visible notification updates briefly pulse the full `message_area_box`, not just the label text. The message area keeps a small non-flashing start margin after the workspace toggle, and pulse selectors must stay scoped to `.status-message-area` so the workspace toggle and metadata controls do not flash.
- Background uses `@headerbar_bg_color` to visually distinguish from the editor area.
- Use the `caption` Adwaita CSS class for status bar text (small font, standard GNOME HIG for secondary UI).

## Inline Alerts

- `LushtextInfoBar` remains the inline notification surface above each editor page; do not replace it with a global status message for restored-file or access-error actions.
- Titles, subtitles, and inline-alert action labels must stay readable on narrow windows by wrapping instead of disappearing or being truncated.
- Workflow actions and the dismiss affordance must share one trailing horizontal action row, with dismiss ordered last. A restored-draft warning should read as `Discard...`, `Save...`, then the close icon in the same group.
- Save/Discard action widths should stay visually balanced so restored-document banners do not jitter while the window is being resized.
- Button contrast changes must use the scoped `.inline-alert-button` class under `.editor-inline-alert`; do not target every nested `button` in the alert.
- The message (`message_box`) and the trailing action row (`actions_box`) are the two children of an `AdwWrapBox` (`content_wrap`). When the editor column is wide enough they share one line with the action group pinned to the trailing edge (`justify=spread` + `justify-last-line`); when the column is too narrow the action group wraps as one atomic unit onto its own row beneath the message. Keep `actions_box` a single `GtkBox` child of the wrap box so its buttons can never be split across rows — `AdwWrapBox` only wraps whole children. `AdwWrapBox` (libadwaita 1.7, available under the `v1_9` feature) must be `ensure_type()`-registered in `class_init` before `bind_template()`.

## Dialog Text Surface Padding (CRITICAL)

Any `GtkTextView`, `GtkSourceView`, or similar document surface placed inside a `GtkScrolledWindow` within a dialog, browser, popover, or other secondary surface must provide explicit inner content padding. Outer shell margins do not pad the document itself.

- Use text-widget margins or an inner padded wrapper so text never renders flush against the frame edge.
- Treat this as a blocking readability issue, not a polish-only follow-up.
- When a repo-local example already solves it correctly, reuse that pattern instead of inventing a one-off layout.

## Dialog Edit/Render Geometry (CRITICAL)

Dialogs, popovers, and browsers that use `GtkStack`, `GtkStackSwitcher`, or another multiplexer for Edit/Render modes must keep the first user-visible mode switch geometry-stable. Hidden stack pages can still participate in measurement, and a Render page that starts as a placeholder can change the parent dialog's natural size by a few pixels when it first renders real content.

- For existing non-empty notes or similar records, pre-render the hidden Render page before presenting the dialog while keeping Edit as the visible mode. The first click on Render must reveal already-measured content, not swap placeholder geometry for content geometry.
- If pre-rendering is not appropriate, make the placeholder and rendered content advertise the same natural size contract. For note-editor `LushtextMarkdownPreview` instances, use the content-surface placeholder path so the final scrolled text surface is visible to measurement before first Render.
- Do not rely only on `set_size_request()` on an outer scroller when the inner visible child changes from placeholder to content.
- Add widget coverage for the first Edit -> Render activation, comparing dialog/content natural sizes, text-origin bounds, and text-surface padding before and after activation. Cover both existing non-empty content and the initially-empty path where the user types note text before clicking Render.

## TextView Child Anchors

`GtkTextView` child anchors do not automatically make embedded widgets fill the visible text column. For anchored Markdown preview widgets that should read as full-width blocks, compute the target width from the text view's allocated `width()` minus left/right margins and apply it to the embedded container with `set_width_request()`.

- Refresh after render, on the preview widget's `size_allocate()`, after readable-column margin changes, and when the text view is mapped or reports a width change.
- Queue one idle refresh after immediate refreshes so code rendered before the preview is mapped can catch the final allocation.
- When the preview lives inside a shell that starts hidden or moves through an Adwaita layout/split-view transition, refresh anchored block widths again after the shell transition settles. Standalone preview-widget tests are primitive coverage; acceptance for hidden-to-visible bugs belongs in window-level tests that assert final allocation and horizontal adjustment state.
- Keep horizontal scrolling inside the embedded block only for real content overflow; do not let the block's natural width create narrow boxes or false scrollbars.

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

- Use `AdwOverlaySplitView` for the outer workspace shell and `AdwMultiLayoutView` for the adaptive document-properties surface instead of an outer `GtkPaned`.
- `workspace_split_view` owns the left workspace pane and stays bound to `win.toggle-sidebar` in the status bar.
- `properties_layout_view` owns the document-properties presentation. `properties_split_view` is the wide right-pane layout, `properties_bottom_sheet` is the compact bottom-sheet layout, and both consume the same `properties` layout slot containing the single `LushtextPropertiesPanel`.
- Do not manually rehost `LushtextPropertiesPanel` with `set_sidebar(None)`, `set_sheet(None)`, or equivalent child moves. Add or adjust `AdwLayoutSlot` presentations instead.
- `win.toggle-properties` is exposed through the header-bar toggle and `F9`, not through the status bar.
- The left pane restores one of the Preferences-driven presets (`20%`, `30%`, `40%`) whenever it is shown, then clamps that preset to the active desktop width before deriving the effective split fraction, while the right pane keeps its quarter-width target.
- Breakpoints switch `properties_layout_view.layout-name` to the compact sheet before collapsing the workspace pane so medium-width windows keep the file tree visible longer.
- The properties-pane breakpoint should be tuned from the workspace pane's effective visible width when the workspace pane consumes width so the center editor width stays protected for restored-document inline alerts and other editor chrome.
- Compact `AdwBottomSheet` presentations that host a `GtkScrolledWindow` must let the scroller advertise a bounded natural height (`propagate-natural-height=true` plus explicit min/max content heights). Without that contract, the sheet can collapse into a thin bottom strip or consume the persistent editor/status-bar budget at short window heights.
- The flexible editor/sidebar/content region must sit behind `LushtextShrinkableBin`, which reports a zero minimum height and clips its child. Do not replace it with a stock bin, scroller, or overlay that propagates the editor/sidebar minimum height into the root window; otherwise the status bar can be allocated below the visible window when vertical space is smaller than the normal-mode floor.
- Allocation-time split-view sync is for live geometry only. `size_allocate()` can clamp the current fractions and update a cached properties breakpoint threshold, but it must not write `workspace-sidebar-width-fraction` / `properties-sidebar-width-fraction` to GSettings or call `AdwBreakpoint::set_condition()` with a newly parsed condition on every animation frame. Persist only explicit user intent or settled animation state, and cache derived thresholds so opening/closing sidebars stays monitor-refresh smooth in the installed Flatpak too.
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

## Large-Buffer Preview And Marker Guardrails

Markdown preview, minimap long-line markers, encoding-warning previews, and
draft/save snapshots all touch the live GTK buffer. Keep these paths bounded:

- Use the shared `ui::buffer_snapshot` helper for any full-buffer text capture.
- Large Markdown buffers should show a clear paused/limited preview state unless
  rendering has been split into bounded main-loop snapshots plus worker-side
  preprocessing.
- Minimap long-line markers are optional; skip the full-buffer marker scan when
  the shared snapshot policy says the buffer is too large for a synchronous
  copy. Ordinary small-document marker behavior must remain covered by tests.
- Replace-preview rows and other large derived result sets should be generated
  from owned data on a worker, with a pending state and stale-result rejection
  keyed by generation counters.
