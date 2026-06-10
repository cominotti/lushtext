## 1. Reproduce And Instrument

- [x] 1.1 Run the exact `1822x1272` native minimap/sidebar threshold scenario and preserve the failing comparison report, before/after screenshots, final geometry samples, and anchor crops as the implementation baseline.
- [x] 1.2 Add or update diagnostic fixture tests proving the screenshot detector sees the native viewport top edge and first minimap content row from the real failing artifacts or hermetic synthetic equivalents.
- [x] 1.3 Record the current app-computed minimap anchors, screenshot-derived anchors, source-view visible rect, source-map visible rect or adjustment state, and final allocation samples for the failing case.
- [x] 1.4 Confirm the native highlight remains the existing `GtkSourceMap` effect before any application fix is attempted.

## 2. Native Minimap Fix

- [x] 2.1 Implement an upstream-informed native slider diagnostic estimate that accounts for editor visible rect, editor/map document height, source-map visible rect or adjustment state, final allocation, border, and minimum slider height.
- [x] 2.2 Expose bounded native minimap diagnostic rows in Automation1 visual geometry snapshots with explicit absence reasons and no document-content leakage.
- [x] 2.3 Strengthen visual-geometry readiness so pending minimap refresh, source-map resize/draw invalidation, dynamic overscroll refresh, and required post-frame minimap sampling can block `visual-geometry-settled`.
- [x] 2.4 Implement the conservative post-reflow native minimap refresh path after sidebar and width-only allocation changes while preserving top scroll, wrap/margin sync, marker refresh, native slider styling, and interaction behavior.
- [x] 2.5 If the conservative refresh path does not stabilize the rendered pixels, evaluate a narrowly scoped `GtkSourceMap::set_view` rebind fallback and accept it only if tests prove no visual, focus, navigation, or marker regression.
- [x] 2.6 Add unit and widget coverage for the native slider diagnostic estimate, no-replacement-overlay invariant, top anchoring, width-only reflow refresh, and semantic marker projection after reflow.

## 3. Rendered-Pixel Proof Framework

- [x] 3.1 Update visual geometry smoke to require screenshot-derived native minimap anchors as the pass/fail oracle for rendered highlight scenarios.
- [x] 3.2 Add final-frame rendered-anchor stability sampling after final allocation settling, with bounded timeout/failure artifacts for stale-frame or mid-animation captures.
- [x] 3.3 Keep the reproduced `1822x1272` threshold case and add a small nearby guard band plus show/hide directions, light/dark controls, wrap-enabled/wrap-disabled controls, and at least one mid-document relationship case.
- [x] 3.4 Update artifact summaries to show scenario id, final geometry, detected anchor rows, row deltas, relationship deltas, app-vs-rendered diagnostics, crop paths, and pixel-verified invariant ids.
- [x] 3.5 Update proof-policy checks so minimap/source-map/sidebar/editor-geometry or visual-proof changes require a passing native minimap rendered-proof artifact, not geometry-only evidence.
- [x] 3.6 Update live visual-geometry capture and replay generation so live minimap/sidebar captures include native minimap pixel anchors, invariant id, final geometry requirements, and explicit missing-field behavior.

## 4. Documentation And Rules

- [x] 4.1 Update `docs/automation.md` and `docs/automation-reference.md` for new visual geometry fields, readiness blockers, client outputs, status names, and artifact-summary fields.
- [x] 4.2 Update `docs/end-user-coverage.md`, `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, and relevant GTK/visual testing skills to state that rendered toolkit/CSS effects require screenshot-derived anchors.
- [x] 4.3 Update comments only where they explain the native `GtkSourceMap` geometry contract or the reason app geometry cannot be the rendered-effect oracle.
- [x] 4.4 Run the learning workflow after implementation so repo guidance captures the new native-rendered-effect proof rule without stale instructions.

## 5. Validation

- [x] 5.1 Verify the targeted native minimap threshold scenario fails on the baseline and passes after the application fix without changing the native effect.
- [x] 5.2 Run `python3 scripts/test-visual-geometry.py` and confirm detector, summary, and proof-policy fixture coverage passes.
- [x] 5.3 Run the targeted visual geometry smoke case for `1822x1272` hide/show plus the selected guard-band and control cases, preserving artifact summaries.
- [x] 5.4 Run `make visual-geometry-smoke` or the broadest locally supported visual-geometry lane and confirm native minimap invariant ids are pixel-verified.
- [x] 5.5 Run `make check-automation-docs` and `make automation-client-self-test`.
- [x] 5.6 Run focused Rust/widget tests for minimap geometry, Automation1 visual geometry, and editor-page reflow behavior.
- [x] 5.7 Run `openspec validate fix-native-minimap-rendered-drift --strict`, `openspec validate --changes --strict`, `openspec validate --specs --strict`, `openspec validate --all --strict`, and `git diff --check`.
