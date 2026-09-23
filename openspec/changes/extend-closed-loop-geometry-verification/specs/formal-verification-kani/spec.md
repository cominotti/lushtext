## ADDED Requirements

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

#### Scenario: A claim needs to be unbounded
- **WHEN** a contributor needs an unbounded version of a bounded Kani result
- **THEN** a loop-contract harness and a compositional argument are attempted and their outcomes recorded before another tool is proposed

#### Scenario: Both attempts succeed
- **WHEN** either attempt verifies the claim
- **THEN** the claim is recorded as proved in Kani with its stated domain, and no other tool is proposed for it

#### Scenario: Lean note reflects the gate
- **WHEN** this change completes
- **THEN** the "Lean is dormant" text in `docs/next/formal-verification.md` and the "Dormant: Lean" section of `docs/next/formal-verification-next.md` say Lean is re-discussed only if both attempts fail and an unbounded claim is needed

### Requirement: Experimental Kani features are declared per shard
A harness that needs an experimental Kani feature SHALL be enabled only
through its shard's entry in the single shard table. Loop contracts
(`-Z loop-contracts`) and function contracts are examples. `make kani` and
the CI workflow SHALL pass those flags from the table. Harnesses in other
shards SHALL NOT receive them. The table's policy check SHALL still assign
every harness to exactly one shard.

#### Scenario: A loop-contract harness runs in its shard
- **WHEN** the shard containing a loop-contract harness runs locally or in CI
- **THEN** Kani is invoked with the loop-contract flag from the table, and the job stays within the 30-minute cap

#### Scenario: Flags do not leak
- **WHEN** any other shard runs
- **THEN** its Kani invocation carries no experimental flag it did not declare
