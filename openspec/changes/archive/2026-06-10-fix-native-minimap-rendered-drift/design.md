## Context

The reproduced defect is not a whole-minimap allocation move. In the failing `1822x1272` light-theme, word-wrap, top-of-file case, the final geometry samples show the workspace sidebar, editor viewport, minimap shell, source map, and marker strip all reaching stable final positions. The app-computed minimap viewport top anchor remains at `y=52`, and the app-computed first content row remains at `y=51`. The screenshot-derived pixels disagree: the native viewport top edge moves `52 -> 50`, and the first rendered content row moves `53 -> 51`.

That means the bug lives inside the rendered native `GtkSourceMap` effect, not in the outer shell. The previous framework work was useful, but it still let app-owned geometry speak too loudly. For toolkit-owned effects, our geometry can only choose safe crops and explain diagnostics; it cannot prove the visible pixels are correct.

The relevant upstream constraint is that `GtkSourceMap` exposes only a small public map API. The viewport highlight is a private slider widget rendered by GtkSourceView. Upstream computes that slider from the editor visible rect, the editor document height, the map document height, the map's own visible rect, border, natural slider height, and a frame/tick update path. LushText's current diagnostic projection mirrors line geometry, but it does not fully mirror the private slider's own visible-rect subtraction or frame update behavior. This explains why app anchors stayed stable while rendered pixels drifted.

## Goals / Non-Goals

**Goals:**

- Preserve the exact existing native `GtkSourceMap` viewport highlight effect.
- Fix the sidebar show/hide and width-only reflow bug for the native rendered highlight, not merely for app-computed geometry.
- Make LushText's bounded minimap diagnostics explain the native slider's rendered position more honestly.
- Strengthen readiness and visual smoke so final sidebar/editor/minimap geometry and post-frame native minimap rendering are both settled before proof captures.
- Make screenshot-derived anchors the pass/fail oracle for native minimap highlight stability.
- Preserve bounded artifacts that an agent can inspect quickly: final geometry, screenshot row detections, app-vs-rendered diagnostics, crop paths, and environment.

**Non-Goals:**

- Do not draw an app-owned replacement viewport overlay.
- Do not restyle, hide, disable, or visually clone the native highlight as a shortcut.
- Do not change minimap navigation, marker colors, marker semantics, preferences, size availability, or Focus Mode policy.
- Do not depend on whole-window golden screenshots.
- Do not expose document text or private persistence identifiers through automation artifacts.

## Decisions

### Decision: Preserve the native slider as the product surface

The implementation SHALL keep `GtkSourceMap`'s native slider as the only visible viewport highlight. Fixes may adjust refresh timing, wrapping/margin synchronization, allocation invalidation, diagnostic projection, or frame readiness, but they must not replace the effect with a second app-drawn overlay.

Rationale: the user's requirement is exact visual continuity. An app overlay could be made easier to test, but it would change the product surface and could mask the native bug instead of fixing it.

Alternatives considered:

- Draw a LushText-owned overlay. Rejected because it violates the exact-effect requirement.
- Disable or hide the native slider and replace it with custom CSS/drawing. Rejected for the same reason and because it risks navigation/rendering drift from upstream behavior.

### Decision: Mirror upstream slider math for diagnostics

LushText's diagnostic geometry should estimate the native slider using the same conceptual inputs as upstream `GtkSourceMap`: editor visible rect, editor end-iter height, map end-iter height, map visible rect, map border, slider natural minimum height, and final map allocation. The estimate is diagnostic and crop-selecting, not authoritative.

The current line-projection helpers remain useful for semantic marker geometry and content-row bounds, but native viewport highlight diagnostics need a separate "native slider estimate" path. Automation should expose both enough source values and the final estimated rect so failures can explain whether the disagreement came from editor reflow, map adjustment, rounding, or stale frame state.

Alternatives considered:

- Keep the existing line-projection as the native slider model. Rejected because it already disagrees with the rendered pixels in the failing case.
- Search for the private slider child through GTK internals. Rejected because relying on private widget structure is more brittle than using public text-view geometry plus screenshot truth.

### Decision: Treat final frame stability as part of readiness

After sidebar show/hide or width-only editor reflow, the app should settle in three layers before proof capture:

1. workflow readiness, such as file load and minimap refresh queues;
2. final allocation readiness, such as sidebar/editor/minimap surfaces reaching their target positions and staying stable;
3. native rendered-effect readiness, such as the source map receiving its post-reflow invalidation/frame and screenshot anchors staying stable across final captures.

