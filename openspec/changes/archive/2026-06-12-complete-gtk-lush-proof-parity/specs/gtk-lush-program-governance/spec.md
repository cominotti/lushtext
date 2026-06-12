## ADDED Requirements

### Requirement: Proof parity phase closes Phase 4 before publishing gates
The `complete-gtk-lush-proof-parity` phase SHALL close the remaining Phase 4
proof-toolchain gap before the program enters Phase 5 publishing work. The
phase MUST record that Rust live visual proof, policy, wrappers, scheduled
smoke, automation summaries, and governance documentation reached parity with
the Python runner before `cargo gtk-proof run` became authoritative.

#### Scenario: Phase 4 cannot close without Rust live parity
- **WHEN** the parity phase is ready to archive
- **THEN** governance records that Rust corpus, live-runner, animation,
  proof-policy, wrapper, and automation-client summary parity are complete
- **AND** any remaining Python path is labeled as oracle or diagnostic
  compatibility rather than the default proof authority

#### Scenario: Phase 5 remains blocked until this archive
- **WHEN** a follow-up proposes publishing, second-consumer adoption,
  repository split, or first `0.1.0` GTK Lush release work
- **THEN** review rejects it unless this parity phase has archived or the
  proposal explicitly supersedes it with maintainer-approved governance notes

### Requirement: Proof parity implementation includes specialist reviews
The proof parity implementation SHALL include delegated specialist review
before archive. Reviews MUST cover GTK test harness behavior, live GTK and
headless runtime behavior, GTK/Libadwaita contract assumptions, responsiveness
and CI runtime cost, data safety and artifact privacy, Rust architecture, and
comment quality for the Rust proof runner and associated wrappers.

#### Scenario: Specialist review evidence is recorded
- **WHEN** implementation tasks are marked complete
- **THEN** the tasks or review notes identify the specialist lanes run for GTK
  testing, live GTK debugging, GTK internals, performance, data safety,
  architecture, and comments
- **AND** actionable findings are fixed or explicitly documented as accepted
  non-blockers before archive

#### Scenario: Artifact privacy review is mandatory
- **WHEN** proof artifacts, schemas, wrappers, summaries, or automation-client
  delegation change
- **THEN** review verifies that outputs remain bounded and do not expose
  unbounded user content
- **AND** any new artifact field is classified as safe, redacted, or app-owned
  private data before it is documented

## MODIFIED Requirements

### Requirement: Phase 4 proof extraction records governance conformance
The proof toolchain and parity phases SHALL record governance review entries
before archive. The entries MUST cover the anti-framework constitution, the
leaf-crate status of `gtk-lush-proof-harness` and
`gtk-lush-proof-spine`, the workspace-tool status of `cargo-gtk-proof`,
Automation1 zero-drift evidence, visual proof compatibility evidence, Rust
live-runner parity evidence, wrapper migration evidence, and the Phase 5
boundary that keeps publication and second-consumer work out of scope.

#### Scenario: Governance review blocks archive when missing
- **WHEN** the proof parity change is ready to archive
- **THEN** `crates/gtk-lush/GOVERNANCE.md` contains a dated review entry for
  `complete-gtk-lush-proof-parity`
- **AND** the entry names the verification gates, parity evidence, wrapper
  migration status, and any constitution exceptions or states that no
  exceptions were taken

#### Scenario: Cargo tool is not a family exception
- **WHEN** governance describes `cargo-gtk-proof`
- **THEN** it identifies the tool as a workspace proof runner outside the GTK
  Lush family
- **AND** it does not record a family package-name or interdependency exception
  for the tool

#### Scenario: Phase 4 audit distinguishes staged and authoritative proof
- **WHEN** governance summarizes Phase 4 proof extraction
- **THEN** it distinguishes the earlier staged schema/corpus/policy extraction
  from this parity phase's live-runner and wrapper migration
- **AND** it states which Python paths remain available only as oracle or
  diagnostic compatibility

### Requirement: Phase 4 keeps Phase 5 publishing gates deferred
The proof toolchain and parity phases SHALL NOT publish functional crates,
claim Phase 5 publication readiness, require a second real consumer, split the
repository, or remove the `0.0.0` pre-publication status of in-tree GTK Lush
APIs. Any docs updated during Phase 4 MUST continue to state that functional
publication requires the later `graduate-and-publish-gtk-lush` phase.

#### Scenario: README and CHANGELOG keep pre-publication status
- **WHEN** the proof crates and existing family docs are updated
- **THEN** their README and CHANGELOG files state that the APIs are functional
  in-tree `0.0.0` APIs and are not Phase 5 publication-ready
- **AND** no release automation publishes them as functional crates

#### Scenario: Proposal does not satisfy second-consumer gate
- **WHEN** the Phase 4 proof parity change completes with LushText consuming
  the extracted pieces and Rust owning the default visual proof runner
- **THEN** governance still requires Phase 5 for a second real consumer,
  timed afternoon-adoption test, API review, publication, and repository split

#### Scenario: Publishing follow-up remains separate
- **WHEN** documentation or governance says proof parity is complete
- **THEN** it also states that publishing gates remain deferred to
  `graduate-and-publish-gtk-lush`
- **AND** proof parity evidence is not treated as a substitute for
  two-consumer adoption or public API review

### Requirement: Proof extraction review includes delegated specialist review
The proof extraction and parity phases SHALL include delegated review
appropriate to their blast radius before archive. Reviews MUST cover GTK test
harness behavior, live GTK/headless runtime behavior, GTK/Libadwaita contract
assumptions, responsiveness and CI runtime cost, data safety/privacy of
artifacts, Rust architecture, and comment quality for extracted public APIs,
the Rust live runner, wrappers, and automation-client delegation.

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

#### Scenario: Runtime review covers live proof orchestration
- **WHEN** the Rust live runner launches desktop sessions, captures
  screenshots, reads D-Bus, or drives Automation1 actions
- **THEN** delegated review verifies host skip behavior, cleanup, warning-scan
  handling, readiness waits, and same-session proof semantics
- **AND** runtime findings are fixed or recorded before archive

### Requirement: Vision document remains aligned with proof toolchain boundaries
`docs/next/gtk-lush.md` SHALL be updated in the same change to reflect the
actual Phase 4 result. It MUST distinguish proof family crates from the cargo
workspace tool, record whether Python remains only as an oracle or diagnostic
compatibility path, state that Rust owns default live visual proof after
parity, and keep Phase 5 and Phase 6 scope separate.

#### Scenario: Tool placement changes the vision document
- **WHEN** `cargo-gtk-proof` is added or promoted outside `crates/gtk-lush/`
- **THEN** `docs/next/gtk-lush.md` explains that placement
- **AND** the OpenSpec specs remain authoritative if the narrative and specs
  disagree

#### Scenario: Python retirement status is documented
- **WHEN** the parity phase ends
- **THEN** the vision document states whether the default Makefile path uses
  Rust, whether Python remains as a compatibility oracle or diagnostic helper,
  and what evidence justified the transition

#### Scenario: Phase 5 boundary remains visible
- **WHEN** the vision document describes completed proof parity
- **THEN** it keeps second-consumer adoption, afternoon-adoption testing,
  publishing, repository split, and upstreaming in their later phases
- **AND** it does not imply that proof parity alone makes GTK Lush
  publication-ready
