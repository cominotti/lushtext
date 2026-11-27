---
name: gtk-testing
description: "Choosing and writing LushText tests: the right test level, the headless widget harness, GTK Lush proof crates and cargo-gtk-proof, flake discipline and the test-seam taxonomy. Use when adding or fixing tests, debugging headless or flaky GTK tests, changing CI test configuration, or editing crates/lushtext/tests/**, crates/gtk-lush/proof-* or crates/cargo-gtk-proof."
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

Pick the lowest level in the Test Map that can prove the behavior. Reach for the
broader visual, portal/sandbox, accessibility, or performance smoke lanes only
when the widget target cannot express it.

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
| Property | Pure or tiny deterministic tempdir-backed invariants over bounded generated inputs | `crates/lushtext-ui/tests/properties.rs` and `crates/lushtext-ui/tests/properties/*.rs` | No | `make test-prop` |
| Fuzz replay | Committed fuzz corpus seeds through narrow non-GTK helpers | `crates/lushtext-ui/tests/fuzz_corpus_replay.rs` and `fuzz/corpus/**` | No | `make fuzz-corpus-replay` |
| Fuzz | Hostile byte ingestion and bounded operation scripts through narrow non-GTK helpers | `fuzz/fuzz_targets/*.rs` and `fuzz/corpus/**` | No | `make fuzz-smoke` |
| Integration | Cross-service workflows with real temp directories | `crates/lushtext/tests/integration.rs` and `crates/lushtext/tests/integration/*.rs` | No | `make test-int` |
| Widget | Widget and real-window behavior, including workflow-level UI regressions | `crates/lushtext/tests/widget.rs` and `crates/lushtext/tests/widget/*.rs` | Private headless Mutter only | `make test-widget` |
| Automation docs drift | Exported action, D-Bus, snapshot, readiness, automation-client, and helper-flag documentation contract | `docs/automation.md`, `docs/automation-reference.md`, `scripts/check-automation-docs.py` | No | `make check-automation-docs` |
| Automation client self-test | Reusable D-Bus helper parser, typed action parameters, statuses, and artifact summaries | `scripts/lushtext-automation.py` | No | `make automation-client-self-test` |
| GTK axioms | Isolated probes of the GTK, Libadwaita, Pango, GIO and GtkSourceView behaviours LushText designs rely on (every GTK axiom ledger entry), plus one interactive/`--check` sample per probe | `crates/gtk-lush/axioms/` (probes, samples) and `crates/gtk-lush-adoption-lab/tests/axiom_probes.rs` (runner) | Private headless Mutter only | `make gtk-axioms` |
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

## GTK Axiom Probes And The Toolkit-Update Alarm

