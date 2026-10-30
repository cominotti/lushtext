## Context

- `scripts/run-widget-tests.sh` runs the widget binary as
  `with-temp-scope.sh --session-runtime widget -- dbus-run-session -- mutter
  --headless … -- cargo test -p lushtext --test widget -- <args>`, with
  `LUSHTEXT_WIDGET_HEADLESS_RUNNER=1` so the harness does not relaunch itself.
  It tees the raw output for its warning classification and retries the whole
  run with `--retries N`.
- `scripts/widget-shards.py` owns the shard table. `run <shard>|all` cross-checks
  static discovery against the binary's `--list`, runs each shard through
  `run-widget-tests.sh --headless --retries 1 -- --exact <names>` **one after
  another**, and asserts the harness's `running N tests` count. CI runs one
  shard per job; `make test-widget-shard` reproduces one job.
- The harness (`gtk-lush-proof-harness`, configured in
  `crates/lushtext/tests/widget.rs`) creates one `lushtext-test-<pid>-<random>`
  root per run inside the inherited `TMPDIR`, gives every child attempt an
  empty subdirectory as `TMPDIR`, and each process re-roots
  `LUSHTEXT_DATA_DIR` and the four XDG base directories inside its own
  `TMPDIR`. Nothing in a widget test reads a path outside its attempt
  directory, so two runs in different scopes cannot see each other's state.
- `scripts/with-temp-scope.sh` makes `lt-scope-<pid>-<label>-XXXXXX` under the
  caller's `TMPDIR`, puts the session runtime at `<scope>/.session-runtime`,
  waits for every process carrying it as `XDG_RUNTIME_DIR` to exit before
  removing it, and fails with `TEMP-LEAK:` on leftovers. The check is scoped,
  so parallel scopes cannot blame each other.
- `scripts/with-memory-cap.sh <cap> <cmd>` runs a command in a
  `MemoryMax=<cap>`, `MemorySwapMax=0`, `OOMPolicy=continue` user scope inside
  `lushtext-heavy.slice` (72G), after an admission that waits until the slice
  budget and `MemAvailable − 16G` reserve both fit the cap. Nested calls run in
  place under the outer cap.
- Constraints: CI must not change; `make test-widget-shard` must not change; a
  `FLAKY` line is a blocker per `preexisting-blockers.md` and must stay loud;
  the 16-CPU affinity the maintainer's sessions run under is inherited, never
  widened; Wayland socket paths are limited to 108 bytes (`sun_path`).

## Goals / Non-Goals

**Goals:**

- Local full widget runs take about as long as the slowest shard.
- Each shard is as isolated from the others as two CI jobs are: own memory
  scope, own temp scope, own D-Bus bus, own compositor, own runtime directory.
- One readable result: which shard failed, which tests failed or were `FLAKY`,
  and whether any shard needed a rerun.
- A serial escape hatch for debugging and for hosts without a user systemd
  manager or the memory for three sessions.

**Non-Goals:**

- Changing CI, the shard table, shard membership, or shard budgets. Local
  timing never feeds `ci_minutes`.
- Running tests in parallel **within** a shard. The harness stays one child at
  a time per session.
- Rebalancing shards for local wall time. If the local slowest shard dominates,
  that is a separate table change judged on CI budgets.
- Changing the harness, its retry policy, or test isolation code.

## Decisions

### D1. The parallel driver lives in `widget-shards.py`, as a `parallel` command

`widget-shards.py parallel [--retries N] [--jobs N]` reuses the table,
`assignment`, `listed_tests`, `WidgetLog`, and `count_problems` that `run`
already has, so serial CI runs and parallel local runs cannot disagree about
membership or counting. `run` is not modified (CI calls it).

*Alternative:* a new shell script fanning out `run-widget-tests.sh`. Rejected:
it would duplicate the `--list` cross-check and count assertion in shell, and
the table is the only place shard names should be read from.

### D2. Build once, then fan out

Before any session starts, the driver builds the widget binary once
(`cargo test -p lushtext --test widget --no-run`, then `--list`), inside a
`with-memory-cap.sh` scope sized for a build (`WIDGET_BUILD_MEMORY_MAX`,
default `16G`) with cargo's jobs sized to that cap as `build.md` requires.
Each shard then still invokes `cargo test` through `run-widget-tests.sh`
(identical to CI), which is a no-op build that only takes cargo's lock briefly.
Without the pre-build, three cold `cargo test` invocations would block on the
same build lock and the first shard's log would carry the whole compile.

### D3. One admitted memory scope per shard

Each shard runs as `with-memory-cap.sh $WIDGET_SHARD_MEMORY_MAX
run-widget-tests.sh --headless --scope-label <label> --retries <n> -- --exact
<names>`. The cap covers the shard's dbus-daemon, Mutter, the harness parent,
and its current test child. Admission is per shard, so on a busy host the
second or third shard waits for headroom (the wrapper prints its waiting line,
which the driver relays with the shard prefix) instead of three sessions
pushing the desktop into reclaim.

The default `WIDGET_SHARD_MEMORY_MAX` is set from measurement (task 3.2):
the largest per-shard peak of three local parallel runs, times 1.5, rounded
up to a whole GiB. Until measured, the proposal's working value is `6G`. A
shard OOM-killed under its cap is a failed run, reported as such (exit 137 in
the summary), never retried as flakiness.

