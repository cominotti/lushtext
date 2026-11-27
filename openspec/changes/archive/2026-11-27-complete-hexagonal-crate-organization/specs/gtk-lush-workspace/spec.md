## MODIFIED Requirements

### Requirement: Family workspace layout
The repository SHALL host family crates at `crates/gtk-lush/<member>` (`gtk-lush-<member>`), wired into the workspace, `workspace-hack`, nextest, workspace lints, and cargo-deny. Family crates MUST NOT depend on LushText crates or each other; LushText consumes them via workspace path dependencies.

#### Scenario: Workspace integration
- **WHEN** `cargo hakari generate`, `make check`, and the non-widget test lane
  run after adding a family crate
- **THEN** all pass with the family crate built, linted, and tested as a
  workspace member

#### Scenario: Dependency direction enforced
- **WHEN** a family crate adds a dependency on `lushtext-ui`, `lushtext-services`, `lushtext-model`, or another
  `gtk-lush-*` crate
- **THEN** the workspace policy check fails the build until the dependency is
  removed

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
- **AND** it does not import `lushtext`, `lushtext-ui`, `lushtext-services`, `lushtext-model`, or another family
  crate

#### Scenario: Fixture check is deterministic
- **WHEN** the fixture verification target runs locally or in CI
- **THEN** it uses committed fixture files and path dependencies only
- **AND** it does not require crates.io publication, network access, or an
  external project checkout
