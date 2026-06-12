## 1. Baseline Audit

- [x] 1.1 Capture the current Python live-runner contract from `scripts/visual-geometry-smoke.py`, `scripts/test-visual-geometry.py`, `scripts/visual_geometry_png.py`, `scripts/check-visual-proof-policy.py`, and existing visual-geometry scenario manifests.
- [x] 1.2 Inventory every current root summary, case summary, comparison report, animation report, warning-scan result, skip summary, status name, exit class, and artifact path used by visual proof.
- [x] 1.3 Record the current `cargo gtk-proof` staged behavior for `schema`, `summarize`, `corpus`, `policy`, and reserved `run` commands before implementation begins.
- [x] 1.4 Identify the exact Makefile targets, workflow lanes, docs checks, automation-client commands, and proof-policy scripts that must remain stable after wrappers move to Rust.
- [x] 1.5 Add or update implementation notes that distinguish Phase 4 parity completion from Phase 5 publishing, second-consumer, repository-split, and upstreaming work.

## 2. Rust Model And Schema Foundation

- [x] 2.1 Extend `crates/cargo-gtk-proof` typed models for live scenario manifests, expanded cases, capture steps, readiness predicates, protected regions, masks, anchors, warning scans, animation settings, and policy metadata.
- [x] 2.2 Extend artifact models for root summaries, per-case summaries, comparison reports, animation reports, skip or unsupported-host reports, parity reports, and automation-client summary data.
- [x] 2.3 Preserve or explicitly version schema descriptors for every live-runner input and output shape introduced by this phase.
- [x] 2.4 Add schema validation tests for valid manifests, missing required fields, unsupported schema versions, malformed summaries, malformed animation reports, and malformed parity metadata.
- [x] 2.5 Add bounded JSON loading and writing tests that reject oversized inputs and preserve stable diagnostics.
- [x] 2.6 Update `docs/gtk-proof-schemas.md` for new or changed schema fields, versions, statuses, and result-envelope metadata.

## 3. Artifact Safety And Reporting

- [x] 3.1 Implement safe artifact-root reset rules in Rust that refuse unsafe roots such as `/`, home, workspace root, empty paths, and non-owned directories.
- [x] 3.2 Implement bounded artifact writers for manifests, expanded cases, root summaries, case summaries, comparison reports, animation reports, warning scans, parity reports, environment reports, and skip reports.
- [x] 3.3 Ensure terminal output prints status, brief detail, and relative artifact paths without embedding screenshots, raw image data, unbounded logs, document text, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers.
- [x] 3.4 Add tests for pass, fail, skip, unsupported-host, malformed-artifact, warning-scan failure, and safe-path rejection artifact output.
- [x] 3.5 Verify generated artifacts remain schema-valid through `cargo gtk-proof schema` and `cargo gtk-proof summarize` fixtures.

## 4. PNG, Mask, Pixel Anchor, And Comparison Port

- [x] 4.1 Port PNG read/write, crop, diff, mask, and comparison behavior from `scripts/visual_geometry_png.py` into Rust with resource caps.
- [x] 4.2 Port screenshot-derived pixel-anchor detectors, including native minimap viewport top edge and first-content-row detectors.
- [x] 4.3 Preserve exact protected-region comparison semantics with declared masks and coordinate transforms.
- [x] 4.4 Preserve allowed-changing region checks and app-vs-rendered diagnostic reporting without letting app geometry satisfy rendered-effect proof.
- [x] 4.5 Add pure Rust tests for exact match, masked match, protected-region mismatch, missing anchor, rendered drift, minimap detector pass/fail, malformed PNG, oversized PNG, and bounded crop artifacts.
- [x] 4.6 Update the compatibility corpus with deterministic PNG and detector fixtures for all ported comparison paths.

## 5. Host And Session Orchestration

