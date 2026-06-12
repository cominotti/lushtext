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

### Requirement: Proof family crates integrate with workspace policy
The workspace SHALL integrate `gtk-lush-proof-harness` and
`gtk-lush-proof-spine` as first-class GTK Lush family members. They MUST be
wired into the root Cargo workspace, workspace dependencies, cargo-hakari,
nextest, cargo-deny, MSRV verification, semver/public-API advisory lanes,
family scaffolding checks, doctests, examples, README/CHANGELOG policy, and
documentation policy before LushText consumes them.

#### Scenario: Phase 4 proof crates build in family lanes
- **WHEN** the family build, lint, doc, doctest, example, MSRV, policy, and
  advisory API lanes run after adding the Phase 4 proof crates
- **THEN** `gtk-lush-proof-harness` and `gtk-lush-proof-spine` are included in
  the same family policy surface as the existing GTK Lush crates
- **AND** no family crate depends on LushText or on another family crate

#### Scenario: LushText consumes proof crates through workspace paths
- **WHEN** LushText migrates widget tests or Automation1 adapters to the Phase
  4 proof crates before publication
- **THEN** the dependency is a workspace path dependency
- **AND** no crates.io functional publication is required to build or test the
  app

### Requirement: Cargo proof tool is a workspace tool outside the family
The workspace SHALL host `cargo-gtk-proof` as a Rust workspace tool outside
`crates/gtk-lush/`. The tool MAY depend on GTK Lush family crates as a normal
workspace consumer, but it MUST NOT be treated as a family leaf crate and MUST
NOT require an exception to the `gtk-lush-<member>` package-name policy.

#### Scenario: Tool package is not checked as a family member
- **WHEN** `make check-gtk-lush-policy` scans `crates/gtk-lush/`
- **THEN** it does not require a `crates/gtk-lush/cargo-gtk-proof` directory
- **AND** it does not apply family README, license, leaf-crate, or
  package-name checks to `cargo-gtk-proof`

#### Scenario: Tool still participates in workspace checks
- **WHEN** `make check`, `cargo clippy --workspace --all-targets`, and
  `cargo doc --workspace --no-deps` run
- **THEN** `cargo-gtk-proof` builds, lints, tests, and documents as a workspace
  member
- **AND** dependency policy gates cover any new crates introduced by the tool

### Requirement: GTK Lush package lists include proof crates
The workspace SHALL update all curated GTK Lush package and crate lists to
include the proof family crates. This includes Makefile variables, CI jobs,
policy scripts, cargo-hakari inputs, cargo-deny coverage, README family lists,
public API advisory output, and any generated documentation or status tables.

#### Scenario: Makefile family variables are complete
- **WHEN** `make gtk-lush-doctests`, `make gtk-lush-examples`,
  `make gtk-lush-msrv`, and `make gtk-lush-api-advisory` run
- **THEN** the proof harness and proof spine crates are included by the same
  variables as the existing family crates
- **AND** the targets do not require maintainers to remember a second ad hoc
  proof-crate list

#### Scenario: Family README distinguishes crates and tools
- **WHEN** a developer reads `crates/gtk-lush/README.md`
- **THEN** it lists `gtk-lush-proof-harness` and `gtk-lush-proof-spine` as
  functional in-tree `0.0.0` family APIs
- **AND** it describes `cargo-gtk-proof` as a workspace proof tool rather than
  a family crate

### Requirement: Proof toolchain fixtures stay bounded in the workspace
The workspace SHALL store compatibility fixtures, schemas, generated expected
summaries, and proof corpus data in bounded, reviewable workspace locations.
The workspace MUST NOT commit large live-smoke dumps, unbounded logs, private
user content, or generated artifacts that can be reproduced cheaply during
tests.

#### Scenario: Corpus fixtures are reviewable
- **WHEN** the proof compatibility corpus is added
- **THEN** fixture files are small enough for code review, named by scenario or
  failure class, and documented with their expected status
- **AND** large screenshots, frame streams, and logs are either minimized,
  generated during tests, or excluded from version control

#### Scenario: Privacy-sensitive artifacts stay out of git
- **WHEN** proof tooling writes screenshots, crop diffs, warning logs, runtime
  snapshots, or smoke summaries under `build/` or another artifact directory
