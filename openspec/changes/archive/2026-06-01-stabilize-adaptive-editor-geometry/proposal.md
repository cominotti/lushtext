## Why

Several reported resize bugs share the same missing contract: the editor shell does not define a stable, testable geometry model for adaptive side surfaces, persistent chrome, and editor viewport projections. The result is visible flicker when both side surfaces are requested, clipped status-bar chrome at very short heights, collapsed workspace layouts that can obscure the editor's left edge, and a minimap viewport indicator that can drift when width changes reflow the editor.

## What Changes

- Add an adaptive editor geometry contract for the main window shell, covering stable secondary-surface arbitration, persistent bottom chrome, narrow-width editor visibility, and width-budget consistency.
- Require workspace and document-properties presentation decisions to settle from stable layout intent rather than from temporary rendered visibility caused by compact-mode suppression.
- Require supported short-window layouts to preserve the normal status bar and avoid GTK/Adwaita geometry warnings, including when search results or side surfaces are present.
- Require passive narrow-width transitions to preserve the editor gutter and line starts unless the user explicitly opens a compact overlay surface.
- Tighten the minimap contract so the viewport overlay follows the settled active editor viewport after sidebar show/hide, width-only allocation changes, and word-wrap-driven reflow.
- Add regression coverage that exercises the problematic medium-width and short-height bands under headless GTK/Mutter, with warning-gate verification.

## Capabilities

### New Capabilities
- `adaptive-editor-geometry`: Defines stable main-window geometry behavior for adaptive secondary surfaces, persistent chrome, narrow workspace layouts, and shell width budgets.

### Modified Capabilities
- `editor-minimap`: Strengthens the minimap viewport-overlay contract for width changes, sidebar toggles, and word-wrap/reflow interactions.

## Impact

- Affected UI code: `crates/lushtext-core/src/ui/window`, `crates/lushtext-core/src/ui/editor_page`, `crates/lushtext-core/src/ui/search_panel`, and the corresponding GTK templates/resources.
- Affected specs: adds `adaptive-editor-geometry`; modifies `editor-minimap`.
- Affected validation: widget tests for adaptive shell state, minimap viewport projection, short-height allocation, collapsed workspace behavior, and headless runtime warning checks.
- No data format, command-line, or dependency changes are expected.
