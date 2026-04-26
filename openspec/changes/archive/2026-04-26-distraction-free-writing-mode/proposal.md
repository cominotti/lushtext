## Why

LushText already supports strong writing primitives, Markdown preview, adaptive side panes, and fullscreen, but it does not yet offer a single, reversible mode for sustained prose work with the surrounding chrome out of the way. A focused writing mode can make LushText feel calmer and more deliberate for writers while preserving the existing GNOME-native shell contracts users rely on.

## What Changes

- Add a per-window Focus Mode that enters fullscreen, hides or suppresses non-writing chrome, and restores the previous shell state when leaving.
- Provide a dedicated `win.toggle-focus-mode` action and `Ctrl+Shift+F11` shortcut without changing the existing `F11` fullscreen shortcut.
- Center editor content in a readable writing column while Focus Mode is active, with a configurable target column width and no effect on normal editor margins.
- Show a gentle source-editor text-origin guide while Focus Mode is active so centered content remains distinguishable from document indentation.
- Apply the same readable-column policy to Markdown preview-only mode so `Alt+P` continues to switch between source editing and rendered Markdown while staying focused.
- Preserve existing shortcut ownership: `Alt+P` remains Markdown preview-only mode, `F9` remains Document Properties, and `Escape` exits Focus Mode only when no higher-priority overlay or inline surface is active.
- Suppress workspace sidebar, document properties, status bar, tab bar, and other persistent chrome while Focus Mode is active without overwriting the user's persisted sidebar or properties preferences.
- Hide the minimap while Focus Mode is active and restore the user's minimap preference when leaving.
- Add preferences for Focus Mode column width and optional typewriter scrolling; typewriter scrolling defaults off.
- Defer decorative effects such as vignette styling unless they can be implemented without reducing readability, accessibility, or GTK-native maintainability.

## Capabilities

### New Capabilities

- `focus-writing-mode`: Defines Focus Mode entry/exit, chrome suppression, shortcut behavior, readable columns, Markdown preview compatibility, and typewriter scrolling.

### Modified Capabilities

- `document-properties-pane`: Document Properties remains owned by `F9`, but Focus Mode suppresses the rendered surface while preserving the requested state for restoration.
- `editor-minimap`: Focus Mode temporarily hides the minimap regardless of the user's minimap preference, then restores normal minimap behavior when Focus Mode exits.

## Impact

- Affected UI shell modules: `crates/lushtext-core/src/ui/window/`, especially action setup, fullscreen integration, secondary-surface layout, preview coordination, and focus/overlay priority handling.
- Affected editor modules: `crates/lushtext-core/src/ui/editor_page/` for readable-column margin policy and optional typewriter scrolling.
- Affected widgets/resources: `resources/ui/window.ui`, `resources/ui/editor-page.ui`, `resources/ui/markdown-preview.ui`, `resources/ui/preferences.ui`, `resources/ui/shortcuts.ui`, and app CSS if a minimal overlaid focus control is added.
- Affected settings: new GSettings keys for Focus Mode column width and typewriter scrolling; Focus Mode active state itself is not persisted.
- Affected existing behavior: normal fullscreen, Markdown preview-only mode, document properties, workspace sidebar, search surfaces, command palette, save/draft flows, and preferences must continue to work outside Focus Mode.
