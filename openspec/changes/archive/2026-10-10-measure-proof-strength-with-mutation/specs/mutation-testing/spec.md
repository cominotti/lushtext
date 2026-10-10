## ADDED Requirements

### Requirement: Formal-verification harness code is outside the mutation scope
The configured mutation scope SHALL exclude every `cfg(kani)` harness file,
namely `**/kani_proofs.rs` and every file under a `kani_proofs/` directory.
Harness code is compiled only by `cargo kani`, so the ordinary test build
cannot kill any mutant in it; including it reports every such mutant as
missed. The exclusion SHALL be established by running the tool: the change
that adds or moves a harness file SHALL show from `cargo mutants --list` that
no mutant is generated in harness code and that the production mutant count of
the owning module is unchanged.

#### Scenario: Harness mutants are not generated
- **WHEN** a maintainer runs `make mutants-list`
- **THEN** no listed mutant names a path matching `kani_proofs.rs` or `kani_proofs/`

#### Scenario: The exclusion does not narrow production scope
- **WHEN** the harness exclusion is added or changed
- **THEN** the total list shrinks by exactly the number of mutants previously generated in harness files
- **AND** the per-file mutant counts of every production file are identical before and after

### Requirement: A proof-strength lane measures Kani-covered modules against two oracles
The project SHALL provide a proof-strength lane that, for every module listed
in the Kani oracle table, classifies each generated mutant under two oracles:
the existing non-widget test surface (unit and property tests, run through
cargo-mutants) and the module's Kani harnesses (run by a project driver,
because cargo-mutants cannot use Kani as its test tool). It SHALL report per
module the mutant count and, separately, the kill counts for tests only, Kani
only, and either oracle, joined on the mutant name. Unviable mutants, timeouts,
and vacuity kills SHALL be reported as their own classes and MUST NOT be
counted as ordinary kills.

#### Scenario: Per-module kill rates are reported
- **WHEN** a maintainer runs `make proof-strength` for an oracle module
- **THEN** the report lists that module's mutant count and its tests-only, Kani-only, and combined kill counts
- **AND** it lists every mutant that survived the Kani oracle by name

#### Scenario: A Kani timeout is not a kill
- **WHEN** a mutant makes a harness exceed its per-harness timeout
- **THEN** the mutant is classified as a timeout, not as killed, and is triaged like a survivor

#### Scenario: A vacuous proof is reported distinctly
- **WHEN** a mutant leaves every harness verified but makes a previously satisfiable `kani::cover!` unsatisfiable
- **THEN** the mutant is classified as a vacuity kill and reported separately from harness-failure kills

### Requirement: The proof-strength lane is resumable, isolated, and within the job cap
The Kani oracle driver SHALL apply each mutant in a disposable checkout, never
in the developer's working tree, and SHALL persist one result per mutant so an
interrupted run resumes without re-verifying finished mutants. It SHALL order a
module's harnesses cheapest first and stop at the first failing harness. Any CI
job it runs SHALL declare `timeout-minutes` no greater than 30 and SHALL be
sharded so every shard finishes inside that cap; modules whose single-mutant
cost cannot fit a shard SHALL be local-only with a documented command.

#### Scenario: Developer tree is untouched
- **WHEN** the driver runs with uncommitted edits in the working tree
- **THEN** the edits are neither mutated nor included in the checked source

#### Scenario: Interrupted run resumes
- **WHEN** a run is interrupted and restarted with the same source revision
- **THEN** mutants with a persisted result are skipped and the rest are checked

#### Scenario: Expensive tiers stay local
- **WHEN** a module's cheapest-first harness sequence for one mutant can exceed a CI shard's budget
- **THEN** that module is absent from the CI job and its local command is documented in `docs/mutation-testing.md`
