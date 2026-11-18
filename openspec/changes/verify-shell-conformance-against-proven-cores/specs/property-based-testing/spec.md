## ADDED Requirements

### Requirement: Model-based conformance properties
The property lane SHALL host conformance properties that generate bounded
operation scripts and run them against real non-GTK shells over tiny
temporary directories, in lockstep with a proven core and its model. Each
operation SHALL choose among the model's enabled actions, so that every
script is valid and proptest's own shrinking applies. Each property SHALL
declare its script bound, case count, shrink budget, and reach floors. No
state-machine testing dependency SHALL be added.

#### Scenario: A failing script shrinks to a minimal sequence
- **WHEN** a conformance property fails
- **THEN** proptest shrinks the operation script by removing and simplifying operations, every candidate remains a valid script, and the minimized script is written to the property regression file and promoted to a deterministic test before the fix

#### Scenario: Generated scripts reach the protocol
- **WHEN** the reach test runs the drafts property's strategy at its default case count with a fixed seed
- **THEN** the counts of body writes, commits, deletion steps, restores, cleanups, and startups each meet their recorded floor

#### Scenario: The pull-request lane stays within budget
- **WHEN** `make test-prop` runs with the default case count
- **THEN** each conformance property completes within its recorded budget, and higher counts run only through `make test-prop-deep`

#### Scenario: Conformance properties stay off GTK
- **WHEN** a conformance property runs
- **THEN** it constructs no GTK widget, starts no watcher, and uses no D-Bus, portal, or compositor
