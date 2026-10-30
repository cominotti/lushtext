## Why

The widget suite (about 1,300 tests) runs serially in one headless Mutter
session locally, which takes about an hour on the reference workstation. CI
already splits the same suite into the shards of `scripts/widget-shards.py`
(today `window`, `editor-page`, `surfaces`), each measured at 11–11.5 runner
minutes. Locally the shards are independent too, so running them side by side,
each in its own session, would bring `make test-widget` down to roughly the
slowest shard (about 20–25 minutes locally), which is the difference between
running the suite before every widget commit and skipping it.

## What Changes

- `make test-widget`, `make test-widget-headless`, and the widget half of
  `make test` run the shard table's shards **in parallel**, one private
  `dbus-run-session` + `mutter --headless` session per shard, after one shared
  build and the existing `--list` cross-check.
- Each shard runs in its **own admitted memory scope**
  (`scripts/with-memory-cap.sh`, inside `lushtext-heavy.slice`), so the
  host-wide admission (72G slice, 16G desktop reserve) staggers shards instead
  of overcommitting the machine.
- Each shard gets its **own temp scope** (`scripts/with-temp-scope.sh
  --session-runtime`) with a short, distinct `XDG_RUNTIME_DIR` and Wayland
  socket; the driver refuses to start when a socket path would exceed the
  `sun_path` limit. Children keep rooting XDG and `LUSHTEXT_DATA_DIR` in their
  own per-attempt `TMPDIR`, and the TEMP-LEAK check stays per scope.
- One **merged report**: live output prefixed by shard, full per-shard logs on
  disk, and a closing per-shard summary (status, attempts, selected/expected
  counts, failed tests, `FLAKY` tests, wall time). Any failing shard fails the
  run; the counts must sum to the binary's total.
- **Retry semantics:** the per-test retry and its `ok (FLAKY: …)` reporting are
  unchanged; the whole-suite rerun (`--retries`) becomes a rerun of the failed
  shard only, in a fresh session, and a shard that passes only on a rerun is
  reported as loudly as a `FLAKY` test.
- **Opt-out:** `WIDGET_PARALLEL=0` restores today's single serial session;
  `WIDGET_PARALLEL_JOBS=<n>` caps concurrency.
- **Unchanged:** `make test-widget-shard WIDGET_SHARD=<shard>`, the shard table
  and its budgets, `make check-widget-shards`, and every CI workflow.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `widget-test-ci-sharding`: the "Local widget commands still run the whole
  suite" requirement changes from "one session" to "the table's shards in
  parallel by default, one session with the opt-out"; new requirements cover
  per-shard isolation (memory scope, runtime directory, temp scope) and the
  merged report and failure semantics of the parallel local run.

## Impact

- `scripts/widget-shards.py` (new `parallel` command, summary and merge logic,
  self-tests), `scripts/run-widget-tests.sh` (a scope-label option so each
  shard's temp scope and runtime directory are distinct and short, and an
  end-of-run memory report that fails with `OOM-KILL:` when anything in its
  memory scope was OOM-killed; printed only under `with-memory-cap.sh`, so CI
  output is unchanged),
  `Makefile` (`test`, `test-widget`, `test-widget-headless`, new knobs
  `WIDGET_PARALLEL`, `WIDGET_PARALLEL_JOBS`, `WIDGET_SHARD_MEMORY_MAX`,
  `WIDGET_BUILD_MEMORY_MAX`, `WIDGET_BUILD_JOBS`).
- No Rust, test, or harness code changes; `crates/gtk-lush/proof-harness` and
  `crates/lushtext/tests/widget.rs` keep their per-run root and per-attempt
  `TMPDIR` isolation as is.
- Docs: `.agents/rules/build.md` (Dev Builds, Temporary Files, CI widget-shard
  paragraph), `AGENTS.md` (Build Commands, Testing), `README.md` (test
  commands), the `gtk-testing` skill reference that describes local widget
  runs.
- CI: none. `.github/workflows/ci.yml` keeps calling
  `scripts/widget-shards.py run <shard>`.
