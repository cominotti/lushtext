# JSONC Syntax Highlighting

## Status: Deferred

## Description
GtkSourceView ships with JSON and Markdown language specs but does **not** include JSONC
(JSON with Comments) out of the box. JSONC extends JSON with `//` line comments and
`/* */` block comments, used by files like `tsconfig.json`, `settings.json` (VS Code),
and `devcontainer.json`.

## Implementation Plan
1. Create a custom `.lang` file: `data/language-specs/jsonc.lang`
2. Base it on the built-in `json.lang` but add comment contexts
3. Register the custom language spec path via `LanguageManager::set_search_path()`
4. Associate it with file patterns: `*.jsonc`, `tsconfig.json`, `jsconfig.json`,
   `devcontainer.json`, `.vscode/*.json`

## Reference
- GtkSourceView language definition format: https://gnome.pages.gitlab.gnome.org/gtksourceview/gtksourceview5/lang-reference.html
- Existing community JSONC lang files: https://github.com/nicholaschiasson/gtksourceview-jsonc
