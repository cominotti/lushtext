## Context

This change is superseded by `stabilize-native-minimap-highlight-anchors`. The previous design diagnosed a real failure in visual proof, but chose the wrong product response by proposing a replacement minimap highlight.

The corrected direction is to preserve the exact native `GtkSourceMap` viewport highlight and improve the visual-geometry framework so screenshot-derived anchors catch missing or shifted rendered pixels.

## Goals / Non-Goals

**Goals:**

- Keep this older active change from steering implementation toward a replacement minimap highlight.
- Preserve the useful diagnostic lesson that geometry-only evidence did not catch the reported bug.

**Non-Goals:**

- Do not implement work from this superseded change.
- Do not change the visible minimap effect through this change.

## Decisions

### Decision: Supersede this change with the native-highlight contract

Implementation SHALL follow `stabilize-native-minimap-highlight-anchors` instead of this older planning artifact.

Rationale: the user explicitly requested the exact previous native effect with only the bug fixed.

## Risks / Trade-offs

- [Risk] Future agents may see this active change and apply the stale direction. -> Mitigation: the artifact now points to the native-highlight change and contains no replacement-effect implementation plan.
