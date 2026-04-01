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

## Syntax Highlighting

Supported via GtkSourceView built-in language specs: JSON, TOML, YAML, Markdown.
JSONC is deferred (requires custom `.lang` file — see `docs/next/jsonc-support.md`).
