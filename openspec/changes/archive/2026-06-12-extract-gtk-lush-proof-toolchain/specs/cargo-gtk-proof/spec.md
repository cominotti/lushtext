## ADDED Requirements

### Requirement: Cargo GTK proof tool exposes stable staged subcommands
The workspace SHALL provide a Rust cargo subcommand named `cargo-gtk-proof`
whose binary can be invoked through cargo as `cargo gtk-proof`. The tool MUST
provide documented subcommands for validating schemas, summarizing artifacts,
replaying the compatibility corpus, and enforcing proof policy. It MUST also
reserve the live-runner command name and report a stable non-coverage status
until live visual parity is implemented in a later phase.

#### Scenario: Help lists proof subcommands
- **WHEN** a developer runs `cargo gtk-proof --help`
- **THEN** the help output lists the run, schema, summarize, corpus, and policy
  subcommands or their documented equivalents
- **AND** it identifies default artifact paths, scenario paths, schema/tool
  metadata, and the current live-runner support boundary

#### Scenario: Reserved live runner does not claim coverage
- **WHEN** a developer runs `cargo gtk-proof run` before Rust live-runner
  parity is recorded
- **THEN** the command exits with a stable unsupported-host or skipped status
- **AND** it does not count any visual invariant as verified

### Requirement: Tool validates versioned scenario and artifact descriptors
`cargo-gtk-proof` SHALL publish and validate versioned machine-readable schema
descriptors for visual scenario manifests, expanded cases, comparison reports,
animation reports, root summaries, proof-policy metadata, and artifact-summary
result envelopes. The tool MUST reject unsupported schema versions and
malformed required fields with stable diagnostics.

#### Scenario: Valid scenario schema is accepted
- **WHEN** a scenario manifest declares a supported schema version and includes
  required fields such as scenario identity, case matrix, capture steps,
  readiness predicates, protected regions, allowed-changing regions, and
  pixel anchors when required
- **THEN** schema validation succeeds
- **AND** the expanded case output records the schema/tool versions used

#### Scenario: Unsupported schema version fails clearly
- **WHEN** a manifest, summary, or policy file declares an unsupported schema
  version
- **THEN** the tool rejects it with a stable unsupported-schema-version status
- **AND** it preserves the offending file path without claiming proof coverage

### Requirement: Tool preserves visual-geometry compatibility boundaries
`cargo-gtk-proof` SHALL implement the pure Rust compatibility surface needed
before the live visual runner can move: versioned schemas, bounded result
envelopes, pure PNG crop/diff/anchor primitives, a frozen corpus replay, and
proof-policy evaluation. The existing Python visual runner SHALL remain the
authoritative live same-session execution path until Rust corpus, live-runner,
animation, and wrapper parity are recorded.

#### Scenario: Pure PNG corpus is replayed
- **WHEN** the compatibility corpus contains exact comparison, masked
  comparison, pixel-anchor, minimap-detector, and drift regression cases
- **THEN** the Rust tool produces the expected pass/fail decision for those
  bounded fixtures
- **AND** the corpus command reports compared and failed counts in a stable
  result envelope

#### Scenario: Rendered pixel oracle remains gated
- **WHEN** a future Rust live runner evaluates native toolkit-rendered,
  CSS-rendered, or compositor-rendered effects
- **THEN** wrapper defaults remain on the Python path until screenshot-derived
  pixel anchors match the current oracle
- **AND** app-owned geometry alone cannot satisfy the future invariant

### Requirement: Tool preserves animation proof policy semantics
`cargo-gtk-proof` SHALL preserve the policy semantics that make animation
proof meaningful before live capture moves to Rust. Rust policy self-tests MUST
reject final-settle-only evidence, missing stream mode, missing mapped
intermediate PNG evidence, stale frame/sample pairing, and missing anchors.

#### Scenario: Intermediate animation evidence remains required by policy
- **WHEN** an animation-frame invariant is declared in a proof summary
- **THEN** Rust policy rejects the summary unless at least one evaluated frame
  maps to an intermediate transition phase inside the declared skew bound
- **AND** final-settle-only evidence is not counted as animation proof

#### Scenario: Animation negative cases are stable
- **WHEN** the Rust proof-policy self-tests run
- **THEN** stale frame/sample pairing, missing stream mode, missing mapped
  intermediate PNG, and final-settle-only fixtures fail with stable statuses
- **AND** no raw image data is embedded in terminal output

### Requirement: Compatibility corpus gates Python retirement
The Rust tool SHALL include a bounded compatibility corpus that blocks any
future move away from the Python runner or policy checker until parity is
proved. This phase MUST cover checked-in status fixtures plus pure PNG
comparison/detector fixtures; later live-runner work MUST extend the corpus to
passing, failing, skipped, unsupported-host, missing-manifest, missing-anchor,
rendered-anchor drift, stale frame/sample pairing, final-settle-only, and
warning-scan cases before wrappers flip.

#### Scenario: Corpus parity blocks wrapper migration
- **WHEN** any compatibility corpus case produces a different Rust pass/fail
  decision, required status, required summary field, or required bounded
  artifact path than the Python oracle
- **THEN** wrapper defaults remain on the Python path
- **AND** the Rust mismatch is reported with the corpus case identity and
  artifact path

#### Scenario: Corpus records replay metadata
- **WHEN** a corpus replay completes
- **THEN** the result envelope records the corpus root, Rust tool metadata,
  schema version, compared count, failed count, and check details
- **AND** this metadata is available to proof-policy checks and CI logs

### Requirement: Proof policy is ported without flipping wrappers
`cargo-gtk-proof` SHALL implement visual proof-policy detection and negative
self-tests while `scripts/check-visual-proof-policy.py` remains the default
local wrapper. The Rust policy MUST preserve visual-sensitive file detection,
required invariant mapping, fingerprint matching, artifact-summary validation,
negative self-tests, and status vocabulary before the wrapper can move.

#### Scenario: Negative self-tests still fail
- **WHEN** the Rust proof-policy self-tests run
- **THEN** cases for final-settle-only evidence, missing stream mode, no mapped
  intermediate PNG, stale frame/sample pairing, missing required anchors,
  rendered pixel drift hidden by app geometry, stale fingerprints, and missing
  summaries all fail with stable statuses

#### Scenario: Sensitive change requires current proof
- **WHEN** a local diff touches visual-sensitive paths
- **THEN** the Rust policy requires a current passing visual-geometry summary
  whose fingerprint matches the diff and whose verified invariant IDs cover the
  required rendered and animation invariants
- **AND** skipped or unsupported-host summaries do not satisfy the policy

### Requirement: Existing LushText visual commands remain stable
Existing LushText script and Makefile entry points SHALL remain functional
while the extracted Rust toolchain matures. Wrappers MUST preserve documented
flags, default artifact locations, result envelopes, and exit-code classes.
This phase MUST document that Python remains the live visual runner and proof
policy wrapper until Rust parity gates pass.

#### Scenario: Visual geometry smoke target keeps live proof stable
- **WHEN** `make visual-geometry-smoke` runs during this phase
- **THEN** it keeps the existing Python live runner and artifact directory
- **AND** Rust schema, corpus, and policy commands document the target shape
  required before future wrapper migration

#### Scenario: Proof policy target has Rust parity evidence
- **WHEN** `cargo gtk-proof policy --self-test` runs
- **THEN** it executes the Rust policy negative cases successfully
- **AND** the default script wrapper is not moved until local policy parity is
  separately recorded
