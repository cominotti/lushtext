## 1. Reproduce And Lock The Exact Failure

- [x] 1.1 Reconcile the older `stabilize-minimap-viewport-pixel-anchors` active change so implementation does not carry forward app-owned replacement-overlay wording or behavior.
- [x] 1.2 Preserve the supplied `ok.png` and `issue.png` evidence in an ignored diagnostic path or create sanitized minimal committed fixtures that retain the native-highlight failure.
- [x] 1.3 Add a detector-level regression test proving the good fixture passes and the bad fixture fails before changing production minimap behavior.
- [x] 1.4 Document the measured screenshot-derived anchors for the good and bad states: native highlight top edge, first minimap content row, and their vertical delta.

## 2. Build The Independent Pixel Oracle

- [x] 2.1 Extend `visual_geometry_png.py` to detect the first minimap content row from screenshot pixels inside a bounded minimap crop.
- [x] 2.2 Extend `visual_geometry_png.py` to detect the native `GtkSourceMap` viewport highlight top edge from screenshot pixels without accepting fill, background, or marker pixels.
- [x] 2.3 Add fixture and synthetic tests for found anchors, missing anchors, shifted anchors, fill/background false positives, light/dark theme predicates, and malformed crop input.
- [x] 2.4 Ensure Automation1 geometry is used only for broad crop bounds, scale factor, readiness, and diagnostics in the rendered-anchor pass/fail path.
- [x] 2.5 Write bounded per-anchor evidence artifacts: crops, detector reports, coordinates, deltas, thresholds, tolerance, and failure reasons.

## 3. Update Live Visual Geometry Coverage

- [x] 3.1 Update the minimap/sidebar scenario manifest with a native minimap highlight invariant ID distinct from any app-owned overlay wording.
- [x] 3.2 Capture and compare sidebar hidden-to-shown and shown-to-hidden transitions in the same visual session.
- [x] 3.3 Cover constrained and maximized-like sizes, light and dark style, and word-wrap enabled and disabled.
- [x] 3.4 Assert the screenshot-derived native-highlight/content-row vertical delta across captures with 0 px tolerance unless live evidence justifies a documented 1 px tolerance.
- [x] 3.5 Add a mid-file case or relationship check proving the native highlight stays synchronized away from top-of-file after width reflow.

## 4. Fix The Native Minimap Behavior Without Visual Redesign

- [x] 4.1 Audit minimap projection, refresh, allocation, CSS, and dynamic overscroll paths to find why sidebar reflow shifts or removes the native highlight edge.
- [x] 4.2 Implement the narrow production fix while preserving the existing native `GtkSourceMap` fill, border, sizing, CSS effect, and click/drag interaction behavior.
- [x] 4.3 Remove or avoid any app-owned replacement viewport overlay code, inert-native-slider styling, or duplicate visible highlight introduced by the prior direction.
- [x] 4.4 Add widget tests for logical minimap top anchoring, width-only refresh, mid-file reflow, and native navigation parity without depending on compositor pixels.
- [x] 4.5 Inspect before/after screenshots from the targeted visual run and confirm the visible minimap effect is unchanged except for the corrected edge position.

## 5. Proof Policy, Documentation, And Agent Guidance

- [x] 5.1 Update visual-geometry summaries so the native minimap highlight invariant records pixel-anchor pass/fail counts and fixture/live evidence paths.
- [x] 5.2 Update proof policy so minimap rendering, minimap CSS, visual detector, scenario manifest, and Automation visual-geometry changes require the native minimap highlight pixel invariant.
- [x] 5.3 Update `docs/automation.md`, `docs/automation-reference.md`, and `docs/end-user-coverage.md` for screenshot-derived pixel anchors and geometry-only insufficiency.
- [x] 5.4 Update `.agents/rules` and GTK testing/debugging skills so rendered-only effects require screenshot-derived anchors when app geometry could share the bug.
- [x] 5.5 Run the `learn` skill after implementation so the corrected lesson is captured in local guidance.

## 6. Validation

- [x] 6.1 Run `cargo fmt --all -- --check`.
- [x] 6.2 Run focused minimap widget tests through `scripts/run-widget-tests.sh --headless`.
- [x] 6.3 Run visual-geometry PNG detector self-tests, including the good/bad minimap fixture regression.
- [x] 6.4 Run `make automation-client-self-test` if automation artifact summaries or helper parsing change.
- [x] 6.5 Run `make check-automation-docs` if Automation1 docs or fields change.
- [x] 6.6 Run a targeted visual-geometry smoke for the native minimap highlight invariant and inspect preserved screenshots/crops.
- [x] 6.7 Run the unfiltered `make visual-geometry-smoke` or equivalent full runner so proof policy can match the current visual-sensitive diff.
- [x] 6.8 Run `make check-visual-proof-policy`.
- [x] 6.9 Run `openspec validate stabilize-native-minimap-highlight-anchors --strict`.
- [x] 6.10 Run `openspec validate --changes --strict`, `openspec validate --specs --strict`, and `openspec validate --all --strict`.
- [x] 6.11 Run `git diff --check`.
