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
├── GtkPaned (horizontal)
│   ├── [start] LushtextSidebar (always visible)
│   └── [end] GtkStack
│       ├── "tabs": AdwTabView → LushtextEditorPage per tab
│       └── "empty": AdwStatusPage
└── LushtextStatusBar (always visible, full width)
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
