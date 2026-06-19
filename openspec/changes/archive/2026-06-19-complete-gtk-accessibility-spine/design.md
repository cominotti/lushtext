## Context

LushText already has a first serious GTK accessibility spine: a user-facing guide, `gtk-accessibility-spine` requirements, centralized `ui::accessibility` helpers, widget tests, AT-SPI smoke, visual smoke, visual geometry proof, and policy checks. The current state is strong but not complete enough to call state-of-the-art because coverage is still scenario-led rather than matrix-led, some named surfaces are not accessibility-smokeed directly, manual Orca verification is documented but not structured as a reviewable artifact, and guardrails mostly catch new diffs rather than proving the current tree.

GTK's accessibility model supports this approach. Standard widgets expose baseline semantics, but app code remains responsible for product-specific names, descriptions, relations, states, announcements, and custom widget semantics. In LushText, the highest-risk areas are custom/factory rows, transient surfaces, editor state projection around GtkSourceView, read-only preview surfaces, keyboard-only context actions, compact/adaptive layouts, and announcement noise.

The implementation should finish the current spine instead of replacing it. Existing smoke lanes and documentation stay valuable; this change makes them complete, synchronized, and release-grade.

## Goals / Non-Goals

**Goals:**

- Define a complete accessibility acceptance matrix for all major LushText surfaces, state extremes, semantic metadata, keyboard paths, announcements, visual accessibility expectations, and proof lanes.
- Bring implementation into alignment with that matrix by filling missing names, descriptions, relations, dynamic states, row cleanup, focus restoration, keyboard parity, and bounded announcements.
- Expand `make accessibility-smoke` into full AT-SPI coverage for the matrix while preserving focused `--case` filters and explicit unsupported-host skips.
- Pair AT-SPI coverage with widget, visual smoke, visual geometry, and manual Orca evidence where each lane is the honest proof for the behavior.
- Strengthen drift checks so direct helper bypasses, missing matrix coverage, stale stable anchors, undocumented exceptions, and stale smoke artifacts are caught before release claims.
- Preserve privacy and data safety by keeping accessibility metadata and artifacts bounded and fixture-driven.

**Non-Goals:**

- Do not claim legal accessibility certification or WCAG compliance beyond what LushText actually proves.
- Do not replace GtkSourceView text accessibility or fork GTK/Libadwaita behavior.
- Do not add new production dependencies solely for accessibility proof unless existing GTK, AT-SPI, Mutter, automation, or Python helper paths cannot cover the requirement.
- Do not make host-sensitive AT-SPI, compositor, screenshot, or Orca checks mandatory in default pull-request CI unless a later change proves a narrow slice is cheap and reliable enough.
- Do not expose private document text, note bodies, draft contents, local-history contents, full search-result lines, or private sidecar identifiers in metadata or artifacts.

## Decisions

### 1. Use an Accessibility Matrix as the Source of Truth

Create a reviewable matrix, likely `docs/accessibility-matrix.md`, whose rows identify:

- surface and workflow
- state extreme: no context, representative, dense/awkward, constrained/compact, error/recovery, or transient-dismissed
- required role/name/description/relation/state behavior
- keyboard path and context-menu/menu/command-palette fallback
- announcement behavior and lane
- visual accessibility expectation
- proof owner: widget test, accessibility smoke, visual smoke, visual geometry, manual Orca, or documentation-only caveat
- stable AT-SPI anchors or fixture-only anchors, when applicable

Alternative considered: keep expanding `docs/accessibility.md` prose only. That would be easier initially, but prose does not make coverage gaps obvious and is hard for tooling to reconcile.

### 2. Keep `ui::accessibility` as the App-Owned Metadata Boundary

All app-owned roles, labels, descriptions, relations, states, value text, shortcut text, row metadata, and announcements should go through `crate::ui::accessibility`, except for documented GTK contract exceptions. This includes normalizing current direct calls outside the helper or recording explicit exceptions with rationale.

Alternative considered: allow direct GTK calls wherever convenient. That matches GTK syntax but weakens policy checks and makes state reset/row recycling mistakes more likely.

### 3. Layer Proof by What Each Lane Can Honestly Prove

Use this proof split:

- Widget tests prove GTK metadata state, row bind/unbind cleanup, focus restoration hooks, throttling policy, and local keyboard wiring with `NO_AT_BRIDGE=1`.
- `make accessibility-smoke` proves AT-SPI-visible roles, names, focus targets, text-interface summaries, stable anchors, and unsupported-host reporting.
- `make visual-smoke` proves rendered visual accessibility across representative states, high contrast, dark style, large text, reduced motion, constrained geometry, and readability.
- `make visual-geometry-smoke` proves same-session pixel invariants and fixed-control/focus geometry where rendered effects matter.
- Manual Orca proof covers speech behavior, caret/selection feedback, announcement quality, and cases where headless AT-SPI reports a tree but not the user-facing screen-reader experience.

