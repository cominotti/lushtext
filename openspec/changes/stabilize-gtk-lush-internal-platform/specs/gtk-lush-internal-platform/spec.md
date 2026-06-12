## ADDED Requirements

### Requirement: Internal platform is an explicit steady state
GTK Lush SHALL be allowed to remain a functional in-tree LushText platform
without publishing functional crates, preparing `0.1.0` releases, splitting a
repository, or moving LushText to published dependencies. This steady state
MUST preserve the anti-framework constitution, leaf-crate boundaries,
workspace path dependencies, and local verification gates that make the family
safe for LushText.

#### Scenario: Internal platform completion is reviewable
- **WHEN** this stabilization change reaches archive readiness
- **THEN** GTK Lush documentation and specs state that the current target is
  stable in-tree infrastructure for LushText
- **AND** no task prepares functional crates.io publication, repository split,
  or LushText dependency migration to published crates

#### Scenario: LushText keeps using path dependencies
- **WHEN** LushText builds against GTK Lush after this change
- **THEN** it uses workspace path dependencies for functional GTK Lush crates
- **AND** no crates.io functional publication is required for normal LushText
  development, testing, or release preparation

### Requirement: Future GTK Lush work is demand-driven
Future GTK Lush work SHALL be proposed only when it addresses current LushText
pain, keeps existing GTK Lush evidence and checks from drifting, materially
improves proof-tooling confidence, or responds to a real external adopter pull
signal. Work that exists only to continue an obsolete roadmap MUST NOT be
treated as sufficient justification.

#### Scenario: Proposed work names its demand signal
- **WHEN** a later GTK Lush change is proposed
- **THEN** its proposal names the LushText pain, evidence drift,
  proof-tooling improvement, or external adopter need that justifies the work
- **AND** review can reject the change when it only advances publication,
  repository split, or upstreaming by momentum

#### Scenario: Tiny fragmented follow-ups are avoided
- **WHEN** future GTK Lush planning starts
- **THEN** the proposal starts as one coherent phase-level change
- **AND** it splits only when implementation ownership, validation cost, or
  artifact clarity requires separate changes

### Requirement: Baseline evidence remains maintained
GTK Lush baseline evidence SHALL remain maintained for internal platform
health. The adoption lab, adoption matrix, stock fixture, API review,
specialist notes, public-API advisory output, doctests, examples, policy
checks, and proof tooling are the baseline evidence set. Evidence MUST be
updated when matching APIs, examples, fixtures, lab workflows, proof schemas,
or policy checks change.

#### Scenario: API change updates evidence
- **WHEN** a functional GTK Lush API, example, fixture, or lab workflow changes
- **THEN** the adoption matrix, examples, doctests, public-API advisory
  snapshots, and affected adoption evidence are updated in the same change
- **AND** GTK Lush policy and adoption checks pass before archive

#### Scenario: Baseline checks stay local
- **WHEN** maintainers run the GTK Lush internal-platform verification lane
- **THEN** it uses local workspace checks and committed bounded evidence
- **AND** it does not require crates.io publication, network access, external
  project checkout retention, or private user content

### Requirement: Publication track requires explicit reopening
GTK Lush publication work SHALL require explicit reopening before it starts.
Functional publication, `0.1.0` release preparation, repository graduation,
and LushText migration to published GTK Lush dependencies require a later
explicit OpenSpec change with maintainer approval. That change MUST cite
current adoption evidence, refresh stale evidence, and perform
publication-specific release, docs.rs, semver, changelog, credential, and
repository-history tasks.

#### Scenario: Publication work is blocked by default
- **WHEN** a change attempts to publish functional GTK Lush crates, prepare
  `0.1.0`, split the repository, or move LushText to published GTK Lush
  dependencies
- **THEN** review requires a dedicated publication or graduation proposal
- **AND** the proposal records maintainer approval and refreshed adoption,
  semver, public-API, documentation, and release evidence

#### Scenario: Current internal work does not create external stability
- **WHEN** an internal-platform change edits GTK Lush crates or documentation
- **THEN** the crates remain pre-publication `0.0.0` workspace APIs
- **AND** docs do not promise external semver stability beyond the current
  workspace contract

### Requirement: Roadmap wording does not override current evidence
GTK Lush roadmap wording SHALL not override the current internal-platform
evidence. README, governance, archive-handoff, and local guidance MUST state
that the internal platform posture supersedes automatic execution of older
publication, graduation, or upstreaming plans. Historical roadmap material MAY
remain as context only when it is clearly marked as superseded by the current
steady-state decision.

#### Scenario: Stale next-step wording is removed
- **WHEN** this change updates GTK Lush documentation
- **THEN** it removes or rewrites language that says Phase 5b publication,
  repository graduation, or Phase 6 upstreaming is the automatic next step
- **AND** it preserves any useful publication gates as optional future-track
  requirements

#### Scenario: Agents see the current posture first
- **WHEN** an agent reads GTK Lush guidance after this change
- **THEN** the first actionable posture is LushText-first in-tree stewardship
- **AND** publication, repository split, and upstreaming appear only as
  reopened tracks requiring explicit approval and fresh evidence