- [x] 5.1 Implement Rust host capability probes for D-Bus, headless Mutter, PipeWire, WirePlumber, GStreamer or capture helpers, PNG/image support, GSettings, the LushText binary, and required runtime tools.
- [x] 5.2 Implement isolated runtime, data, config, cache, session bus, and artifact directory setup for live runs.
- [x] 5.3 Launch and supervise the required compositor, capture helpers, and LushText process from Rust with bounded logs and deterministic cleanup.
- [x] 5.4 Preserve current GSettings setup and fixture environment behavior from the Python runner.
- [x] 5.5 Write unsupported-host summaries for missing host capabilities and ensure skipped or unsupported runs never count invariants as verified.
- [x] 5.6 Add tests for host probe classification, missing-tool diagnostics, cleanup after failure, log truncation, and unsupported-host artifact shape.

## 6. Automation1 Live Runner Integration

- [x] 6.1 Implement the Rust Automation1 client path needed by the live runner for introspection, readiness predicates, snapshot reads, workflow events, and action activation.
- [x] 6.2 Drive state changes only through documented GTK/GIO actions and Automation1 readiness waits, matching current Python behavior.
- [x] 6.3 Preserve narrow readiness predicates for visual-geometry settled states before falling back to broad idle waits.
- [x] 6.4 Add same-session before/after capture orchestration for scenario steps, including state assertions before every screenshot.
- [x] 6.5 Record bounded Automation1 state, geometry samples, action results, workflow events, and readiness blockers in artifacts.
- [x] 6.6 Add tests with fixture D-Bus responses or isolated helpers for ready, timeout, unknown predicate, action failure, state mismatch, and bounded snapshot privacy.

## 7. Live Scenario Matrix And Same-Session Proof

- [x] 7.1 Port scenario manifest loading, matrix expansion, case naming, fixture creation, and per-case runtime directories into Rust.
- [x] 7.2 Preserve same-session comparison requirements for renderer, theme, scale factor, font, fixture, window size, process, and compositor session.
- [x] 7.3 Port final geometry stability waits for sidebar hide/show, editor viewport, minimap rectangles, and visual-geometry readiness.
- [x] 7.4 Implement root and per-case summaries that aggregate only actually verified invariant IDs.
- [x] 7.5 Add live-runner tests or controlled fixtures for passing paired capture, protected crop failure, state mismatch, readiness timeout, unsupported host, and warning-scan failure.
- [x] 7.6 Verify filtered runs do not overclaim full invariant coverage when required threshold or animation cases are skipped or excluded.

## 8. Animation-Frame Stream Proof

- [x] 8.1 Port bounded frame-stream capture orchestration into Rust for animation-frame scenarios.
- [x] 8.2 Port timestamped Automation1 geometry sampling and frame-to-sample pairing with declared skew bounds.
- [x] 8.3 Preserve intermediate-frame requirements, final-settle separation, per-frame anchor evaluation, row drift limits, and stale-frame rejection.
- [x] 8.4 Write animation reports with frame counts, sample counts, intermediate counts, maximum skew, maximum row drift, failing frame details, crop/frame paths, and final-settle status.
- [x] 8.5 Add Rust tests and corpus fixtures for passing animation, final-settle-only rejection, missing stream mode, no mapped intermediate PNG, stale frame/sample pairing, missing anchors, rendered drift, and unsupported animation capture.
- [x] 8.6 Ensure proof policy and automation summaries do not let final-settle evidence mask missing or failing animation evidence.

## 9. Corpus And Python/Rust Parity Gate

- [x] 9.1 Expand the compatibility corpus with representative pass, fail, skip, unsupported-host, missing-manifest, malformed-artifact, missing-anchor, rendered-drift, warning-scan, stale-frame, final-settle-only, and passing-animation cases.
- [x] 9.2 Add a parity mode that runs or replays the Python oracle and Rust runner over the required corpus cases.
- [x] 9.3 Compare Rust and Python status, exit class, required fields, verified invariant IDs, warning-scan results, summary paths, artifact roots, engine metadata, and bounded diagnostic details.
- [x] 9.4 Make any parity mismatch block wrapper migration and emit a bounded mismatch report.
- [x] 9.5 Add tests proving parity success records corpus identity, compared count, failed mismatch count, schema versions, engine metadata, and oracle status.
- [x] 9.6 Add documentation explaining how to run parity mode and how parity evidence justifies default Rust migration.

