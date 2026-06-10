## Why

The minimap viewport highlight currently can shift or lose its top-edge treatment when the workspace sidebar changes, even though the first minimap content row remains in place. The previous visual checks did not catch this because they trusted app-computed geometry instead of independently detecting the rendered native `GtkSourceMap` pixels.

## What Changes

- Preserve the exact existing native `GtkSourceMap` viewport highlight effect: no replacement overlay, no restyle, no visual redesign.
- Fix the minimap geometry path so sidebar show/hide and width-only reflow do not move the native highlight edge relative to the first minimap content row.
- Upgrade visual-geometry proof so rendered minimap anchors are detected from screenshot pixels first, with Automation geometry used only to bound safe crops.
- Add regression fixtures from the reported `ok.png` and `issue.png` screenshots so the detector proves it accepts the good state and rejects the observed bad state before production changes.
- Require live visual-geometry scenarios to compare sidebar shown/hidden and hidden/shown transitions using screenshot-derived native-highlight and first-content-row anchors.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `editor-minimap`: Require the native minimap viewport highlight edge to remain visually stable relative to rendered minimap content across shell reflow, without replacing the native effect.
- `visual-geometry-invariants`: Require independent screenshot-derived pixel anchors for rendered-only effects when app geometry could share the same bug as the UI.

## Impact

- Affected code: minimap projection/refresh logic, widget tests for minimap geometry, visual-geometry PNG analysis, scenario manifests, smoke summaries, and proof-policy checks.
- Affected docs/rules: automation visual-geometry documentation and agent testing/debugging guidance for rendered effects.
- Dependencies: no new runtime or image-processing dependencies.
- Compatibility: no intended user-visible visual change except fixing the misplaced or missing native minimap highlight edge.
