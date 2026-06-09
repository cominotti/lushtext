## Context

LushText already has several strong pieces: widget tests for real `LushtextWindow` behavior, Automation1 snapshots and readiness waits, headless Mutter screenshot capture, visual smoke manifests, warning scans, Blueprint drift checks, and local rules for GTK geometry. Those pieces still leave a gap: a UI change can pass logical state checks while a user-visible region is clipped, shifted, overlapped, or changed in a region the change was not supposed to affect.

The current minimap issue shows the gap. The editor can remain logically at the top of the file while hiding the workspace sidebar changes the editor allocation and the minimap's first rendered content appears clipped. That failure sits across editor-page geometry, Libadwaita shell allocation, `GtkSourceMap` projection, visual smoke coverage, and screenshot comparison tooling.

GTK and Libadwaita geometry must be treated as a runtime contract. Widgets measure, allocate, snapshot, map, and draw in separate phases, and adaptive containers feed real measurements into later phases. A robust solution needs both code-level geometry ownership and visual proof that unaffected regions stay unchanged.

## Goals / Non-Goals

**Goals:**

- Make visual geometry invariants first-class requirements rather than ad hoc review comments.
- Fix the minimap top-edge clipping class by making minimap top margins, width-only reflow, scroll anchoring, and post-allocation refresh explicit.
- Add same-session visual scenarios that capture before/after states, collect automation geometry anchors, and compare protected crops with exact zero-difference expectations where no movement is allowed.
- Preserve reviewable artifacts for failures: screenshots, masks, crop reports, geometry snapshots, runtime logs, warning scans, and scenario manifests.
- Keep privacy boundaries intact by exposing only bounded geometry state and artifact metadata.
- Update rules, skills, and docs so future visual/template/adaptive work must name the affected invariant and provide appropriate proof.

**Non-Goals:**

- Do not replace widget tests with screenshot-only tests.
- Do not introduce broad golden screenshots for the whole application.
- Do not make PR CI depend on host-sensitive compositor capture where the current workflow treats those lanes as scheduled/manual or skip-aware.
- Do not expose arbitrary widget tree mutation, full document text, note bodies, draft bodies, local-history contents, or private persistence identifiers through automation.
- Do not redesign the workspace or properties shell solely to address the minimap symptom.

## Decisions

### Decision: Use an invariant registry, not free-form screenshots

Define a small manifest format for visual geometry invariants. Each invariant names:

- scenario id and fixture setup;
- actions and readiness predicates;
- compared capture steps;
- protected regions that must be pixel-identical;
- regions allowed to move, resize, or repaint;
- geometry anchors used to compute masks;
- required widget/allocation assertions;
- expected warning policy;
- artifact paths written on pass, fail, or skip.

Rationale: the app needs to know which pixels are supposed to remain unchanged. A screenshot file by itself only proves that an image exists.

Alternative considered: manually inspect stored screenshots. That remains useful for review, but it cannot enforce the user's requirement that unaffected elements have no pixel variance.

### Decision: Add a same-session visual scenario runner

Add a Python scenario runner around the existing headless Mutter capture path. It should launch one isolated LushText process, drive cataloged actions through D-Bus, wait on Automation1 readiness, capture multiple steps from the same process/session, and compare images after all step state has settled.

Rationale: exact pixel comparisons are only meaningful when renderer, font, scale, theme, fixture, process state, and monitor are identical. Separate one-shot captures can differ for reasons unrelated to the change.

Alternative considered: extend the shell `run-visual-smoke.sh` directly. Bash remains suitable for orchestration, but image decoding, masks, JSON manifests, and multi-step failure reports belong in a typed Python helper.

### Decision: Keep image comparison local and deterministic

Extend the existing pure-Python PNG approach used by `scripts/assert-png-smoke.py` into a bounded visual comparison helper. It should decode PNGs without requiring Pillow, support exact crop equality, masked equality, simple per-region statistics, and generated diff images or text summaries when differences occur.

Rationale: visual proof should run in the same dev/CI environment as current smoke helpers without adding heavyweight dependencies.

Alternative considered: depend on ImageMagick or Pillow. That would simplify implementation but adds host/package variability to a lane already sensitive to compositor support.

### Decision: Use Automation1 for geometry anchors and readiness, not private widget mutation

Add bounded automation fields for named visual anchors such as window content, header bar, tab strip, editor viewport, source view visible rect, minimap shell, minimap map content, status bar, workspace sidebar, document properties, active transient surface, and compact sheet when present. Use stable names, rectangles, visibility, allocation size, scroll top/left state, and scale factor. Add a visual/layout readiness predicate that waits for known layout and visual blockers to settle.

Rationale: image masks need coordinates, and readiness must wait for GTK idle work, shell layout sync, minimap refresh debounce, workspace refresh, and relevant animations. Coordinates inferred from pixels alone are brittle.

Alternative considered: locate regions by image processing. That would be fragile across themes and fonts and would not explain whether the app was in the intended state.

### Decision: Fix the minimap through editor-page geometry ownership

The minimap should remain tab-local. The implementation should:

