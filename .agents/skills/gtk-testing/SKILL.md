---
name: gtk-testing
description: "Guide testing strategy for GTK4/Libadwaita Rust applications, especially LushText's current unit, integration, GTK Lush proof harness/spine, cargo-gtk-proof, and custom widget-test harness. Use when adding or fixing tests, deciding the right test level, debugging headless GTK failures, modifying `crates/lushtext/tests/**`, `crates/gtk-lush/proof-*`, or `crates/cargo-gtk-proof`, or reasoning about widget visibility, animation, focus, lifecycle assertions, proof artifacts, or visual geometry in tests. Trigger on mutter headless, widget harness, TestContext, gtk-lush-proof-harness, gtk-lush-proof-spine, cargo-gtk-proof, flaky GTK tests, CI test configuration, or any request for GTK test coverage."
---

Guide testing for LushText as it exists today. The repo already has solid coverage layers and a custom GTK widget harness; use them before inventing new structure.

Before selecting paths or Cargo package/target names, discover the current workspace test
surfaces:

```bash
scripts/agent-topology.py testing-surfaces
```

Treat its Cargo-metadata output as authoritative for package names, manifests, target source
paths, target kinds, and required features. The paths below are current checked examples and
navigation hints, not immutable topology. Keep repository-owned `make` targets as the stable
operator interface when they exist.

The testing approach is pragmatic:

- Keep pure logic in unit and service tests where no display server is needed.
- Use the property-test lane for pure deterministic invariants and tiny
  deterministic tempdir-backed service fixtures that benefit from bounded
  generated inputs.
- Use the fuzz lane for hostile byte-ingestion and parser/preprocessing crash
  surfaces that property tests should not try to exhaust.
- Use stable corpus replay when the question is whether committed fuzz seeds
  still pass on stable Rust without nightly or sanitizer setup.
- Use the existing integration helpers for cross-service filesystem workflows.
- Use the custom widget harness for real window and widget behavior.
- Reach for the automation client self-test for reusable D-Bus client/parser
  changes. Use widget tests for allocation and state contracts that GTK can
  expose directly. Use the visual geometry smoke lane when the invariant is
  screenshot-visible, same-session, or says that one surface must not change
  pixels while another surface appears, disappears, or resizes. Reach for the
  broader visual, portal/sandbox, accessibility, or performance smoke lanes only
  when the current widget target cannot express the behavior.

## GTK Lush Proof Stack

Prefer the existing GTK Lush proof infrastructure before adding new harness
shape:

- Use `gtk-lush-proof-harness` for reusable headless widget-test mechanics,
  test registration, and environment recommendations.
- Use `gtk-lush-proof-spine` for readiness, blocker, snapshot,
  workflow-event, and artifact-envelope value objects. Keep app state,
  transport, and D-Bus ownership outside the crate.
- Use `cargo-gtk-proof` and `make visual-geometry-smoke` for same-session
  visual proof, schema/policy validation, pixel anchors, and bounded proof
  artifacts.
- If a test change needs new proof-harness, proof-spine, or proof-tool API,
  also use `gtk-lush-stewardship` and update the adoption matrix, examples,
  doctests, policy checks, and public-API advisory surface as needed.

## Current Test Map

