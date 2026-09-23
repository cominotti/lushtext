## ADDED Requirements

### Requirement: Imperative shells are conformance-tested against their proven cores
Each imperative shell that drives a Kani-proven pure core SHALL have a
model-based conformance test in which that proven core, inside its Kani
environment model, is the oracle. No separate specification of the expected
behaviour SHALL be written. This covers at least:

- the durable-write shell over the real filesystem, against `WriteProtocol`,
  `MoveProtocol`, and `RenameProtocol`;
- the draft service over a real data directory, against `journal_core` inside
  the journal model;
- the GTK drafts coordination, against the same journal model.

The test SHALL drive the real shell and the model in lockstep with the same
operation script, including injected faults and crashes. A crash SHALL mean
stop, drop all in-memory state, and reload from disk. After every step, the
test SHALL compare an abstraction of the real shell's observable state with
the model's state. Every divergence SHALL be classified as one of three
kinds, and handled as follows:

- **shell defect:** committed first as a failing deterministic regression
  test, and then fixed in the shell;
- **environment defect:** the model omitted or misstated real behaviour. It
  is fixed in the model, and the affected Kani harnesses are re-run;
- **core defect:** the proven core is itself wrong. The programme record marks
  the affected property as invalidated from the commit that found it, until a
  harness that failed first is proved again after the fix.

Every classification SHALL be recorded in the programme record.

#### Scenario: The oracle is the proven code
- **WHEN** a conformance test decides what the shell should have done after a step
- **THEN** that expectation comes from calling the production core and the shared environment model that the Kani harnesses check
- **AND** no hand-written expected-state table or parallel state machine is consulted

#### Scenario: A shell divergence is fixed failing-first
- **WHEN** a conformance run finds a sequence after which the real disk or the shell's returned state differs from the model's
- **THEN** the shrunk sequence is committed as a deterministic regression test that fails before the fix and passes after it
- **AND** the programme record lists the finding with its classification

#### Scenario: A core defect invalidates a proof
- **WHEN** a divergence is traced to the proven core deciding unsafely
- **THEN** the programme record marks the affected property as invalidated from the finding's commit
- **AND** a Kani harness exhibits the counterexample before the core is fixed, and the property is recorded as proved again only after the harness passes

### Requirement: Verification environment models are shared between Kani and conformance tests and cover every outcome the real shell can produce
The environment model of each proven core (the journal model and the
write-protocol disk model) SHALL live in one module. That module SHALL be
compiled for Kani and for test and `test-utils` builds, and never for a
shipping build. It SHALL take every nondeterministic choice from one choice
source: `kani::any()` under Kani, and the generated script in a conformance
test. Moving a model into such a module SHALL leave every harness proving the
same properties at the same bounds, and this SHALL be confirmed by re-running
the lane. A model's fault domain SHALL include every outcome class the real
shell can produce at that step. In particular, the journal model SHALL
distinguish a fault before an effect persists from a fault reported after the
effect persisted, such as an after-rename durable-write failure. The model
modules SHALL be excluded from the mutation scope, like harness modules.

#### Scenario: One model, two drivers
- **WHEN** the Kani lane and the conformance tests run after this change
- **THEN** both execute the same environment-model source, differing only in their choice source
- **AND** the journal and write-protocol harnesses report the same verdicts at the same stated bounds as before the move

#### Scenario: An after-effect fault is in the journal model
- **WHEN** a journal action's durable write fails after its rename
- **THEN** the model represents that step as persisted but reported failed, and S1–S4 and L1 are checked over that outcome

#### Scenario: A shipping build compiles no model
- **WHEN** the release binary is built with default features
- **THEN** neither environment model module is compiled into it
