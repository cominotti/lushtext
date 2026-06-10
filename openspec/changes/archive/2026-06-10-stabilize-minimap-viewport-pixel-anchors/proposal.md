## Why

This older change originally proposed replacing the native minimap viewport effect. That direction is superseded by `stabilize-native-minimap-highlight-anchors`, which preserves the exact native `GtkSourceMap` highlight and fixes the visual-geometry oracle instead.

## What Changes

- Treat this change as reconciled with the native-highlight direction.
- Do not implement an app-drawn replacement highlight from this change.
- Keep only the useful visual-geometry lesson: rendered minimap pixels need independent screenshot-derived anchors, not geometry-only proof.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `editor-minimap`: superseded by `stabilize-native-minimap-highlight-anchors`.
- `visual-geometry-invariants`: superseded by `stabilize-native-minimap-highlight-anchors`.

## Impact

- This artifact remains only to avoid carrying forward stale planning language.
- Implementation work belongs to `stabilize-native-minimap-highlight-anchors`.
