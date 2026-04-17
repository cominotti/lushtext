## Why

LushText currently exposes overlapping current-document information and controls in both the bottom status bar and the right properties pane. That duplication weakens the distinction between navigation sidebars, utility panes, and status surfaces, and it makes narrow-window behavior harder to reason about now that the shell already supports both a workspace sidebar and a document-adjacent pane.

This is the right time to tighten that contract. The window already uses nested `AdwOverlaySplitView`s, GNOME Text Editor has converged on a header-bar document-properties toggle plus an adaptive properties surface, and GNOME Builder shows that a richer bottom bar can be a good fit for a development-oriented editor. LushText sits between those products, so the right move is a clearer split of responsibilities rather than collapsing everything into either surface.

## What Changes

- Add a GNOME Text Editor-style document properties toggle to the top-right header bar using `info-outline-symbolic`, with the same primary behavior and `F9` shortcut ownership as the document utility pane.
- Keep a Builder-like bottom bar for high-frequency, glanceable editor state, with encoding and line-ending controls remaining there in this change.
- Keep the `EditorConfig` signal split by surface role: the bottom bar keeps a terse per-file badge, the document properties surface keeps the richer formatting-source explanation, and Preferences remains the only home for the global `Use EditorConfig` toggle.
- Reframe the right properties surface around slower, inspectable document information such as path or location, file size, formatting source, statistics, and file-health details instead of duplicating the bottom bar's quick editor-state controls.
- Keep app-wide editor defaults in Preferences rather than in the document properties surface so the properties surface stays document-specific.
- Reuse the existing dynamic properties breakpoint guard, which already accounts for workspace-sidebar width and editor-content width, instead of introducing a new fixed pane-to-sheet threshold.
- Introduce a compact-layout rule where the workspace sidebar and document properties surface never remain visible together when width is constrained; opening document properties in compact mode closes the workspace sidebar, and the document properties surface adapts from side pane to bottom-sheet behavior on the narrowest layouts.
- Update the existing encoding and file-health workflows so encoding and line-ending actions remain in the bottom bar while file-health inspection and slower document details move into the document properties surface without weakening the current modal safety flows for destructive or lossy actions.
- Leave document type or language controls out of this delta; if LushText wants a read-only language row or an editable type picker later, that should be a follow-up change.

## Capabilities

### New Capabilities
- `document-properties-pane`: Defines the document-properties surface, its header-bar toggle, its relationship to the bottom bar and Preferences, and its adaptive coordination with the workspace sidebar across wide, medium, and narrow window sizes.

### Modified Capabilities
- `encoding-toolkit`: Refines where encoding, line-ending, and file-health controls are surfaced so encoding and line endings stay in the bottom bar while file-health details move into the document properties surface.

## Impact

- Affected UI shell and templates: `resources/ui/window.ui`, `resources/ui/status-bar.ui`, `resources/ui/properties-panel.ui`
- Affected window orchestration and actions: `crates/lushtext-core/src/ui/window/{actions.rs,documents.rs,imp.rs}`
- Affected widget behavior and copy: `crates/lushtext-core/src/ui/{status_bar,properties_panel}/`
- Affected preferences/document-boundary decisions for controls currently duplicated between the properties pane and global settings
- Affected keyboard shortcut ownership and adaptive pane behavior
- Affected OpenSpec and notes that currently describe status-bar-owned document metadata or properties-pane-owned editor defaults, especially `encoding-toolkit` and the older `docs/next/dual-sidebars.md` guidance
