# formal-verification-kani Specification

## Purpose
Establish Kani as LushText's single formal verification tool and define where harnesses live, how the Kani lane runs locally and in sharded CI, how safety regressions are pinned with should_panic harnesses, and how proved properties state their input domain.

## Requirements
### Requirement: Kani is the single formal verification tool
The project SHALL use Kani as its only formal verification tool. Formal
properties SHALL be expressed as `#[kani::proof]` harnesses over the Rust code
that ships, or over pure decision cores that production code drives. They MUST
NOT be expressed as separate models in another specification language. The
programme record MUST record any exception as a maintainer decision.

#### Scenario: A new formal property is added
- **WHEN** a contributor adds a machine-checked property
- **THEN** it is a Kani harness in the crate that owns the checked code
- **AND** it checks production code or a production-driven pure core, not a re-implementation

### Requirement: Harnesses live beside the checked code behind cfg(kani)
Harnesses SHALL live in a `#[cfg(kani)]` module of the crate that owns the
checked code. That code SHALL be GTK-free and I/O-free. The workspace SHALL
declare `cfg(kani)` as an expected cfg, so that ordinary builds and Clippy
emit no `unexpected_cfgs` warning. Ordinary `cargo build`, `cargo test`, and
the stable-toolchain gates MUST NOT depend on Kani being installed.

#### Scenario: Ordinary gates without Kani
- **WHEN** `make check` and `make test` run on a machine without Kani
- **THEN** they pass and compile no harness code

### Requirement: Harness modules are verification code, not production code
A Kani harness module SHALL be named `kani_proofs.rs` and declared by its parent
module as `#[cfg(kani)] mod kani_proofs;`. Such a module is verification code.
The mutation scope SHALL exclude it: no ordinary build compiles it, so no test
can kill a mutant inside it. The workflow-boundaries gate SHALL recognise it as
a verification module, not as undeclared decision logic or as an undeclared
module in a role home, but only when its parent declares it under `cfg(kani)`.
Any `kani_proofs.rs` under `crates/`, in `ui/` or not, whose parent module
does not declare it under `cfg(kani)`, or that has no parent module file,
SHALL fail the gate.

#### Scenario: Harness code generates no mutants
- **WHEN** the configured mutation scope is listed
- **THEN** no mutant is listed in any `kani_proofs.rs` file

#### Scenario: A harness over a ui policy passes the boundaries gate
- **WHEN** a `ui/**/policy.rs` module declares `#[cfg(kani)] mod kani_proofs;` and the harness file sits in its child directory
- **THEN** `make check-workflow-boundaries` reports no undeclared-module or unclassified-decision-logic finding for that file

#### Scenario: An ungated harness module is caught
- **WHEN** a parent declares `mod kani_proofs;` without `#[cfg(kani)]`, or a `kani_proofs.rs` has no parent module file
- **THEN** `make check-workflow-boundaries` fails

### Requirement: The Kani lane has a local command and a bounded CI job
The project SHALL provide `make kani` to run every harness, and a scheduled or
manually dispatched CI workflow that runs the same harnesses. When the whole
lane would exceed the repository's 30-minute job budget, the workflow SHALL
split it into shards, one job each and each within the budget. The shards
SHALL come from a single table, which assigns every harness to exactly one
shard and is checked by the local policy gate. The table SHALL also give each
shard a gate: `pull-request` or `scheduled`; any other gate SHALL fail the
check. The same workflow SHALL run on pull requests and on pushes to `main`,
and those events SHALL run only the shards gated `pull-request`. Scheduled and
manually dispatched runs SHALL run every shard. The workflow SHALL build its
matrix from the table only after re-running the table check, so a table that
fails the check runs no shard. A shard MAY be gated `pull-request` only when
its measured runner wall time is at most 15 minutes. Each harness SHALL declare the unwind
bounds it needs. The lane SHALL report per-harness results.

#### Scenario: Local run
- **WHEN** a maintainer runs `make kani` with Kani installed
- **THEN** every harness is checked and the command fails if any harness fails

#### Scenario: CI run
- **WHEN** the Kani workflow runs on its schedule or by manual dispatch
- **THEN** it installs the pinned Kani version, runs every harness across its shards, and every job finishes within the job timeout

#### Scenario: Pull-request run
- **WHEN** a pull request is opened or updated, or a commit is pushed to `main`
- **THEN** the Kani workflow runs exactly the shards gated `pull-request` (initially `widgets-geometry`) and no other shard
- **AND** a failing harness in those shards fails the pull-request check

#### Scenario: A harness outside every shard is caught
- **WHEN** a new harness matches no shard, or more than one
- **THEN** the local policy gate fails before the workflow runs
- **AND** the workflow's shard-table job fails before any shard job starts

#### Scenario: A slow shard cannot join the pull-request gate
- **WHEN** a shard is gated `pull-request` but its recorded runner wall time exceeds 15 minutes
- **THEN** the local policy gate fails, and the workflow's shard-table job fails before any shard job starts

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

#### Scenario: A dispatched run reports its budget
- **WHEN** the Kani workflow runs a shard job, on any event
- **THEN** the job summary lists the shard's wall time, exit status, peak resident memory, and every harness's verdict and verification time
- **AND** the same figures are uploaded as a per-shard JSON artifact

#### Scenario: An unmeasured shard is caught
- **WHEN** a shard is added to the table without a recorded runner measurement
- **THEN** the local policy gate fails

#### Scenario: A shard near the runner's memory is caught
- **WHEN** a shard's recorded peak memory exceeds 12 GiB
- **THEN** the local policy gate fails until the shard is split or its recorded measurement is back within the margin

### Requirement: Safety regressions are pinned by should_panic harnesses
For each ordering or guard whose removal is known to break a safety property,
the project SHALL keep a `#[kani::should_panic]` harness showing that the
property fails without that ordering or guard. The canonical example is
renaming before the temp-file sync.

#### Scenario: Removing the temp sync is caught
- **WHEN** the durable-write protocol harness runs the sequence "write temp, rename" without the temp sync
- **THEN** Kani finds the torn-target counterexample and the should_panic harness passes

### Requirement: Proved properties state their input domain
A harness that proves a property only on a restricted domain SHALL state that
domain in the harness, and in the checked function's rustdoc or spec. An
example is whole-pixel `i32` inputs rather than arbitrary `f64`. A rustdoc
promise MUST NOT claim a broader domain than the one proved.

#### Scenario: Slice containment domain
- **WHEN** the slice containment property is documented
- **THEN** its rustdoc and spec state the whole-pixel domain on which Kani proves it
