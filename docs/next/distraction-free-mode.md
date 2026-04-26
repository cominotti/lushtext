# Focus Mode

## Status: Implemented by `distraction-free-writing-mode`

## Contract

Focus Mode is a reversible per-window writing shell. It enters fullscreen, hides persistent chrome, centers source editing and rendered Markdown in a readable column, and restores the previous shell presentation when it exits. The active Focus Mode state is not persisted across launches.

The mode composes with the current LushText shell instead of replacing it:

- `F11` remains ordinary fullscreen through `win.toggle-fullscreen`.
- `Ctrl+Shift+F11` toggles Focus Mode through `win.toggle-focus-mode`.
- `F9` remains Document Properties. While focused, it changes the requested properties state for after Focus Mode exits, but the properties surface stays suppressed.
- `Alt+P` remains Markdown preview-only mode. While focused, it switches between focused source editing and focused rendered Markdown without leaving Focus Mode.
- `Escape` exits Focus Mode only when higher-priority transient surfaces are not active. Command palette, in-tab search, workspace search, menus, popovers, and dialogs keep priority.

## Shell Behavior

Entering Focus Mode:

1. Records whether the window was already fullscreen.
2. Enters fullscreen if needed.
3. Hides the ordinary header bar, tab bar, status bar, workspace sidebar, and document-properties surface.
4. Suppresses side-by-side Markdown preview so `Alt+P` can operate as preview-only mode.
5. Applies readable-column margins to the active editor, shows the source text-origin guide, and temporarily hides the minimap.
6. Shows a minimal overlaid leave affordance when Focus Mode starts and when the pointer reaches the top edge.

Exiting Focus Mode:

1. Restores ordinary chrome.
2. Restores workspace sidebar and document properties from requested state without writing visibility preferences during suppression.
3. Leaves fullscreen only if Focus Mode entered fullscreen itself.
4. Restores side-by-side Markdown preview only when it was visible before entry and the user did not make a conflicting preview choice while focused.
5. Restores normal editor, preview, and minimap presentation.

## Readable Columns

The readable column uses margin-based layout on the native text surfaces:

- Source editing applies dynamic left and right margins to `GtkSourceView`.
- Source editing also shows a subtle text-origin guide at column zero so Focus Mode centering is visually distinct from document indentation.
- Rendered Markdown applies the same policy to the preview `GtkTextView`.
- The default target is 80 columns, backed by the `focus-mode-target-columns` GSettings key.
- Margins are calculated from allocated width and current font metrics, with minimum margins so narrow windows stay usable.
- Normal-mode margins are restored when Focus Mode exits.

This keeps the editor and preview inside their existing GTK containers, avoiding wrapper layouts that would fight the tab view, minimap, and preview paned structure.

The text-origin guide is editor-only. It is hidden outside Focus Mode and does not appear over rendered Markdown preview.

## Typewriter Scrolling

Typewriter scrolling is optional and defaults off through `focus-mode-typewriter-scrolling`.

When enabled, Focus Mode keeps the source editor cursor near the vertical center after cursor movement or text edits. The behavior is source-editing only; rendered Markdown preview does not attempt to scroll a source cursor, and session cursor/scroll restore semantics stay unchanged.

## Non-Goals

- Do not persist active Focus Mode state.
- Do not replace ordinary fullscreen.
- Do not reassign `F9` or `Alt+P`.
- Do not redesign sidebar, document properties, notes, search, or status-bar content outside Focus Mode.
- Do not add decorative effects such as vignettes unless they remain accessible, readable, and clearly separate from the core writing-mode contract.
