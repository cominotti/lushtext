## 1. Failing-first driver tests (no GTK)

- [x] 1.1 Add a `parallel` self-test to `scripts/widget-shards.py --self-test` that drives the parallel scheduler with a fake runner (a short shell stub per shard) and asserts, before the scheduler exists: one shard exiting non-zero makes the run exit non-zero while the other shards still complete; the summary names the failed shard and test; a shard whose stream carries the runner's `Widget test run failed on attempt` line and then passes exits zero and is reported `FLAKY-SHARD`; a `widget-memory:` peak is parsed into the summary; a SIGTERM to the driver reaches every running shard and starts no queued one; a `FLAKY` result line is repeated in the summary; selected counts that do not sum to the total fail the run. Confirm it fails against the current script.
- [x] 1.2 Add a self-test that the socket-path guard refuses a base `TMPDIR` whose `lt-scope-<pid>-wN-XXXXXX/.session-runtime/wayland-0` path exceeds 100 bytes, and accepts `/tmp`. (After /simplify the guard and its self-test live in `with-temp-scope.sh --session-runtime`.)
- [x] 1.3 Add a `run-widget-tests.sh --self-test` case that `--scope-label` reaches the temp scope's directory name and that the default label stays `widget`, and a case that the memory report prints `OOM-KILL:` and fails for a cgroup whose `memory.events` records an `oom_kill` (read from a fixture directory, not by provoking the OOM killer). (After /simplify only the memory-report half remains: the label half restated an assignment, and the label is exercised end to end by every parallel run's `lt-scope-<pid>-wN-*` scopes.)

## 2. Implementation

- [x] 2.1 Add `--scope-label LABEL` to `scripts/run-widget-tests.sh` (default `widget`), passed to `with-temp-scope.sh`; existing callers unchanged. Add the end-of-run memory report (D3): only under `LUSHTEXT_MEMORY_CAP_KIB` with cgroup v2 files present, print `widget-memory: peak … oom_kill …` and fail with `OOM-KILL:` on a non-zero `oom_kill`.
- [x] 2.2 Add `widget-shards.py parallel [--retries N] [--jobs N] [--measure JSON]`: shared pre-build under `with-memory-cap.sh $WIDGET_BUILD_MEMORY_MAX` with `-j $WIDGET_BUILD_JOBS` (stop with the `WIDGET_PARALLEL=0` hint if no capped scope can be made), the existing `--list` cross-check, then one `with-memory-cap.sh $WIDGET_SHARD_MEMORY_MAX run-widget-tests.sh --headless --scope-label wN …` per shard, at most `--jobs` at once.
- [x] 2.3 Relay each shard's stream live with a `[<shard>] ` prefix, write `target/widget-parallel/<shard>.log`, feed each shard's `WidgetLog`, and print the closing summary table, the repeated `FLAKY` lines, and `FLAKY-SHARD` lines on stderr.
- [x] 2.4 Pass `--retries N` through to the runner (whose attempts each get a fresh scope and session) and detect reruns from its `failed on attempt` line; implement the count assertions (per shard and summed) and the nested-cap notice when `LUSHTEXT_MEMORY_CAP_KIB` is already set.
- [x] 2.5 Start each shard in its own session; handle `SIGINT`/`SIGTERM`: start no queued shard, signal every running shard's process group, wait for each, and exit 130/143 only after every scope's trap has run.
- [x] 2.6 Wire the Makefile: `test-widget`, `test-widget-headless`, and the widget half of `test` call `widget-shards.py parallel` unless `WIDGET_PARALLEL=0`, which keeps today's `CARGO_TEST_WIDGET` / `CARGO_TEST_WIDGET_HEADLESS` commands; add `WIDGET_PARALLEL`, `WIDGET_PARALLEL_JOBS`, `WIDGET_SHARD_MEMORY_MAX`, `WIDGET_BUILD_MEMORY_MAX`, `WIDGET_BUILD_JOBS` with comments; leave `test-widget-shard` untouched.
- [x] 2.7 Run `./scripts/widget-shards.py check --self-test`, `./scripts/run-widget-tests.sh --self-test`, `make check-temp-scope`, and `make check-widget-shards`; all green, and the 1.x self-tests now pass.

## 3. Measurement

- [x] 3.1 Run `make test-widget WIDGET_PARALLEL=0` once and `make test-widget` once on the reference workstation, both with nothing else heavy admitted; record wall times, per-shard wall times, and host load. Query `btrfs-stall-report` over any slow stretch.
- [x] 3.2 Record each shard's peak memory (the runner's `widget-memory:` line, i.e. the scope's `memory.peak`) over three parallel runs and set the `WIDGET_SHARD_MEMORY_MAX` default to the largest peak times 1.5, rounded up to a whole GiB; record the figures in the Makefile comment and `build.md`.

## 4. Failing-first end-to-end verification

- [x] 4.1 On a scratch change (never pushed; an uncommitted edit, because the pre-commit hook requires accessibility and visual smoke artifacts for any widget-test commit and `--no-verify` is not allowed), add one widget test that always fails to a module of the `editor-page` shard; run `make test-widget`; confirm the `window` and `surfaces` shards complete and pass, the summary names the failing shard and test, the exit status is non-zero, and no `TEMP-LEAK:` line appears; drop the scratch commit.
- [x] 4.2 On a scratch change (as in 4.1), make one test leave a file in its `TMPDIR`; confirm the run fails with `TEMP-LEAK:` attributed to that shard's scope; drop the scratch commit.
- [x] 4.3 Interrupt a parallel run with Ctrl-C mid-shard; confirm no `lt-scope-*`, `lushtext-test-*`, or session runtime directory remains under the base `TMPDIR` and no `mutter`/`dbus-daemon` of the run survives (`pgrep -x`). The first attempt left two scopes behind; fixed in `with-temp-scope.sh` (design D6, verification finding) with a failing-first self-test.
- [x] 4.4 Run `make test-widget` three times in a row green with no `FLAKY` or `FLAKY-SHARD` line; any flake found is a blocker fixed by the flake discipline in this change.
- [x] 4.5 Confirm `make test-widget-shard WIDGET_SHARD=surfaces` output and exit status match the pre-change behaviour, and that `.github/workflows/ci.yml` is unchanged in the diff.

## 5. Documentation

- [x] 5.1 Update `.agents/rules/build.md` (Dev Builds list, the CI widget-shard paragraph's "`make test` and `make test-widget` still run the whole suite in one session" sentence, Temporary Files "every test lane" list, and the heavy-command wrapping note: the parallel run is started unwrapped and admits its own scopes).
- [x] 5.2 Update `AGENTS.md` (Build Commands entries for `make test-widget`/`test-widget-headless`, the Testing section's widget paragraph) and `README.md` test commands.
- [x] 5.3 Update `.agents/skills/gtk-testing/SKILL.md` and `.agents/skills/gtk-testing/references/widget-testing.md` where they describe running the widget suite locally, and the `widget-shards.py` module docstring.
- [x] 5.4 Run `make check-agent-docs` and `openspec validate parallel-local-widget-shards --strict`.
