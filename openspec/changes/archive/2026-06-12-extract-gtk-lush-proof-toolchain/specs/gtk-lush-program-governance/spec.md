## ADDED Requirements

### Requirement: Phase 4 proof extraction records governance conformance
The `extract-gtk-lush-proof-toolchain` phase SHALL record a governance review
entry before archive. The entry MUST cover the anti-framework constitution,
the leaf-crate status of `gtk-lush-proof-harness` and
`gtk-lush-proof-spine`, the workspace-tool status of `cargo-gtk-proof`,
Automation1 zero-drift evidence, visual proof compatibility evidence, and the
Phase 5 boundary that keeps publication and second-consumer work out of scope.

#### Scenario: Governance review blocks archive when missing
- **WHEN** the Phase 4 change is ready to archive
- **THEN** `crates/gtk-lush/GOVERNANCE.md` contains a dated review entry for
  `extract-gtk-lush-proof-toolchain`
- **AND** the entry names the verification gates and any constitution
  exceptions or states that no exceptions were taken

#### Scenario: Cargo tool is not a family exception
- **WHEN** governance describes `cargo-gtk-proof`
- **THEN** it identifies the tool as a workspace proof runner outside the GTK
  Lush family
- **AND** it does not record a family package-name or interdependency exception
  for the tool

### Requirement: Phase 4 keeps Phase 5 publishing gates deferred
The proof toolchain phase SHALL NOT publish functional crates, claim Phase 5
publication readiness, require a second real consumer, split the repository,
or remove the `0.0.0` pre-publication status of in-tree GTK Lush APIs. Any
docs updated during Phase 4 MUST continue to state that functional publication
requires the later `graduate-and-publish-gtk-lush` phase.

#### Scenario: README and CHANGELOG keep pre-publication status
- **WHEN** the proof crates and existing family docs are updated
- **THEN** their README and CHANGELOG files state that the APIs are functional
  in-tree `0.0.0` APIs and are not Phase 5 publication-ready
- **AND** no release automation publishes them as functional crates

#### Scenario: Proposal does not satisfy second-consumer gate
- **WHEN** the Phase 4 change completes with LushText consuming the extracted
  pieces
- **THEN** governance still requires Phase 5 for a second real consumer,
  timed afternoon-adoption test, API review, publication, and repository split

### Requirement: Proof extraction review includes delegated specialist review
The phase SHALL include delegated review appropriate to its blast radius before
archive. Reviews MUST cover GTK test harness behavior, live GTK/headless
runtime behavior, GTK/Libadwaita contract assumptions, responsiveness and CI
runtime cost, data safety/privacy of artifacts, Rust architecture, and comment
quality for extracted public APIs.

#### Scenario: Review evidence is recorded in tasks
- **WHEN** implementation tasks are marked complete
- **THEN** the tasks or review notes identify the delegated review lanes run
  for GTK testing, live GTK debugging, GTK internals, performance, data safety,
  architecture, and comments
- **AND** actionable findings are fixed or explicitly documented as accepted
  non-blockers before archive

#### Scenario: Privacy review covers artifacts
- **WHEN** proof artifacts, schemas, wrappers, or summaries change
- **THEN** the review verifies that outputs remain bounded and do not expose
  unbounded user content
- **AND** any new artifact field is classified as safe, redacted, or app-owned
  private data before it is documented

### Requirement: Vision document remains aligned with proof toolchain boundaries
`docs/next/gtk-lush.md` SHALL be updated in the same change to reflect the
actual Phase 4 result. It MUST distinguish proof family crates from the cargo
workspace tool, record whether Python remains only as an oracle or diagnostic
compatibility path, and keep Phase 5 and Phase 6 scope separate.

#### Scenario: Tool placement changes the vision document
- **WHEN** `cargo-gtk-proof` is added outside `crates/gtk-lush/`
- **THEN** `docs/next/gtk-lush.md` explains that placement
- **AND** the OpenSpec specs remain authoritative if the narrative and specs
  disagree

#### Scenario: Python retirement status is documented
- **WHEN** the phase ends
- **THEN** the vision document states whether the default Makefile path uses
  Rust, whether Python remains as a compatibility oracle or diagnostic helper,
  and what evidence justified the transition
