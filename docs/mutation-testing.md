# Mutation Testing

LushText uses `cargo-mutants` to measure whether deterministic tests actually
fail when production behavior changes. It is a companion gate: it does not
replace `cargo nextest`, GTK widget tests, benchmarks, formatting, Clippy, or
dependency-policy checks. Property testing is another companion gate for
generated input invariants; it does not replace mutation testing either.

## Scope

The default scope is configured in `.cargo/mutants.toml`:

- Domain model code under `crates/lushtext-core/src/model`
- Service code under `crates/lushtext-core/src/services`
- A narrow set of pure helper-heavy UI modules that do not need a display server

Do not add broad GTK widget modules to the default mutation scope. Widget
construction, signal wiring, focus behavior, dialogs, file choosers, and live
allocation behavior belong in `scripts/run-widget-tests.sh`, where the harness
owns Mutter, D-Bus, renderer settings, retries, and warning filtering.

## Local Setup

Install the tools once:

```sh
cargo install --locked cargo-mutants --version 27.0.0
curl -LsSf "https://get.nexte.st/0.9.137/linux" | tar zxf - -C "${CARGO_HOME:-$HOME/.cargo}/bin"
```

The wrapper checks for both binaries before running.

## Commands

Use the Makefile targets for day-to-day work:

```sh
make mutants-smoke
make mutants-diff
make mutants-full
make mutants-list
```

Those call `scripts/run-mutants.sh`, which centralizes flags and safety checks.
Useful environment overrides:

```sh
MUTANTS_TIMEOUT=600 make mutants-full
MUTANTS_SMOKE_FILE=crates/lushtext-core/src/services/file_limits.rs make mutants-smoke
MUTANTS_BASE=origin/main make mutants-diff
MUTANTS_SHARD=0/4 make mutants-full
MUTANTS_JOBS=8 MUTANTS_TEST_THREADS=3 MUTANTS_BUILD_JOBS=3 make mutants-full   # override local parallelism
```

`mutants-smoke` is the fast sanity check for tool installation, config parsing,
and timeout behavior. `mutants-diff` creates a diff against `origin/main` when
no diff file is supplied and filters mutants to changed hunks. `mutants-full`
runs the configured deterministic scope and can be sharded with `MUTANTS_SHARD`.
`mutants-list` prints the configured candidates without running tests.

## Local Parallelism

cargo-mutants is serial by default — one mutant builds and tests at a time —
which leaves a multi-core host mostly idle on the slowest gate. The local
Makefile targets (`mutants-smoke`, `mutants-diff`, `mutants-full`) auto-tune
this: `MUTANTS_JOBS` defaults to about `nproc / 4`, and the two per-job caps
default so that `jobs x per-job-parallelism` stays near the logical CPU count:

- `MUTANTS_TEST_THREADS` (default `4`) bounds the test phase — each mutant job
  launches its own nextest, which otherwise grabs every core.
- `MUTANTS_BUILD_JOBS` (derived, ~`nproc / jobs`) bounds the build phase via
  `CARGO_BUILD_JOBS` — without it, the concurrent cold builds each fan out to
  every core and spike load average far above `nproc` even though IO and memory
  stay quiet (the build phase is the one that pushed load to ~100 in testing).

Together these keep both phases near `nproc` instead of thrashing. Override any
knob inline (see above) or via `MUTANTS_LOCAL_JOBS` /
`MUTANTS_LOCAL_TEST_THREADS` / `MUTANTS_LOCAL_BUILD_JOBS` in the Makefile
invocation.

CI deliberately does not use this. The mutation workflow calls
`scripts/run-mutants.sh` directly and leaves `MUTANTS_JOBS` unset, so the small
sharded runners keep cargo-mutants' serial default; cross-machine fan-out there
comes from `MUTANTS_SHARD`, not local jobs.

The wrapper intentionally does not pass `--features property-tests`. The
dedicated property target runs through `make test-prop` so generated cases do
not multiply by every mutant.

## In-Place Safety

CI uses `--in-place` because the checkout is disposable and a separate nextest
baseline has already passed. Local runs are copy-based by default. If you set
`MUTANTS_IN_PLACE=1` outside CI, `scripts/run-mutants.sh` refuses to run unless
the worktree, index, and untracked-file set are clean.

Use a clean checkout or a disposable worktree for local in-place experiments:

```sh
git worktree add ../lushtext-mutants HEAD
cd ../lushtext-mutants
MUTANTS_IN_PLACE=1 MUTANTS_BASELINE_SKIP=1 make mutants-full
```

## CI Behavior

The mutation workflow has two lanes:

- Pull requests run non-widget `cargo nextest run --workspace`, generate a full
  history diff against the PR base, then run changed-code mutation with
  `--baseline=skip`.
- Scheduled and manual runs first prove the same non-widget baseline, then run
  the configured full scope in shards. The full-scope lane starts report-only
  so survivor backlog is visible without blocking unrelated work.

Every mutation job uploads `mutants.out` when it exists. Those directories are
ignored locally because they contain generated diffs, logs, and JSON outcome
data.

The mutation workflow also intentionally omits `lushtext-core/property-tests`.
The separate CI property-test job proves generated-input invariants without
folding that cost into cargo-mutants.

## Initial Calibration

The first local full-scope calibration on June 1, 2026 ran all four shards with
`cargo-mutants 27.0.0` and `MUTANTS_BASELINE_SKIP=1`. The pre-adjustment scope
selected 1,518 mutants and finished with:

