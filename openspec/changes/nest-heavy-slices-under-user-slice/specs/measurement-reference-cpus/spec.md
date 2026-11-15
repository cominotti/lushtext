## ADDED Requirements

### Requirement: Measurements run on one configured reference CPU set
`scripts/measure.sh` SHALL define one reference CPU set, `8-23` by default, overridden by `LUSHTEXT_REFERENCE_CPUS` and printed by `--print-reference-cpus`. Before taking any lock it SHALL set its own affinity to the reference and read it back, and the command SHALL inherit it. The allowed set SHALL be the nearest `cpuset.cpus.effective` of its cgroup, read from `/sys/fs/cgroup` (the online CPUs when none exists), never the caller's mask.

#### Scenario: The default reference is applied whatever the caller's mask
- **WHEN** a caller with an affinity other than `8-23` runs `scripts/measure.sh -- <command>` on a host whose cgroup allows CPUs 8-23
- **THEN** the command runs with affinity exactly 8-23

#### Scenario: An override is applied
- **WHEN** `LUSHTEXT_REFERENCE_CPUS` names a set within the allowed set
- **THEN** the command runs with exactly that affinity

#### Scenario: A malformed reference is refused
- **WHEN** `LUSHTEXT_REFERENCE_CPUS` is not a CPU list
- **THEN** `measure.sh` exits 2 without taking a lock or running the command

### Requirement: A measurement never runs on a different CPU set silently
When the reference is not a subset of the allowed set, or the affinity read back differs from it, `measure.sh` SHALL exit 78 before taking any lock, naming both sets, and SHALL NOT measure on their intersection. When its affinity changed during the command, the after record SHALL say so beside the command's status, and `measure.sh` SHALL exit 78 after releasing the locks.

#### Scenario: The reference is narrowed by a game profile
- **WHEN** the toolbox cpuset is 12-23 and the reference is the default 8-23
- **THEN** `measure.sh` exits 78, names both sets, and the command does not run

#### Scenario: A partial overlap is refused, not intersected
- **WHEN** the reference contains one CPU the cgroup allows and one online CPU it does not
- **THEN** `measure.sh` exits 78 and the command does not run

#### Scenario: A refused measurement holds no lock
- **WHEN** `measure.sh` refuses a reference
- **THEN** INTENT and LOCK were never taken, so other agents' heavy work did not drain for it

#### Scenario: The CPU set changes during the run
- **WHEN** the affinity of `measure.sh` changes while its command runs
- **THEN** the after record carries `cpus_changed=` with the new set and the command's `status=`, and `measure.sh` exits 78

#### Scenario: Refusal inside heavy work still comes first
- **WHEN** `measure.sh` runs inside `scripts/heavy.sh` on a host without the reference CPUs
- **THEN** it exits 2 for the inside-heavy-work refusal, as before

### Requirement: Every measurement records the CPU set it ran on
The before and after records of `measure.sh` SHALL carry `cpus=<affinity read back>`, `reference=<reference>` and `source=<default|override>`. The command SHALL receive `LUSHTEXT_MEASURE_CPUS` (the reference, as a canonical CPU list) and `LUSHTEXT_MEASURE_CPUS_SOURCE` (`default` or `override`).

#### Scenario: A default run is recorded as the reference
- **WHEN** a measurement runs with no override
- **THEN** its records carry `cpus=8-23 reference=8-23 source=default`, and the command sees `LUSHTEXT_MEASURE_CPUS=8-23` and `LUSHTEXT_MEASURE_CPUS_SOURCE=default`

#### Scenario: An override is labelled
- **WHEN** a measurement runs with `LUSHTEXT_REFERENCE_CPUS=12-23`
- **THEN** its records carry `source=override`, and the command sees `LUSHTEXT_MEASURE_CPUS_SOURCE=override`

#### Scenario: The self-test is host-independent
- **WHEN** `measure.sh --self-test` runs on a host whose cgroup does not allow CPUs 8-23, such as a CI runner
- **THEN** it sets the reference from the CPUs it is allowed, computes any unavailable CPU it needs at run time, skips a case that needs one when none exists, and passes

#### Scenario: Tests that call measure.sh pass during a game
- **WHEN** `heavy.sh --self-test`, which runs real measurements, runs while the toolbox cpuset excludes part of the default reference (a game profile, or a host without CPUs 8-23)
- **THEN** its measurements run on its own affinity as an override and the self-test passes, so the pre-commit hook does not fail for the host's CPU state

### Requirement: The CPU set is re-checked across waits and flips
`measure.sh` SHALL read its affinity again once both locks are held and before the `before` record, and SHALL exit 78 without running the command when the set changed during the lock wait. A failing CPU comparison SHALL be read once more before refusing, so a single host cpuset flip between two reads does not refuse a valid run.

#### Scenario: The set narrows during the lock wait
- **WHEN** the affinity of `measure.sh` changes while it waits for LOCK
- **THEN** it releases both locks, exits 78 and does not run the command

#### Scenario: A transient disagreement is re-read
- **WHEN** the first comparison of the affinity with the allowed set disagrees and the second agrees
- **THEN** the measurement runs