| Level | What | Location | Display | Command |
|-------|------|----------|---------|---------|
| Unit | Domain invariants and small pure helpers | crate-local `#[cfg(test)]` modules | No | `make test-unit` |
| Service | Filesystem and persistence logic in lib tests | crate-local `#[cfg(test)]` modules | No | `make test-unit` |
| Property | Pure or tiny deterministic tempdir-backed invariants over bounded generated inputs | `crates/lushtext-core/tests/properties.rs` and `crates/lushtext-core/tests/properties/*.rs` | No | `make test-prop` |
| Fuzz replay | Committed fuzz corpus seeds through narrow non-GTK helpers | `crates/lushtext-core/tests/fuzz_corpus_replay.rs` and `fuzz/corpus/**` | No | `make fuzz-corpus-replay` |
| Fuzz | Hostile byte ingestion and bounded operation scripts through narrow non-GTK helpers | `fuzz/fuzz_targets/*.rs` and `fuzz/corpus/**` | No | `make fuzz-smoke` |
| Integration | Cross-service workflows with real temp directories | `crates/lushtext/tests/integration.rs` and `crates/lushtext/tests/integration/*.rs` | No | `make test-int` |
| Widget | Widget and real-window behavior, including workflow-level UI regressions | `crates/lushtext/tests/widget.rs` and `crates/lushtext/tests/widget/*.rs` | Private headless Mutter only | `make test-widget` |
| Automation docs drift | Exported action, D-Bus, snapshot, readiness, automation-client, and helper-flag documentation contract | `docs/automation.md`, `docs/automation-reference.md`, `scripts/check-automation-docs.py` | No | `make check-automation-docs` |
| Automation client self-test | Reusable D-Bus helper parser, typed action parameters, statuses, and artifact summaries | `scripts/lushtext-automation.py` | No | `make automation-client-self-test` |
| Visual smoke | Rendered desktop screenshots and compositor/session artifacts | `scripts/run-visual-smoke.sh` | Yes | `make visual-smoke` |
| Visual geometry smoke | Same-process before/after screenshots with protected-region pixel comparisons, pixel anchors, and bounded geometry snapshots | `scripts/visual-geometry-smoke.py` | Yes | `make visual-geometry-smoke` |
| Portal/sandbox smoke | Confined Flatpak/Snap runtime diagnostics and skip-aware package checks | `scripts/run-portal-sandbox-smoke.sh` | Host-dependent | `make portal-sandbox-smoke` |
| Accessibility smoke | AT-SPI-enabled focus and accessible metadata checks outside the accessibility-disabled widget harness | `scripts/run-accessibility-smoke.sh` | Yes | `make accessibility-smoke` |
| Performance smoke | Lightweight user-visible latency and throughput sanity checks | `scripts/run-performance-smoke.sh` | No | `make performance-smoke` |

There is **no standalone `make test-e2e` target today**. Many user workflows
already belong in the widget harness, especially when they can be expressed by
constructing a real `LushtextWindow` and waiting for observable state changes.
Use `docs/end-user-coverage.md` for the current lane ownership map before
creating another broad harness.

## Boundary with gtk4-libadwaita-internals

Use this skill to choose the test level, structure assertions, and run the harness.

Use `gtk4-libadwaita-internals` when the tricky part is the GTK contract behind the assertion:

- `WidgetExt::is_visible()` versus the widget's own `visible` property
- `GtkRevealer`, `GtkPaned`, or layout-measurement behavior in unpresented windows
- `GtkSignalListItemFactory` lifecycle, row reuse, or `GtkTreeListModel` behavior
- focus, actions, builder templates, CSS nodes, or accessibility-tree assumptions

If a test is flaky because the assertion is built on the wrong GTK mental model, fix that model first.

## Repo-Specific Harness Facts

