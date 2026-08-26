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
