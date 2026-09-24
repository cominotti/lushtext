## ADDED Requirements

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
every widget test, unsharded. `make test-widget-shard WIDGET_SHARD=<name>`
SHALL run one shard exactly as CI does.

#### Scenario: Local full run
- **WHEN** a developer runs `make test-widget`
- **THEN** all widget tests run in one headless session, as before this change
