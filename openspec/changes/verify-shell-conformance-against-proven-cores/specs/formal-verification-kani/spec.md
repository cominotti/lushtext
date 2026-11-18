## ADDED Requirements

### Requirement: Imperative shells are conformance-tested against their proven cores
The durable-write shells and the draft service SHALL each have a model-based
conformance test whose oracle is the Kani-proven core inside its shared
environment model, with no separately written expected behaviour. Each test
SHALL drive the real shell and the model in lockstep with one operation
script, including injected faults and crashes, and SHALL compare an
abstraction of the real observable state with the model's state after every
step.

#### Scenario: The oracle is the proven code
- **WHEN** a conformance test decides what the shell should have done after a step
- **THEN** that expectation comes from the production core and the shared environment model that the Kani harnesses check
- **AND** no hand-written expected-state table or parallel state machine is consulted

#### Scenario: The durable-write and draft-service shells are covered
- **WHEN** the conformance tests run
- **THEN** the atomic write, copy, move, and rename shells are checked against `WriteProtocol`, `MoveProtocol`, `RenameProtocol`, and the write disk model
- **AND** the draft service is checked against `journal_core` inside the journal model

### Requirement: Conformance divergences are classified and recorded
Every divergence a conformance test finds SHALL be recorded in the programme
record with one of three classes and handled by it:

- **shell**: a failing deterministic regression test first, then the fix;
- **environment**: the model is corrected and the affected harnesses re-run;
- **core**: the property is marked invalidated from the finding's commit, and
  re-proved only after a harness that failed first passes.

#### Scenario: A shell divergence is fixed failing-first
- **WHEN** a conformance run finds a sequence after which the real disk or the shell's returned state differs from the model's
- **THEN** the shrunk sequence is committed as a deterministic regression test that fails before the fix and passes after it
- **AND** the programme record lists the finding with its classification

#### Scenario: A core defect invalidates a proof
- **WHEN** a divergence is traced to the proven core deciding unsafely
- **THEN** the programme record marks the affected property as invalidated from the finding's commit
- **AND** a Kani harness exhibits the counterexample before the core is fixed, and the property is recorded as proved again only after that harness passes

### Requirement: Verification environment models are shared between Kani and conformance tests
The journal model and the write disk model SHALL each live in one module,
compiled for Kani and for test and `test-utils` builds and never for a
shipping build. Each SHALL take its nondeterministic choices from its caller,
from `kani::any()` under Kani and from the script in a conformance test. The
move SHALL leave each harness proving the same properties at the same bounds.
The modules SHALL be excluded from the mutation scope.

#### Scenario: One model, two drivers
- **WHEN** the Kani lane and the conformance tests run after the move
- **THEN** both execute the same environment-model source, differing only in where its choices come from
- **AND** every journal and write-protocol harness whose property is unchanged reports the same verdict at the same bound as before the move

#### Scenario: A shipping build compiles no model
- **WHEN** the release binary is built with default features
- **THEN** neither environment model module is compiled into it

### Requirement: The journal model distinguishes a fault before an effect from one after it
The journal model's fault SHALL be `None`, `BeforeEffect`, or `AfterEffect`.
`AfterEffect` SHALL leave a rename step's effect live but unconfirmed while
the window treats the step as failed. An unsynced body removal SHALL also be
unconfirmed. A crash SHALL be free to keep or revert any unconfirmed effect.
A later successful durable write into the same directory SHALL confirm it.
S1–S4 SHALL be checked over this domain, and L1 SHALL be re-checked with its
fault-admitting prefix widened to it.

#### Scenario: An after-rename failure is in the proved domain
- **WHEN** a registration, body write, manifest commit, or entry removal fails after its rename landed
- **THEN** the model represents that step as live, unconfirmed, and reported failed, S1–S4 are checked over that outcome, and L1 holds after any prefix that contains it

#### Scenario: A crash may revert an unconfirmed effect
- **WHEN** a crash follows an after-effect fault, or an unsynced body removal, before any later durable write into the same directory
- **THEN** the model explores both the kept and the reverted effect

#### Scenario: A restriction is stated, never silent
- **WHEN** a harness cannot carry the after-effect fault within its shard budget after the shard-fitting order
- **THEN** the restriction is stated in that harness, in this specification's delta, and in the programme record as a named residual
