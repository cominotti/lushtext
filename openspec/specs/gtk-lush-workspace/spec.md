# gtk-lush-workspace Specification

## Purpose
Specify how GTK Lush family crates live inside the LushText workspace before
graduation, including crate layout, scaffolding, CI lanes, name reservation, and
the governance document that keeps the family independently adoptable.

## Requirements
### Requirement: Family workspace layout
The repository SHALL host the family under `crates/gtk-lush/<member>` with
package names `gtk-lush-<member>`, wired into the root Cargo workspace,
cargo-hakari (`workspace-hack`), the nextest configuration, the curated
workspace lint table, and cargo-deny. Family members SHALL NOT depend on
LushText crates or on each other; LushText consumes family crates via
workspace path dependencies until the graduation phase.

#### Scenario: Workspace integration
- **WHEN** `cargo hakari generate`, `make check`, and the non-widget test lane
  run after adding a family crate
- **THEN** all pass with the family crate built, linted, and tested as a
  workspace member

#### Scenario: Dependency direction enforced
- **WHEN** a family crate adds a dependency on `lushtext-core` or another
  `gtk-lush-*` crate
- **THEN** the workspace policy check fails the build until the dependency is
  removed

### Requirement: Per-crate scaffolding
Each family crate SHALL ship: `src/lib.rs` with crate-level docs stating the
constitution sentence and the discriminating test, `README.md` seeded from the
corresponding rules section, `CHANGELOG.md`, at least one single-crate
adoption example under `examples/`, doctested public items, SPDX headers,
declared `rust-version`, and dual `MIT OR Apache-2.0` license files.

#### Scenario: Scaffolding completeness check
- **WHEN** the family scaffolding check runs over `crates/gtk-lush/`
- **THEN** it fails if any member lacks README, CHANGELOG, at least one
  adoption example under `examples/`, license metadata, SPDX headers, or
  `rust-version`

#### Scenario: Named examples are accepted
- **WHEN** a crate ships examples named for the crate behavior such as
  `signals_lifetime.rs`, `settle_timers.rs`, `tasks_worker.rs`,
  `viewport_observer.rs`, or `widgets_geometry.rs`
- **THEN** workspace policy accepts those examples as the adoption proof
- **AND** no check requires every crate to use the literal filename
  `examples/standalone.rs`

### Requirement: CI lanes for the family
CI SHALL build, lint, doc, and test family crates in the existing GNOME
container matrix, and SHALL add: an MSRV verification job that builds the
family at the declared `rust-version`, and a semver/public-API job
(`cargo-semver-checks` plus a public-API snapshot) that runs in advisory mode
until the first real publication and blocking mode afterward. New CI tooling
versions MUST be pinned alongside the existing tool pins.

#### Scenario: MSRV regression
- **WHEN** a family crate uses a language or library feature newer than its
  declared `rust-version`
- **THEN** the MSRV verification job fails

#### Scenario: Public API drift after publication
- **WHEN** a change alters a published family crate's public API without a
  matching version bump
- **THEN** the semver job fails in blocking mode

### Requirement: Name reservation
Crates.io reservations of `gtk-lush-*` names SHALL be limited to initial
`0.0.0` placeholders until the publishing gates pass. Placeholder releases SHALL
contain no functional code, SHALL document their placeholder status, and
SHALL point at the umbrella vision. In-tree pre-publication workspace crates
MAY expose functional APIs at version `0.0.0` for LushText migration and proof,
but their README and CHANGELOG MUST clearly state that they are not
publication-ready and are not yet covered by the Phase 5 publishing gate.

#### Scenario: Placeholder content audit
- **WHEN** a `0.0.0` placeholder is prepared
- **THEN** the release contains only metadata, a README referencing
  `docs/next/gtk-lush.md`, and no public API items

#### Scenario: Functional in-tree crate is not treated as published
- **WHEN** an in-tree GTK Lush crate exposes functional APIs before Phase 5
- **THEN** its docs and CHANGELOG identify it as pre-publication `0.0.0`
  workspace API
- **AND** release automation does not publish it as a functional crate until
  the publishing gates pass

### Requirement: Governance document
`crates/gtk-lush/GOVERNANCE.md` SHALL exist and record: the constitution
checklist used in review, the exception register, treadmill SLAs, publishing
gates, the bus-factor/archiving policy, and the repo-graduation plan
(in-tree until the publishing gates pass, then a dedicated repository with
history preserved and LushText pinning published versions).

#### Scenario: Exception is recorded
- **WHEN** a constitution exception is approved for a family crate
- **THEN** GOVERNANCE.md gains a dated entry describing the exception, its
  invariant, and its sunset condition before the change merges

### Requirement: Runtime geometry crates integrate with workspace policy
The workspace SHALL integrate `gtk-lush-tasks`, `gtk-lush-viewport`, and
`gtk-lush-widgets` as first-class family members. They MUST be wired into the
root Cargo workspace, cargo-hakari, nextest, cargo-deny, MSRV verification,
semver/public-API advisory lanes, family scaffolding checks, and documentation
policy before LushText consumes them.

#### Scenario: Phase 3 crates build in family lanes
- **WHEN** the family build, lint, doc, doctest, example, MSRV, and advisory
  API lanes run after adding the Phase 3 crates
- **THEN** `gtk-lush-tasks`, `gtk-lush-viewport`, and `gtk-lush-widgets` are
  included in the same policy surface as the existing family crates

#### Scenario: LushText consumes path dependencies only
- **WHEN** LushText migrates to a Phase 3 GTK Lush crate before publication
- **THEN** the dependency is a workspace path dependency
- **AND** no crates.io functional publication is required to build or test the
  app

#### Scenario: README no longer claims every crate is placeholder-only
- **WHEN** functional in-tree Phase 2 or Phase 3 APIs exist under
  `crates/gtk-lush/`
- **THEN** the family README describes which crates expose in-tree APIs and
  which crates, if any, remain reservation placeholders
- **AND** it does not state that all crates expose no public API