## 10. Proof Policy Migration

- [x] 10.1 Move default local proof-policy evaluation to `cargo gtk-proof policy` after parity is recorded.
- [x] 10.2 Preserve visual-sensitive path detection, required invariant mapping, fingerprint matching, status vocabulary, unsupported-host semantics, and negative self-tests.
- [x] 10.3 Keep `scripts/check-visual-proof-policy.py` as a compatibility wrapper or shim that delegates to Rust and cannot mask Rust policy failures.
- [x] 10.4 Add negative policy fixtures for final-settle-only, missing stream mode, missing intermediate PNG, stale frame/sample pairing, missing anchors, rendered drift hidden by app geometry, stale fingerprints, missing summaries, Python-only summaries after migration, and unsupported-host overclaiming.
- [x] 10.5 Update Makefile targets so `make check-visual-proof-policy` exercises the Rust-backed policy path and its self-tests.
- [x] 10.6 Verify visual-sensitive changes require current Rust proof summaries with matching fingerprints and required rendered/animation invariant IDs.

## 11. Wrapper, Makefile, And Workflow Migration

- [x] 11.1 Switch `make visual-geometry-smoke` to the Rust live runner only after parity evidence exists.
- [x] 11.2 Preserve existing artifact directory flags, default artifact roots, exit classes, status vocabulary, and evidence layout for visual smoke callers.
- [x] 11.3 Add documented Python oracle or diagnostic compatibility flags without making Python the default proof authority after migration.
- [x] 11.4 Update scheduled/manual end-user smoke workflow commands and artifact upload paths while preserving the visual-geometry lane identity.
- [x] 11.5 Update `scripts/check-end-user-smoke-workflow.py` so it validates the Rust-backed default command, artifact path, and documented Python oracle alias.
- [x] 11.6 Verify scheduled/manual smoke artifacts identify engine metadata, schema versions, scenario source, parity status, and unsupported-host reasons.

## 12. Automation Client Compatibility

- [x] 12.1 Teach `scripts/lushtext-automation.py artifact-summary` to read Rust-produced visual-geometry summaries and parity metadata while preserving its result envelope.
- [x] 12.2 Preserve automation-client exit classes for pass, fail, skipped, unsupported-host, usage-error, and artifact-error outcomes when delegating to Rust.
- [x] 12.3 Ensure delegation failures, missing Rust proof tooling, unsupported schema versions, and malformed Rust output report stable automation-client statuses without claiming proof passed.
- [x] 12.4 Add self-test fixtures for Rust pass, Rust fail, Rust skip, unsupported-host, malformed-artifact, Python oracle, and parity summaries.
- [x] 12.5 Update `make automation-client-self-test` coverage for Rust visual proof summaries and delegation failure cases.
- [x] 12.6 Preserve bounded summary output and privacy exclusions for screenshots, logs, snapshots, document text, note bodies, draft bodies, local-history contents, search result text, and private identifiers.

## 13. Documentation And Governance

