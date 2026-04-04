# EditorConfig Support

LushText respects [EditorConfig](https://editorconfig.org/) files for per-file
formatting overrides. When you open a file, LushText walks up the directory
tree looking for `.editorconfig` files and applies matching properties to that
tab's editor settings.

## Supported Properties

| `.editorconfig` property | Effect | GtkSourceView property |
|--------------------------|--------|------------------------|
| `indent_style` (`space` / `tab`) | Sets spaces vs tabs for indentation | `insert-spaces-instead-of-tabs` |
| `tab_width` | Sets the visual width of a tab character (1-12) | `tab-width` |
| `indent_size` | Sets the number of columns per indentation level (1-12) | `indent-width` |

Properties not listed above are parsed but not yet applied. See
[editorconfig-future.md](next/editorconfig-future.md) for the roadmap.

## Resolution Priority

When multiple sources provide a value for the same setting, the highest
priority source wins:

```
EditorConfig (.editorconfig files)   ← highest priority
GSettings (Preferences dialog)       ← fallback
```

This means a project's `.editorconfig` will override your global preferences
for files within that project, but your preferences still apply to files
without EditorConfig coverage.

## Multi-File Merging

EditorConfig files are merged following the
[EditorConfig spec](https://spec.editorconfig.org/):

1. Starting from the file's directory, walk up to the filesystem root.
2. Parse each `.editorconfig` file found along the way.
3. Closer files take priority over farther ones.
4. Stop walking when a file with `root = true` is encountered.
5. Within each file, later matching sections override earlier ones.

The `unset` keyword is supported: `tab_width = unset` in a closer file
cancels a parent file's `tab_width` value, falling back to GSettings.

## Toggling EditorConfig

EditorConfig support is enabled by default. To disable it:

- Open **Preferences** and toggle **Use EditorConfig** off.

When disabled, all tabs revert to GSettings values immediately. When
re-enabled, all tabs with file paths re-resolve their `.editorconfig`
settings.

## Status Bar Indicator

When EditorConfig overrides are active for the current tab, the status bar
shows an **EditorConfig** label to the left of the encoding indicator. This
helps you understand why a tab's formatting differs from your global
preferences.

## Architecture

The implementation follows a layered architecture:

```
model/formatting_overrides.rs    Pure Rust struct: FormattingOverrides
        |
services/editorconfig.rs         Directory walk + parsing (background thread)
        |
ui/editor_page                   Stores overrides, resolves against GSettings
ui/window                        Triggers resolution on file open / save-as
ui/status_bar                    Shows "EditorConfig" indicator
ui/preferences                   "Use EditorConfig" toggle
```

- **Domain layer** (`FormattingOverrides`): a `Copy` struct with
  `Option<u32>`, `Option<bool>`, `Option<i32>` fields. `None` means
  "no override, use GSettings."
- **Service layer** (`editorconfig::resolve_for_path`): blocking I/O that
  reads `.editorconfig` files. Runs on a background thread via
  `spawn_blocking_then`. Returns `FormattingOverrides`.
- **UI layer**: `EditorPage` replaces `Settings::bind(GET)` for formatting
  settings with manual `connect_changed` handlers that consult the stored
  overrides first. `apply_formatting_settings()` is the single resolution
  point called from both GSettings changes and EditorConfig updates.

## Extending with New Providers

The architecture supports adding new settings providers (modelines, language
defaults) without changing the existing code:

1. Add a new domain type to `model/` (e.g., `ModelineOverrides`).
2. Add a new service to `services/` that parses the source.
3. Have `EditorPage` store both overrides and compose them in priority order
   inside `apply_formatting_settings()`.

The `Option::unwrap_or_else` chain is the natural Rust idiom for first-wins
composition across providers.
