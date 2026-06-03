## ADDED Requirements

### Requirement: Stable Fuzz Replay CI
The project SHALL run stable fuzz corpus replay in ordinary push and
pull-request CI so feature-gated replay tests and committed corpus seeds cannot
regress unnoticed.

#### Scenario: Pull request validates committed fuzz seeds
- **WHEN** GitHub Actions runs the normal CI workflow for a pull request or push
- **THEN** a CI job or step runs `make fuzz-corpus-replay`
- **AND** the workflow fails if any committed fuzz corpus seed panics or violates
  the replay assertions

#### Scenario: Replay CI stays on stable tooling
- **WHEN** the stable fuzz replay CI job runs
- **THEN** it uses stable Rust and the existing Fedora build environment
- **AND** it does not require nightly Rust, `cargo-fuzz`, libFuzzer sanitizer
  runtime, or a C/C++ compiler solely to replay committed seeds

#### Scenario: Feature-gated replay is explicit
- **WHEN** default workspace test commands skip tests that require the `fuzzing`
  feature
- **THEN** CI still invokes the documented replay command that enables the
  feature-gated replay harness

### Requirement: Scheduled Fuzz Smoke
The project SHALL provide a bounded scheduled/manual fuzz smoke lane for
coverage-guided exploration outside default pull-request validation.

#### Scenario: Maintainer starts fuzz smoke manually
- **WHEN** a maintainer dispatches the fuzz smoke workflow
- **THEN** the workflow installs the required fuzz tooling and runs the
  documented bounded fuzz smoke command

#### Scenario: Fuzz smoke runs on schedule
- **WHEN** the scheduled fuzz smoke workflow runs
- **THEN** it executes bounded coverage-guided fuzzing with explicit time,
  input-size, and target-selection limits

#### Scenario: Pull request CI remains bounded
- **WHEN** ordinary pull-request CI runs
- **THEN** coverage-guided `cargo-fuzz` smoke is not required unless explicitly
  requested by a separate manual or scheduled workflow

### Requirement: Corrupt Persistence JSON Fuzz Coverage
Structured operation fuzzing SHALL exercise corrupt raw-byte JSON decode
boundaries for persisted session and draft data.

#### Scenario: Corrupt session bytes are decoded
- **WHEN** the structured operation fuzz target receives arbitrary bounded bytes
- **THEN** some operation paths feed raw bytes into
  `serde_json::from_slice::<SessionData>(...)`
- **AND** malformed, truncated, or random bytes are allowed to return errors
  without panicking

#### Scenario: Corrupt draft manifest bytes are decoded
- **WHEN** the structured operation fuzz target receives arbitrary bounded bytes
- **THEN** some operation paths feed raw bytes into
  `serde_json::from_slice::<DraftManifest>(...)`
- **AND** malformed, truncated, or random bytes are allowed to return errors
  without panicking

#### Scenario: Durable corrupt-byte seeds exist
- **WHEN** persistence decode operations are added to operation fuzzing
- **THEN** the committed operation corpus includes reviewable seeds for invalid,
  truncated, and minimally valid persistence JSON inputs

### Requirement: Markdown Byte/Text Boundary Documentation
The project SHALL document that Markdown preprocessing fuzzing is text-level
coverage and that invalid UTF-8 byte ingestion is covered by the editor
byte-ingestion target.

#### Scenario: Developer reads fuzzing documentation
- **WHEN** a developer reads the fuzzing documentation
- **THEN** it explains that Markdown preprocessing receives lossy UTF-8 text
  before parser setup
- **AND** it identifies the editor byte-ingestion fuzz target as the raw invalid
  UTF-8 boundary

#### Scenario: Fuzz targets are reviewed
- **WHEN** a fuzz target for Markdown preprocessing is changed
- **THEN** reviewers can distinguish text preprocessing invariants from raw
  byte-decoding invariants using the documented boundary

### Requirement: Fuzz Command Discoverability
The project SHALL list fuzz replay and smoke commands in the standard Makefile
help output.

#### Scenario: Developer asks for available make targets
- **WHEN** a developer runs `make help`
- **THEN** the output includes the stable corpus replay command
- **AND** the output includes the bounded fuzz smoke commands
- **AND** the output makes clear these commands are explicit fuzz lanes rather
  than default test aliases