- `crates/lushtext/tests/widget.rs` is a custom single-threaded harness. GTK widgets stay on one stable thread, and each test runs in its own child process. Each test is retried once on failure in a fresh process: a transient passes on attempt 2 and is reported as `ok (FLAKY: passed on attempt N)` plus a loud stderr `FLAKY:` warning, while a real failure fails both attempts and stays `FAILED`. A `FLAKY` line is not a clean run — it is a bug to root-cause, never noise to mute.
- Shared widget wait/flush helpers live once in `crates/lushtext/tests/widget/common.rs` (`wait_until`, `flush_events`, `flush_after_delay`, `present_window`). Import them; do not paste private per-file copies (there were once five copy-pasted `wait_until`s, so fixing one missed the rest). `wait_until` polls and then **drains every ready main-loop source** (`while iteration(false) {}`). Drain-to-exhaustion is load-bearing: `spawn_blocking_then` delivers completion via a low-priority `idle_add_once` source, and only draining dispatches it reliably. Do **not** "optimize" `wait_until` into a single blocking `MainContext::iteration(true)` — a higher-priority timeout source starves the idle and every `spawn_blocking_then`-backed wait then times out (verified: that rewrite fails such tests 5/5).
- `build.rs` generates the widget registry from `crates/lushtext/tests/widget/*.rs`.
- `make test` may use `cargo nextest` for non-widget tests across the workspace, but widget tests still run through `scripts/run-widget-tests.sh`, which owns the headless `cargo test --test widget` path. Native/live-display widget runs are forbidden.
- `make test-prop` runs the discovered feature-gated property target. Keep this lane deterministic, bounded, and out of default nextest and mutation runs.
- `make fuzz-corpus-replay` runs the discovered feature-gated replay target on stable Rust. Keep it read-only and out of default test and mutation lanes; CI runs it as a separate explicit job so committed seeds stay guarded.
- `make fuzz-smoke` runs the repository's isolated `cargo-fuzz` project. Keep it out of default test, property, widget, benchmark, mutation, and pull-request CI lanes; use scheduled/manual fuzz smoke for coverage-guided discovery.
- `make automation-client-self-test` proves the reusable
  `scripts/lushtext-automation.py` contract without launching the app. It is a
  fast policy check, not a replacement for real-process automation smoke.
- `make visual-geometry-smoke`, `make visual-smoke`,
  `make portal-sandbox-smoke`, `make accessibility-smoke`, and
  `make performance-smoke` are host-sensitive smoke lanes. They preserve
  artifacts and skip clearly when the host lacks required desktop, portal,
  accessibility, packaging, or benchmark support.
- The widget harness supports `--list --format terse`, which matters for CI and nextest-style discovery.

## Test Seams And Evidence Surfaces

Test-only production seams are four different things under one `_for_test`
suffix. Classify before adding one, and check the workflow's row in
`docs/workflow-readability-matrix.md` for its current counts and status.

| Kind | Recognized by | Disposition |
|---|---|---|
| Inspection | a gated getter so a test can read internal state: counters, pending flags, queue depth, bounds, freshness | consolidate into the workflow's `evidence.rs` surface |
| Configuration | a gated `static` or setter that shortens a delay, lowers a byte limit, or otherwise overrides a policy value | collapse into one per-workflow test policy value |
| Actuation | a gated function that drives a workflow step otherwise reachable only through a file chooser, alert dialog, timer, or worker completion | **deferred**; a missing workflow/presentation boundary, not a pattern to extend |
| Lifecycle probe | a gated hook observing thread identity, disposal completion, or another lifecycle fact with no non-test equivalent | retain |

Rules for reading state in tests:

- A migrated workflow exposes **one** typed evidence surface (`evidence.rs`) that
  is the single source of its observable state. Read it. When a test needs a fact
  the surface does not carry, extend the surface — do not add another per-field
  `pub fn *_for_test` getter.
- Reading the surface must not mutate state, timers, queues, or generation
  counters, and must not require the workflow to be in a particular stage. If an
  accessor advances or resets anything, it is a probe, not evidence.
- Evidence is an internal crate type at the narrowest visibility its readers
  need, never part of the public D-Bus schema. Once a workflow migrates, its
  automation snapshot fields project from that surface, and
  `make check-automation-docs` covers the projection as it lands.
- Test-only timing and limit overrides live in the workflow's single test policy
  value, and no override storage may compile without the test feature. Do not
  scatter new module-level override statics.
- Actuation seams are deferred by decision, not by oversight. Needing a new one
  is a finding to report — name the dialog or timer boundary that is missing —
  rather than a seam to add quietly.
- Unmigrated workflows keep their existing `_for_test` inspection functions;
  consult the matrix row before choosing where to read state, and do not grow the
  count in a workflow whose migration slot is upcoming.

Seam counts have two denominators: gated declarations and gate attribute sites.
State which one you are reporting; one gated `impl` or `mod` block can cover many
functions.

## Default Workflow