- make the source map's top geometry explicit so first content cannot paint flush into a clipped border at top-of-file;
- synchronize minimap top/bottom/margin geometry with the main editor viewport and dynamic overscroll policy;
- treat width-only allocation changes as capable of changing vertical visual-line projection;
- run top-edge scroll clamping and minimap refresh after width-only shell reflow when the editor was at the top;
- cover word-wrap enabled and disabled controls, light and dark style, and sidebar on/off transitions.

Rationale: the shell visibility toggle is the stimulus, but the bug appears in the editor/minimap projection. Fixing the editor-page contract avoids overfitting the workspace shell.

Alternative considered: special-case workspace sidebar toggles in the window layer. That would miss other width-only reflows such as properties changes, maximization, compact mode, or future side surfaces.

### Decision: Pair widget allocation assertions with screenshot invariants

Widget tests should prove cheap, deterministic logical contracts: top line remains visible, scroll adjustments stay at expected lower bounds, minimap/source-map allocations are positive, shell requested/rendered states settle once, persistent chrome remains allocated, and no test-local helper copies are introduced.

Visual scenarios should prove human-visible contracts: no clipping, no unexpected chrome movement, item-region-only scrolling, preserved buttons/header/status, no unintended scrollbars, and exact no-difference masks for unaffected regions.

Rationale: widget tests are fast and precise but cannot reliably prove rendered pixels; visual smoke is human-real but host-sensitive and slower. The layers should complement each other.

Alternative considered: pixel assertions inside widget tests. The existing testing guidance warns against testing GTK rendering details in the widget harness; compositor screenshots are the right lane for rendered pixels.

### Decision: Make rule and skill updates part of the change

Update `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, relevant GTK testing/debugging skills, automation docs, end-user coverage docs, and any visual smoke reference docs. The updated guidance should say that screenshot-reported, geometry-sensitive, adaptive, or template-layout work must include one of:

- widget allocation/overflow assertions that directly prove the visible contract;
- same-session screenshot/crop proof with protected-region invariants;
- an explicit reason the surface is out of scope plus a documented follow-up.

Rationale: this change is only durable if future agents get pulled into the same proof discipline automatically.

Alternative considered: rely on this proposal only. That would not help future work once the implementation context is compacted or forgotten.

## Risks / Trade-offs

- **Risk: Pixel comparisons are brittle across renderers, scale, fonts, or themes.** Mitigation: compare only same-session captures by default, mask dynamic regions, record environment details, and reserve cross-environment comparisons for coarse invariants.
- **Risk: Geometry automation fields expose sensitive content or implementation details.** Mitigation: publish only bounded names, rectangles, visibility, dimensions, scroll positions, stable scenario ids, and artifact paths; never include document text or private persistence identifiers.
- **Risk: Visual smoke becomes too slow or too noisy.** Mitigation: keep the default lane representative, add targeted invariant suites for geometry-sensitive changes, and keep host-sensitive lanes skip-aware rather than PR-required unless the project later chooses otherwise.
- **Risk: The minimap fix overfits one screenshot.** Mitigation: cover multiple state axes: sidebar on/off, default/maximized-like sizes, light/dark, wrap on/off, top-of-file and mid-file scroll positions, long path/header geometry, and dynamic overscroll.
- **Risk: Agents start adding broad screenshots instead of tight assertions.** Mitigation: invariant manifests must name protected and allowed regions; tasks include docs/rules updates emphasizing targeted evidence over screenshot volume.
- **Risk: New readiness predicates miss an animation or debounce source.** Mitigation: start with known blockers, preserve failure artifacts, and make readiness details visible in timeout reports so missing blockers can be added without guessing.

## Migration Plan

1. Add the new specs and design contract.
2. Implement the minimap top-edge and width-reflow fix with targeted widget regression tests.
3. Add bounded automation geometry fields and readiness predicate support, with docs and drift checks.
4. Add same-session visual scenario and PNG comparison helpers, then integrate a minimap/sidebar invariant scenario into visual smoke.
5. Expand the invariant matrix to representative cross-surface states: workspace, properties, command palette, search panel, notes/bookmarks, markdown preview, short/compact layouts, and template-sensitive shells.
6. Update rules, skills, and docs so future work uses the new proof chain.
7. Run the layered validation ladder: targeted widget tests, helper self-tests, automation docs checks, automation client self-test, visual smoke or targeted visual invariant smoke, and broader tests as touched code requires.

Rollback is straightforward for the minimap code change but not for the governance/docs change. If the visual comparison lane proves too noisy, keep geometry automation and widget tests, disable only the noisy scenario from default smoke, preserve the helper for manual/scheduled use, and document the reason.

## Open Questions

- Should the first implementation make the new visual invariant scenario part of default `make visual-smoke`, or expose a narrower `make visual-geometry-smoke` target first and then fold it into the default lane after stability is proven?
- Which exact regions should be protected for zero-difference in the first minimap/sidebar scenario: header bar, status bar, minimap top crop, or all three?
- Should geometry anchors be included in the existing Automation1 snapshot, or exposed as a nested optional `visual_geometry` object to make the new fields easier to version and document?
