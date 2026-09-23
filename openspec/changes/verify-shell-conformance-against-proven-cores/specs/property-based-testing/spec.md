## ADDED Requirements

### Requirement: Model-based conformance properties
The property lane SHALL host model-based conformance properties that generate
bounded operation scripts (`Vec<Op>`), including faults and crashes. They SHALL
run those scripts against real non-GTK production shells over tiny temporary
directories, in lockstep with a Kani-proven core and its environment model.
Every model step SHALL be total, meaning an inapplicable operation is a no-op
in both the model and the driver. Proptest's own vector and element shrinking
then yields a minimal failing script without precondition machinery, and no
additional state-machine testing dependency SHALL be added for this purpose.
Conformance properties SHALL declare their script length bound, case count,
and a shrink-time budget. Each SHALL finish within its recorded share of the
pull-request property job at the default case count, and higher counts SHALL
run only through the deep property lane. A minimized failing script SHALL be
persisted in the property regression file and promoted to a deterministic
test before it is fixed. These properties MUST NOT construct GTK widgets,
start watchers, or use D-Bus, portals, or a compositor.

#### Scenario: A failing script shrinks to a minimal sequence
- **WHEN** a conformance property fails
- **THEN** proptest shrinks the operation script by removing and simplifying operations, every candidate remains a valid script, and the minimized script is written to the property regression file

#### Scenario: The pull-request lane stays within budget
- **WHEN** `make test-prop` runs with the default case count
- **THEN** each conformance property completes within its recorded budget, and the property job stays within its timeout

#### Scenario: Deep conformance runs are opt-in
- **WHEN** a maintainer runs `make test-prop-deep`
- **THEN** the conformance properties run with the deep case count and longer scripts, without changing the pull-request configuration
