## ADDED Requirements

### Requirement: Test-only seams are classified before they are consolidated

The project SHALL classify every feature-gated test seam in production source as
inspection, configuration, actuation, or lifecycle probe before consolidating it.
The classification MUST be recorded per workflow in
`docs/workflow-readability-matrix.md`, and each kind MUST have a stated disposition.

The unit of classification SHALL be the test-gated declaration that a test calls or
sets: the test-only function definition, and the test-only static that overrides a
policy value. The count of test-gate attribute sites MUST NOT be used as the
classification unit, because one gated `impl` or `mod` block can cover many
functions and many sites gate struct fields, imports, or in-body hook calls rather
than seams. Any figure reported for seam counts MUST state which unit it uses.

#### Scenario: Seam counts state their unit

- **WHEN** a change reports how many test seams a workflow has
- **THEN** it states whether the figure counts gated declarations or gate attribute
  sites
- **AND** figures using different units are not compared as if they were the same
  measurement

#### Scenario: Inspection seam is identified

- **WHEN** a feature-gated function exists so a test can read internal workflow
  state such as counters, pending flags, queue depth, bounds, or freshness
- **THEN** it is classified as inspection
- **AND** its disposition is consolidation into the workflow's evidence surface

#### Scenario: Configuration seam is identified

- **WHEN** a feature-gated static or setter exists so a test can shorten a delay,
  lower a byte limit, or otherwise alter a policy value
- **THEN** it is classified as configuration
- **AND** its disposition is consolidation into one per-workflow test policy value

#### Scenario: Actuation seam is identified and deferred

- **WHEN** a feature-gated function exists so a test can drive a workflow step that
  is otherwise reachable only through a file chooser, alert dialog, timer, or worker
  completion
- **THEN** it is classified as actuation
- **AND** it is recorded as a missing workflow/presentation boundary and deferred to
  a later change rather than consolidated here

#### Scenario: Lifecycle probe is retained

- **WHEN** a feature-gated hook observes thread identity, disposal completion, or
  another lifecycle fact that has no non-test equivalent
- **THEN** it is classified as a lifecycle probe
- **AND** it is retained

### Requirement: A migrated workflow exposes one typed evidence surface

A migrated workflow SHALL expose one typed evidence surface that is the single
source of that workflow's observable state. The surface MUST expose every field
previously readable through the workflow's inspection seams. Tests MUST read the
surface instead of per-field inspection functions, and the retired inspection
functions MUST have no remaining callers.

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

### Requirement: Automation snapshots project from workflow evidence

Automation snapshot construction SHALL read migrated workflows through their
evidence surfaces rather than gathering the same widget state independently. The
externally visible D-Bus automation contract MUST remain unchanged by this
projection, and documentation drift checking MUST cover the projection.

#### Scenario: Snapshot reads evidence instead of re-deriving state

- **WHEN** an automation snapshot reports state for a migrated workflow
- **THEN** it projects that state from the workflow's evidence surface
- **AND** it does not separately reimplement the same derivation from widgets

#### Scenario: External contract is unchanged

- **WHEN** the projection is introduced for a migrated workflow
- **THEN** the exported D-Bus snapshot fields, names, and semantics are unchanged
- **AND** existing automation clients and scenarios continue to pass

#### Scenario: Projection drift is detected

- **WHEN** a workflow's evidence surface gains, removes, or renames a field that an
  automation snapshot projects
- **THEN** `make check-automation-docs` fails until the documentation is updated
- **AND** the failure names the evidence field and the snapshot field

### Requirement: Configuration seams collapse into one per-workflow test policy

A migrated workflow SHALL express its test-only timing and limit overrides through
one per-workflow test policy value rather than several independent module-level
statics. Production code paths MUST NOT be able to select test overrides.

#### Scenario: Timing and limit overrides share one owner

- **WHEN** a workflow needs test-only delay and limit overrides
- **THEN** they are fields of one per-workflow test policy value
- **AND** the workflow's production reading path is unchanged when no override is
  set

#### Scenario: Production cannot select an override

- **WHEN** the crate is built without the test feature
- **THEN** no override storage or override selection code is compiled
- **AND** the workflow reads its ordinary policy values

#### Scenario: Domain logic is not preceded by override scaffolding

- **WHEN** a reader opens a migrated workflow module
- **THEN** test override declarations do not occupy the module's opening section
  ahead of its workflow logic
- **AND** the overrides live with the workflow's test policy value
