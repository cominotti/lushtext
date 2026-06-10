## 1. Live Repro Capture

- [x] 1.1 Add a live visual-geometry capture command or helper that reads the current Automation1 snapshot and writes a bounded live capture manifest.
- [x] 1.2 Generate a runnable visual-geometry scenario from live window state, including size, scale factor, sidebar/minimap state, direction, theme when known, word-wrap when known, and fixture kind when known.
- [x] 1.3 Add explicit override flags for live fields that cannot be inferred safely, and return stable missing-field errors instead of guessing.
- [x] 1.4 Mark optional portal screenshots as context-only evidence and keep invariant proof based on Automation1 snapshots plus headless replay.
- [x] 1.5 Add safe fixture-based tests for live capture output, scenario validation, unknown-field handling, and generated replay command text.

## 2. Final Geometry Settling

- [x] 2.1 Add visual-geometry runner waits for final sidebar hide geometry: sidebar `x == -width`, editor `x == 0`, and relevant editor/minimap rectangles stable across multiple samples.
- [x] 2.2 Add visual-geometry runner waits for final sidebar show geometry: sidebar `x == 0`, editor starts after the sidebar, and relevant editor/minimap rectangles stable across multiple samples.
- [x] 2.3 Preserve sampled geometry rows when final geometry does not settle, and fail with `state-mismatch` or `predicate-timeout` instead of capturing a transitional frame.
- [x] 2.4 Align Automation1 visual readiness or helper-side readiness documentation so `visual-geometry-settled` cannot be mistaken for final animated allocation proof.
- [x] 2.5 Add regression tests proving mid-animation sidebar allocations are rejected by the visual-geometry runner.

## 3. Minimap Threshold Coverage

- [x] 3.1 Add the reproduced `1822x1272` light-theme, word-wrap-enabled, long plain-line, top-of-file minimap/sidebar case to the scenario matrix.
- [x] 3.2 Add at least one nearby intermediate-size guard case or document why the single live-size case is the bounded coverage point.
- [x] 3.3 Verify the new targeted case fails before the minimap rendering bug is fixed, with `pixel-anchor-failed` evidence.
- [x] 3.4 Preserve before/after screenshots, minimap top-edge crops, first-content-row crops, geometry snapshots, and comparison reports for the targeted failure.
- [x] 3.5 Ensure passing 720p, 1080p, 1440p, or `1600x1000` cases do not count as covering the intermediate wrapped long-line threshold case.

## 4. Pixel Evidence Reporting

- [x] 4.1 Extend comparison reports to include app-owned anchor rows alongside screenshot-derived pixel rows for minimap pixel anchors.
- [x] 4.2 Fail and label app-vs-rendered disagreement when Automation1 anchor rows are stable but screenshot-derived rows move beyond the manifest threshold.
- [x] 4.3 Add per-case summary fields for pixel-verified invariant ids, final sidebar/editor geometry, row deltas, failure status, and crop artifact paths.
- [x] 4.4 Update root summary aggregation so `pixel_verified_invariant_ids` includes only invariants actually verified by passing screenshot-derived pixel anchors.
- [x] 4.5 Update artifact-summary output to surface pixel-anchor failure details without embedding unbounded logs or image data.

## 5. Documentation And Policy

- [x] 5.1 Update `docs/automation.md` and `docs/automation-reference.md` for the live visual-geometry capture command, output artifacts, readiness behavior, and failure statuses.
- [x] 5.2 Update visual proof policy checks so rendered-effect work requires named pixel verification and useful per-case evidence.
- [x] 5.3 Update `.agents` rules or skills that instruct agents how to inspect live visual geometry, generate repro scenarios, and verify minimap/sidebar threshold cases.
- [x] 5.4 Run `make check-automation-docs` and update documentation drift checks for any new command, flag, status, snapshot field, or artifact-summary output.
- [x] 5.5 Run `make automation-client-self-test` if the automation client changes.

## 6. Verification

- [x] 6.1 Run the generated live-size scenario through `scripts/visual-geometry-smoke.py --scenario-dir ...` and record the expected failing evidence before the app bug is fixed.
- [x] 6.2 Run the visual-geometry unit tests for scenario loading, final-geometry waiting, pixel-anchor comparison, and artifact summary parsing.
- [x] 6.3 Run `make visual-geometry-smoke` or a justified targeted subset that includes the new intermediate wrapped long-line minimap/sidebar case.
- [x] 6.4 Run `openspec validate capture-live-visual-geometry-regressions --strict`.
- [x] 6.5 Run `git diff --check`.
