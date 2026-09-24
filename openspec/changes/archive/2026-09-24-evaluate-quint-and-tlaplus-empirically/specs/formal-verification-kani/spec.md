## MODIFIED Requirements

### Requirement: Kani is the single formal verification tool
The project SHALL use Kani as its only **maintained** formal verification
tool. Maintained formal properties SHALL be expressed as `#[kani::proof]`
harnesses over the Rust code that ships, or over pure decision cores that
production code drives. They MUST NOT be expressed as separate models in
another specification language. The programme record MUST record any
exception as a maintainer decision.

A **disposable evaluation model** in another language or tool is allowed only
when all of the following hold:
- a recorded maintainer decision authorises it for a named evaluation;
- it lives in a non-production location that no build, test, lint, policy,
  audit, or CI gate reads;
- no production crate depends on it;
- no claim in rustdoc, specs, or the programme record rests on it alone.

An evaluation model is evidence for a tool decision, not a maintained
property.

#### Scenario: A new formal property is added
- **WHEN** a contributor adds a machine-checked property that the project maintains
- **THEN** it is a Kani harness in the crate that owns the checked code
- **AND** it checks production code or a production-driven pure core, not a re-implementation

#### Scenario: An evaluation model stays outside every gate
- **WHEN** an authorised evaluation adds Quint, TLA+, or other non-Kani models
- **THEN** they live under the evaluation's non-production location, for example `formal/evaluation/`
- **AND** `make check`, `make test`, `make kani`, the Cargo workspace, `cargo deny`, the Sonar scope, and every CI workflow pass unchanged without the evaluation's tools installed

## ADDED Requirements

### Requirement: External modelling tools are adopted only on recorded empirical evidence
The project SHALL adopt an external modelling or model-checking tool only
after a written comparison on this project's own targets, and only through an
explicit decision section in that comparison. This covers Quint, TLA+ and its
checkers, `stateright`, and any other tool in addition to or instead of Kani,
whether it is adopted as a maintained lane, a design-sketch tool, or a test
driver.

The comparison SHALL record, for each tool and each target:
- modelling effort;
- check time, memory, and state counts;
- which safety, liveness, and fairness properties were actually exercised;
- whether known answers were reproduced;
- install and CI cost against the repository's 30-minute job cap;
- bridge options back to Rust.

Exploratory attempts SHALL be recorded, including those that failed. Until
such a decision says otherwise, Kani SHALL remain the single maintained
formal tool. A recommended lane, dependency, or maintained model SHALL be
carried by a separate follow-up change.

#### Scenario: A tool is proposed on opinion alone
- **WHEN** a change proposes a maintained lane or a production dependency for a non-Kani modelling tool
- **AND** no recorded comparison with a decision section supports that adoption
- **THEN** the change is not accepted until such a comparison exists

#### Scenario: The evaluation calibrates against known answers
- **WHEN** a tool is evaluated on a target whose answer Kani already established, such as durable-write crash atomicity and the torn destination when the temp sync is skipped
- **THEN** the comparison records whether the tool reproduced each known answer
- **AND** a tool that fails to reproduce a known answer is not adopted on the strength of that target

#### Scenario: The decision keeps Kani only
- **WHEN** the comparison's decision section chooses to keep Kani only
- **THEN** no external tool gains a CI lane, dependency, or maintained model
- **AND** the evaluation models remain disposable evidence in their non-production location, or are removed, as the decision states
