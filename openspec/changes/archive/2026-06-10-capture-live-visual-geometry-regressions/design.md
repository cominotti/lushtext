## Context

The minimap/sidebar regression was reproduced only after inspecting the user's live window. The live window was around `1822x1272`, light theme, word wrap enabled, long plain text lines, minimap at the top of the file, and workspace sidebar initially open. The rendered minimap viewport/top content shifted by 2px when the sidebar reached its fully hidden allocation. Earlier checks missed the defect because they exercised other sizes/themes/fixtures and because `visual-geometry-settled` could return while the sidebar animation was still between final positions.

Current visual-geometry smoke already captures same-session before/after screenshots and screenshot-derived pixel anchors, and the exact live-size synthetic scenario failed once reproduced. The missing framework pieces are: a way to turn live state into a committed or temporary scenario, final-allocation settling for animated sidebar transitions, threshold-oriented scenario expansion, and reports that make pixel drift obvious to agents.

## Goals / Non-Goals

**Goals:**

- Provide a live visual-geometry capture helper that reads the current Automation1 snapshot and emits a runnable visual-geometry scenario plus bounded evidence artifacts.
- Strengthen sidebar/minimap scenario waits so before/after captures happen only after the sidebar/editor allocations have reached their final expected positions and remained stable for multiple samples.
- Add regression coverage for the exact failure class: light theme, word wrap enabled, long plain-line fixture, top-of-file minimap viewport, and intermediate window sizes around the live `1822x1272` geometry.
- Make rendered-pixel evidence authoritative for rendered effects: if app-owned geometry and screenshot-derived rows disagree, the report must say so clearly and fail the invariant.
- Improve summaries so an agent can see the scenario id, final sidebar/editor geometry, pixel row deltas, verified pixel invariant ids, and minimap crop paths without reverse-engineering artifact structure.

**Non-Goals:**

- This change does not fix the minimap rendering bug itself.
- This change does not introduce live-display widget tests; widget tests remain headless-only.
- This change does not rely on GNOME portal screenshots for core live evidence because focus and overview state can contaminate full-desktop screenshots.
- This change does not expose document contents through automation artifacts.

## Decisions

1. Capture live repros from Automation1 snapshots, not desktop screenshots.

The live workflow will first use `scripts/lushtext-automation.py snapshot --json` and the visual geometry snapshot to record window dimensions, visible surface state, scale factor, active tab metadata, minimap/sidebar state, and bounded geometry anchors. Portal screenshots may be saved as optional context, but they are not trusted for invariant proof because they can capture GNOME overview, another focused app, or a terminal covering LushText.

2. Emit generated scenarios as explicit artifacts.

The helper will write a temporary or requested scenario JSON that can be passed to `scripts/visual-geometry-smoke.py --scenario-dir`. The generated manifest will preserve enough live-derived metadata to explain why it exists, including source window size, color-scheme mode when knowable, word-wrap mode when knowable, fixture kind inference, active file path only when it is already exposed by the bounded tab snapshot, and the intended action direction.

3. Add final allocation settling to the visual-geometry runner.

For sidebar hide, the runner will wait until `workspace-sidebar.x == -workspace-sidebar.width`, `editor-viewport.x == 0`, and the relevant minimap surfaces remain stable for multiple samples. For sidebar show, it will wait until `workspace-sidebar.x == 0`, `editor-viewport.x == workspace-sidebar.width`, and the same surfaces are stable. This final-geometry wait complements `visual-geometry-settled`; it does not replace workflow readiness for file load, search, or other async operations.

4. Keep screenshot-derived anchors as the rendered-effect authority.

The app-owned `visual_geometry.pixel_anchors` remain useful diagnostic geometry, but native rendered effects can differ from those rectangles. The comparison report will keep using screenshot detectors for top-edge and content-row anchors and will add an explicit app-vs-rendered diagnostic when the Automation1 anchor rows disagree with detected pixel rows.

5. Add threshold matrix entries instead of only named display sizes.

The minimap/sidebar top scenario will include intermediate geometry around the reproduced failure, starting with `1822x1272`, and should include a nearby guard band when practical. This catches width/height thresholds that do not appear at 720p, 1080p, 1440p, or the existing `1600x1000` maximized-like case.

## Risks / Trade-offs

- Live state inference may be incomplete -> The helper will emit explicit `unknown` fields and require the caller to override ambiguous values rather than pretending a perfect scenario was generated.
- Final allocation waits could hang on real regressions -> The waits will have bounded timeouts, preserve every sampled geometry row, and fail with `state-mismatch` or `predicate-timeout` instead of falling back to fixed sleeps.
- More visual cases increase smoke runtime -> Keep the committed regression matrix targeted and allow case filters for focused reruns.
- Screenshots may contain user paths or visible text -> Automation snapshots remain bounded, and generated artifacts must avoid document contents beyond the already-open fixture identity and screenshot pixels required for visual proof.
- Pixel detectors can become too specific -> Keep detector thresholds named in scenario manifests and preserve before/after crops so failures are reviewable rather than opaque.

## Migration Plan

1. Add the live capture helper and tests for its JSON output using recorded safe snapshot fixtures.
2. Add final sidebar allocation settling to visual-geometry smoke and prove it fails if the sidebar remains mid-animation.
3. Add the `1822x1272` light/wrap/plain-lines minimap/sidebar regression case and verify it fails before the minimap rendering fix.
4. Improve summary/report output and update automation documentation plus proof-policy checks.
5. After the application bug is fixed, rerun the targeted failing scenario and then the broader visual-geometry smoke lane.

Rollback is simple: revert the new helper, scenario entries, and runner/report changes. The core app behavior is not changed by this proposal.
