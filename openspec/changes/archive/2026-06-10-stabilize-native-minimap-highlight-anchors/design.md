## Context

The reported defect is not that the minimap lacks a viewport highlight; it is that the exact native `GtkSourceMap` highlight effect shifts or loses its top-edge treatment when workspace-sidebar state changes. The prior framework work added useful visual-geometry infrastructure, but it failed this case because the proof trusted LushText-computed minimap geometry and then sampled pixels near that geometry. When the app computation shares the UI bug, that style of check can still pass.

The corrected design treats the native minimap highlight as the product contract. LushText must preserve the existing neutral `GtkSourceMap` slider effect, including its fill, border, size, and interaction behavior. Visual proof must independently identify rendered pixels in screenshots, using Automation1 geometry only to select safe bounded crops and preserve privacy.

Implementation evidence: the supplied screenshots are preserved in
`build/diagnostics/minimap-viewport-pixel-anchors/`. In the bounded right-side
minimap crop `x=1580, y=90, width=158, height=30`, the good screenshot finds
the native bright viewport-edge row at `y=98`, the first rendered minimap
content row at `y=101`, and a top-edge/content delta of `-3`. The bad screenshot
still finds the content row at `y=101`, but the bright native top edge fails the
12-pixel contiguous-run threshold; its best later candidate is at `y=110` with
only 10 contiguous pixels. The detector therefore proves the reported bad state
is missing or shifting the native top-edge treatment instead of merely moving
app-computed geometry.

## Goals / Non-Goals

**Goals:**

- Preserve the exact existing native `GtkSourceMap` viewport highlight effect.
- Fix the sidebar show/hide and width-reflow bug without replacing, restyling, or duplicating the highlight.
- Add a screenshot-derived oracle that detects the rendered top edge of the native highlight and the first minimap content row independently of app-computed viewport bounds.
- Prove the detector against the supplied `ok.png` and `issue.png` pair before relying on live smoke results.
- Make visual-geometry summaries and proof policy fail when the required minimap native-highlight invariant is skipped, missing, or satisfied only by geometry-level evidence.

**Non-Goals:**

- Do not add an app-owned replacement viewport overlay.
- Do not make the native `GtkSourceMap` slider visually inert.
- Do not change minimap marker colors, marker semantics, navigation behavior, preferences, or file-size availability.
- Do not introduce new image-processing dependencies.
- Do not require whole-window golden screenshots.

## Decisions

### Decision: Keep the native slider as the only visible viewport highlight

The implementation SHALL preserve the existing `GtkSourceMap` slider rendering path and CSS effect. Fixes must target projection, refresh, allocation, or styling regressions that cause the native highlight to shift; they must not install a second visible overlay or substitute a new app-drawn effect.

Rationale: the user's requirement is exact visual continuity. Replacing the effect could make the testable surface easier, but it changes the product behavior and risks losing native source-map interaction fidelity.

Alternatives considered:

- Draw an app-owned overlay. Rejected because it changes the effect and hides the original bug instead of preserving the requested native behavior.
- Replace the whole minimap renderer. Rejected as disproportionate and risky.

### Decision: Use screenshot-derived anchors as the primary oracle

The visual-geometry lane SHALL detect two key anchors from the screenshot pixels themselves:

- the first rendered minimap content row
- the top edge of the native viewport highlight

Automation1 geometry may provide broad minimap/source-map crop bounds, scale factor, and readiness metadata, but it must not be the source of truth for the anchor row. The comparison should assert the vertical delta between these screenshot-derived anchors across sidebar hidden/shown and shown/hidden captures.

Rationale: this prevents self-fulfilling tests. If the app computes the wrong projection, a screenshot detector can still observe that the rendered effect moved.

Alternatives considered:

- Continue sampling near app-computed viewport bounds. Rejected because it already missed this failure mode.
- Exact crop equality over the whole minimap. Rejected because legitimate width reflow, antialiasing, and content repaint can change the minimap body while the anchor relationship remains correct.

### Decision: Add fixture-level detector tests from the reported screenshots

The PNG detector SHALL include a fixture test in which the good screenshot passes and the bad screenshot fails for the native-highlight/content-row relationship. This fixture test must run before production changes are considered verified.

Rationale: the detector must prove it recognizes the real reported defect, not a simplified synthetic proxy.

Alternatives considered:

- Use only synthetic PNG fixtures. Synthetic fixtures remain useful for edge cases, but by themselves they do not prove the actual visual oddity is covered.
- Use only live smoke. Live smoke proves current behavior, but without a known failing fixture it cannot prove the detector would have caught the original bug.

### Decision: Cross-check geometry after pixel truth, not before

After screenshot anchors are detected, the runner may compare those anchors with Automation1 geometry to aid diagnosis. A geometry mismatch should be reported as diagnostic evidence, but the invariant pass/fail result comes from the rendered anchor relationship.

Rationale: app geometry is still valuable for privacy-bounded crop selection and debugging, but it must not be able to make the visual invariant green on its own.

## Risks / Trade-offs

- [Risk] Pixel detectors become too theme-specific. -> Mitigation: scope detection to the existing native neutral effect, cover light and dark variants, and keep detector thresholds documented in fixture tests.
- [Risk] Antialiasing or compositor rounding causes a one-pixel row change. -> Mitigation: allow at most a narrowly documented 1 px tolerance only after fixture and live evidence prove it is necessary.
- [Risk] Broad crops include unrelated minimap marks or syntax colors. -> Mitigation: detect both anchor kind and row relationship, and write bounded crop reports when competing pixels are found.
- [Risk] Fixture screenshots contain user document content. -> Mitigation: keep fixture use bounded to ignored diagnostic paths or sanitized committed fixtures, and never expose document text in Automation1 artifacts.
- [Risk] The earlier app-owned overlay proposal remains active and confuses implementation. -> Mitigation: this change explicitly supersedes that direction for the minimap viewport effect and validation should reject app-owned replacement-overlay wording in this change.

## Migration Plan

1. Keep the current native slider CSS/effect as the baseline.
2. Add or repair detector fixture tests so `ok.png` passes and `issue.png` fails.
3. Add live same-session visual scenarios that capture sidebar hidden/shown and shown/hidden transitions and compare screenshot-derived anchor deltas.
4. Fix minimap refresh/projection so the live scenarios pass without visual effect changes.
5. Update Automation1 visual geometry only as needed for safe crop bounds, diagnostic metadata, and artifact summaries.
6. Update docs, rules, and skills so agents know that rendered-only effects need independent pixel anchors when app geometry could share the bug.

Rollback should leave the native `GtkSourceMap` slider effect in place. If detector changes are noisy, narrow or improve the detector and keep the regression fixture; do not replace the visual effect to satisfy the test.

## Resolved Questions

- The reported screenshot pair may remain in the ignored diagnostic path, while
  detector self-tests synthesize the same minimal color signature when the
  diagnostic screenshots are absent. This keeps clean checkouts hermetic without
  discarding the local evidence.
- Top-of-file live scenarios use 0 px delta tolerance for the screenshot-derived
  native-highlight/content-row relationship. Mid-file scenarios compare the
  viewport edge to a rendered search marker because legitimate width reflow can
  move both rows together while preserving the synchronized relationship.
