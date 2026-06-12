## ADDED Requirements

### Requirement: Rust live runner preserves same-session pixel oracle
The visual invariant system SHALL require the Rust live runner to preserve the
same-session screenshot-derived pixel oracle before it marks rendered-effect
coverage as verified. The runner MUST compare protected regions only across
captures from the same process, compositor session, renderer, theme, scale
factor, fixture, font configuration, and window size, and it MUST use declared
masks and pixel anchors exactly as the Python runner does.

#### Scenario: Protected regions remain exact
- **WHEN** the Rust runner compares a protected header, status, or chrome crop
  across paired captures
- **THEN** all unmasked pixels in that protected region match exactly
- **AND** any nonzero difference fails the case with crop, mask, detector, and
  comparison artifacts

#### Scenario: Rendered anchor drift fails despite stable app geometry
- **WHEN** Automation1 geometry reports a stable rendered-effect crop
- **AND** screenshot-derived anchors for the native, CSS, or compositor-rendered
  effect drift outside the manifest tolerance
- **THEN** the Rust runner fails the invariant
- **AND** the report records both app-owned geometry and screenshot-derived
  anchor rows

#### Scenario: Cross-session evidence cannot prove exact equality
- **WHEN** Rust receives screenshots or summaries from different launches or
  compositor sessions
- **THEN** it refuses to count exact protected-region equality or
  rendered-anchor proof from those artifacts
- **AND** it may only report bounded diagnostics or unsupported evidence

### Requirement: Rust live runner preserves warning-scan proof semantics
The visual invariant system SHALL require Rust live visual runs to preserve the
warning-scan behavior used by current visual smoke. Unexpected GTK,
Libadwaita, GDK, renderer, accessibility, application, capture, or proof-tool
warnings MUST fail the affected case or run with bounded logs and a stable
warning-scan status.

#### Scenario: Unexpected warning fails proof
- **WHEN** a Rust live visual case emits an unexpected toolkit, renderer,
  accessibility, application, capture, or proof-tool warning
- **THEN** the case reports a warning-scan failure
- **AND** the root summary does not list that case's invariants as verified

#### Scenario: Warning evidence is bounded
- **WHEN** warning scan fails
- **THEN** the artifacts preserve bounded log excerpts, full log paths when
  available, warning classes, and the case identity
- **AND** terminal output does not embed unbounded logs

## MODIFIED Requirements

### Requirement: Rust proof tool preserves invariant migration gates
The visual geometry invariant system SHALL accept `cargo-gtk-proof` as the
authoritative live runner only after this parity phase proves the existing
invariant semantics: same-session capture, protected-region comparison, masks,
allowed-changing regions, screenshot-derived rendered anchors,
app-vs-rendered diagnostics, final geometry stability waits, warning scans,
skip reasons, bounded artifacts, and animation-frame stream proof. After that
evidence is recorded, the Rust runner SHALL become the default live visual
proof engine, and the Python runner SHALL remain only as an explicit oracle or
diagnostic compatibility path.

#### Scenario: Rust and Python retirement is corpus-gated
- **WHEN** the frozen compatibility and parity corpus is replayed
- **THEN** wrappers remain on the Python live path unless Rust and Python agree
  on every required pass/fail/skip status, summary field, verified invariant
  ID, exit class, warning-scan result, and bounded artifact path for the
  migration corpus
- **AND** the Rust corpus records compared and failed counts for pure
  PNG/status fixtures and representative live-result fixtures

#### Scenario: Runner migration does not weaken pixel oracle
- **WHEN** a rendered-effect invariant is evaluated by the Rust runner
- **THEN** screenshot-derived pixel anchors remain the pass/fail oracle
- **AND** app-owned geometry is used only to bound crops and explain failures

#### Scenario: Rust engine becomes authoritative after parity
- **WHEN** corpus, live-runner, animation, policy, and wrapper parity evidence
  is complete
- **THEN** default visual proof summaries identify the Rust engine as
  authoritative
- **AND** Python-oracle evidence is labeled as diagnostic or compatibility
  evidence unless an explicit parity command is running

### Requirement: Animation-frame evidence policy survives migration staging
Animation-frame rendered-effect invariants SHALL retain the current
timestamp-correlated stream proof requirements while Rust becomes the live
runner. The Rust runner and proof-policy implementation MUST reject summaries
missing stream mode, mapped intermediate PNG frames, per-frame anchor
evaluation, skew metadata, final-settle status, or bounded failure artifacts
for animation-sensitive coverage.

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

#### Scenario: Intermediate Rust frames prove animation
- **WHEN** the Rust runner executes a scenario that declares animation-frame
  proof
- **THEN** at least one evaluated PNG frame maps to an intermediate transition
  phase within the declared skew bound
- **AND** each required rendered anchor is evaluated for the mapped frame
- **AND** missing or drifting anchors fail the animation invariant

### Requirement: Visual summaries keep engine and compatibility metadata
Visual geometry summaries SHALL identify the engine that produced them, the
tool version, schema versions, scenario source, artifact root, proof-policy
fingerprint when available, compatibility-corpus status, and Python-oracle
comparison status when a parity run is used to migrate from Python to Rust.
After Rust becomes authoritative, summaries without required engine and schema
metadata MUST NOT satisfy visual proof policy.

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

#### Scenario: Parity metadata explains wrapper migration
- **WHEN** wrapper defaults are changed to Rust
- **THEN** the proof artifacts or governance notes identify the parity corpus
  run that justified the transition
- **AND** future summaries identify Python output as oracle or diagnostic
  compatibility evidence rather than the default proof authority
