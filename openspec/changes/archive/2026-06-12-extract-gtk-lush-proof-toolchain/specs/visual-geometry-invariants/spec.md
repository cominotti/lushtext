## ADDED Requirements

### Requirement: Rust proof tool preserves invariant migration gates
The visual geometry invariant system SHALL accept `cargo-gtk-proof` as the
authoritative live runner only after a later parity phase proves the existing
invariant semantics: same-session capture, protected-region comparison, masks,
allowed-changing regions, screenshot-derived rendered anchors,
app-vs-rendered diagnostics, final geometry stability waits, warning scans,
skip reasons, and bounded artifacts. During this phase, the Rust tool provides
schema validation, pure PNG/corpus checks, and proof-policy parity while the
Python runner remains authoritative for live visual proof.

#### Scenario: Rust and Python retirement is corpus-gated
- **WHEN** the frozen compatibility corpus is replayed
- **THEN** wrappers remain on the Python live path unless Rust and Python agree
  on every required pass/fail/skip status, summary field, and bounded artifact
  path for the migration corpus
- **AND** the current Rust corpus records compared and failed counts for the
  implemented pure PNG/status fixtures

#### Scenario: Runner migration does not weaken pixel oracle
- **WHEN** a rendered-effect invariant is evaluated by the Rust runner
- **THEN** screenshot-derived pixel anchors remain the pass/fail oracle
- **AND** app-owned geometry is used only to bound crops and explain failures

### Requirement: Visual scenario schema descriptors are versioned and published
Visual geometry scenario manifests and generated artifacts SHALL have
versioned schema descriptors published with the Rust proof tool and linked
from the visual proof documentation. The descriptors MUST cover scenario
manifests, expanded cases, summary files, comparison reports, animation
reports, proof-policy metadata, and result envelopes. Later live-runner work
MUST deepen those descriptors as capture steps, masks, relative anchors, and
animation configuration move fully into Rust.

#### Scenario: Scenario schema validates current manifests
- **WHEN** schema validation runs over `scripts/visual-geometry-scenarios`
- **THEN** every current manifest validates against the supported schema
- **AND** validation failure names the manifest path, schema version, missing
  or malformed field, and stable status

#### Scenario: Artifact schema validates generated summaries
- **WHEN** a visual-geometry run completes
- **THEN** root summaries, case summaries, comparison reports, animation
  reports, and skip summaries validate against their supported schemas
- **AND** unsupported schema versions do not count as verified proof

### Requirement: Animation-frame evidence policy survives migration staging
Animation-frame rendered-effect invariants SHALL retain the current
timestamp-correlated stream proof requirements while Rust migration is staged.
The Rust proof-policy implementation MUST reject summaries missing stream
mode, mapped intermediate PNG frames, per-frame anchor evaluation, skew
metadata, final-settle status, or bounded failure artifacts for
animation-sensitive coverage.

#### Scenario: Final-settle-only Rust summary is rejected
- **WHEN** a Rust-generated summary for an animation-sensitive change contains
  only before/after final-settle captures
- **THEN** proof policy rejects the summary for missing animation-frame
  evidence
- **AND** the rejection status matches the documented proof-policy vocabulary

#### Scenario: Stale frame mapping remains a failure
- **WHEN** an animation frame cannot be matched to a geometry sample inside the
  declared skew bound
- **THEN** the Rust runner cannot use that frame as passing proof
- **AND** the report identifies stale pairing with frame/sample timing detail

### Requirement: Visual summaries keep engine and compatibility metadata
Visual geometry summaries SHALL identify the engine that produced them, the
tool version, schema versions, scenario source, artifact root, proof-policy
fingerprint when available, and compatibility-corpus status when a future run
is used to migrate from Python to Rust.

#### Scenario: Agent can tell which engine produced proof
- **WHEN** a developer or agent reads a visual root summary
- **THEN** it can identify whether the proof came from the Python runner, the
  Rust runner, or a parity run comparing both
- **AND** it can see the schema and tool versions used for policy validation

#### Scenario: Policy rejects stale engine metadata
- **WHEN** a summary lacks required engine or schema metadata after the Rust
  migration point
- **THEN** proof policy rejects it as incomplete
- **AND** the failure points to the summary path and missing metadata class