| Shard | Total | Caught | Missed | Unviable | Timed out |
|-------|-------|--------|--------|----------|-----------|
| `0/4` | 380 | 205 | 142 | 33 | 0 |
| `1/4` | 380 | 210 | 127 | 43 | 0 |
| `2/4` | 380 | 144 | 189 | 47 | 0 |
| `3/4` | 378 | 165 | 189 | 24 | 0 |
| **Total** | **1,518** | **724** | **647** | **147** | **0** |

The largest missed clusters were:

| File | Missed mutants | Ratchet direction |
|------|----------------|-------------------|
| `crates/lushtext-core/src/ui/editor_page/minimap.rs` | 215 | Geometry and marker-color helper assertions |
| `crates/lushtext-core/src/services/editor_io.rs` | 74 | Lossy preview, line ending, file health, and encoding-analysis assertions |
| `crates/lushtext-core/src/ui/markdown_preview/inline_footnotes.rs` | 49 | Scan-plan, delimiter, escape, and lowered-output tests |
| `crates/lushtext-core/src/services/palette/index.rs` | 26 | Index construction, root interning, recursion cap, and path filtering tests |
| `crates/lushtext-core/src/model/encoding.rs` | 24 | Table tests for IDs, labels, BOM policy, display, and mode parsing |
| `crates/lushtext-core/src/services/local_history_service.rs` | 23 | Availability, snapshot lifecycle, and pruning tests |
| `crates/lushtext-core/src/services/bookmark_service.rs` | 23 | Sidecar delete, move, root matching, and list-workspace tests |
| `crates/lushtext-core/src/services/notifications.rs` | 20 | Progress, expiry, dismiss, and inline-view reducer tests |

Calibration also found that `crates/lushtext-core/src/ui/window/tabs.rs`
produced 40 missed mutants dominated by `LushtextWindow::...` GTK adapter
methods. That file was removed from the default mutation scope rather than
excluded by a broad pattern; tab behavior stays in the widget harness until
smaller pure tab policy helpers are extracted. After that correction,
`scripts/run-mutants.sh list` reported 1,431 configured mutants.

On June 2, 2026, the minimap cluster was ratcheted separately. The
non-widget-only focused slice for
`crates/lushtext-core/src/ui/editor_page/minimap.rs` moved from 215 missed
mutants to 86 after adding deterministic tests for minimap policy constants,
availability priority, wrapped-layout size policy, line-budget scanning,
long-line warning lines, marker bounds, lane widths, lane positioning, and the
light/dark marker palette. The remaining 86 survivors were classified as:

- `LushtextEditorPage::...` GTK adapter methods already covered by the widget
  harness for visibility, settings, marker counts, search/bookmark/modified
  markers, long-line toggles, Focus Mode, and too-large feedback.
- Mapped `GtkSourceMap` geometry and Cairo drawing functions whose observable
  contracts are asserted by widget projection tests, while their pure math and
  color helpers remain in the mutation scope.
- Five equivalent `fit_marker_bounds` exact-boundary mutants that produce the
  same final clamped marker bounds or mutate unreachable post-clamp states.

Those minimap leftovers are captured as narrow documented exclusions in
`.cargo/mutants.toml`.

After the remaining model, service, palette, search, persistence, and
Markdown-footnote survivors were ratcheted with focused deterministic tests,
the June 2, 2026 full-scope sharded run selected 1,313 configured mutants and
finished cleanly:

| Shard | Total | Caught | Missed | Unviable | Timed out |
|-------|-------|--------|--------|----------|-----------|
| `0/4` | 329 | 299 | 0 | 30 | 0 |
| `1/4` | 329 | 292 | 0 | 37 | 0 |
| `2/4` | 328 | 281 | 0 | 47 | 0 |
| `3/4` | 327 | 311 | 0 | 16 | 0 |
| **Total** | **1,313** | **1,183** | **0** | **130** | **0** |

The scheduled/manual full-scope lane can be ratcheted from report-only to a
blocking gate once the CI runtime budget is accepted.

## Triage Policy

Start from `mutants.out/outcomes.json`, then inspect the per-mutant log and diff.
Classify each survivor:

- **Missed behavior:** add or tighten a deterministic test. Prefer unit tests
  near model and service code. For GTK behavior, move pure decision logic behind
  a deterministic helper before testing it.
- **Equivalent mutant:** the mutation does not change observable behavior.
  Prefer no exclusion when the survivor count is small; otherwise add a narrow
  `exclude_re` or `exclude_globs` entry with a reason in `.cargo/mutants.toml`.
- **Unviable mutant:** the mutation is outside the intended scope, flaky under
  the non-widget runner, or better covered by the widget harness. Exclude only
  the smallest stable path or pattern.
- **Timeout:** first check whether the baseline test is too broad or blocked.
  Increase `MUTANTS_TIMEOUT` only when the test is legitimately slow and stable.

Do not silence a survivor just because the current test suite misses it. The
preferred ratchet is tests first, small deterministic extraction second, narrow
documented exclusion last.

## Relation to Other Gates

- `cargo nextest run --workspace` remains the baseline for non-widget Rust tests.
- `make test-prop` runs bounded property tests for pure deterministic invariants.
- `scripts/run-widget-tests.sh --headless --retries 1` remains the GTK behavior
  gate for display-server-sensitive code.
- `cargo bench -p lushtext-core --no-run` still compile-checks performance
  harnesses without requiring a full benchmark run.
- `cargo fmt`, Clippy, rustdoc lints, and `cargo deny` keep their existing roles.

Mutation testing answers a narrower question: if deterministic production logic
is changed in small ways, do the tests catch it?

Property testing answers a different question: do pure invariants hold across
many generated inputs? Keep the default lanes separate. If a future change
intentionally wants mutation testing to exercise a tiny property, add a new
documented mutation mode or narrow opt-in that passes `--features property-tests`
explicitly instead of changing the default wrapper.
