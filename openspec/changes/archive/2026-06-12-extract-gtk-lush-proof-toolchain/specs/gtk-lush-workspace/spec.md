## ADDED Requirements

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
