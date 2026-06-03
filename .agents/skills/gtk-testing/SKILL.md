---
name: gtk-testing
description: "Guide testing strategy for GTK4/Libadwaita Rust applications, especially LushText's current unit, integration, and custom widget-test harness. Use when adding or fixing tests, deciding the right test level, debugging headless GTK failures, modifying `crates/lushtext/tests/**`, or reasoning about widget visibility, animation, focus, and lifecycle assertions in tests. Trigger on mutter headless, widget harness, `TestContext`, flaky GTK tests, CI test configuration, or any request for GTK test coverage."
---

Guide testing for LushText as it exists today. The repo already has solid coverage layers and a custom GTK widget harness; use them before inventing new structure.

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
- Reach for the visual, portal/sandbox, accessibility, or performance smoke
  lanes only when the current widget target cannot express the behavior.

## Current Test Map

| Level | What | Location | Display | Command |
|-------|------|----------|---------|---------|
| Unit | Domain invariants and small pure helpers | crate-local `#[cfg(test)]` modules | No | `make test-unit` |
| Service | Filesystem and persistence logic in lib tests | crate-local `#[cfg(test)]` modules | No | `make test-unit` |
| Property | Pure or tiny deterministic tempdir-backed invariants over bounded generated inputs | `crates/lushtext-core/tests/properties.rs` and `crates/lushtext-core/tests/properties/*.rs` | No | `make test-prop` |
| Fuzz replay | Committed fuzz corpus seeds through narrow non-GTK helpers | `crates/lushtext-core/tests/fuzz_corpus_replay.rs` and `fuzz/corpus/**` | No | `make fuzz-corpus-replay` |
| Fuzz | Hostile byte ingestion and bounded operation scripts through narrow non-GTK helpers | `fuzz/fuzz_targets/*.rs` and `fuzz/corpus/**` | No | `make fuzz-smoke` |
| Integration | Cross-service workflows with real temp directories | `crates/lushtext/tests/integration.rs` and `crates/lushtext/tests/integration/*.rs` | No | `make test-int` |
| Widget | Widget and real-window behavior, including workflow-level UI regressions | `crates/lushtext/tests/widget.rs` and `crates/lushtext/tests/widget/*.rs` | Yes | `make test-widget` |
| Visual smoke | Rendered desktop screenshots and compositor/session artifacts | `scripts/run-visual-smoke.sh` | Yes | `make visual-smoke` |
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
- `make test` may use `cargo nextest` for non-widget tests across the workspace, but widget tests still run through `scripts/run-widget-tests.sh`, which owns the native and headless `cargo test --test widget` paths.
- `make test-prop` runs the feature-gated `lushtext-core/property-tests` target. Keep this lane deterministic, bounded, and out of default nextest and mutation runs.
- `make fuzz-corpus-replay` runs the feature-gated `lushtext-core/fuzzing` replay target on stable Rust. Keep it read-only and out of default test and mutation lanes; CI runs it as a separate explicit job so committed seeds stay guarded.
- `make fuzz-smoke` runs the isolated `cargo-fuzz` project under `fuzz/` with `lushtext-core/fuzzing`. Keep it out of default test, property, widget, benchmark, mutation, and pull-request CI lanes; use scheduled/manual fuzz smoke for coverage-guided discovery.
- `make visual-smoke`, `make portal-sandbox-smoke`, `make accessibility-smoke`,
  and `make performance-smoke` are host-sensitive smoke lanes. They preserve
  artifacts and skip clearly when the host lacks required desktop, portal,
  accessibility, packaging, or benchmark support.
- The widget harness supports `--list --format terse`, which matters for CI and nextest-style discovery.

## Default Workflow

1. Pick the lowest level that can prove the behavior.
2. Reuse the existing helpers:
   - `crates/lushtext/tests/integration/common.rs` for `TestContext`
   - `crates/lushtext/tests/widget/common.rs` for `ensure_gtk_init()`, `test_application()`, and `test_window()`