If the driver itself already runs under a cap (`LUSHTEXT_MEMORY_CAP_KIB` set),
the wrapper's nesting rule runs every shard in place inside that one scope; the
driver prints that the shards share the outer cap rather than silently
changing the isolation it promised.

*Alternative:* one scope for the whole parallel run. Rejected: one shard's
leak would OOM the others, and admission could not stagger them.

### D4. Distinct, short runtime directories through distinct temp scopes

`run-widget-tests.sh` gains `--scope-label LABEL` (default `widget`, so
existing callers are unchanged) and passes it to `with-temp-scope.sh`. The
driver uses `w0`, `w1`, `w2` … (index in table order), not the shard name, so
the socket path stays short:
`$TMPDIR/lt-scope-<pid>-w0-XXXXXX/.session-runtime/wayland-0`. Because each
shard is its own temp scope, the runtime directories, D-Bus addresses, and
`wayland-0` sockets are distinct by construction, and each scope's TEMP-LEAK
check covers exactly one shard.

Before launching, the driver computes the longest socket path its scopes can
produce from the resolved base `TMPDIR` and fails with a clear message when it
exceeds 100 bytes (a margin under 108), naming `TMPDIR` as the cause. When the
driver runs inside another temp scope (for example from a wrapped
`make test`), the shard scopes nest under it; the length check covers that
case too.

### D5. Merged output: prefixed live lines, per-shard logs, one closing summary

- **Live:** each shard's sanitized stream (what `run-widget-tests.sh` already
  prints) is relayed line by line with a `[<shard>] ` prefix, so progress and
  failures are visible while the run is in flight.
- **Logs:** the full relayed stream of each shard is written to
  `target/widget-parallel/<shard>.log` (the latest run only; under `target/`,
  outside any temp scope, so it is neither a leak nor tracked).
- **Summary:** after every shard ends, the driver prints one table: shard,
  status, attempts, selected/expected, wall minutes, failed tests (by name, from
  `WidgetLog`), and `FLAKY` tests. Below it, it repeats every `FLAKY:` line and
  every shard rerun as a `FLAKY-SHARD:` line on stderr. It then checks that the
  selected counts sum to the binary's total.
- `--measure JSON` records the same fields as `run --measure` plus the shard's
  attempts, for local comparison only.

### D6. Failure and retry semantics

- The run fails when any shard fails, when any shard's selected count differs
  from its expected count, when the counts do not sum to the total, or when any
  scope reports `TEMP-LEAK:` or an unexpected warning (both already fail
  `run-widget-tests.sh`).
- A failing shard does not stop the others: every shard runs to completion so
  one run shows every failure. `Ctrl-C` or `SIGTERM` signals each shard's
  process group and waits for it, so each scope's traps release its runtime
  directory and the run leaves no `lt-scope-*` entry behind.
- Per-test retry (the harness's fresh-process retry and its
  `ok (FLAKY: passed on attempt N)` line) is unchanged.
- `--retries N` keeps its meaning per unit of isolation: a failed shard reruns
  in a brand-new scope and session, up to N times; the other shards' results
  stand. `make test` and `make test-widget-headless` pass `--retries 1` (as
  their serial forms do today); `make test-widget` passes none. A shard that
  passes on a rerun leaves the run green, as the serial `--retries 1` does
  today, but is reported as `FLAKY-SHARD` in the summary and on stderr.

### D7. Make targets and knobs

| Knob | Default | Effect |
|---|---|---|
| `WIDGET_PARALLEL` | `1` | `0` runs today's serial command (`run-widget-tests.sh --headless …`) |
| `WIDGET_PARALLEL_JOBS` | number of shards | at most this many shards at once (queue the rest) |
| `WIDGET_SHARD_MEMORY_MAX` | measured (D3) | per-shard scope cap |
| `WIDGET_BUILD_MEMORY_MAX` | `16G` | pre-build scope cap |

`make test-widget-shard` keeps calling `widget-shards.py run` and ignores
these knobs. The driver refuses to run when no user systemd manager exists and
`CI` is unset (the wrapper's own rule) and suggests `WIDGET_PARALLEL=0`.

## Risks / Trade-offs

- [Three Mutter sessions and three test children contend for CPU and disk, so
  timing-sensitive waits may flake where the serial run did not] → the shards
  already run concurrently on CI runners as separate jobs; any new `FLAKY` is a
  blocker to fix by the flake discipline (async waits with adequate budgets),
  measured by three consecutive parallel runs before sign-off (task 4.4). The
  btrfs stall report is queried for any slow run.
- [Local wall time is bounded by the slowest shard, which may be well over
  `total / 3`] → reported per shard in the summary; rebalancing is a separate,
  CI-budget-driven table change (non-goal).
- [Memory admission can serialize shards on a loaded host, making the run no
  faster than serial] → acceptable by design (the alternative is swapping);
  the waiting line makes it visible.
- [Interleaved prefixed output is harder to read than one stream] → the
  per-shard logs and the closing summary are the reading surface; the live
  stream is for progress.
- [A `cargo test` no-op build per shard still contends on cargo's lock] → it
  is a fingerprint check only after D2's pre-build.

## Migration Plan

Script and Makefile only. Rollback is `WIDGET_PARALLEL=0` for a session, or a
revert of the Makefile default. No CI or data migration.

## Open Questions

- The default `WIDGET_SHARD_MEMORY_MAX` awaits the measurement in task 3.2.
- Whether `make test` should keep the per-shard `--retries 1` locally or drop
  it to surface shard-level flakes as failures is the maintainer's call; this
  design keeps today's behaviour.
