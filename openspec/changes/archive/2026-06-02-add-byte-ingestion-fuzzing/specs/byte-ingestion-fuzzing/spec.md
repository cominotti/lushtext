## ADDED Requirements

### Requirement: Cargo Fuzz Project
The project SHALL provide a dedicated `cargo-fuzz` fuzzing project for
byte-ingestion robustness testing.

#### Scenario: Developer lists fuzz targets
- **WHEN** a developer runs the documented fuzz-target listing command
- **THEN** the configured byte-ingestion fuzz targets are listed without running
  normal application tests

#### Scenario: Fuzz dependencies stay isolated
- **WHEN** default workspace builds, tests, property tests, or mutation tests run
- **THEN** fuzz-only dependencies and sanitizer settings are not required unless
  a fuzz command is explicitly invoked

### Requirement: Initial Byte-Ingestion Fuzz Targets
The project SHALL provide initial fuzz targets for deterministic byte decoding
and Markdown preprocessing paths that can be exercised without GTK widgets.

#### Scenario: Editor bytes are fuzzed
- **WHEN** arbitrary bounded byte slices are provided to the editor-ingestion fuzz
  target
- **THEN** the target exercises the production decoding, encoding-state, line
  ending, and file-health classification logic without panicking

#### Scenario: Markdown preprocessing is fuzzed
- **WHEN** arbitrary bounded text is provided to the Markdown preprocessing fuzz
  target
- **THEN** the target exercises production Markdown preprocessing and parser
  setup logic without constructing GTK widgets or panicking

#### Scenario: Fuzz targets stay GTK-free
- **WHEN** a fuzz target is added or changed
- **THEN** it calls deterministic model, service, or helper logic only
- **AND** it does not start GTK, create widgets, access file choosers, watch the
  filesystem, use portals, or require a compositor

### Requirement: Bounded Fuzz Commands
The project SHALL expose documented local fuzz commands that keep routine runs
bounded and explicit.

#### Scenario: Developer runs fuzz smoke
- **WHEN** a developer runs the fuzz smoke command
- **THEN** the command runs the selected fuzz targets with explicit time and/or
  input-size bounds

#### Scenario: Default validation excludes fuzzing
- **WHEN** default test, property, widget, benchmark, or mutation validation runs
- **THEN** fuzzing is not run unless a fuzz command or fuzz workflow is invoked
  explicitly

### Requirement: Fuzz Crash Handling
The project SHALL document how fuzz crashes are reproduced, minimized, and
converted into durable regression coverage.

#### Scenario: Fuzz target finds a crash
- **WHEN** a fuzz run produces a crash artifact
- **THEN** the documented workflow explains how to reproduce it and minimize it
  with the matching cargo-fuzz target

#### Scenario: Crash fix is reviewed
- **WHEN** a real fuzz-found crash is fixed
- **THEN** the fix includes a minimized corpus seed, deterministic regression
  test, or documented reason why a durable seed is not appropriate

### Requirement: Fuzz Documentation and CI Policy
The project SHALL document fuzzing scope, commands, artifact handling, and CI
policy alongside the other test lanes.

#### Scenario: Developer reads fuzzing documentation
- **WHEN** a developer opens the fuzzing documentation or build rules
- **THEN** they can see which fuzz targets exist, how to run bounded smoke
  checks, how to run longer manual or scheduled fuzz jobs, and why fuzzing stays
  separate from property and mutation testing
