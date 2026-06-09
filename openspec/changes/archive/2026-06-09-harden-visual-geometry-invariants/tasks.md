## 1. Minimap And Editor Geometry Fix

- [x] 1.1 Add a focused minimap top-edge regression fixture that reproduces sidebar on/off reflow at top-of-file with minimap enabled.
- [x] 1.2 Make minimap source-map top geometry explicit so first rendered content cannot be clipped by the minimap shell or source-map border.
- [x] 1.3 Treat width-only editor allocation changes as possible vertical visual-line projection changes and refresh top-edge clamping, minimap wrap policy, minimap markers, and viewport projection after layout settles.
- [x] 1.4 Preserve existing mid-file minimap behavior by keeping accepted visible-start-line tolerance and marker projection semantics unchanged.
- [x] 1.5 Add widget assertions for sidebar hide and sidebar show at top-of-file with word wrap enabled and disabled.
- [x] 1.6 Add widget assertions that minimap/source-map allocation, visible top line, scroll lower-bound state, and marker projection remain internally consistent after width-only reflow.

## 2. Bounded Automation Geometry State

- [x] 2.1 Add a bounded visual geometry snapshot model for named surfaces, rectangles, allocation sizes, visibility, scale factor, scroll anchor state, and absence reasons.
- [x] 2.2 Populate geometry anchors for header, tab strip, editor viewport, source view, minimap shell/source map/marker strip, status bar, workspace sidebar, document properties, preview, search panel, and active transient surface where present.
- [x] 2.3 Add a `visual-geometry-settled` readiness predicate that waits for GTK idle layout work, split-view sync, minimap refresh/debounce, relevant animations, workspace refresh, and active scenario setup.
- [x] 2.4 Ensure visual geometry state never includes document text, note bodies, draft bodies, local-history contents, complete search result text, minimap-rendered text, or private persistence identifiers.
- [x] 2.5 Update `docs/automation.md`, `docs/automation-reference.md`, action/readiness documentation, and automation drift checks for all new fields and predicates.
- [x] 2.6 Add unit or self-test coverage proving geometry snapshots serialize bounded fields and absence reasons correctly.

## 3. Same-Session Visual Scenario Infrastructure

- [x] 3.1 Add a Python visual geometry scenario runner that launches one isolated headless Mutter session, drives documented actions through D-Bus, waits on readiness predicates, and captures multiple screenshots from the same process.
- [x] 3.2 Add a pure-Python PNG crop/mask comparison helper based on the existing `assert-png-smoke.py` decoder, with exact equality for protected regions and bounded reports for failures.
- [x] 3.3 Define a manifest schema for visual invariants: fixture setup, capture steps, protected regions, allowed-changing regions, readiness gates, masks, comparison mode, and artifact outputs.
- [x] 3.4 Write comparison artifacts for pass, fail, and skip: before/after screenshots, crop coordinates, masks, geometry snapshots, diff summaries, warning scans, logs, environment, and scenario manifest.
- [x] 3.5 Integrate the scenario runner into `scripts/run-visual-smoke.sh` or a new make target while preserving clear skips when host compositor, PipeWire, D-Bus, AT-SPI, or screenshot tools are unavailable.
- [x] 3.6 Add script-level tests for manifest parsing, PNG crop equality, masked equality, nonzero protected differences, missing manifest failures, and artifact-summary shape.

## 4. Visual Invariant Scenario Matrix

- [x] 4.1 Add the first same-session minimap/sidebar invariant scenario covering sidebar visible to hidden at top-of-file with minimap enabled.
- [x] 4.2 Add the reverse minimap/sidebar invariant scenario covering sidebar hidden to visible at top-of-file with minimap enabled.
- [x] 4.3 Cover light and dark style preferences for the minimap/sidebar invariant.
- [x] 4.4 Cover word-wrap enabled and disabled controls for the minimap/sidebar invariant.
- [x] 4.5 Mark header and status chrome as protected zero-difference regions where the scenario does not intentionally change them.
- [x] 4.6 Mark editor body and minimap body as allowed-changing regions with explicit geometry-anchor assertions rather than unbounded ignored differences.
- [x] 4.7 Add at least one constrained or maximized-like window size for the minimap/sidebar invariant so the originally reported geometry class is covered.
- [x] 4.8 Extend the matrix to one representative non-minimap surface after the runner is stable, such as document properties, command palette, workspace sidebar, notes/bookmarks, markdown preview, or search panel.

## 5. Automation Client And Artifact Summaries

- [x] 5.1 Extend `scripts/lushtext-automation.py artifact-summary` to recognize visual geometry scenario manifests and comparison reports.
- [x] 5.2 Report passing visual geometry artifacts with scenario id, compared steps, protected zero-difference regions, allowed-changing regions, warning status, and manifest path.
- [x] 5.3 Report failed visual geometry artifacts with stable statuses such as `visual-comparison-failed`, `predicate-timeout`, `state-mismatch`, and `warning-scan-failed`.
- [x] 5.4 Report skipped visual geometry artifacts distinctly from pass/fail and avoid counting skipped invariant coverage as verified.
- [x] 5.5 Add client self-tests for passing, failing, skipped, malformed, and privacy-sensitive visual geometry artifact summaries.
- [x] 5.6 Update automation-client documentation and `make automation-client-self-test` expectations.

## 6. Rules, Skills, And Contributor Guidance

- [x] 6.1 Update `.agents/rules/ui.md` to require widget allocation proof or same-session visual invariant proof for screenshot-reported, visual, adaptive, or geometry-sensitive changes.
- [x] 6.2 Update `.agents/rules/widget-wiring.md` with guidance for protected-region comparisons, visual readiness waits, and avoiding per-frame geometry churn.
- [x] 6.3 Update `gtk-testing` guidance to describe when widget assertions are sufficient and when rendered screenshot proof is required.
- [x] 6.4 Update `gtk-agentic-debugging` guidance to prefer same-session paired captures for pixel invariants and to preserve artifacts outside the repo unless intentionally checked in.
- [x] 6.5 Update `gtk4-libadwaita-internals` geometry guidance with the visual invariant workflow when GTK allocation contracts cross into screenshot-visible bugs.
- [x] 6.6 Update `docs/end-user-coverage.md` and visual smoke docs so the new visual geometry lane and its host-sensitive skip behavior are clear.
- [x] 6.7 Ensure UI template guidance states that Blueprint drift checks do not replace visual invariant proof for geometry-sensitive template edits.

## 7. Validation

- [x] 7.1 Run `cargo fmt --all -- --check`.
- [x] 7.2 Run targeted widget tests for minimap/editor geometry through `scripts/run-widget-tests.sh --headless -- <test-filter>`.
- [x] 7.3 Run relevant script self-tests for PNG comparison, visual scenario manifests, and automation artifact summaries.
- [x] 7.4 Run `make check-automation-docs`.
- [x] 7.5 Run `make automation-client-self-test`.
- [x] 7.6 Run targeted visual geometry smoke for the minimap/sidebar scenarios and inspect preserved screenshots.
- [x] 7.7 Run `make visual-smoke` when the host supports the existing visual lane.
- [x] 7.8 Run `openspec validate harden-visual-geometry-invariants --strict`.
- [x] 7.9 Run `openspec validate --changes --strict`.
- [x] 7.10 Run `openspec validate --specs --strict`.
- [x] 7.11 Run `openspec validate --all --strict`.
- [x] 7.12 Run `git diff --check`.
