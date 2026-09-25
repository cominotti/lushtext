## MODIFIED Requirements

### Requirement: External modelling tools are adopted only on recorded empirical evidence
The project SHALL adopt an external modelling, model-checking, or deductive
verification tool only after a written comparison on this project's own
targets, and only through an explicit decision section in that comparison.
This covers Quint, TLA+ and its checkers, `stateright`, deductive verifiers
for Rust such as Verus, and any other tool in addition to or instead of Kani,
whether it is adopted as a maintained lane, a design-sketch tool, or a test
driver.

The comparison SHALL record, for each tool and each target:
- modelling effort;
- check time, memory, and state counts;
- which safety, liveness, and fairness properties were actually exercised;
- whether known answers were reproduced;
- install and CI cost against the repository's 30-minute job cap;
- bridge options back to Rust.

For a deductive verifier it SHALL also record:
- the ratio of specification, proof, and invariant lines to verified code
  lines;
- the rewrite footprint the verified subset imposes on the production module;
- the trusted base: every unverified specification the proof relies on;
- how a production change propagates to the proof.

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

#### Scenario: A deductive verifier's cost is recorded
- **WHEN** a deductive verifier such as Verus is evaluated on a production pure core
- **THEN** the comparison records its annotation ratio, rewrite footprint, trusted base, and proof-maintenance cost next to its verification results
- **AND** an unbounded proof counts as evidence only if it fails on at least two real pre-fix defects that a Kani harness also rejects
- **AND** a proof that relies on a trusted specification for a decision under test does not count as evidence for that decision

### Requirement: Unbounded claims are attempted in Kani first
Before any tool other than Kani is discussed for a claim that needs to be
unbounded, the project SHALL make two recorded attempts inside Kani:

1. Kani loop contracts.
2. A compositional argument: small harnesses plus a written proof note that
   extends a bounded proof to any size.

A tool other than Kani, including Lean, SHALL be re-discussed only if both
attempts fail **and** an unbounded claim is actually needed, for example as
evidence for GTK Lush publication. It SHALL then be decided by a recorded
maintainer decision. The programme record's tool section and its candidate
list SHALL state this gate.

A recorded maintainer decision MAY authorise a time-boxed, disposable
evaluation that measures another tool on an unbounded claim before the claim
is needed. The evaluation SHALL record both Kani attempts for each claim it
targets, next to the other tool's result: the attempts already on record, or
new ones made during the evaluation. It SHALL NOT target a claim that either
Kani attempt already verified. Its result SHALL NOT lead to adoption for a
claim unless both Kani attempts for that claim failed and the claim is
needed.

#### Scenario: A claim needs to be unbounded
- **WHEN** a contributor needs an unbounded version of a bounded Kani result
- **THEN** a loop-contract harness and a compositional argument are attempted and their outcomes recorded before another tool is proposed

#### Scenario: Both attempts succeed
- **WHEN** either attempt verifies the claim
- **THEN** the claim is recorded as proved in Kani with its stated domain, and no other tool is proposed for it

#### Scenario: Lean note reflects the gate
- **WHEN** this change completes
- **THEN** the "Lean is dormant" text in `docs/next/formal-verification.md` and the "Dormant: Lean" section of `docs/next/formal-verification-next.md` say Lean is re-discussed only if both attempts fail and an unbounded claim is needed

#### Scenario: An authorised evaluation measures a tool before the need
- **WHEN** a maintainer-authorised disposable evaluation measures a tool other than Kani on an unbounded claim that no trigger yet needs
- **THEN** its report records both Kani attempts for that claim next to the other tool's result
- **AND** it does not target a claim that a Kani attempt already verified, such as the slice loop within its per-bin domain
- **AND** its decision adopts nothing for that claim unless both Kani attempts failed and the claim is needed