- **THEN** those outputs remain ignored/generated artifacts unless explicitly
  curated as bounded fixtures
- **AND** no fixture contains user document text, note bodies, draft bodies, or
  private persistence identifiers

### Requirement: Adoption lab is a workspace consumer outside the family
The workspace SHALL host the GTK Lush adoption lab outside `crates/gtk-lush/`.
The lab MUST be integrated as a normal workspace build and test target, MAY
depend on multiple GTK Lush family crates, and MUST NOT be checked as a GTK
Lush family crate. Family policy tooling MUST continue to apply leaf-crate
rules only to crates under `crates/gtk-lush/`.

#### Scenario: Lab is excluded from family leaf policy
- **WHEN** `make check-gtk-lush-policy` runs after the lab is added
- **THEN** the lab is not required to follow family package-name,
  no-family-dependency, README, license, or publication scaffolding rules
- **AND** every crate under `crates/gtk-lush/` remains subject to those rules

#### Scenario: Lab participates in workspace checks
- **WHEN** `make check`, cargo workspace checks, and the adoption-lab target
  run
- **THEN** the lab builds and tests as a workspace consumer
- **AND** dependency policy covers any new dependencies introduced for the lab

### Requirement: Stock adoption fixtures stay independent of the workspace app
The workspace SHALL store stock gtk-rs starter-style adoption fixtures in a
bounded reviewable location outside the GTK Lush family crates. Each stock
fixture MUST adopt exactly one GTK Lush crate through a path dependency and
MUST NOT rely on LushText application crates, generated LushText resources,
LushText GSettings schemas, or another GTK Lush family crate.

#### Scenario: Fixture imports one family crate
- **WHEN** the stock adoption fixture check runs
- **THEN** each checked fixture declares exactly one `gtk-lush-*` path
  dependency
- **AND** it does not import `lushtext`, `lushtext-core`, or another family
  crate

#### Scenario: Fixture check is deterministic
- **WHEN** the fixture verification target runs locally or in CI
- **THEN** it uses committed fixture files and path dependencies only
- **AND** it does not require crates.io publication, network access, or an
  external project checkout

### Requirement: Adoption evidence locations are bounded and documented
The workspace SHALL document where adoption-lab code, stock fixtures, adoption
matrices, timed journals, unrelated-project notes, and generated artifacts
live. Committed adoption evidence MUST be reviewable and bounded; generated
screenshots, large logs, temporary external checkouts, and proof artifacts
MUST remain ignored unless curated as small fixtures.

#### Scenario: Evidence directory contract is documented
- **WHEN** a developer reads the GTK Lush adoption documentation or roadmap
- **THEN** it identifies the maintained lab, stock fixture directory, matrix,
  journal location, external-spike note location, and generated artifact roots
- **AND** it states which artifacts are committed and which are generated

#### Scenario: Unbounded artifacts stay out of git
- **WHEN** adoption checks generate screenshots, frame streams, logs, external
  project checkouts, or runtime artifacts
- **THEN** those outputs live under ignored build or temporary directories
- **AND** committed evidence contains only bounded summaries, fixtures, or
  sanitized excerpts

### Requirement: Workspace checks include adoption validation targets
The workspace SHALL expose deterministic adoption-validation targets for the
maintained lab, stock fixture checks, matrix completeness, and any adoption
documentation drift checks introduced by the phase. These targets MUST be
reachable from the phase verification ladder and MUST NOT require functional
GTK Lush publication.

#### Scenario: Adoption targets are discoverable
- **WHEN** a developer runs `make help` or reads build documentation after the
  phase
- **THEN** the adoption-lab, stock-fixture, and matrix validation targets are
  listed or referenced with their purpose
- **AND** the targets distinguish permanent in-tree checks from the one-time
  unrelated-project spike evidence

#### Scenario: Verification ladder includes adoption checks
- **WHEN** the adoption-validation phase is ready to archive
- **THEN** verification includes `make check-gtk-lush-policy`,
  `make gtk-lush-doctests`, `make gtk-lush-examples`,
  `make gtk-lush-msrv`, `make gtk-lush-api-advisory`, adoption-lab checks,
  stock-fixture checks, matrix checks, and `make check`
- **AND** visual-sensitive work also includes the required visual-geometry
  proof lane
