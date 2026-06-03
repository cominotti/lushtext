## ADDED Requirements

### Requirement: Stable Corpus Replay Command
The project SHALL provide an explicit stable Rust command that replays committed
fuzz corpus seeds through deterministic non-GTK helper surfaces without requiring
`cargo-fuzz`, nightly Rust, sanitizer runtime, or C/C++ compiler setup.

#### Scenario: Developer replays committed corpus seeds
- **WHEN** a developer runs the documented corpus replay command on stable Rust
- **THEN** the command reads the committed seeds under the configured
  `fuzz/corpus/**` directories
- **AND** each seed is replayed through its matching deterministic helper surface

#### Scenario: Replay does not require fuzz tooling
- **WHEN** the corpus replay command runs
- **THEN** it does not compile or execute `libfuzzer-sys`
- **AND** it does not invoke `cargo-fuzz`, sanitizer flags, nightly-only
  compiler features, or a C/C++ compiler

### Requirement: Replay Scope and Isolation
The stable corpus replay lane SHALL be deterministic, read-only with respect to
the committed corpus, and scoped to non-GTK helper logic.

#### Scenario: Replay leaves corpus and artifacts unchanged
- **WHEN** the corpus replay command processes committed seeds
- **THEN** it does not mutate files under `fuzz/corpus/**`
- **AND** it does not write crash artifacts, coverage data, or generated corpus
  growth under `fuzz/artifacts/**`, `fuzz/coverage/**`, or `fuzz/corpus/**`

#### Scenario: Replay remains GTK-free
- **WHEN** a seed is replayed
- **THEN** the replay path does not start GTK, construct widgets, access
  GSettings-backed UI state, open file choosers, watch the filesystem, use
  portals, or require a compositor

### Requirement: Replay Diagnostics
The stable corpus replay lane SHALL report enough information for maintainers to
reproduce and promote failing seeds.

#### Scenario: Corpus seed fails replay
- **WHEN** a committed corpus seed panics or violates a replay assertion
- **THEN** the failure output identifies the logical target and corpus seed path

#### Scenario: Replay failure is fixed
- **WHEN** a replay failure represents a real product bug
- **THEN** the fix keeps or adds a durable corpus seed, deterministic regression
  test, or reviewed rationale explaining why no durable seed is appropriate

### Requirement: Replay Lane Policy
The project SHALL document how stable corpus replay relates to default tests,
`cargo-fuzz`, property tests, widget tests, and mutation testing.

#### Scenario: Default validation runs
- **WHEN** default test, property, widget, benchmark, or mutation validation runs
- **THEN** stable corpus replay is not run unless a documented replay command or
  explicit workflow invokes it

#### Scenario: Developer reads fuzzing documentation
- **WHEN** a developer opens the fuzzing documentation or build rules
- **THEN** they can see how to run stable corpus replay and how it differs from
  coverage-guided fuzz discovery
