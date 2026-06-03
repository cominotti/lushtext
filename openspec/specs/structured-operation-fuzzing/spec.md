# structured-operation-fuzzing Specification

## Purpose
Define LushText's structured operation fuzzing lane so arbitrary bounded bytes
can exercise deterministic editor and service helper operations through existing
fuzz infrastructure without introducing LibAFL or custom framework machinery.

## Requirements
### Requirement: Non-LibAFL Structured Operation Fuzzing
The project SHALL provide structured operation fuzzing for bounded
editor/service operation scripts without adding LibAFL or a custom fuzzing
framework.

#### Scenario: Operation fuzzing uses existing lanes
- **WHEN** structured operation fuzzing is implemented
- **THEN** it uses the existing `cargo-fuzz` infrastructure, the existing
  property-test infrastructure, or another stable ordinary Rust test target
- **AND** it does not add LibAFL, custom schedulers, custom coverage feedback,
  distributed fuzzing launchers, or custom fuzzer state persistence

#### Scenario: Fuzz-only dependencies remain isolated
- **WHEN** default workspace builds, tests, property tests, widget tests,
  benchmark checks, or mutation tests run
- **THEN** operation-fuzzing-only dependencies and sanitizer settings are not
  required unless an operation fuzzing command is invoked explicitly

### Requirement: Operation Script Model
Structured operation fuzzing SHALL map arbitrary bounded bytes into deterministic
operation scripts over non-GTK LushText helper surfaces.

#### Scenario: Arbitrary bytes produce bounded operation scripts
- **WHEN** the operation fuzz target receives arbitrary bytes
- **THEN** those bytes are decoded into a deterministic script with explicit
  limits for operation count, input length, generated string sizes, path sizes,
  file counts, and per-operation work

#### Scenario: Operation scripts exercise service and editor helpers
- **WHEN** an operation script runs
- **THEN** it exercises bounded combinations of deterministic helper surfaces
  such as editor save-formatting, byte decode/redecode, Markdown preprocessing,
  replacement preview generation, session serialization, or draft serialization

### Requirement: Operation Fuzzing Boundaries
Structured operation fuzzing SHALL stay independent of live GTK and desktop
session behavior.

#### Scenario: Operation fuzz target runs
- **WHEN** a structured operation fuzz target or replay test runs
- **THEN** it does not start GTK, construct widgets, use GSettings-backed UI
  state, open file choosers, watch filesystems, use portals, or require a
  compositor

#### Scenario: Tempdir-backed operation is added
- **WHEN** an operation script includes production logic that writes files
- **THEN** the operation is limited to tiny deterministic tempdir-backed inputs
- **AND** it does not involve file watchers, live application sessions, portals,
  file choosers, or user home directories

### Requirement: Operation Fuzz Commands and Seeds
The project SHALL expose documented bounded commands and seed handling for
structured operation fuzzing.

#### Scenario: Developer runs operation fuzz smoke
- **WHEN** a developer runs the documented operation fuzz smoke command
- **THEN** the command runs structured operation fuzzing with explicit run,
  time, input-length, and operation-count bounds

#### Scenario: Operation fuzzing has reviewable seeds
- **WHEN** structured operation fuzzing adds or discovers durable inputs
- **THEN** intentional seeds are stored in a reviewable source location and
  generated crash artifacts remain ignored

### Requirement: Operation Fuzz Failure Promotion
Structured operation fuzzing SHALL make real failures reproducible through
durable seeds or deterministic tests.

#### Scenario: Operation fuzzing finds a failure
- **WHEN** structured operation fuzzing finds a panic or violated invariant
- **THEN** the failure can be reproduced from the failing byte input or corpus
  seed without requiring LibAFL

#### Scenario: Operation fuzz failure is fixed
- **WHEN** a real operation-fuzz-found bug is fixed
- **THEN** the fix includes a minimized corpus seed, deterministic unit/service/
  property regression test, or reviewed rationale explaining why no durable seed
  is appropriate
