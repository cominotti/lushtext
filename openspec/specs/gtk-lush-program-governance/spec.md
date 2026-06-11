# gtk-lush-program-governance Specification

## Purpose
Define the governance, adoption, maintenance, licensing, publishing, and
roadmap constraints for the GTK Lush crate family so the project remains a set
of small, stock gtk-rs-compatible helper crates rather than a framework.

## Requirements
### Requirement: Anti-framework constitution
Every GTK Lush crate, API, and follow-up change SHALL satisfy the program
constitution: no ownership of GTK control flow (main loop, widget lifecycle,
rendering, or scheduling around them), no view DSL or custom UI syntax that
replaces ordinary widget code or Blueprint, no state/message/component system,
no dependencies between family crates (leaf crates only), and no
re-implementation of adaptive behavior that Libadwaita provides. Macros are
permitted only when derive-style and additive (generating code an author would
otherwise write by hand).

#### Scenario: Constitution gate on family changes
- **WHEN** a change adds or modifies code under `crates/gtk-lush/`
- **THEN** the change documents a constitution checklist answer for each
  principle, and any violation blocks the change unless GOVERNANCE.md records
  an explicitly approved exception entry

#### Scenario: Inter-crate dependency is rejected
- **WHEN** a family crate declares another family crate as a non-dev
  dependency
- **THEN** the workspace policy check fails and the dependency must be removed
  or the design reworked so each crate remains independently adoptable

### Requirement: Afternoon-adoption test
Each published GTK Lush crate SHALL be adoptable, in isolation, by a stock
gtk4-rs application without restructuring that application. Before any crate's
first `0.1.0` publication, the program MUST run the test literally: a fresh
session adopts exactly one crate into a stock gtk-rs starter application,
the elapsed effort is journaled, and every friction point is filed as an
issue.

#### Scenario: Standalone example proves single-crate adoption
- **WHEN** a family crate is built with its examples
- **THEN** an `examples/standalone.rs` compiles and runs against stock gtk-rs
  using only that crate from the family

#### Scenario: Publishing blocked without the timed test
- **WHEN** a crate is proposed for its first `0.1.0` release without a
  journaled afternoon-adoption test result
- **THEN** the publishing gate fails and the release does not proceed

### Requirement: Engineering bar for family crates
Every family crate SHALL enforce: `#![forbid(unsafe_code)]` (exceptions
require a documented invariant and a GOVERNANCE.md entry),
`#![deny(missing_docs)]` with runnable doctests for observable behavior, the
workspace's curated lint table, SPDX headers, a README derived from the
corresponding `.agents/rules` section, a Keep-a-Changelog CHANGELOG, declared
`rust-version`, and test coverage appropriate to its surface (unit and
doctests always; headless widget tests for widget-touching behavior; property
tests for pure decision logic; inclusion of pure logic in the deterministic
mutation scope).

#### Scenario: Documentation gate
- **WHEN** a public item is added to a family crate without documentation
- **THEN** the crate fails to build under its lint configuration

#### Scenario: Pure logic enters the mutation scope
- **WHEN** a family crate adds pure deterministic decision logic
- **THEN** the cargo-mutants configuration includes it and survivors are
  triaged under the existing mutation policy

### Requirement: Licensing policy
Family crates SHALL be dual-licensed `MIT OR Apache-2.0` with SPDX headers on
every source file, while the LushText application remains
`GPL-3.0-or-later`. License metadata MUST be declared per crate and verified
by the dependency policy gate.

#### Scenario: License declaration verified
- **WHEN** the dependency policy gate runs over the workspace
- **THEN** each `gtk-lush-*` crate reports `MIT OR Apache-2.0` and the check
  passes only when headers and metadata agree

### Requirement: Maintenance treadmill SLAs
The program SHALL document and honor: support for a new gtk-rs major release
within one family release cycle, a GNOME SDK floor raise at most once per
year, and an MSRV no newer than latest stable minus two at each publication.
A blocked gtk-rs bump SHALL halt publishing rather than fork behavior.

#### Scenario: gtk-rs major release lands
- **WHEN** a new gtk-rs major series is released
- **THEN** an issue tracking the family bump is opened with the SLA deadline,
  and no crate publishes a release that skips the bump after the deadline

### Requirement: Publishing gates
No family crate SHALL publish `0.1.0` or later before all of the following
hold: at least two real consumers exist (LushText plus one application that is
not a contrived example), the afternoon-adoption test has passed and is
journaled, semver and public-API tooling are green, and documentation is
complete per the engineering bar. Pre-gate crates MAY reserve names on
crates.io only as initial `0.0.0` placeholders whose README points at the
vision document and declares that no public API is available yet.

#### Scenario: Premature publish attempt
- **WHEN** a release is prepared for a crate with only LushText as a consumer
- **THEN** the release checklist fails the two-consumer gate and the
  publication is aborted

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

### Requirement: Maintenance honesty and archiving
Each family crate SHALL document a bus-factor plan, maintainer handoff path,
and archiving policy. If a crate or the family can no longer meet its
treadmill SLAs or becomes unmaintained, maintainers MUST stop functional
publishing and archive deliberately with migration notes rather than leaving
stale guidance or unsupported packages behind.

#### Scenario: Governance records maintenance plan
- **WHEN** `crates/gtk-lush/GOVERNANCE.md` is created or updated
- **THEN** it records the family bus-factor plan, maintainer handoff path,
  archiving policy, and the conditions that stop functional releases

#### Scenario: Treadmill failure stops publishing
- **WHEN** a crate cannot meet its gtk-rs, GNOME SDK, MSRV, or maintainer
  coverage commitments
- **THEN** the release checklist blocks functional publication until
  GOVERNANCE.md records a recovery plan or an archive/deprecation decision
  with migration notes

### Requirement: Vision document consistency
`docs/next/gtk-lush.md` SHALL remain the umbrella narrative for the program.
Any change that alters program scope, crate naming, phase ordering, or
principles MUST update the vision document in the same change, and the
OpenSpec specs remain authoritative on conflict.

#### Scenario: Scope change without vision update
- **WHEN** a family change renames a crate or reorders phases without editing
  `docs/next/gtk-lush.md`
- **THEN** review rejects the change until the vision document is updated in
  the same change

### Requirement: Declarative binding normalization remains app-internal
The `normalize-declarative-bindings` follow-up SHALL remain a Phase 0
LushText-internal simplification. It MUST reduce extraction noise by separating
pure UI projections from real workflow side effects, and MUST NOT introduce a
GTK Lush public API, custom view DSL, control-flow owner, state/message system,
component framework, or inter-crate dependency.

#### Scenario: Follow-up proposal stays within Phase 0
- **WHEN** `normalize-declarative-bindings` is proposed or implemented
- **THEN** its artifacts reference GTK Lush governance as the controlling
  program capability
- **AND** the change states that no GTK Lush public crate API, view DSL,
  control-flow owner, or state/message system is introduced

#### Scenario: Safe conversion is not extraction
- **WHEN** a pure projection is converted during this follow-up
- **THEN** the conversion uses existing GTK, Libadwaita, GtkBuilder, GSettings,
  or app-local widget mechanisms
- **AND** any reusable GTK Lush API design is deferred to a later reserved
  extraction change

#### Scenario: Phase boundary uses full gates
- **WHEN** this follow-up reaches completion
- **THEN** LushText's full phase gate set passes, including visual-geometry
  proof whenever visual-sensitive files changed
- **AND** the audit records which candidate handlers remain imperative for
  governance-relevant side-effect or lifecycle reasons

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