1. Pick the lowest level that can prove the behavior.
2. For any UI surface with variable content, write the state matrix before
   choosing assertions: no items/no required context, one or a few
   representative items, many or awkward items, and the narrow/short geometry
   where the surface still promises to work. Do not sign off on a collection,
   picker, browser, command palette, search panel, sidebar, tab header, dialog,
   or empty state after testing only the happy populated path.
3. For transient shell surfaces such as command palette overlays, search bars,
   search panels, menus, popovers, and Focus Mode affordances, cover dismissal
   as a workflow: Escape after focus moved elsewhere, click-away outside the
   surface, inside clicks that must not dismiss, and one-Escape-only topmost
   ordering when another dismissible surface sits underneath.
4. Reuse the existing helpers:
   - `crates/lushtext/tests/integration/common.rs` for `TestContext`
   - `crates/lushtext/tests/widget/common.rs` for `ensure_gtk_init()`, `test_application()`, and `test_window()`
5. For widget tests, use the shared `flush_events()` / `wait_until(...)` from `common.rs` over fixed sleeps. Never copy a private `wait_until`/`flush_*` into a test module — a fix to the shared version would then silently miss your copy.
6. If the code under test uses async GTK callbacks or `spawn_blocking_then`, wait on a visible predicate with a **generous** budget (≥5–10s). These waits depend on background-thread scheduling and I/O, not just main-loop ticks, so a tight 2s budget flakes under load. A larger ceiling never costs time on the fast path — the predicate returns the instant the work lands — and only matters when a loaded machine delays the thread. Reserve short budgets for synchronous UI-state flips.
7. If the behavior depends on real frame-clock progression, do **not** just sleep for the nominal animation duration. Assert on stable invariants or use the repo's narrow test-only knobs when they already exist.
8. Run the smallest relevant command before broadening to the full suite.
9. For action-catalog, read-only automation D-Bus, snapshot, readiness predicate/blocker,
   automation-client, or helper-flag changes, update `docs/automation.md` plus
   `docs/automation-reference.md` and run `make check-automation-docs`. If
   `scripts/lushtext-automation.py` changed, also run
   `make automation-client-self-test` before any heavier smoke lane.
10. When refactoring a large widget or window into sibling modules without changing behavior, still run the widget target for that surface and its adjacent orchestration surface. Visibility and wiring regressions often show up only from the external `crates/lushtext` test crate, not from `lushtext-core` alone.
11. For rendered pixels, first decide whether the proof needs a same-session
   before/after invariant. If yes, use `make visual-geometry-smoke` and inspect
   the protected-region and pixel-anchor comparison artifacts; if the state is
   a standalone visual coverage point, use `make visual-smoke`. For portals,
   installed package confinement, AT-SPI, or coarse user-visible latency, use
   the matching smoke lane and keep its artifacts.

## Headless GTK Runs

Prefer `make test-widget-headless` or `scripts/run-widget-tests.sh --headless` over hand-copying the mutter command. The shared runner owns the CI path, and plain `cargo test --test widget` self-supervises into the same kind of private headless session before GTK initializes. Two retry layers exist and serve different failures: the harness retries each **test** once in a fresh process (nets a one-off per-test transient), and `--retries N` reruns the **whole suite** in a brand-new Mutter + dbus session (nets a degraded compositor session). Neither is a license to ignore the failure — see Flake Discipline.

The underlying headless invocation is:

```bash
export XDG_RUNTIME_DIR="$(mktemp -d)"
export GDK_BACKEND=wayland
export LUSHTEXT_WIDGET_HEADLESS_RUNNER=1
dbus-run-session -- \
  mutter --headless --wayland --no-x11 --virtual-monitor 2560x1600 -- \
    cargo test --test widget
```

Do not add or use a live-display/native widget mode. If a behavior only reproduces on the human's desktop, switch to `gtk-agentic-debugging` and keep that separate from the widget harness.

## Flake Discipline (read before muting any flake)