Implementation should first try the conservative native path: preserve top scroll when applicable, sync source-map wrapping/margins, refresh dynamic overscroll, schedule minimap refresh, and queue resize/draw on the source map after final width allocation. If that is insufficient, a narrowly scoped rebind of `source_map.set_view(source_view)` may be considered as a last resort and must prove it does not change the visible effect or interaction behavior.

Alternatives considered:

- Add fixed sleeps after sidebar animation. Rejected because the final-geometry samples already prove the bug survives after allocation settles.
- Treat `visual-geometry-settled` as complete once app queues are empty. Rejected because the failure is specifically an app-vs-rendered mismatch.

### Decision: Screenshot pixels decide native rendered-effect pass/fail

For the native minimap highlight, the visual smoke runner must detect the relevant rows from screenshots:

- native viewport top edge;
- first rendered minimap content row;
- optionally viewport fill and bottom edge for richer diagnostics.

Automation geometry may bound the minimap crop and provide app-vs-rendered diagnostics. A scenario passes only when the screenshot-derived anchors satisfy the declared invariant after final geometry and frame stability. If app geometry says stable and rendered pixels drift, the scenario fails.

Alternatives considered:

- Exact whole-minimap crop equality. Rejected because legitimate document reflow changes minimap body pixels.
- Geometry-only proof. Rejected because this is the exact blind spot that let the bug continue.

### Decision: Keep the reproduced threshold as a first-class case

The `1822x1272` live-size case is not an incidental artifact; it is the known reproducer. The visual matrix should keep that exact case and add a small guard band around it when runtime permits. Standard display sizes such as 720p, 1080p, 1440p, and `1600x1000` remain useful controls but do not replace the threshold case.

The scenario should cover both sidebar hide and sidebar show, top-of-file viewport, word wrap enabled, long plain-line content, and light theme. Broader confidence should add dark theme, word-wrap-disabled control, mid-document marker relationship coverage, and short-document/full-document-fit coverage where appropriate.

Alternatives considered:

- Rely on conventional viewport sizes only. Rejected because the user already verified some conventional sizes do not reproduce the defect.
- Keep the live-size case as an uncommitted manual note. Rejected because future agents would lose the exact coverage point.

## Risks / Trade-offs

- [Risk] Upstream private slider math changes in a future GtkSourceView release. -> Mitigation: keep screenshot pixels authoritative and document that the diagnostic estimate is version-family informed, not a product oracle.
- [Risk] Pixel detectors become theme- or renderer-fragile. -> Mitigation: verify with light/dark fixtures, use bounded row relationships rather than full golden images, and preserve crops for human review.
- [Risk] A refresh-only fix hides a stale-frame race without solving geometry. -> Mitigation: require app-vs-rendered diagnostics plus repeated final-frame pixel stability.
- [Risk] A rebind fallback could disturb navigation or focus behavior. -> Mitigation: treat rebind as last resort and require navigation, focus, and marker coverage before accepting it.
- [Risk] More visual cases slow local smoke. -> Mitigation: keep the mandatory matrix targeted, support case filters, and place broad sweeps in scheduled or release validation.
- [Risk] Screenshots can contain visible document text. -> Mitigation: use generated fixtures for committed smoke, keep Automation1 state bounded, and summarize image paths rather than embedding image data.

## Migration Plan

1. Add native-slider diagnostic fields and tests that compare the current app estimate, upstream-informed estimate, and rendered screenshot anchors on existing artifacts.
2. Implement the conservative native refresh path after width reflow and sidebar transitions.
3. Re-run the exact `1822x1272` failing case and confirm rendered anchors are stable without changing the native effect.
4. Add final-frame pixel stability checks and proof-policy gating for native minimap rendered-effect invariants.
5. Expand targeted controls: show/hide directions, light/dark, wrap on/off, conventional sizes, threshold guard-band sizes, and at least one mid-document marker relationship case.
6. Update automation docs, visual coverage docs, repo rules, and skills so future visual-sensitive work inherits the rendered-effect proof rule.

Rollback should leave the native `GtkSourceMap` slider visible and interactive. If the visual proof tooling is noisy, narrow the detector or matrix; do not replace the product effect to satisfy the test.

## Open Questions

- Whether queueing resize/draw plus post-allocation minimap sync is sufficient, or whether the implementation needs a scoped `set_view` rebind after final width reflow.
- The exact guard-band size around `1822x1272`; implementation should determine the smallest useful width/height sweep from fast local runs.
