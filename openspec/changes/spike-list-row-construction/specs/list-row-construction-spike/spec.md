# Spec Delta

## Purpose

Measure what each row-construction form costs inside one GTK dispatch, and record which form LushText's list factories adopt.

## ADDED Requirements

### Requirement: Row construction forms are measured on the same row
The spike SHALL measure three ways of building one realized list row with the same visible content: the current factory that finds children by sibling order, a row subclass built in Rust, and a row subclass from a composite template. Each MUST be measured for creating 32 rows and 205 rows in one dispatch, and for the frame after retexting a 205-row band.

#### Scenario: Three arms, same row
- **WHEN** the probe runs
- **THEN** each arm builds a row with the same widget tree depth, text children and badge
- **AND** the observation records each arm's medians for 32 created rows, 205 created rows and the retext frame

#### Scenario: Measurements are timing verdicts
- **WHEN** the spike's figures are taken for the decision
- **THEN** they come from `scripts/measure.sh` runs on the reference CPU set under the load gate, with the protocol and pass bar written before the run

### Requirement: The spike records one adoption decision
The spike SHALL record which construction form LushText's list factories adopt. The decision MUST follow the pass bar written before measuring: an arm passes when its 32-row creation fits the dispatch budget alongside the rest of the first publish and its 205-row creation and retext frame are not worse than the current factory by more than the recorded margin.

#### Scenario: Template arm passes
- **WHEN** the composite-template arm meets the bar
- **THEN** the decision names it, and the typed-row change converts factories to template rows

#### Scenario: Template arm fails, Rust subclass passes
- **WHEN** only the Rust-built subclass meets the bar
- **THEN** the decision names it and records the template arm's figures as the reason

#### Scenario: No subclass arm passes
- **WHEN** neither subclass arm meets the bar
- **THEN** the decision keeps the factory form, and the typed-row change uses a typed registry for row-owned state instead
