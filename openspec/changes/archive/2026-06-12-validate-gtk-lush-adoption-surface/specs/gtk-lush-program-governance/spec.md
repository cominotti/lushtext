## ADDED Requirements

### Requirement: Adoption validation precedes GTK Lush publication
The GTK Lush program SHALL treat `validate-gtk-lush-adoption-surface` as the
non-publication adoption-validation phase that precedes
`graduate-and-publish-gtk-lush`. This phase MUST prove second-consumer
adoption, timed stock-starter adoption, unrelated-existing-project friction,
and API review before any later functional `0.1.0` publication or repository
graduation work begins.

#### Scenario: Adoption phase stays before publishing
- **WHEN** a follow-up proposes functional crates.io publication,
  `0.1.0`, repository split, or LushText migration to published GTK Lush
  crates
- **THEN** review rejects it unless adoption validation has archived or the
  proposal records an explicit maintainer-approved supersession

#### Scenario: Adoption phase does not publish
- **WHEN** `validate-gtk-lush-adoption-surface` is implemented
- **THEN** it records adoption evidence and API review
- **AND** it does not publish functional GTK Lush crates, prepare `0.1.0`, or
  split the repository

### Requirement: Adoption validation implementation includes specialist reviews
The adoption-validation implementation SHALL include delegated or otherwise
focused specialist review before archive. Reviews MUST cover GTK testing,
live/headless GTK behavior when the lab or visual proof changes, GTK and
Libadwaita contract assumptions, responsiveness and CI runtime cost, data
safety and privacy for journals/artifacts, Rust architecture, and comment
quality for public APIs and adoption examples.

#### Scenario: Review evidence is recorded
- **WHEN** implementation tasks are marked complete
- **THEN** tasks or review notes identify the review lanes run for GTK testing,
  GTK runtime behavior, GTK internals, performance, data safety/privacy,
  architecture, and comments
- **AND** actionable findings are fixed or documented as accepted non-blockers
  before archive

#### Scenario: Artifact privacy review covers adoption evidence
- **WHEN** adoption journals, external-project notes, proof summaries, or
  generated fixtures are added
- **THEN** review verifies they remain bounded and do not expose private user
  content, unbounded logs, raw image data, or vendored external source trees

### Requirement: Vision document splits Phase 5 adoption and publishing
`docs/next/gtk-lush.md` SHALL distinguish the adoption-validation phase from
the later publication/graduation phase. The vision document MUST continue to
state that OpenSpec specs are authoritative, and it MUST NOT imply that
adoption validation alone publishes crates or creates external stability
guarantees.

#### Scenario: Roadmap names both halves
- **WHEN** the vision document is updated for this change
- **THEN** it names the adoption-validation phase before the
  publication/graduation phase
- **AND** it keeps publication, repo split, LushText published dependencies,
  and upstreaming in later work

#### Scenario: Vision and specs stay aligned
- **WHEN** adoption scope, crate naming, phase ordering, or publishing gates
  change
- **THEN** `docs/next/gtk-lush.md` is updated in the same change
- **AND** review treats the OpenSpec specs as authoritative if narrative text
  and specs conflict

## MODIFIED Requirements

### Requirement: Publishing gates
No family crate SHALL publish `0.1.0` or later before all of the following
hold: the `validate-gtk-lush-adoption-surface` phase has archived; at least
two real consumers exist (LushText plus one application that is not a
contrived example); the afternoon-adoption test has passed and is journaled;
an unrelated-existing-project adoption spike has been recorded for at least
one crate; semver and public-API tooling are green; and documentation is
complete per the engineering bar. Pre-gate crates MAY reserve names on
crates.io only as initial `0.0.0` placeholders whose README points at the
vision document and declares that no public API is available yet.

#### Scenario: Premature publish attempt
- **WHEN** a release is prepared for a crate before adoption validation has
  archived or with only LushText as a real consumer
- **THEN** the release checklist fails the adoption and two-consumer gates
- **AND** the publication is aborted

#### Scenario: Timed adoption and external spike are required
- **WHEN** a crate is proposed for its first `0.1.0` release
- **THEN** release review can cite a timed stock gtk-rs adoption journal and
  unrelated-existing-project adoption notes from the adoption-validation phase
- **AND** unresolved friction is either fixed before release or explicitly
  documented as an accepted limitation

### Requirement: Follow-up roadmap conformance
Each reserved follow-up phase SHALL arrive as its own OpenSpec change, MUST
declare conformance to this governance capability, and MUST keep LushText's
full existing gate set green at its phase boundary, including visual-geometry
proof whenever visual-sensitive files change. The reserved phases are
`migrate-preview-pane-to-adwaita`, `normalize-declarative-bindings`,
`normalize-settle-timer-helpers`, `extract-gtk-lush-signals-and-settle`,
`extract-gtk-lush-runtime-geometry`, `extract-gtk-lush-proof-toolchain`,
`complete-gtk-lush-proof-parity`,
`validate-gtk-lush-adoption-surface`, `graduate-and-publish-gtk-lush`, and
`gtk-lush-upstreaming-round-one`, as named in the umbrella vision.

#### Scenario: Follow-up phase proposed
- **WHEN** a reserved follow-up change is proposed
- **THEN** its proposal references this capability
- **AND** its tasks include the full LushText gate set at the phase boundary

#### Scenario: Geometry extraction phase verification
- **WHEN** the runtime-geometry or proof-toolchain phase migrates
  visual-sensitive LushText code
- **THEN** the phase passes widget suites plus a visual-geometry run that
  pixel-verifies the affected invariants before it can archive

#### Scenario: Settle normalization precedes extraction
- **WHEN** `extract-gtk-lush-signals-and-settle` is proposed or implemented
- **THEN** the `normalize-settle-timer-helpers` Phase 0 follow-up has already
  archived or the extraction proposal records why the prerequisite was
  deliberately superseded

#### Scenario: Adoption validation precedes publication follow-up
- **WHEN** `graduate-and-publish-gtk-lush` is proposed or implemented
- **THEN** `validate-gtk-lush-adoption-surface` has already archived or the
  proposal records an explicit maintainer-approved supersession
- **AND** publication-specific tasks remain separate from adoption-validation
  tasks
