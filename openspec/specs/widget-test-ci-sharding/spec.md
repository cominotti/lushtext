# widget-test-ci-sharding Specification

## Purpose
Keep the CI widget lane inside the job time limit by running the widget suite as parallel shards drawn from one checked table, with every test in exactly one shard and every shard held to a measured runner budget.
## Requirements
### Requirement: Widget tests are sharded in CI from one checked table
The CI widget lane SHALL run the widget suite as parallel shard jobs whose
membership comes from a single table, `scripts/widget-shards.py`. The table
SHALL assign every widget test to exactly one shard, by module or by explicit
test name, where an explicit name takes precedence over its module's shard.
The local policy gate `make check-widget-shards`, included in `make check`,
SHALL discover every widget test from `crates/lushtext/tests/widget/*.rs` with
the same rule the widget registry uses, and SHALL fail when a test matches no
shard, when a module or explicit name is claimed by more than one shard, or
when a table entry names a module or test that does not exist. The workflow
SHALL build its shard matrix only from the output of the same check, so a
table that fails the check runs no shard.

#### Scenario: An unassigned test is caught
- **WHEN** a widget test is added in a module no shard owns
- **THEN** `make check-widget-shards` fails naming the test
- **AND** the workflow's shard-table job fails before any shard job starts

#### Scenario: A doubly assigned test is caught
- **WHEN** a module or explicit test name appears in two shards
- **THEN** `make check-widget-shards` fails naming it

#### Scenario: A stale table entry is caught
- **WHEN** a shard names a module or explicit test that no longer exists
- **THEN** `make check-widget-shards` fails naming the entry

### Requirement: Each shard job proves it ran exactly its share of the full suite
Each CI shard job SHALL compare the compiled widget binary's `--list` output
with the table's static discovery and fail on any difference. It SHALL run its
shard through `scripts/run-widget-tests.sh --headless --retries 1` with the
shard's exact test names, and SHALL fail when the harness reports a selected
test count different from the shard's expected count. The shard counts SHALL
sum to the full suite. The headless compositor, per-test retry, whole-run
retry, `FLAKY` reporting, and warning classification SHALL be those of the
unsharded runner.

#### Scenario: Binary and table disagree
- **WHEN** the compiled binary lists a test the static discovery does not, or the reverse
- **THEN** the shard job fails before running any test

#### Scenario: Shards cover the suite
- **WHEN** every shard job of one CI run completes
- **THEN** the per-shard selected counts sum to the binary's total test count

### Requirement: Shard budgets are measured on the CI runner and enforced with margin
Every shard SHALL record its measured wall time on the CI runner (the largest
across at least two runs), and the runs it came from; developer-machine timing
does not count. The recorded figure is the shard's widget step wall time,
including the test-binary build. `make check-widget-shards` and the shard-table
job SHALL fail when a shard has no recorded measurement or named run, or when
its recorded wall time exceeds 20 minutes, leaving at least 10 minutes of the
30-minute job cap for setup and runner variance. Each shard job SHALL publish
its wall time, exit status, selected count, and per-test durations as the job
summary and as a `widget-measure-<shard>` JSON artifact.

#### Scenario: A shard over the margin is caught
- **WHEN** a shard's recorded wall time exceeds 20 minutes
- **THEN** the local policy gate fails until the shard is split or re-measured within the margin

#### Scenario: A shard job reports its budget
- **WHEN** a CI shard job finishes
- **THEN** its summary and `widget-measure-<shard>` artifact carry its wall time, status, selected count, and per-test durations

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

### Requirement: Parallel local shards are isolated from each other
In a parallel local run, each shard SHALL run in its own admitted memory scope
(`scripts/with-memory-cap.sh` inside `lushtext-heavy.slice`, capped by
`WIDGET_SHARD_MEMORY_MAX`), its own temporary-file scope
(`scripts/with-temp-scope.sh --session-runtime`), its own D-Bus session, and
its own headless compositor, with a distinct `XDG_RUNTIME_DIR` and Wayland
socket. A shard's session SHALL refuse to start when its Wayland socket path
would exceed 100 bytes. Every shard's scope SHALL fail the run with
`TEMP-LEAK:` when its shard leaves a temporary entry behind, as the serial run
does. When memory admission cannot yet admit a shard, that shard SHALL wait and
the run SHALL say so, rather than start uncapped. When the kernel OOM-kills any
process inside a shard's memory scope, that shard SHALL fail with `OOM-KILL:`
even if a retry recovered the test.

#### Scenario: Shards do not share a session
- **WHEN** a parallel local run is in flight
- **THEN** each shard's compositor listens on a socket inside its own runtime directory, and no two shards share a runtime directory, bus, or memory scope

#### Scenario: Overlong socket path is refused
- **WHEN** the resolved temporary directory would make a shard's Wayland socket path longer than 100 bytes
- **THEN** each shard fails before its session starts, the message names the temporary directory as the cause, and the run fails

#### Scenario: A leak is attributed to its shard
- **WHEN** a test in one shard leaves a temporary entry in its scope
- **THEN** that shard's scope reports `TEMP-LEAK:`, the summary marks that shard failed, and the run fails

#### Scenario: An out-of-memory kill is never green
- **WHEN** a process in one shard's memory scope is OOM-killed and the harness's retry then passes the test
- **THEN** that shard reports `OOM-KILL:`, the summary marks it failed, and the run fails

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

