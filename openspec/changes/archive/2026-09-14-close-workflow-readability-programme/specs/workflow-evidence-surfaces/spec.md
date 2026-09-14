## MODIFIED Requirements

### Requirement: A migrated workflow exposes one typed evidence surface
A migrated workflow SHALL expose one typed evidence surface that is the single
source of that workflow's observable state. The surface MUST expose every field
previously readable through the workflow's inspection seams. Tests MUST read the
surface instead of per-field inspection functions, and the retired inspection
functions MUST have no remaining callers.

Because one accessor reads the whole surface, and because a GTK workflow's state
lives behind interior mutability, reading the surface takes shared borrows of the
workflow's state cells. It follows that **no evidence field may be read from
inside a mutable borrow of the same state**: doing so panics at runtime rather
than failing to compile. This constraint is a property of the convention rather
than of any one workflow, so it SHALL be stated once here rather than
rediscovered as a per-workflow module note. A migrating workflow MUST record the
constraint where its surface is defined, MUST NOT introduce a second narrower
accessor to work around it, and MUST prove it with a test that drives the
workflow through each operation taking a mutable borrow of the state the accessor
reads, reads the evidence surface after each such operation, and asserts that
repeated reads of unchanged state are identical.

Reading the surface MUST also not cause the **toolkit** to do work. A GTK
collection may create its children on demand, so an accessor that walks such a
collection to answer a question performs work — it can materialize descendants,
register stores, start background scans, and cause dependent lifecycles such as
filesystem watches to restart — while every field it produced still reads as a
pure observation. An evidence surface therefore MUST NOT call an accessor that
lazily creates toolkit state, and MUST NOT call a derivation that mutates a cache
or advances a counter that the surface itself reports, because an observer that
changes the metric it observes is not an observation. Where the workflow's own
code reaches such an accessor safely only because of a guard, the surface MUST
derive the field from the workflow's authoritative state instead of repeating the
guarded walk. A migrating workflow MUST prove this rather than assert it, by
reading the surface in both the lazily-unmaterialized and materialized states and
showing that the workflow's admission counters, registries, generations, and
derivation metrics are identical before and after the read.

Where a field is aggregated over a **variable-sized collection of child
widgets**, that aggregation MUST be bounded, MUST answer honestly when the
collection is empty, and MUST skip a disposed child rather than panicking on it —
the same reasoning as the disposed-widget rule, applied to a set rather than to
one child.

**A cross-cutting coordination lane owes the surface even though it owes no
facade.** A lane that the matrix records as `cross-cutting` — shared coordination
consumed by several workflows, with no user-initiated operation of its own — is not
a workflow: it carries no narrative facade, no coordination role names, and no
`policy.rs`. Where such a lane nonetheless exposes observable state through
test-only inspection seams, it SHALL expose that state through **one** typed
surface, subject to the same visibility, reentrancy, non-materialization, and
bounded-child rules stated above, with the same three proofs. Two or more parallel
typed observation values over one lane's state are the duplication this requirement
already forbids, and consolidating them MUST NOT move or duplicate a limit the lane
shares with a workflow that calls it.

The surface's file MAY keep the lane's own name rather than being called
`evidence.rs`, because `evidence.rs` is a workflow **role** name and a lane carries
no roles; what is fixed is that exactly one surface exists.

Because every settled rule above is written to fire when a workflow migrates, and a
`cross-cutting` lane never migrates, that trigger never arrives. The obligation is
therefore discharged by the change that closes the migration programme, and MUST
NOT be deferred to a migration that will not occur. Discharging it does not change
the lane's status: it stays `cross-cutting`, and its census resolution against
relocation stands.

#### Scenario: Evidence replaces scattered getters
- **WHEN** a workflow is migrated
- **THEN** its inspection state is readable from one typed evidence value
- **AND** the feature-gated per-field inspection functions it replaces are removed

#### Scenario: No observable field is lost
- **WHEN** inspection seams are consolidated
- **THEN** every field the retired functions exposed remains readable from the
  evidence surface
- **AND** the project's test count does not decrease as a result of the
  consolidation

