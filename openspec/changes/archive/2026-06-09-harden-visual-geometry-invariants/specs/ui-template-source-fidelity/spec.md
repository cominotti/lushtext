## ADDED Requirements

### Requirement: Geometry-sensitive template edits prove visual invariants
Any UI template edit that changes layout roles, child order, expand flags, margins, size requests, scroll policies, overlay placement, Adwaita layout slots, paned child properties, CSS classes that affect geometry, or accessibility anchors SHALL run or update the relevant visual invariant matrix before claiming layout fidelity.

#### Scenario: Template edit runs invariant proof
- **WHEN** a Blueprint or generated GtkBuilder template edit can affect geometry-sensitive surfaces
- **THEN** the change includes relevant widget allocation assertions or same-session visual invariant proof
- **AND** the proof names protected regions, allowed-changing regions, and state extremes exercised

#### Scenario: Nonzero pixel differences are explained
- **WHEN** a geometry-sensitive template edit produces nonzero differences in a protected or comparison region
- **THEN** the change cannot claim 1:1 UI/UX fidelity unless those differences are reclassified through an updated invariant manifest and justified as intentional
- **AND** the explanation is captured in the change artifacts or review notes

#### Scenario: Generated output drift is not visual proof
- **WHEN** Blueprint regeneration produces no `.ui` drift or passes template validation
- **THEN** that result alone does not satisfy visual geometry proof for a geometry-sensitive change
- **AND** the relevant widget or visual invariant coverage still runs

### Requirement: Template fidelity docs name visual proof responsibilities
Project guidance for UI template work SHALL explain when visual invariant proof is required and where its artifacts are stored.

#### Scenario: Contributor guidance includes visual invariant lane
- **WHEN** maintainers read template editing guidance
- **THEN** it identifies Blueprint regeneration, drift checks, widget tests, and visual invariant proof as separate responsibilities
- **AND** it explains that visual artifacts live under ignored build or smoke directories rather than committed screenshot files
