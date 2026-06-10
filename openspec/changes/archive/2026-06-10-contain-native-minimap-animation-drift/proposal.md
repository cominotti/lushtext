## Why

The animation-frame investigation showed that the native `GtkSourceMap`
viewport highlight can paint one or two frames from stale private slider
geometry while the workspace sidebar changes the editor width, even though the
final settled endpoint is correct. The previous checks missed this because they
proved final geometry after the native map had caught up; the next change must
make rendered animation frames the authority and fix the product path without
changing the native minimap effect.

## What Changes

- Treat native minimap animation drift as a rendered-frame invariant, not a
  final-settle invariant: every sampled frame during workspace sidebar show/hide
  must preserve the required screenshot-derived native minimap pixel anchors
  within the declared tolerance. The native viewport top edge is mandatory; the
  first content row remains available for diagnostics or scenarios that declare
  it explicitly.
- Preserve the exact existing native `GtkSourceMap` highlight effect, styling,
  interaction behavior, marker layering, and final geometry; do not replace,
  re-skin, or recolor the highlight as the fix.
- Change editor/minimap reflow coordination so the page observes real viewport
  allocation through scroll-adjustment page-size changes, freezes the last
  native-rendered minimap pixels while wrapped geometry is in flux, then applies
  one settled repair before revealing the live native map again.
- Strengthen the visual-geometry framework with timestamp-correlated stream
  capture, intermediate-phase proof, per-frame pixel-anchor gates, stale
  frame/geometry-pair detection, and negative self-tests that fail the exact
  classes that escaped earlier.
- Keep app-owned geometry diagnostics explanatory only: screenshot-derived
  pixels decide pass/fail for native rendered effects.
- Preserve reviewable artifacts for failing and passing runs: frame timestamps,
  mapped geometry sample timestamps, phase sequence, max sample skew, anchor
  rows, row drift, representative crops, and final-settle evidence.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `editor-minimap`: require the native minimap viewport highlight to remain
  rendered-pixel stable during workspace-sidebar animation frames while
  preserving the exact native effect.
- `adaptive-editor-geometry`: require shell transitions to avoid exposing
  toolkit-owned editor/minimap rendered effects at stale intermediate geometry
  during consuming side-surface animations.
- `visual-geometry-invariants`: require stream-based, timestamp-correlated,
  per-frame pixel proof for animation-sensitive rendered-effect invariants.
- `dbus-automation-spine`: require bounded animation-phase geometry and timing
  diagnostics that can correlate Automation1 state with captured frames without
  leaking document contents.
- `automation-client-tools`: require live/replay summaries and proof-policy
  checks to distinguish final-settle evidence from during-animation evidence and
  reject stale or incomplete animation proof.

## Impact

- Affected code: editor-page minimap and width-reflow coordination, adaptive
  shell/sidebar animation coordination, Automation1 visual geometry snapshots,
  visual geometry smoke tooling, proof-policy checks, and automation client
  artifact summaries.
- Affected tests: animation-frame visual geometry smoke, visual proof-policy
  self-tests, visual geometry unit tests, Automation1/client self-tests, focused
  GTK/widget geometry tests, and final-settle minimap regressions.
- Affected docs/rules: `docs/automation.md`, `docs/automation-reference.md`,
  `docs/end-user-coverage.md`, `.agents/rules/build.md`,
  `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, and GTK visual
  testing/debugging skills.
- No breaking user-facing API changes are expected.