A flaky test is a bug, not weather. Tolerating it — adding a blanket retry, bumping a timeout at random, or shrugging because "it passed on rerun" — without finding the cause is not allowed. The retry layers and `FLAKY:` reporting exist to keep the pipeline moving **and to make flakes loud**, not to let them be ignored.

When a widget test flakes (fails then passes, or `FLAKY:` appears):

1. **Get the real failure.** Read the panic location and message from the full log, not a grep-filtered tail (parent `print!`/child-panic stderr interleave, so the test name can sit a few lines above the panic). `condition was not met within Ns` means a `wait_until` predicate never came true in time; the file:line is the helper, so find the **caller**.
2. **Classify the wait.** Window/surface realization and `spawn_blocking_then`/file-I/O completion are async and scheduling-dependent — they need generous budgets (≥5–10s). A synchronous UI flip that flakes usually means the assertion is racing real work that has not happened yet (wrong predicate), not a budget problem.
3. **Fix the root cause, then prove it.** Give the wait an adequate budget, fix the predicate, or fix the production race — then rerun the affected test several times *in isolation* (`run-widget-tests.sh --headless -- <test_name>`), which separates a genuine break (fails every run) from load amplification. A timeout bump alone, with no understanding of *why* it timed out, is tolerating, not fixing — but equally, do not change the poll *mechanism* without proving it against `spawn_blocking_then` delivery (the idle-source trap above).
4. **Never mask a real hang.** A generous timeout only changes how long a genuinely broken test takes to fail; it must never turn a deterministic failure green. If a test only passes *because* of the retry every time, it is broken — investigate it as a failure.
5. **De-duplicate the helper.** If the flaky wait used a copy-pasted local helper, delete the copy and route through `common.rs` so the fix can't be missed elsewhere.

This applies to load-amplified flakes too: heavy local load exposing a 2s async budget is a real fragility (the budget was always too tight), so fix the budget rather than blaming the machine.

## Assertion Rules That Save Time

- For collection, picker, browser, command-palette, search-result, and
  status-page surfaces, assert the state extremes directly when possible:
  empty/no-context copy and reachable controls, representative populated rows,
  dense scrolling behavior, and constrained geometry. A model count assertion
  is not enough if the regression risk is visibility, legibility, clipping,
  unintended scrollbars, or controls pushed outside the allocation.
- For transient overlays, test the user's dismissal contract rather than only
  the revealer property: focus moved away but Escape still closes, outside
  pointer presses close and restore focus, inside pointer presses keep the
  surface open, and one Escape closes only the topmost surface when stacked.
- For parented widgets inside an unpresented window, prefer `widget.property::<bool>("visible")` when you need the widget's own visibility flag. `is_visible()` answers a different question and walks the parent chain.
- For `GtkListView` keyboard shortcuts, do not assume the focused widget is the list itself. In real sessions focus usually sits on a realized row descendant, so synthetic key delivery should target the focused widget and, if needed, walk up its parent chain until it reaches the ancestor that owns the `EventControllerKey`. Emitting the key directly on the list view can produce false-green tests for shortcuts that do nothing in the live app.
- For adaptive surfaces that animate, especially `AdwBottomSheet` used by the document-properties pane, do not close the animated surface as a cleanup step in headless widget tests unless the test is specifically proving the close animation. Assert the open/compact state, then explicitly destroy the presented test window and flush once. Closing the sheet right before process teardown can make Mutter report `gdk-frame-clock: layout continuously requested` and `Trying to snapshot LushtextWindow ... without a current allocation`, even when the production UI state is correct.
- For split-view animation regressions, add widget tests around the contracts the harness can prove: allocation sync should not rewrite persisted GSettings fractions, and cached breakpoint thresholds should only change when the effective integer threshold changes. Do not try to prove frame-rate smoothness in the widget harness; confirm that part in the real app or locally installed Flatpak after the contract tests pass.
- For same-session visual invariants, widget tests should assert the GTK
  allocation and scroll-anchor contract, while `make visual-geometry-smoke`
  should protect unaffected chrome with exact pixel comparisons and describe
  every allowed-changing region. Do not count two unrelated screenshots from
  separate launches as proof that an unaffected element had zero pixel variance.