- [x] 13.1 Update `docs/gtk-proof-schemas.md` with live-runner schemas, engine metadata, parity metadata, policy metadata, and artifact examples.
- [x] 13.2 Update `docs/end-user-coverage.md` to describe Rust-backed visual-geometry smoke, scheduled/manual workflow behavior, unsupported-host semantics, and artifact roots.
- [x] 13.3 Update `docs/automation.md` and `docs/automation-reference.md` for automation-client summary delegation, status vocabulary, exit classes, and Rust proof boundaries.
- [x] 13.4 Update `docs/next/gtk-lush.md` to record that Phase 4 proof parity is complete, Rust owns default live visual proof after parity, Python remains oracle/diagnostic only, and Phase 5 remains separate.
- [x] 13.5 Update `crates/gtk-lush/GOVERNANCE.md` with a dated `complete-gtk-lush-proof-parity` review entry covering constitution, workspace-tool placement, parity evidence, wrapper migration, Python status, and Phase 5 deferral.
- [x] 13.6 Update README, CHANGELOG, or crate docs touched by the phase to keep GTK Lush APIs marked as in-tree `0.0.0` and not publication-ready.
- [x] 13.7 Run documentation drift checks and fix every mismatch rather than documenting exceptions.

## 14. Specialist Reviews

- [x] 14.1 Run the `gtk-testing` review lane for widget/headless test coverage and live visual fixture reliability; fix actionable findings.
- [x] 14.2 Run the `gtk-agentic-debugging` review lane for real headless GTK runtime behavior, logs, compositor/capture assumptions, and warning-scan behavior; fix actionable findings.
- [x] 14.3 Run the `gtk4-libadwaita-internals` review lane for GTK, Libadwaita, allocation, rendering, focus, and animation contract assumptions; fix actionable findings.
- [x] 14.4 Run the `gtk-perf-review` review lane for runner responsiveness, CI runtime cost, PNG/frame resource caps, and memory behavior; fix actionable findings.
- [x] 14.5 Run the `data-safety` review lane for artifact privacy, safe path resets, filesystem writes, cleanup, and bounded output; fix actionable findings.
- [x] 14.6 Run the `rust-hex-arch` review lane for module boundaries, command/query separation, and avoiding GTK Lush family-crate framework drift; fix actionable findings.
- [x] 14.7 Run the `rust-comments` review lane for public Rust APIs, non-obvious orchestration code, GTK/runtime assumptions, and proof-policy comments; fix actionable findings.
- [x] 14.8 Record each review lane, findings, fixes, and any accepted non-blockers in tasks or review notes before archive.

## 15. Verification Gates

- [x] 15.1 Run Rust formatting and lint gates for the workspace changes.
- [x] 15.2 Run `cargo test -p cargo-gtk-proof` and all new unit, corpus, schema, policy, PNG, parity, and artifact tests.
- [x] 15.3 Run the full proof corpus, including pure fixtures and Python/Rust parity fixtures.
- [x] 15.4 Run `make check-visual-proof-policy` and confirm Rust-backed policy rejects negative fixtures and accepts current valid proof.
- [x] 15.5 Run `make automation-client-self-test` and `make check-automation-docs`.
- [x] 15.6 Run `make check-end-user-smoke-workflow`.
- [x] 15.7 Run `make test-widget-headless` and treat any `FLAKY:` output as a blocker.
- [x] 15.8 Run `make visual-geometry-smoke` on a supported host and preserve the Rust engine artifact summary.
- [x] 15.9 Run the strict OpenSpec validation ladder: `openspec validate complete-gtk-lush-proof-parity --strict`, `openspec validate --changes --strict`, `openspec validate --specs --strict`, and `openspec validate --all --strict`.
- [x] 15.10 Run `git diff --check` and fix whitespace or conflict-marker issues.

## 16. Archive Readiness

- [x] 16.1 Confirm every task above is complete or explicitly documented with maintainer-approved rationale.
- [x] 16.2 Confirm no Phase 5 publishing, second-consumer, repository-split, crates.io release, or Phase 6 upstreaming work entered the implementation.
- [x] 16.3 Confirm default local and scheduled visual proof paths are Rust-backed after parity and Python is documented only as oracle or diagnostic compatibility.
- [x] 16.4 Confirm proof artifacts, governance notes, and docs identify the parity evidence that justified wrapper migration.
- [x] 16.5 Re-run `openspec status --change complete-gtk-lush-proof-parity` and verify the change is ready for implementation archive after the validation ladder passes.
