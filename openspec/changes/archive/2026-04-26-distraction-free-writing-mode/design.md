## Context

LushText already has a GNOME-native application shell with a header bar, tab bar, left workspace sidebar, right document-properties surface, bottom status bar, command palette, search surfaces, fullscreen actions, and Markdown preview modes. The current Markdown preview has three states managed by `window/preview.rs`: editor-only, side-by-side preview, and preview-only through `Alt+P`. The current document-properties surface owns `F9`, while the workspace sidebar has its own requested/rendered state and adaptive split-view behavior.

The existing note in `docs/next/distraction-free-mode.md` is directionally useful but stale in several places: fullscreen already exists, `F9` now belongs to Document Properties, and Markdown preview-only mode already owns `Alt+P`. This change adapts the idea into a new Focus Mode that composes with the current shell rather than replacing it.

## Goals / Non-Goals

**Goals:**

- Add a reversible per-window Focus Mode for sustained writing.
- Keep `F11` as ordinary fullscreen and add `Ctrl+Shift+F11` for Focus Mode.
- Preserve `Alt+P` as the Markdown preview-only shortcut inside and outside Focus Mode.
- Preserve `F9` as the Document Properties shortcut while allowing Focus Mode to suppress the rendered surface.
- Suppress chrome and side surfaces without overwriting the user's persisted sidebar, properties, preview, or minimap preferences.
- Center source editing and rendered Markdown in a readable column while Focus Mode is active.
- Keep `Escape` priority aligned with existing overlays: close search/palette/dialog-like surfaces first, exit Focus Mode only when no higher-priority transient surface is active.
- Keep implementation in workflow-oriented UI modules and avoid introducing a broad new architectural layer.

**Non-Goals:**

- Do not replace the existing fullscreen action or change its shortcut.
- Do not move Markdown preview behavior into a new preview engine.
- Do not redesign document properties, workspace sidebar, notes, or status-bar content outside Focus Mode.
- Do not persist the active Focus Mode state across application launches.
- Do not introduce decorative effects such as vignettes unless they remain small, accessible, and clearly separated from the core behavior.
- Do not require typewriter scrolling for all users; it remains an opt-in Focus Mode preference.

## Decisions

### Focus Mode is a window-shell state, not a document mode

Focus Mode should live in a new `ui/window/focus_mode.rs` workflow module with state stored on `imp::LushtextWindow`. The state should track whether Focus Mode is active, whether the window was already fullscreen before entry, which shell surfaces were temporarily suppressed, and whether the user changed preview state while focused.

This keeps the mode above editor content and Markdown preview. Source editing, rendered preview, save/draft behavior, search, and tab lifecycle continue to use their existing workflows.

Alternative considered: model Focus Mode as another preview/editor mode on `preview_paned`. That would make `Alt+P`, side-by-side preview, and editor visibility fight over the same paned state and would not cover header, status, sidebar, or properties chrome.

### Suppress rendered surfaces without mutating user intent

Focus Mode should add a temporary suppression condition to the existing secondary-surface rendering path. The stored requested state for workspace sidebar and document properties should remain untouched. `F9` and other explicit toggles can still update requested state while focused, but rendering remains suppressed until Focus Mode exits.

This mirrors the existing compact-layout distinction between requested and rendered surfaces, but with a different reason for suppression.

Alternative considered: activate existing sidebar/properties toggle actions on enter and exit. That would persist false values to GSettings and accidentally change the user's desktop layout.

### Preserve Markdown preview-only as the `Alt+P` behavior

Entering Focus Mode should suppress any side-by-side Markdown preview pane because the current `Alt+P` preview-only action intentionally no-ops while side-by-side preview is visible. The previous side-by-side visibility should be remembered and restored on exit only if the user did not make a conflicting preview choice while focused.

When Focus Mode is active, `Alt+P` should keep toggling preview-only mode. The readable-column policy applies both to the editor and to the rendered Markdown surface.

Alternative considered: create a separate "focused rendered mode" shortcut. That would duplicate existing preview-only behavior and violate the user's explicit requirement that `Alt+P` keep working.

### Hide persistent chrome and use a small overlaid focus affordance