#### Scenario: Evidence surface does not mutate workflow state
- **WHEN** an evidence surface is read
- **THEN** reading it has no effect on workflow state, timers, queues, or generation
  counters
- **AND** it does not require the workflow to be in a particular stage

#### Scenario: Evidence surface does not materialize toolkit state
- **WHEN** a workflow's observable state includes a toolkit collection whose
  children are created on demand
- **THEN** the evidence surface derives its fields from the workflow's
  authoritative state rather than walking that collection
- **AND** reading the surface starts no background work, registers no store, and
  causes no dependent lifecycle such as a filesystem watch to restart

#### Scenario: Observation does not advance the metric it reports
- **WHEN** a derivation that the workflow uses elsewhere mutates a cache or
  advances a capture counter
- **THEN** the evidence surface does not call that derivation
- **AND** a field reporting such a counter reports it without changing it

#### Scenario: Non-materialization is proved rather than assumed
- **WHEN** a migrating workflow's evidence surface covers a lazily created
  collection
- **THEN** the change reads the surface with the collection unmaterialized and with
  it materialized, and shows the workflow's admission counters, registries,
  generations, and derivation metrics identical before and after each read
- **AND** the proof is recorded as evidence rather than asserted in review

#### Scenario: Field aggregated over child widgets is bounded and honest
- **WHEN** an evidence field aggregates state across a variable-sized set of child
  widgets
- **THEN** the aggregation is bounded and returns an honest answer when the set is
  empty
- **AND** a disposed child is skipped rather than panicked on

#### Scenario: Evidence surface is not read from inside a mutation
- **WHEN** a workflow holds a mutable borrow of state that its evidence accessor
  reads
- **THEN** the workflow does not read its evidence surface until that borrow ends
- **AND** it does not add a second narrower accessor to make the nested read
  possible

#### Scenario: Reentrancy is proved rather than assumed
- **WHEN** a workflow's evidence surface is introduced or gains fields
- **THEN** the change includes a test that drives the workflow through each
  operation taking a mutable borrow of the state the accessor reads, reads the
  evidence surface after each such operation, and asserts that repeated reads of
  unchanged state are identical
- **AND** the constraint is recorded where the surface is defined

#### Scenario: Evidence surface remains internal
- **WHEN** an evidence surface is defined
- **THEN** it is an internal type of the owning crate
- **AND** it is not added to the public D-Bus automation schema

#### Scenario: Evidence surfaces share one visibility rule
- **WHEN** a workflow's evidence surface is declared
- **THEN** its visibility is the narrowest that its readers require, and workflows
  MUST NOT differ in this choice without a stated reason
- **AND** an existing evidence type whose visibility is wider than its readers need
  is narrowed when its workflow migrates

#### Scenario: Existing typed observation is folded in rather than duplicated
- **WHEN** a workflow already exposes typed observation values that predate this
  convention
- **THEN** its migration folds them into the workflow's single evidence surface
- **AND** it does not leave a second typed observation path alongside the surface

#### Scenario: Cross-cutting lane consolidates onto one surface
- **WHEN** a lane the matrix records as `cross-cutting` exposes observable state
  through test-only inspection seams
- **THEN** that state is readable from exactly one typed surface, subject to the
  visibility, reentrancy, non-materialization, and bounded-child rules and their
  three proofs
- **AND** the lane gains no facade, no coordination role name, and no `policy.rs`

#### Scenario: Parallel observation types on one lane are consolidated
- **WHEN** a cross-cutting lane exposes two or more parallel typed observation
  values over the same state
- **THEN** they are consolidated into that lane's single surface
- **AND** a limit the lane shares with a workflow that calls it is neither moved nor
  duplicated by the consolidation

#### Scenario: A lane's surface obligation is discharged at programme close
- **WHEN** the migration programme's closing change runs and a `cross-cutting` lane
  still owes its surface consolidation or a visibility narrowing
- **THEN** that change discharges the obligation, because no migration event will
  ever fire for the lane
- **AND** the lane's status stays `cross-cutting` and its resolution against
  relocation is unchanged
