## MODIFIED Requirements

### Requirement: Kani shard budgets are measured on the CI runner and enforced with margin
Every shard in the Kani shard table SHALL record its wall time and peak
resident memory, measured on the CI runner the workflow uses. When a shard has
been measured by more than one run, the record SHALL keep the largest wall time
and the largest peak memory seen across those runs. The record SHALL also name
the runs it came from; timing from a developer machine does not count as a
measurement. The recorded wall time is the shard's `cargo kani` wall time
(crate build plus verification), not the whole job; the margin to the
30-minute job cap covers the job's setup around it. The lane's measurement mode
SHALL report, per shard, its wall time, its exit status, its peak resident
memory (the largest process in that shard's own process tree, not a figure
accumulated across earlier shards), and each harness's verdict and
verification time. It SHALL publish these figures as the job summary and as a
machine-readable artifact. Every CI shard job SHALL run in measurement mode.
The local policy gate, and the workflow's shard-table job, SHALL fail when a
shard has no recorded measurement or no named run, when its recorded wall time
exceeds 25 minutes, or when its recorded peak memory exceeds 12 GiB. Running a
shard SHALL NOT be refused for breaking a margin, so an over-margin shard can
still be re-measured. A shard that breaks a margin SHALL be split before any proof
bound is reduced. A bound that is reduced anyway SHALL be recorded in the
programme record, together with the property statement it weakens.

A change that adds or re-models a shard MAY carry a local figure in the table
while it is being developed, because the table must be complete before the
branch can be pushed and dispatched. That record SHALL say in its
`measured_in` text that it is pending dispatch and is a local figure, not a
runner measurement, and SHALL name the open change that owes its dispatch.
The local policy gate SHALL fail when such a record names no owning change,
or names one that is no longer an active change (archived or absent), so a
change cannot be archived while it still owes a figure. Before the change is archived, the local figure SHALL be
replaced by the larger figures of two `workflow_dispatch` runs of the Kani
workflow on the change's final form, naming both run ids. A dispatch re-measures
every shard: a shard whose dispatched figure exceeds its recorded one SHALL be
raised to it, naming the runs, and a recorded figure SHALL NOT be lowered by a
later run. The programme record SHALL list every shard whose table entry is
still pending, with the change that owes its dispatch.

#### Scenario: A dispatched run reports its budget
- **WHEN** the Kani workflow runs a shard job, on any event
- **THEN** the job summary lists the shard's wall time, exit status, peak resident memory, and every harness's verdict and verification time
- **AND** the same figures are uploaded as a per-shard JSON artifact

#### Scenario: An unmeasured shard is caught
- **WHEN** a shard is added to the table with no recorded wall time, peak memory or `measured_in` text (neither a runner measurement nor a local figure marked pending dispatch)
- **THEN** the local policy gate fails

#### Scenario: A shard near the runner's memory is caught
- **WHEN** a shard's recorded peak memory exceeds 12 GiB
- **THEN** the local policy gate fails until the shard is split or its recorded measurement is back within the margin

#### Scenario: A local figure is replaced before the change is archived
- **WHEN** a change adds a shard and records its budget from a local `make kani KANI_MEASURE` run
- **THEN** the record's `measured_in` says it is pending dispatch
- **AND** before the change is archived, two dispatched Kani workflow runs replace it with the larger of their figures and both run ids

#### Scenario: A pending figure outlives its change
- **WHEN** a shard's `measured_in` marks a local figure pending dispatch and names no owning change, or names a change that has been archived
- **THEN** the local policy gate fails
- **AND** the same record owed by a change that is still active passes

#### Scenario: A dispatch raises an older shard's figure
- **WHEN** a dispatched run measures an already-measured shard above its recorded wall time or peak memory
- **THEN** the record is raised to the larger value and names that run, and it is never lowered by a smaller later run
