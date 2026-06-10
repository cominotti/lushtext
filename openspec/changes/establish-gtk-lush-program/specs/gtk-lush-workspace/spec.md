## ADDED Requirements

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
corresponding rules section, `CHANGELOG.md`, `examples/standalone.rs`,
doctested public items, SPDX headers, declared `rust-version`, and dual
`MIT OR Apache-2.0` license files.

#### Scenario: Scaffolding completeness check
- **WHEN** the family scaffolding check runs over `crates/gtk-lush/`
- **THEN** it fails if any member lacks README, CHANGELOG, standalone example,
  license metadata, SPDX headers, or `rust-version`

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
Crates.io reservations of `gtk-lush-*` names SHALL be limited to `0.0.x`
placeholders until the publishing gates pass. Placeholder releases SHALL
contain no functional code, SHALL document their placeholder status, and
SHALL point at the umbrella vision.

#### Scenario: Placeholder content audit
- **WHEN** a `0.0.x` placeholder is prepared
- **THEN** the release contains only metadata, a README referencing
  `docs/next/gtk-lush.md`, and no public API items

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
