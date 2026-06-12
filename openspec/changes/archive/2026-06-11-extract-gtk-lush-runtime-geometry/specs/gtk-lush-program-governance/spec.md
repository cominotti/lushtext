## MODIFIED Requirements

### Requirement: Afternoon-adoption test
Each published GTK Lush crate SHALL be adoptable, in isolation, by a stock
gtk4-rs application without restructuring that application. Before any crate's
first `0.1.0` publication, the program MUST run the test literally: a fresh
session adopts exactly one crate into a stock gtk-rs starter application, the
elapsed effort is journaled, and every friction point is filed as an issue.
Every functional crate MUST also ship at least one example under `examples/`
that proves single-crate adoption before publication; the example MAY be named
for the crate or scenario and is not required to be literally
`examples/standalone.rs`.

#### Scenario: Standalone example proves single-crate adoption
- **WHEN** a family crate is built with its examples
- **THEN** at least one `examples/*.rs` adoption example compiles and runs
  against stock gtk-rs using only that crate from the family
- **AND** policy tooling does not require the example file to be named exactly
  `standalone.rs`

#### Scenario: Publishing blocked without the timed test
- **WHEN** a crate is proposed for its first `0.1.0` release without a
  journaled afternoon-adoption test result
- **THEN** the publishing gate fails and the release does not proceed

## ADDED Requirements

### Requirement: Runtime geometry phase review gate
The `extract-gtk-lush-runtime-geometry` phase SHALL keep the GTK Lush
anti-framework constitution enforceable while extracting task, viewport, and
widget primitives. The phase MUST include focused audits and delegated reviews
for task freshness, data safety, GTK allocation/rendering assumptions,
responsiveness, Rust architecture, comments, and visual proof before archive.

#### Scenario: Constitution review covers all Phase 3 crates
- **WHEN** the runtime-geometry phase adds or modifies `gtk-lush-tasks`,
  `gtk-lush-viewport`, or `gtk-lush-widgets`
- **THEN** the governance review records that each crate remains a leaf crate,
  owns no GTK control flow, introduces no view DSL, introduces no
  state/message/component system, and leaves Libadwaita adaptive behavior
  authoritative

#### Scenario: Focused reviews are required before archive
- **WHEN** the runtime-geometry implementation is otherwise complete
- **THEN** focused reviews examine the task, viewport, clipping, render-hold,
  documentation, and test/proof surfaces
- **AND** actionable findings are fixed or explicitly recorded with maintainer
  approval before the phase can archive

#### Scenario: Retained-site audits remain part of governance
- **WHEN** a LushText task, freshness, viewport, idle repair, clipping, or
  render-hold site remains explicit after migration
- **THEN** the phase audit records the file, owner, classification, and reason
  it remains outside the reusable crate contract
- **AND** project rules do not present the retained site as an accidental miss