- For before/after visual-geometry cases with setup such as search queries,
  scrolling, fixture selection, or focus state, drive that precondition and
  wait for its narrow readiness predicate before capturing the baseline image.
  A baseline taken before the intended state exists can make the comparison
  pass or fail for the wrong reason.
- For rendered effects such as highlights, edge lines, minimap markers, or
  overlay chrome, widget tests should assert the app-owned allocation/projection
  and `make visual-geometry-smoke` should assert named screenshot-derived
  `pixel_anchors` for the actual pixels. If the proof policy requires a named
  pixel invariant, the root summary must include it in
  `pixel_verified_invariant_ids`, and the relevant case rows must include
  screenshot-derived pixel row evidence plus final sidebar/editor/minimap
  geometry and final-frame rendered-anchor stability; `verified_invariant_ids`
  alone is not enough. If the rendered effect can drift during an animation,
  require stream-frame evidence with at least one mapped intermediate frame;
  a final settled frame is not proof for that class.
- For user-reported visual oddities that only reproduce at a live desktop size,
  run `scripts/lushtext-automation.py visual-geometry-capture ...` while the
  window is in the reproduced state, provide explicit overrides for unknown
  theme, word-wrap, fixture, direction, or viewport fields, and replay the
  generated scenario with `scripts/visual-geometry-smoke.py --scenario-dir ...`.
  Do not substitute nearby 720p, 1080p, 1440p, or generic maximized-like passes
  for the captured threshold class.
- Test behavior, not GTK implementation details. Avoid pixel assertions, CSS rendering expectations, or proving that GTK's own containers work.
- Keep widget tests narrowly scoped. A real window is fine; an enormous end-to-end script is usually not.
- If a failure only reproduces in a live desktop session with compositor or portal behavior, switch to `gtk-agentic-debugging`.

## What To Add After a Change

- New domain or model logic: unit tests.
- Pure or tiny deterministic tempdir-backed invariants with broad input space: property tests with bounded generators.
- Hostile byte ingestion, encoding/parser setup, or panic-resistance over arbitrary bytes: fuzz targets plus minimized corpus seeds for real crashes.
- Known fuzz seed regression checks that should not require nightly or sanitizer setup: stable corpus replay.
- New service or persistence behavior: service or integration tests with `TestContext`.
- New widget state, signal wiring, or window orchestration: widget tests.
- New internal state a test must observe: a field on the workflow's evidence
  surface, plus the matrix row update — not a new per-field inspection seam.
- Bug fix: add the lowest-level regression test that reproduces the bug reliably.
- Workflow that truly needs compositor behavior beyond the current widget harness: discuss whether a new dedicated target is justified before creating one.
- Large adapter refactor with no intended behavior change: rerun the widget suites for the touched widget plus neighboring window/sidebar/search orchestration so extracted helper visibility and callback wiring stay covered.

## References

- [references/widget-testing.md](references/widget-testing.md): current harness behavior, headless commands, wait helpers, and contract-sensitive assertions
- [references/test-recipes.md](references/test-recipes.md): concrete service, integration, and widget recipes aligned with the repo's current helpers
- [../../../docs/fuzzing.md](../../../docs/fuzzing.md): fuzz target scope, smoke commands, corpus handling, and crash minimization
- [../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md](../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md): layout and visibility contracts that affect widget assertions
- [../gtk4-libadwaita-internals/references/containers-lists-and-factories.md](../gtk4-libadwaita-internals/references/containers-lists-and-factories.md): `GtkListView` and `GtkTreeListModel` lifecycle rules for test reasoning
- [../gtk4-libadwaita-internals/references/builder-templates-actions-css-accessibility.md](../gtk4-libadwaita-internals/references/builder-templates-actions-css-accessibility.md): focus, builder-template, CSS, and accessibility guidance for test assertions
