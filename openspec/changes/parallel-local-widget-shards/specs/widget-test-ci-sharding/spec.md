## MODIFIED Requirements

### Requirement: Local widget commands still run the whole suite
`make test`, `make test-widget`, and `make test-widget-headless` SHALL run
every widget test. By default they SHALL run the shards of
`scripts/widget-shards.py` in parallel, each shard in its own private headless
session, after one shared build of the widget binary and the same `--list`
cross-check the CI shard jobs perform, and SHALL fail unless the shards'
selected counts sum to the binary's total test count. With `WIDGET_PARALLEL=0`
they SHALL run every widget test, unsharded, in one headless session, as before
parallel local runs. `make test-widget-shard WIDGET_SHARD=<name>` SHALL run one
shard exactly as CI does, unaffected by the parallel-run settings. The CI widget
jobs SHALL keep running one shard per job through
`scripts/widget-shards.py run <shard>`.

#### Scenario: Local full run
- **WHEN** a developer runs `make test-widget`
- **THEN** every shard of the table runs, concurrently, each in its own headless session
- **AND** the run fails unless every shard passes and the shards' selected counts sum to the binary's total

#### Scenario: Serial opt-out
- **WHEN** a developer runs `make test-widget WIDGET_PARALLEL=0`
- **THEN** all widget tests run in one headless session, as before parallel local runs

#### Scenario: One shard and CI are unchanged
- **WHEN** a developer runs `make test-widget-shard WIDGET_SHARD=window`, or a CI shard job runs
- **THEN** that one shard runs exactly as the `Widget Tests (window)` job runs it, whatever `WIDGET_PARALLEL` is set to

## ADDED Requirements

### Requirement: Parallel local shards are isolated from each other
In a parallel local run, each shard SHALL run in its own admitted memory scope
(`scripts/with-memory-cap.sh` inside `lushtext-heavy.slice`, capped by
`WIDGET_SHARD_MEMORY_MAX`), its own temporary-file scope
(`scripts/with-temp-scope.sh --session-runtime`), its own D-Bus session, and
its own headless compositor, with a distinct `XDG_RUNTIME_DIR` and Wayland
socket. The driver SHALL refuse to start when a shard's Wayland socket path
would exceed 100 bytes. Every shard's scope SHALL fail the run with
`TEMP-LEAK:` when its shard leaves a temporary entry behind, as the serial run
does. When memory admission cannot yet admit a shard, that shard SHALL wait and
the run SHALL say so, rather than start uncapped.

#### Scenario: Shards do not share a session
- **WHEN** a parallel local run is in flight
- **THEN** each shard's compositor listens on a socket inside its own runtime directory, and no two shards share a runtime directory, bus, or memory scope

#### Scenario: Overlong socket path is refused
- **WHEN** the resolved temporary directory would make a shard's Wayland socket path longer than 100 bytes
- **THEN** the run fails before any session starts and names the temporary directory as the cause

#### Scenario: A leak is attributed to its shard
- **WHEN** a test in one shard leaves a temporary entry in its scope
- **THEN** that shard's scope reports `TEMP-LEAK:`, the summary marks that shard failed, and the run fails

#### Scenario: Interrupted run leaves nothing behind
- **WHEN** a developer interrupts a parallel run with Ctrl-C
- **THEN** every shard's session is stopped and its runtime directory and scope are removed

### Requirement: Parallel local runs report per shard and fail as a whole
A parallel local run SHALL relay each shard's output live with the shard's name
as a prefix, keep each shard's full output in a per-shard log, and end with one
summary giving each shard's status, attempts, selected and expected counts,
wall time, failed tests, and `FLAKY` tests. The run SHALL fail when any shard
fails, and SHALL let every other shard finish so one run reports every failure.
The per-test retry and its `ok (FLAKY: passed on attempt N)` reporting SHALL be
those of the serial runner. A whole-run retry (`--retries N`) SHALL rerun only a
failed shard, in a new session, and a shard that passes only on a rerun SHALL be
reported in the summary and on stderr as `FLAKY-SHARD`.

#### Scenario: A failure in one shard fails the run
- **WHEN** one test in one shard fails on every attempt
- **THEN** the other shards still run to completion, the summary names the failed shard and test, and the run exits non-zero

#### Scenario: Flaky results stay loud
- **WHEN** a test passes only on its harness retry, or a shard passes only on its rerun
- **THEN** the summary lists the test as `FLAKY` or the shard as `FLAKY-SHARD`, and the matching line also appears on stderr
