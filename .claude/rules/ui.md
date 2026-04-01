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
└── GtkPaned (horizontal)
    ├── [start] LushtextSidebar (always visible)
    └── [end] GtkStack
        ├── "tabs": AdwTabView → LushtextEditorPage per tab
        └── "empty": AdwStatusPage
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

## Syntax Highlighting

Supported via GtkSourceView built-in language specs: JSON, TOML, YAML, Markdown.
JSONC is deferred (requires custom `.lang` file — see `docs/next/jsonc-support.md`).
