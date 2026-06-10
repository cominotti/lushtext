## 1. Baseline And Guardrails

- [x] 1.1 Preserve the current final-settle minimap/sidebar proof that passes after layout quiets, so endpoint correctness remains protected while animation-frame work proceeds.
- [x] 1.2 Capture or reuse a failing animation-frame baseline that shows the native minimap viewport top edge or first content row drifting during workspace-sidebar show/hide while final-settle evidence still passes.
- [x] 1.3 Record the reproduced intermediate window size, theme, wrap mode, minimap visibility, sidebar direction, scroll anchor, frame timing, anchor rows, and artifact paths in the change notes or smoke summary.
- [x] 1.4 Audit the current minimap/sidebar experiments and remove any app-owned highlight replacement, re-skin, endpoint-only workaround, or visual freeze that is not a temporary copy of already-rendered native map pixels.
- [x] 1.5 Keep the exact native `GtkSourceMap` effect as the product guardrail before starting implementation changes.

## 2. Animation-Frame Proof Framework

- [x] 2.1 Replace proportional frame/sample matching with timestamp-correlated mapping using stream frame timestamps, action trigger time, Automation1 geometry sample timestamps, and a declared maximum skew.
- [x] 2.2 Add phase detection and reporting for settled, showing, hiding, and intermediate workspace-sidebar/editor geometry samples.
- [x] 2.3 Require at least one evaluated PNG frame mapped to an intermediate transition phase for every animation-frame invariant run.
- [x] 2.4 Detect and fail stale frame-to-geometry pairings instead of allowing them to count as passing evidence.
- [x] 2.5 Gate each evaluated protected frame on required rendered pixel anchors, with the native minimap viewport top edge mandatory and additional anchors enforced only when declared by the scenario.
- [x] 2.6 Preserve bounded per-frame reports with frame index, timestamps, mapped sample timestamp, phase, anchor rows, row deltas, max row drift, sample skew, crop paths, and failure reason.
- [x] 2.7 Update root and case summaries to distinguish animation-frame evidence from final-settle evidence.
- [x] 2.8 Add negative self-tests for final-settle-only proof, screenshot sampling without stream mode, no mapped intermediate PNG, stale timestamp pairing, missing required anchors, and rendered pixel drift hidden by acceptable app geometry.
- [x] 2.9 Update visual proof policy so minimap/source-map/editor-width/sidebar-animation sensitive diffs require valid native-minimap animation-frame artifacts.

## 3. Automation And Client Support

- [x] 3.1 Extend Automation1 visual geometry snapshots with bounded animation timing, phase, surface rectangles, minimap/source-map diagnostics, and readiness blockers needed for frame correlation.
- [x] 3.2 Keep animation sampling and final-settle readiness separate: capture starts from a settled baseline, samples the moving action, then waits for final visual readiness after the stream ends.
- [x] 3.3 Update `scripts/lushtext-automation.py visual-geometry-capture` to generate replayable animation scenarios from live windows, including size, theme, wrap mode, action direction, stream settings, required anchors, and tolerances.
- [x] 3.4 Update `scripts/lushtext-automation.py artifact-summary` to report animation invariant status, mapped intermediate frame count, max row drift, max sample skew, failing frame evidence, and final-settle status separately.
- [x] 3.5 Update automation client self-tests for missing animation fields, incomplete live capture, failing animation artifacts, passing animation artifacts, and final-settle-only artifact rejection.

## 4. Product Fix

- [x] 4.1 Use the failing animation baseline and live instrumentation to verify that page-level `size_allocate` was a dead repair hook for the editor page and that scroll-adjustment page-size changes observe width reflow.
- [x] 4.2 Implement adjustment-driven width/height reflow observation, preserving top and left rest anchors without running expensive marker scans on every animation allocation.
- [x] 4.3 Implement a native-pixel freeze during width-reflow bursts and one settled repair that reapplies fixed native-map geometry, clears stale source-map scroll, refreshes markers, and reveals the live native map after the cover has protected the quiet repaint window.
- [x] 4.4 Preserve workspace-sidebar requested visibility, compact secondary-surface arbitration, document-properties presentation, focus mode suppression, and saved visibility preferences while the minimap is protected during the transition.
- [x] 4.5 Generation-guard reflow settle callbacks, readiness blockers, and freeze state so rapid sidebar toggles cannot leave stale callbacks or stuck readiness.
- [x] 4.6 Preserve minimap navigation, read-only behavior, marker layering, Focus Mode suppression, large-file policy, final settled top anchoring, and existing native highlight styling.
- [x] 4.7 Confirm the product fix does not introduce GTK, Libadwaita, GDK, renderer, accessibility, or allocation warnings at the protected size classes.

## 5. Scenario Coverage

- [x] 5.1 Add or update the native minimap/sidebar animation scenario for the reproduced intermediate desktop size class.
- [x] 5.2 Include control captures at smaller and larger sizes where the bug is not expected, so the detector proves both failing and non-failing size classes intentionally.
- [x] 5.3 Cover sidebar show and sidebar hide directions, or document why one direction is not reproducible after the improved framework.
- [x] 5.4 Cover top-of-document anchoring with word wrap enabled, and keep at least one control for word-wrap-disabled or mid-document state if the existing final-settle scenario already covers it.
- [x] 5.5 Ensure generated fixtures avoid private user document text while still producing realistic minimap content and native highlight pixels.

## 6. Documentation, Rules, And Skills

- [x] 6.1 Update `docs/automation.md` and `docs/automation-reference.md` for animation sampling fields, phase/timing fields, artifact-summary output, statuses, and replay commands.
- [x] 6.2 Update `docs/end-user-coverage.md` to distinguish final-settle minimap proof from during-animation rendered-effect proof.
- [x] 6.3 Update `.agents/rules/build.md`, `.agents/rules/ui.md`, and `.agents/rules/widget-wiring.md` with the new requirement for stream animation evidence on sensitive visual changes.
- [x] 6.4 Update GTK visual testing/debugging skills so agents know to capture real animation frames and reject final-settle-only proof for native rendered effects.
- [x] 6.5 Run the learning workflow after implementation and remove stale guidance that implies settled app geometry is enough for native rendered effects.

## 7. Validation

- [x] 7.1 Prove the animation-frame scenario fails on the preserved baseline for the intended reason before applying the product fix.
- [x] 7.2 Prove the same scenario passes after the product fix with all required frames, anchors, timestamp mappings, intermediate phase proof, and final-settle evidence present.
- [x] 7.3 Run `python3 scripts/test-visual-geometry.py`.
- [x] 7.4 Run `make check-visual-proof-policy`.
- [x] 7.5 Run `python3 scripts/lushtext-automation.py self-test`.
- [x] 7.6 Run `make automation-client-self-test`.
- [x] 7.7 Run `make check-automation-docs`.
- [x] 7.8 Run focused Rust unit/widget tests for minimap geometry, editor width reflow, Automation1 visual geometry, and adaptive sidebar transition staging.
- [x] 7.9 Run `cargo check -p lushtext-core --features test-utils`.
- [x] 7.10 Run `cargo clippy -p lushtext-core --features test-utils --all-targets -- -D warnings`.
- [x] 7.11 Run `openspec validate contain-native-minimap-animation-drift --strict`.
- [x] 7.12 Run `openspec validate --changes --strict`, `openspec validate --specs --strict`, `openspec validate --all --strict`, and `git diff --check`.