Alternative considered: make AT-SPI smoke the only acceptance lane. That would miss visual affordances, speech behavior, and local GTK state that is cheaper and more deterministic in widget tests.

### 4. Expand Smoke by Matrix Case IDs, Not by Ad Hoc Assertions

Each accessibility smoke case should map to one or more matrix rows. Case manifests should include enough bounded metadata to review what row was covered, which anchors were asserted, whether focus fallback was used, whether text-interface proof was available, and why a host skip occurred.

Alternative considered: keep one large shell script with implicit coverage. That is workable for a first pass, but it makes omissions easy when new surfaces are added.

### 5. Treat Manual Orca Evidence as a Release Artifact

Add a repeatable manual check document or template for normal GNOME sessions. It should record environment, LushText build, screen reader version, workflows checked, expected speech/focus outcomes, caveats, and whether AT-SPI smoke/visual artifacts already cover the same workflow.

Alternative considered: keep manual checks as an informal release note. That is too easy to skip or under-document when a release is close.

### 6. Guard Current Tree and Current Artifacts

Strengthen policy checks so they can inspect the current tree, not only added lines. The checks should catch:

- direct GTK accessible calls outside `ui::accessibility` without an allowlisted exception
- list factories without row accessibility apply/clear coverage
- icon-only controls without product-facing names/tooltips
- hover/pointer-only affordances without keyboard/context paths
- new transient surfaces without matrix coverage and focus proof
- stable AT-SPI anchors changed without docs and smoke updates
- accessibility smoke summaries that are filtered, skipped, stale, or not for the current relevant tree when release proof is requested

Alternative considered: rely on code review discipline. Accessibility regressions are easy to miss visually, so automated drift checks are worth the maintenance cost.

### 7. Preserve Bounded, Fixture-Driven Proof

Smoke fixtures should use synthetic text and bounded display names. Assertions may record counts, roles, states, fixture names, short status strings, and artifact paths. They must not dump private user content or unbounded text.

Alternative considered: capture full AT-SPI trees and text dumps for easier debugging. That would be convenient but conflicts with LushText's privacy and data-safety posture.

## Risks / Trade-offs

- [Risk] The change is broad and can sprawl across many UI modules. -> Mitigation: start with the matrix, implement one surface family at a time, and keep every task tied to a matrix row and proof lane.
- [Risk] Host-sensitive AT-SPI, screenshot, and Orca checks can skip or behave differently across environments. -> Mitigation: keep explicit unsupported-host summaries, avoid counting skips as coverage, and require manual or alternate-runner evidence for release claims.
- [Risk] Smoke runtime may become too slow for day-to-day work. -> Mitigation: preserve `--case` filters, keep default PR lanes bounded, and reserve full end-user smoke for local, scheduled, or release validation.
- [Risk] AT-SPI exposes some GTK widgets differently than GTK metadata tests. -> Mitigation: document GTK/AT-SPI caveats, assert stable product-facing anchors where possible, and use manual Orca checks for user-facing speech behavior.
- [Risk] Matrix and docs can drift from code. -> Mitigation: add policy checks that reconcile smoke cases, stable anchors, docs, and matrix rows.
- [Risk] Accessibility metadata could leak user content through labels or artifacts. -> Mitigation: cap announcement text, use fixture data, avoid full document/search/note/local-history contents, and audit generated artifacts.

## Migration Plan

1. Add the accessibility matrix and align docs/rules with it.
2. Normalize helper usage and explicit exceptions before adding large new coverage, so later checks have a clean baseline.
3. Expand widget tests and `make accessibility-smoke` by surface family.
4. Add visual and visual-geometry coverage for state extremes that cannot be proven semantically.
5. Add manual Orca validation template and release guidance.
6. Strengthen policy and docs drift checks.
7. Run focused lanes as coverage grows, then run the full release-grade validation set before marking the change complete.

Rollback is straightforward because this is an additive quality and proof change. If one host-sensitive smoke case is unstable, keep the implementation improvements, quarantine that scenario behind an explicit unsupported/flaky diagnostic, and avoid counting it as coverage until repaired.

## Open Questions

- Whether the accessibility matrix should remain Markdown-only or also gain a small machine-readable manifest for policy checks. Prefer Markdown first unless policy parsing becomes brittle.
- Whether any current direct GTK accessibility calls need permanent exceptions. Prefer normalization unless a GTK template or binding contract clearly requires direct use.
- Whether one narrow accessibility smoke freshness check can eventually become a default PR gate. That decision should wait until runtime cost and reliability are measured.
