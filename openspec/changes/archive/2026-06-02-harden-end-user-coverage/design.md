## Context

LushText already has a layered test system:

- unit and service tests for deterministic model/service behavior;
- integration tests for tempdir-backed filesystem workflows;
- property tests for bounded generated-input invariants;
- fuzz replay and fuzz smoke for hostile byte/parser surfaces;
- widget tests under the custom GTK harness and headless Mutter for real widget
  behavior;
- Criterion benchmarks that are compile-checked in CI;
- mutation testing for calibrated deterministic Rust scope.

That system is intentionally strongest where behavior is deterministic and
weakest where the product depends on the live desktop: rendered pixels, portals,
native dialogs, file monitor timing, accessibility, desktop activation,
printing, confinement, and performance budgets. The proposal adds coverage for
those end-user risks without collapsing every workflow into one brittle
end-to-end test.

The implementation must respect the existing harness boundaries from
`gtk-testing`: use the lowest reliable test level, prefer predicates and
observable state over sleeps, keep widget tests narrow, and move to compositor,
portal, or installed-app smoke checks only when the widget harness cannot prove
the behavior.

## Goals / Non-Goals

**Goals:**

- Turn the eight identified coverage gaps into durable OpenSpec requirements
  with concrete scenarios.
- Add coverage where the current lanes cannot provide proof, especially real
  desktop rendering, portal/sandbox behavior, accessibility, and performance.
- Keep ordinary pull-request CI bounded and deterministic.
- Preserve useful artifacts for heavier smoke checks: screenshots, logs,
  benchmark summaries, and denial/warning reports.
- Document which lane owns each class of end-user risk so future work does not
  accidentally push live-session behavior into property or mutation tests.

**Non-Goals:**

- Do not replace the custom widget harness with screenshot-only tests.
- Do not require every pull request to run slow installed-app, Flatpak, Snap,
  portal, accessibility, or full benchmark suites.
- Do not introduce broad production debug controls. Any testability seams must
  be narrow, read-only where possible, and safe for release builds or explicitly
  test-only.
- Do not change product behavior unless an implementation gap is discovered
  while adding the required coverage.

## Decisions

### Decision: Keep a layered validation model

Coverage SHALL be routed to the cheapest reliable lane:

- pure decisions and parsing stay in unit, property, fuzz, and mutation lanes;
- filesystem workflows without GTK stay in integration tests;
- widget state, actions, focus, geometry allocation, and narrow workflow wiring
  stay in the widget harness;
- rendered-pixel, compositor, portal, sandbox, accessibility, native-dialog, and
  installed-app workflows use dedicated smoke scripts or scheduled/manual CI;
- latency and throughput use benchmark or performance-smoke lanes.

Alternative considered: create one full E2E suite that drives the whole app.
That would catch some cross-cutting failures, but it would be slower, harder to
debug, and more fragile than focused tests with clear ownership.

### Decision: Treat real desktop coverage as smoke, not screenshot approval

The visual lane SHALL capture screenshots and logs for representative states,
but it SHALL assert stable invariants rather than maintaining large golden image
sets. Examples include nonblank captures, expected active state before capture,
known surfaces visible, no warning output, and no obvious overlap or clipping for
selected geometry-sensitive views.

Alternative considered: pixel-perfect approval images. That is too brittle for
GTK themes, fonts, scale factors, and renderer differences, and would make
legitimate toolkit changes expensive.

### Decision: Keep heavy checks gated and artifact-rich

Pull-request CI SHALL keep fast checks as the default. Environment-sensitive
checks such as portal/sandbox smoke, AT-SPI accessibility smoke, installed
Flatpak/Snap launch, and performance thresholds SHALL run as scheduled, manual,
release, or opt-in lanes unless a narrow portion is cheap and deterministic
enough for PRs. All heavy lanes SHALL upload enough artifacts to explain a
failure without requiring immediate local reproduction.

Alternative considered: make every coverage gap a required PR gate. That would
raise confidence, but it would slow everyday development and introduce host
environment failures into routine feedback.

### Decision: Add narrow testability seams only where observation is otherwise brittle

When real-session smoke needs to set or inspect app state, the implementation
SHALL prefer existing actions, stable accessible names, and read-only inspection
surfaces. Any new automation affordance MUST be narrow and documented. For
example, a search-entry accessible name or read-only active-state query is
acceptable; arbitrary widget-tree mutation is not.

Alternative considered: drive everything by coordinates, timing, or private GTK
tree assumptions. That would be less invasive, but it creates false failures
when layout, focus, or toolkit internals change.

### Decision: Make performance checks thresholded but forgiving

Performance coverage SHALL focus on workflows users feel: startup/open,
workspace indexing, content search, large-file load/save refusal/degradation,
and memory-pressure tab eviction. Thresholds SHALL be loose enough for CI
variance, based on documented baselines, and separated from full Criterion
reports where possible.

Alternative considered: strict Criterion comparisons as required PR gates.
Those would be noisy on shared runners and could block unrelated work.

## Risks / Trade-offs

- Real-session tests become flaky due to compositor, portal, or runner changes
  -> keep them small, assert observable predicates, use shared scripts, upload
  logs/screenshots, and gate them separately from fast PR tests.
- Screenshot smoke misses subtle visual regressions -> combine screenshots with
  existing widget geometry tests and targeted manual/release inspection for
  high-risk UI changes.
- Accessibility smoke depends on AT-SPI availability -> make the lane explicit,
  skip with a clear message when dependencies are absent, and keep widget tests
  independent of AT-SPI.
- Portal/sandbox smoke may differ across Flatpak/Snap/runtime versions -> record
  runtime versions in artifacts and separate native, Flatpak, and Snap
  expectations.
- Performance thresholds may fail on noisy hosts -> use warmup, coarse budgets,
  representative fixtures, and scheduled/manual deeper reports before making a
  threshold blocking.
- Testability seams may leak into product surface area -> prefer safe normal
  actions/accessibility metadata and document any release-visible automation API.

## Migration Plan

1. Add the eight coverage spec files and a task plan that maps each requirement
   to the intended harness.
2. Implement cheap widget/integration tests first: external file monitor,
   close-request flows, desktop open activation, and menu actions.
3. Add desktop visual, portal/sandbox, accessibility, and performance scripts
   behind Make targets with clear skip behavior when host dependencies are
   unavailable.
4. Wire only the cheap deterministic pieces into default PR CI; add heavier
   lanes as scheduled/manual/release checks with artifacts.
5. Update testing documentation, build rules, and agent guidance so the lane
   boundaries remain discoverable.
6. Validate with focused targets first, then the broad OpenSpec and test gates.

Rollback is straightforward because this change adds coverage and documentation.
If a new heavy lane is too flaky, disable that CI trigger while keeping the
script, Make target, and spec requirement so it can be repaired intentionally.

## Open Questions

- Which visual smoke states should be mandatory on PRs, if any, versus scheduled
  or release-only?
- Should the first portal/sandbox smoke focus on Flatpak only, then extend to
  Snap after the Snap platform gate clears?
- What baseline and threshold policy should performance smoke use before it is
  trusted as a blocking signal?