3. For widget tests, use the shared `flush_events()` / `wait_until(...)` from `common.rs` over fixed sleeps. Never copy a private `wait_until`/`flush_*` into a test module — a fix to the shared version would then silently miss your copy.
4. If the code under test uses async GTK callbacks or `spawn_blocking_then`, wait on a visible predicate with a **generous** budget (≥5–10s). These waits depend on background-thread scheduling and I/O, not just main-loop ticks, so a tight 2s budget flakes under load. A larger ceiling never costs time on the fast path — the predicate returns the instant the work lands — and only matters when a loaded machine delays the thread. Reserve short budgets for synchronous UI-state flips.
5. If the behavior depends on real frame-clock progression, do **not** just sleep for the nominal animation duration. Assert on stable invariants or use the repo's narrow test-only knobs when they already exist.
6. Run the smallest relevant command before broadening to the full suite.
7. When refactoring a large widget or window into sibling modules without changing behavior, still run the widget target for that surface and its adjacent orchestration surface. Visibility and wiring regressions often show up only from the external `crates/lushtext` test crate, not from `lushtext-core` alone.
8. For rendered pixels, portals, installed package confinement, AT-SPI, or
   coarse user-visible latency, use the matching smoke lane and keep its
   artifacts.

## Headless GTK Runs

Prefer `make test-widget-headless` or `scripts/run-widget-tests.sh --headless` over hand-copying the mutter command. The shared runner owns the CI path. Two retry layers exist and serve different failures: the harness retries each **test** once in a fresh process (nets a one-off per-test transient), and `--retries N` reruns the **whole suite** in a brand-new Mutter + dbus session (nets a degraded compositor session). Neither is a license to ignore the failure — see Flake Discipline.

The underlying headless invocation is:

```bash
export XDG_RUNTIME_DIR="$(mktemp -d)"
export GDK_BACKEND=wayland
dbus-run-session -- \
  mutter --headless --wayland --no-x11 --virtual-monitor 2560x1600 -- \
    cargo test --test widget
```

Use `make test-widget` locally when a display server is already available.

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

- For parented widgets inside an unpresented window, prefer `widget.property::<bool>("visible")` when you need the widget's own visibility flag. `is_visible()` answers a different question and walks the parent chain.
- For `GtkListView` keyboard shortcuts, do not assume the focused widget is the list itself. In real sessions focus usually sits on a realized row descendant, so synthetic key delivery should target the focused widget and, if needed, walk up its parent chain until it reaches the ancestor that owns the `EventControllerKey`. Emitting the key directly on the list view can produce false-green tests for shortcuts that do nothing in the live app.
- For adaptive surfaces that animate, especially `AdwBottomSheet` used by the document-properties pane, do not close the animated surface as a cleanup step in headless widget tests unless the test is specifically proving the close animation. Assert the open/compact state, then explicitly destroy the presented test window and flush once. Closing the sheet right before process teardown can make Mutter report `gdk-frame-clock: layout continuously requested` and `Trying to snapshot LushtextWindow ... without a current allocation`, even when the production UI state is correct.
- For split-view animation regressions, add widget tests around the contracts the harness can prove: allocation sync should not rewrite persisted GSettings fractions, and cached breakpoint thresholds should only change when the effective integer threshold changes. Do not try to prove frame-rate smoothness in the widget harness; confirm that part in the real app or locally installed Flatpak after the contract tests pass.
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
- Bug fix: add the lowest-level regression test that reproduces the bug reliably.
- Workflow that truly needs compositor behavior beyond the current widget harness: discuss whether a new dedicated target is justified before creating one.
- Large adapter refactor with no intended behavior change: rerun the widget suites for the touched widget plus neighboring window/sidebar/search orchestration so extracted helper visibility and callback wiring stay covered.

## References

- [references/widget-testing.md](references/widget-testing.md): current harness behavior, headless commands, wait helpers, and contract-sensitive assertions
- [references/test-recipes.md](references/test-recipes.md): concrete service, integration, and widget recipes aligned with the repo's current helpers
- [../../docs/fuzzing.md](../../docs/fuzzing.md): fuzz target scope, smoke commands, corpus handling, and crash minimization
- [../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md](../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md): layout and visibility contracts that affect widget assertions
- [../gtk4-libadwaita-internals/references/containers-lists-and-factories.md](../gtk4-libadwaita-internals/references/containers-lists-and-factories.md): `GtkListView` and `GtkTreeListModel` lifecycle rules for test reasoning
- [../gtk4-libadwaita-internals/references/builder-templates-actions-css-accessibility.md](../gtk4-libadwaita-internals/references/builder-templates-actions-css-accessibility.md): focus, builder-template, CSS, and accessibility guidance for test assertions
