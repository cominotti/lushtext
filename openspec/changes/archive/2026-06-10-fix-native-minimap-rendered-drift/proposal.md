## Why

The minimap/sidebar defect is still reproducible because the previous framework work proved app-computed geometry, while the visible native `GtkSourceMap` pixels drifted inside a stable minimap shell. We need a fix that preserves the exact native highlight effect and a proof framework that treats rendered pixels, not our own geometry model, as the authority for toolkit-owned visual effects.

## What Changes

- Fix the native minimap viewport highlight so sidebar show/hide, width-only editor reflow, word-wrap layout changes, and top-of-document anchoring do not shift the rendered top edge or first minimap content row after the final layout settles.
- Preserve the existing native `GtkSourceMap` slider effect, interaction behavior, neutral styling, fill, border, and marker layering; do not replace it with an app-owned overlay unless a separately approved change explicitly accepts a visual replacement.
- Align LushText's diagnostic projection with upstream `GtkSourceMap` slider math closely enough to explain native rendered pixels, including the map's own visible rect/adjustment and post-frame slider allocation behavior.
- Strengthen `visual-geometry-settled` and smoke tooling so native rendered-effect cases wait for final sidebar/editor/minimap geometry and then verify stable screenshot-derived pixel anchors across final frames.
- Make visual proof policy fail when native minimap highlight coverage is skipped, geometry-only, missing app-vs-rendered diagnostics, or not run at the reproduced intermediate size class.
- Preserve bounded artifacts that let agents see the final geometry, screenshot-detected anchor rows, app-vs-rendered disagreement, crop paths, and environment without exposing document contents.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `editor-minimap`: The native minimap viewport highlight must remain rendered-pixel stable after sidebar and width-reflow transitions while preserving the exact native effect.
- `visual-geometry-invariants`: Native or CSS-rendered effects require screenshot-derived pixel anchors as the pass/fail oracle; app geometry can bound crops and diagnose mismatches but cannot satisfy the invariant by itself.
- `dbus-automation-spine`: Visual geometry snapshots and readiness need bounded native-minimap diagnostics that expose enough geometry to explain rendered-effect drift without exposing document contents.
- `automation-client-tools`: Visual geometry capture, replay, summaries, and proof-policy checks must make live-size rendered-effect regressions obvious and mandatory for visual-sensitive minimap work.

## Impact

- Affected code: `crates/lushtext-core/src/ui/editor_page/minimap.rs`, `crates/lushtext-core/src/ui/editor_page/overscroll.rs`, `crates/lushtext-core/src/ui/editor_page/imp.rs`, `crates/lushtext-core/src/ui/automation.rs`, visual geometry scripts, proof-policy scripts, and targeted visual scenario manifests.
- Affected tests: minimap projection unit/widget coverage, visual geometry smoke, detector fixture tests, automation client self-tests, proof-policy self-tests, and strict OpenSpec validation.
- Affected docs/rules: `docs/automation.md`, `docs/automation-reference.md`, `docs/end-user-coverage.md`, `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, and relevant GTK/visual testing skills.
- No breaking user-facing API changes are expected.