The ordinary header bar, tab bar, and status bar should be hidden while Focus Mode is active. To avoid trapping users in an invisible state, the main window overlay should expose a small overlaid focus affordance near the top edge when the pointer moves near the top or keyboard focus requests it. The affordance can contain the document title and a Leave Focus Mode action, but it should not become a second full header bar.

This follows the GNOME pattern of keeping overlaid controls minimal and visible only when useful. It also avoids trying to make `AdwHeaderBar` itself auto-hide, which is not a native header-bar feature.

Alternative considered: wrap the real `AdwHeaderBar` in a revealer and slide it over the content. That risks layout churn and can obscure the writing column with the full application header.

### Readable columns use measured font metrics with clamped margins

Editor and Markdown preview column width should be derived from the active surface's allocated width and font metrics. The default target is 80 columns. The algorithm should clamp margins so narrow windows remain usable and ultrawide windows do not stretch prose across the screen.

For source editing, margin changes can be applied to `GtkSourceView`. For rendered Markdown, the same policy should apply to the preview `GtkTextView`. Existing normal-mode margins must be restored when Focus Mode exits.

Alternative considered: wrap editor and preview in new fixed-width containers. That would fight the existing scrolled-window, minimap, and preview paned structure more than margin-based layout.

### Text origin is shown with a subtle editor-only guide

Focus Mode should show a gentle vertical guide at the source editor's text origin while source editing is active. The guide marks column zero, not a right-margin or per-indent guide, so users can tell whether leading whitespace belongs to the document or is just the readable-column centering margin.

The preferred implementation is a non-interactive drawing layer associated with the editor page. It should draw a low-emphasis 1px vertical line at the current `GtkSourceView` left margin when Focus Mode is active and hide outside Focus Mode. Because the existing readable-column policy already owns the centered source-view margin, the guide should refresh from that same margin on Focus Mode entry, resize, and column-width preference changes.

The guide should be editor-only for this change. Rendered Markdown preview already communicates structure visually, and adding a matching preview guide risks making prose mode feel more technical than calm.

Alternative considered: use GtkSourceView's built-in right-margin guide. That guide marks the configured line-length edge rather than the document's left origin, so it would not solve the indentation-versus-centering ambiguity.

### Typewriter scrolling is opt-in and editor-scoped

Typewriter scrolling should default off. When enabled and Focus Mode is active, cursor movement and edits should keep the cursor line near the vertical center of the editor viewport using existing GtkSourceView scrolling APIs. It must not change session restore semantics or force preview scrolling behavior.

Alternative considered: always center the cursor in Focus Mode. That is more opinionated and can feel disorienting for users who only want a cleaner shell.

## Risks / Trade-offs

- Header/control hiding can feel jarring or trapping if the reveal affordance is unreliable. Mitigation: provide both `Ctrl+Shift+F11` and `Escape` exit paths, and keep the overlaid affordance simple and testable.
- Focus Mode suppression can drift from existing secondary-surface action state if rendered state and requested state are confused. Mitigation: keep explicit helper names for requested state versus focus-suppressed rendering, and add widget tests for `F9` while focused.
- Markdown side-by-side restoration can surprise users if they toggle preview while focused. Mitigation: treat focused preview changes as user intent and restore the previous side-by-side pane only when no conflicting focused preview choice was made.
- Dynamic column margins can interact with line numbers, gutters, minimap, and preview text metrics. Mitigation: hide minimap while focused, keep gutters preference-preserving unless a later test proves them too noisy, and cover narrow/ultrawide allocations in widget tests.
- A visible origin guide can become distracting if it is too strong or appears when it adds no useful information. Mitigation: keep it low-emphasis, editor-only, non-interactive, hidden outside Focus Mode, and tied directly to the actual source-view text origin.
- Typewriter scrolling can create motion discomfort. Mitigation: default off, avoid decorative or continuous animation requirements, and make the behavior interruptible by normal user scrolling.
- Full live validation may need a real GTK session for top-edge reveal and fullscreen behavior. Mitigation: lock deterministic state transitions with widget tests first, then validate live behavior with the GTK debugging harness before implementation is considered complete.
