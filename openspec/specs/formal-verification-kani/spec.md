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

### Requirement: The Kani lane has a local command and a bounded CI job
The project SHALL provide `make kani` to run every harness, and a scheduled or
manually dispatched CI workflow that runs the same harnesses. When the whole
lane would exceed the repository's 30-minute job budget, the workflow SHALL
split it into shards, one job each and each within the budget. The shards
SHALL come from a single table, which assigns every harness to exactly one
shard and is checked by the local policy gate. Each harness SHALL declare the unwind
bounds it needs. The lane SHALL report per-harness results.

#### Scenario: Local run
- **WHEN** a maintainer runs `make kani` with Kani installed
- **THEN** every harness is checked and the command fails if any harness fails

#### Scenario: CI run
- **WHEN** the Kani workflow runs
- **THEN** it installs the pinned Kani version, runs every harness across its shards, and every job finishes within the job timeout

#### Scenario: A harness outside every shard is caught
- **WHEN** a new harness matches no shard, or more than one
- **THEN** the local policy gate fails before the workflow runs

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
