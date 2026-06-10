## 1. Reproduce And Capture Animation Drift

- [ ] 1.1 Run the current native minimap/sidebar scenario and preserve final-settle proof showing the endpoint remains correct.
- [ ] 1.2 Add a targeted animation-frame capture mode that samples frames immediately after the workspace sidebar show/hide action and before final geometry settles.
- [ ] 1.3 Capture the reported intermediate-size case with the animation-frame mode and preserve per-frame screenshots, snapshots, detected rows, timing, and failure artifacts as the baseline.
- [ ] 1.4 Confirm whether the transient occurs on sidebar hide, sidebar show, or both, and whether it requires top-of-file, word wrap, light theme, or the reproduced intermediate width.
- [ ] 1.5 Record per-frame app-vs-rendered diagnostics: sidebar/editor/minimap geometry, native minimap diagnostics, source-map adjustment, source-view adjustment, document-height ratio inputs, and detected pixel rows.

## 2. Animation-Proof Tooling

- [ ] 2.1 Extend visual geometry scenario manifests with animation sampling fields: trigger action, frame count or duration, sample cadence, required anchors, row tolerances, and invariant id.
- [ ] 2.2 Implement bounded animation-frame sampling in `scripts/visual-geometry-smoke.py` without replacing existing before/after final-state capture.
- [ ] 2.3 Add per-frame native minimap pixel-anchor detection for the viewport top edge and first content row, including crop/frame artifacts for failed and representative passing frames.
- [ ] 2.4 Update root and per-case summaries to expose animation invariant ids, sampled frame count, maximum row drift, failing frame details, and final-settle evidence separately.
- [ ] 2.5 Add or update visual geometry self-tests covering animation-summary parsing, missing animation coverage, failing frame evidence, and unsupported-host skips.
- [ ] 2.6 Update proof-policy checks so minimap/source-map/sidebar-animation/editor-width or animation-tooling diffs require a passing native minimap animation artifact when applicable.

## 3. Automation And Replay Support

- [ ] 3.1 Extend Automation1 visual geometry snapshots only as needed to correlate animation frames, keeping fields bounded and document-content-free.
- [ ] 3.2 Ensure animation capture starts from a deterministic baseline using existing readiness predicates before the sidebar action, then samples intermediate frames without waiting for final geometry.
- [ ] 3.3 Keep `visual-geometry-settled` available after animation sampling so the same scenario can still prove final endpoint stability.
- [ ] 3.4 Update `scripts/lushtext-automation.py visual-geometry-capture` to generate replay scenarios with animation sampling fields and explicit missing-field behavior.
- [ ] 3.5 Update `scripts/lushtext-automation.py artifact-summary` to report animation-frame evidence separately from final-settle evidence.

## 4. Product Fix

- [ ] 4.1 Use the animation baseline artifacts to identify whether the transient comes from idle repair timing, mixed editor/source-map layout epochs, integer margin rounding, stale source-map adjustment, or native slider invalidation.
- [ ] 4.2 Implement the narrowest source-map synchronization path that prevents stale native highlight frames during sidebar animation while preserving the exact native `GtkSourceMap` effect.
- [ ] 4.3 Keep expensive semantic marker work debounced or bounded; do not introduce full-buffer scans, filesystem work, or unbounded snapshots on the animation frame path.
- [ ] 4.4 Coalesce and generation-guard any frame callbacks or animation-active state so rapid sidebar toggles cannot leave stale callbacks or pending readiness blockers.
- [ ] 4.5 Preserve minimap navigation, focus behavior, marker layering, large-file minimap policy, Focus Mode suppression, and final-settle viewport behavior.

## 5. Documentation, Rules, And Comments

- [ ] 5.1 Update `docs/automation.md` and `docs/automation-reference.md` for animation sampling fields, artifact-summary fields, statuses, and replay commands.
- [ ] 5.2 Update `docs/end-user-coverage.md`, `.agents/rules/build.md`, `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, and relevant GTK/visual testing skills to distinguish final-settle proof from during-animation proof.
- [ ] 5.3 Add code comments only where they explain GTK frame-clock, allocation/snapshot ordering, or why lightweight native minimap sync must happen before rendered frames.
- [ ] 5.4 Run the learning workflow after implementation and remove stale or duplicate visual-proof guidance if the new animation rule supersedes older wording.

## 6. Validation

- [ ] 6.1 Verify the animation-frame scenario fails on the baseline and passes after the product fix, preserving artifacts for both.
- [ ] 6.2 Run the exact reproduced intermediate-size sidebar hide/show animation cases and confirm the native top edge and first content row stay within tolerance across sampled frames.
- [ ] 6.3 Run final-settle native minimap visual geometry cases to confirm the previous endpoint invariant remains pixel-verified.
- [ ] 6.4 Run `python3 scripts/test-visual-geometry.py`, `make check-visual-proof-policy`, `make automation-client-self-test`, and `make check-automation-docs`.
- [ ] 6.5 Run focused Rust unit/widget tests for minimap geometry, animation-frame synchronization, Automation1 visual geometry, and editor-page width reflow.
- [ ] 6.6 Run `cargo clippy -p lushtext-core --features test-utils --all-targets -- -D warnings` and any broader build/test targets required by touched files.
- [ ] 6.7 Run `openspec validate stabilize-minimap-highlight-during-sidebar-animation --strict`, `openspec validate --changes --strict`, `openspec validate --specs --strict`, `openspec validate --all --strict`, and `git diff --check`.
