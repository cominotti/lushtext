## MODIFIED Requirements

### Requirement: Follow-up roadmap conformance
Each reserved follow-up phase SHALL arrive as its own OpenSpec change, MUST
declare conformance to this governance capability, and MUST keep LushText's
full existing gate set green at its phase boundary, including visual-geometry
proof whenever visual-sensitive files change. The reserved phases are
`migrate-preview-pane-to-adwaita`, `normalize-declarative-bindings`,
`normalize-settle-timer-helpers`, `extract-gtk-lush-signals-and-settle`,
`extract-gtk-lush-runtime-geometry`, `extract-gtk-lush-proof-toolchain`,
`graduate-and-publish-gtk-lush`, and `gtk-lush-upstreaming-round-one`, as
named in the umbrella vision.

#### Scenario: Follow-up phase proposed
- **WHEN** a reserved follow-up change is proposed
- **THEN** its proposal references this capability, and its tasks include the
  full LushText gate set at the phase boundary

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

## ADDED Requirements

### Requirement: Settle timer normalization remains app-internal
The `normalize-settle-timer-helpers` follow-up SHALL remain a Phase 0
LushText-internal simplification. It MUST reduce extraction noise by auditing
and normalizing app-local generation-counter debounce, settle-burst, and
superseding one-shot timer patterns, and MUST NOT introduce a public GTK Lush
crate API, custom control-flow owner, view DSL, state/message system,
component framework, or inter-crate dependency.

#### Scenario: Follow-up proposal stays within Phase 0
- **WHEN** `normalize-settle-timer-helpers` is proposed or implemented
- **THEN** its artifacts reference GTK Lush governance as the controlling
  program capability
- **AND** the change states that no public GTK Lush API, control-flow owner,
  view DSL, or state/message system is introduced

#### Scenario: Private helper is not extraction
- **WHEN** a timer pattern is normalized during this follow-up
- **THEN** the implementation uses a private LushText helper or existing GTK
  main-loop mechanisms
- **AND** any reusable GTK Lush API design remains deferred to
  `extract-gtk-lush-signals-and-settle`

#### Scenario: Phase boundary uses full gates
- **WHEN** this follow-up reaches completion
- **THEN** LushText's full phase gate set passes, including visual-geometry
  proof whenever visual-sensitive files changed
- **AND** the audit records which timer-like candidates remain explicit
  exceptions for polling, chunking, async freshness, or domain-generation
  reasons
