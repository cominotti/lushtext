## ADDED Requirements

### Requirement: Adoption lab is a workspace consumer outside the family
The workspace SHALL host the GTK Lush adoption lab outside
`crates/gtk-lush/`. The lab MUST be integrated as a normal workspace build and
test target, MAY depend on multiple GTK Lush family crates, and MUST NOT be
checked as a GTK Lush family crate. Family policy tooling MUST continue to
apply leaf-crate rules only to crates under `crates/gtk-lush/`.

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
The workspace SHALL document where adoption-lab code, stock fixtures,
adoption matrices, timed journals, unrelated-project notes, and generated
artifacts live. Committed adoption evidence MUST be reviewable and bounded;
generated screenshots, large logs, temporary external checkouts, and proof
artifacts MUST remain ignored unless curated as small fixtures.

#### Scenario: Evidence directory contract is documented
- **WHEN** a developer reads the GTK Lush adoption documentation or roadmap
- **THEN** it identifies the maintained lab, stock fixture directory, matrix,
  journal location, external-spike note location, and generated artifact roots
- **AND** it states which artifacts are committed and which are generated

#### Scenario: Unbounded artifacts stay out of git
- **WHEN** adoption checks generate screenshots, frame streams, logs,
  external project checkouts, or runtime artifacts
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