A belief about GTK that a design depends on is a ledger axiom
([gtk-axiom-ledger.md](../gtk4-libadwaita-internals/references/gtk-axiom-ledger.md));
its probe lives in `gtk-lush-axioms` (add one with the crate README's "Adding an
axiom"), and the LushText consumer tests stay in the widget binary. On any GTK or
Libadwaita version change, run `make gtk-axioms` first and follow
`.agents/rules/testing.md`, "GTK Axiom Probes and Samples", `build.md`, "GTK
Axioms and Toolkit Updates", and `docs/runbooks/gtk-toolkit-update.md`: a
failing probe is an axiom change, never a probe to adjust first.

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
- Shared widget wait/flush helpers live once in `crates/lushtext/tests/widget/common.rs` (`wait_until`, `flush_events`, `flush_after_delay`, `present_window`); import them (`widget-wiring.md`, "Wait helpers are shared"). `wait_until` polls and then **drains every ready main-loop source**, which is what dispatches `spawn_blocking_then` completions (one source at `gtk_lush_tasks::completion_priority()`). **The idle-source trap:** never rewrite that drain into one blocking `MainContext::iteration(true)`; a higher-priority timeout source starves the completion source and every `spawn_blocking_then`-backed wait times out.
- `build.rs` generates the widget registry from `crates/lushtext/tests/widget/*.rs`.
- CI runs the widget suite in shards from `scripts/widget-shards.py` (`Widget Tests (<shard>)` jobs). A new test in an existing module needs no table edit; a **new widget module file must be added to a shard** in that table, or `make check-widget-shards` fails. Reproduce one CI shard locally with `make test-widget-shard WIDGET_SHARD=<shard>`. `make test-widget` runs every shard **in parallel**, each in its own capped headless session, and ends with a per-shard summary (`target/widget-parallel/<shard>.log` keeps each shard's full output); `WIDGET_PARALLEL=0` runs everything in one serial session, which is the one to use when bisecting an ordering or load question.
- `make test` may use `cargo nextest` for non-widget tests, but widget tests run through `scripts/run-widget-tests.sh`; native/live-display widget runs are forbidden. Property tests, corpus replay and `make fuzz-smoke` stay out of default nextest and mutation runs, and fuzz smoke out of pull-request CI.
- The widget harness supports `--list --format terse`, which matters for CI and nextest-style discovery.
- Each child attempt gets its own empty `TMPDIR`, and a `TEMP-LEAK:` is a defect to fix at its owner, never to allowlist: `.agents/rules/testing.md`, "Temporary Files".

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

Reading state in tests: read the workflow's one `evidence.rs` surface and extend
it rather than add a `*_for_test` getter; the surface invariants and the
`test_policy.rs` rule are in `.agents/rules/workflow-convention.md`, "Evidence
surfaces", and `widget-wiring.md`, "Testing". Two points the rules leave to this
skill:

- Actuation seams are deferred by decision, not by oversight. Needing a new one
  is a finding to report — name the dialog or timer boundary that is missing —
  rather than a seam to add quietly.
- Seam counts have two denominators: gated declarations and gate attribute sites.
  State which one you are reporting; one gated `impl` or `mod` block can cover many
  functions.

## Default Workflow

1. Pick the lowest level that can prove the behavior.
2. Reuse the existing helpers:
   - `crates/lushtext/tests/integration/common.rs` for `TestContext`
   - `crates/lushtext/tests/widget/common.rs` for `ensure_gtk_init()`, `test_application()`, and `test_window()`
3. Wait on a visible predicate with the shared helpers, **≥5–10 s for async waits** (`spawn_blocking_then`, realization, file I/O) and short budgets only for synchronous UI-state flips; never sleep for an animation's nominal duration.
4. Run the smallest relevant command before broadening to the full suite.
5. When refactoring a large widget or window into sibling modules without changing behavior, still run the widget target for that surface and its adjacent orchestration surface. Visibility and wiring regressions often show up only from the external `crates/lushtext` test crate, not from `lushtext-ui` alone.

The widget-test obligations live in the rules, which load only for `ui/**` and
widget-test paths; read them before writing a widget test: `widget-wiring.md`,
"State-extreme coverage", "Window-Level Transient Surface Dismissal", "Wait
helpers are shared" and "Budget the wait"; `ui.md`, "Transient Shell Surfaces";
`testing.md`, "Proof Obligations of a Change" (visual proof). Automation-surface
changes update the docs and run `make check-automation-docs` (plus
`make automation-client-self-test` when `scripts/lushtext-automation.py`
changed), as `documentation.md` requires.

## Headless GTK Runs

Prefer `make test-widget-headless` or `scripts/run-widget-tests.sh --headless` over hand-copying the mutter command. The shared runner owns the CI path, and plain `cargo test --test widget` self-supervises into the same kind of private headless session before GTK initializes. Two retry layers exist and serve different failures: the harness retries each **test** once in a fresh process (nets a one-off per-test transient), and `--retries N` reruns the **whole suite** in a brand-new Mutter + dbus session (nets a degraded compositor session); in a parallel local run it reruns only the failed **shard**, which the summary reports as `FLAKY-SHARD:`, and an `OOM-KILL:` line means the shard's memory scope killed a process, which fails the run even if a retry passed. Neither is a license to ignore the failure — see Flake Discipline.

The underlying headless invocation is:

```bash
export GDK_BACKEND=wayland
export LUSHTEXT_WIDGET_HEADLESS_RUNNER=1
scripts/with-temp-scope.sh --session-runtime widget -- \
  dbus-run-session -- \
  mutter --headless --wayland --no-x11 --virtual-monitor 2560x1600 -- \
    cargo test --test widget
```

`--session-runtime` provides the short private `XDG_RUNTIME_DIR` and removes it only once the session's activated services (notably `xdg-document-portal` and its `doc/` FUSE mount) have exited; removing it straight after `dbus-run-session` returns leaks it.

Do not add or use a live-display/native widget mode. If a behavior only reproduces on the human's desktop, switch to `gtk-agentic-debugging` and keep that separate from the widget harness.

## Flake Discipline (read before muting any flake)

A flaky test is a bug, not weather. Tolerating it — adding a blanket retry, bumping a timeout at random, or shrugging because "it passed on rerun" — without finding the cause is not allowed. The retry layers and `FLAKY:` reporting exist to keep the pipeline moving **and to make flakes loud**, not to let them be ignored.

When a widget test flakes (fails then passes, or `FLAKY:` appears):

1. **Get the real failure.** Read the panic location and message from the full log, not a grep-filtered tail (parent `print!`/child-panic stderr interleave, so the test name can sit a few lines above the panic). `condition was not met within Ns` means a `wait_until` predicate never came true in time; the file:line is the helper, so find the **caller**.
2. **Classify the wait.** Window/surface realization and `spawn_blocking_then`/file-I/O completion are async and scheduling-dependent — they need generous budgets (≥5–10s). A synchronous UI flip that flakes usually means the assertion is racing real work that has not happened yet (wrong predicate), not a budget problem.
3. **Fix the root cause, then prove it.** Give the wait an adequate budget, fix the predicate, or fix the production race — then rerun the affected test several times *in isolation* (`run-widget-tests.sh --headless -- <test_name>`), which separates a genuine break (fails every run) from load amplification. A timeout bump alone, with no understanding of *why* it timed out, is tolerating, not fixing — but equally, do not change the poll *mechanism* without proving it against `spawn_blocking_then` delivery (the idle-source trap in Repo-Specific Harness Facts).
4. **Never mask a real hang.** A generous timeout only changes how long a genuinely broken test takes to fail; it must never turn a deterministic failure green. If a test only passes *because* of the retry every time, it is broken — investigate it as a failure.
5. **De-duplicate the helper.** If the flaky wait used a copy-pasted local helper, delete the copy and route through `common.rs` so the fix can't be missed elsewhere.

This applies to load-amplified flakes too: heavy local load exposing a 2s async budget is a real fragility (the budget was always too tight), so fix the budget rather than blaming the machine.

The same holds for smoke-lane probes. A probe that *reads* state (an AT-SPI tree walk, a screenshot, a D-Bus snapshot) has a cost of its own that grows with load: an AT-SPI walk is several synchronous calls per node, each answered by the app's main thread (measurements: the `capture-lushtext-mutter.py` header). Never cap one attempt below the overall wait and "retry on timeout": a killed attempt discards its work, so once the read costs more than the cap every retry dies too and the wait cannot succeed however long it is. Bound each attempt by the time left before the one deadline, make the read stop as soon as it has the answer, and record per-attempt durations (`probe_ms=` in `automation-waits.txt`, `walk_ms=`/`call_failures=` in the probe's stdout) so the next budget is derived from data.

## Assertion Rules That Save Time

The detailed assertion rules (dismissal after focus moved away, accessible `Hidden` by value, `GtkListView` key routing, `AdwBottomSheet` teardown, split-view contracts, visual-geometry baselines, stream-frame evidence, live-size capture, and `RowStillnessProbe`) are in [references/widget-testing.md](references/widget-testing.md#assertion-rules); state extremes and `is_visible()` are in `widget-wiring.md`, "Testing".

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
