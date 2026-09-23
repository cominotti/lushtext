## ADDED Requirements

### Requirement: Proof strength is measured by mutation
Every module whose behaviour a Kani harness is claimed to check SHALL appear
in the Kani oracle table with the ordered list of harnesses that check it, and
the policy gate SHALL fail when a listed harness does not exist or a listed
module has no harness. The programme record SHALL hold, for each such module,
the latest measured mutant count and kill counts under the tests oracle, the
Kani oracle, and both, with the source revision and tool versions measured.
Every mutant that survives the Kani oracle, including timeouts, SHALL be
recorded with exactly one triage class: **add harness/assertion** (the harness
or assertion is added and the mutant is shown killed), **equivalent mutant**
(with the reasoning that no observable behaviour of the module changes), or
**accepted gap** (with the reason, such as outside the proved input domain or
constrained only by tests). A harness added for a survivor SHALL be assigned to
exactly one shard and SHALL keep that shard within the CI job budget.

#### Scenario: A new harness module enters the oracle table
- **WHEN** a contributor adds a Kani harness for a module not yet in the oracle table
- **THEN** `make check-kani-shards` fails until the module and its harnesses are listed

#### Scenario: A survivor is triaged
- **WHEN** a mutant of an oracle module survives every harness in its list
- **THEN** the programme record names that mutant and gives it exactly one triage class with its justification

#### Scenario: An added assertion kills its survivor
- **WHEN** a survivor is triaged as add harness/assertion
- **THEN** the change that adds the assertion re-runs the Kani oracle for that mutant and records it as killed
- **AND** the unmutated harness still verifies within its shard budget

#### Scenario: A survivor exposes a defect
- **WHEN** triage of a survivor shows that the shipped code violates its documented contract
- **THEN** the defect is fixed in the same change behind a test that failed before the fix
