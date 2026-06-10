## Why

The previous minimap work fixed the final settled state, but the native
`GtkSourceMap` highlight can still visibly jump during the workspace-sidebar
show/hide animation before ending in the correct position. This matters because
the user-visible bug is now a transient rendered-frame defect, and the current
visual geometry lane waits until the animation is over before it proves pixels.

## What Changes

- Add an animation-frame visual proof lane for sidebar show/hide that captures a
  burst of frames while the workspace sidebar is moving, not only before and
  after final geometry settles.
- Detect the native minimap viewport top edge and first rendered minimap content
  row in each sampled animation frame, correlate them with Automation1 geometry,
  and fail when the rendered rows jump outside a tight tolerance.
- Fix the product path so the native `GtkSourceMap` viewport highlight remains
  visually stable through intermediate editor widths while preserving the exact
  existing native effect, styling, interaction behavior, and marker layering.
- Keep expensive marker scans debounced, but make any lightweight source-map
  wrap, margin, adjustment, and native-frame synchronization needed for the
  slider happen early enough that the next animation frame does not paint stale
  geometry.
- Extend visual summaries, proof-policy metadata, and docs/rules so future
  agents can distinguish final-settle proof from during-animation proof.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `editor-minimap`: The native minimap viewport highlight must remain
  rendered-pixel stable during workspace-sidebar animation frames, not only
  after final layout settles.
- `visual-geometry-invariants`: Visual geometry smoke must support
  animation-frame sampling for rendered-effect invariants and report per-frame
  pixel-anchor evidence.
- `dbus-automation-spine`: Automation readiness and snapshots must expose
  enough bounded frame-phase minimap/sidebar geometry to correlate sampled
  animation frames without leaking document content.
- `automation-client-tools`: Visual capture, replay, artifact summaries, and
  proof-policy checks must surface animation-frame native minimap drift
  separately from final-settle drift.

## Impact

- Affected code: `crates/lushtext-core/src/ui/editor_page/minimap.rs`,
  `crates/lushtext-core/src/ui/editor_page/overscroll.rs`,
  `crates/lushtext-core/src/ui/editor_page/imp.rs`,
  `crates/lushtext-core/src/ui/automation.rs`, window/sidebar animation
  coordination, visual geometry scripts, live capture/replay tooling, and
  scenario manifests.
- Affected tests: targeted animation-frame visual geometry smoke, pixel detector
  fixture tests, minimap unit/widget coverage, Automation1 snapshot/readiness
  tests, proof-policy self-tests, and focused GTK/widget regression tests.
- Affected docs/rules: `docs/automation.md`, `docs/automation-reference.md`,
  `docs/end-user-coverage.md`, `.agents/rules/ui.md`,
  `.agents/rules/widget-wiring.md`, `.agents/rules/build.md`, and relevant
  GTK/visual testing skills.
- No breaking user-facing API changes are expected.
