## ADDED Requirements

### Requirement: Scheduled visual smoke identifies the Rust proof engine
The scheduled and manually dispatched end-user visual-geometry smoke lane SHALL
identify the proof engine, tool version, schema version, scenario source, and
parity status for each run. After parity is recorded, scheduled smoke MUST
default to the Rust live runner while preserving artifact upload roots and
unsupported-host semantics.

#### Scenario: Scheduled smoke reports Rust engine metadata
- **WHEN** the scheduled or manually dispatched visual-geometry lane runs after
  parity is recorded
- **THEN** the uploaded summary identifies `cargo-gtk-proof` or equivalent Rust
  engine metadata, schema versions, and scenario source
- **AND** maintainers can distinguish Rust proof from Python oracle or
  diagnostic compatibility artifacts

#### Scenario: Scheduled smoke keeps artifact upload compatibility
- **WHEN** the visual-geometry lane passes, fails, or skips
- **THEN** the workflow uploads the documented visual-geometry artifact root
- **AND** the root contains bounded summaries, warning scans, screenshots or
  frames when produced, and skip or failure reasons

## MODIFIED Requirements

### Requirement: End-user visual geometry smoke uses the extracted toolchain
The end-user visual-geometry smoke lane SHALL continue to run the same
LushText visual invariants while the extracted Rust proof toolchain becomes
the default live execution path. After parity is recorded, `cargo gtk-proof
run` SHALL own schema validation, corpus evidence, live same-session capture,
pixel-anchor comparison, animation-frame proof, warning scans, policy-ready
summaries, and bounded artifact generation. The scheduled/manual smoke
workflow, Makefile target, artifact directory, pass/fail/skip semantics, and
reviewable evidence layout MUST remain stable or provide documented
compatibility aliases.

#### Scenario: Scheduled visual-geometry lane still uploads evidence
- **WHEN** the scheduled or manually dispatched end-user smoke workflow runs
  the visual-geometry matrix lane after parity is recorded
- **THEN** it executes the stable visual-geometry Makefile or script wrapper
  backed by `cargo gtk-proof run`
- **AND** documentation identifies which evidence is produced by the Rust live
  runner and which optional diagnostics are produced by the Python oracle path
- **AND** it uploads the same bounded artifact root expected by existing
  maintainers and agents

#### Scenario: Unsupported host does not count as coverage
- **WHEN** the smoke host lacks required compositor, screenshot, D-Bus,
  PipeWire, GStreamer, image tooling, or LushText binary support
- **THEN** the lane reports a distinct skipped or unsupported status with
  bounded diagnostics
- **AND** proof policy and smoke summaries do not count the skipped invariant
  as verified

#### Scenario: Local command shape remains stable
- **WHEN** a developer runs `make visual-geometry-smoke` with existing artifact
  directory flags
- **THEN** the command remains accepted
- **AND** the default implementation invokes the Rust runner after parity
- **AND** Python diagnostics require an explicit documented oracle or
  compatibility flag

### Requirement: Smoke artifacts remain bounded and schema-valid
Visual smoke artifacts produced through the extracted Rust toolchain SHALL
remain bounded, privacy-preserving, and schema-valid where Rust descriptors
cover the artifact type. They MUST include enough evidence for review: root
summary, per-case summaries, scenario manifests, screenshots or generated
fixtures, crop reports, animation reports when required, warning scans,
environment details, skip/failure reasons, engine metadata, and parity
metadata when the run compares Rust against the Python oracle.

#### Scenario: Passing smoke has reviewable proof
- **WHEN** the visual-geometry smoke lane passes
- **THEN** the artifact root contains schema-valid summaries that identify
  verified invariant IDs, capture modes, rendered-anchor evidence,
  animation-frame evidence when required, warning-scan status, engine metadata,
  and schema versions
- **AND** the terminal output points to artifact paths rather than embedding
  large screenshots or logs

#### Scenario: Failing smoke points to diagnosis
- **WHEN** a visual-geometry smoke case fails
- **THEN** the artifact root preserves the relevant scenario manifest,
  screenshots or frames, crops, comparison reports, geometry samples, warning
  scan, and failure reason
- **AND** none of those summaries expose user document text, note bodies,
  draft bodies, local-history contents, or private persistence identifiers

#### Scenario: Parity smoke records oracle comparison
- **WHEN** the visual-geometry smoke lane runs in parity mode before wrapper
  migration
- **THEN** artifacts record the Rust status, Python-oracle status, compared
  fields, mismatch count, and bounded paths for both engines
- **AND** a mismatch prevents default smoke from moving to Rust

### Requirement: Smoke workflow checks cover the new proof tool
Workflow and policy checks SHALL be updated so the end-user smoke workflow,
Makefile, and proof-policy gates cannot drift away from the extracted Rust
toolchain. The same guard that checks the smoke matrix MUST recognize the
documented visual-geometry command, artifact path, Rust proof-tool boundary,
and Python oracle or diagnostic compatibility path.

#### Scenario: Smoke workflow matrix remains documented
- **WHEN** `make check-end-user-smoke-workflow` runs
- **THEN** it accepts the visual-geometry lane command only if it matches the
  documented wrapper or cargo-command boundary for the current phase
- **AND** the lane still names the expected artifact path
- **AND** it rejects undocumented workflow commands that bypass the Rust runner
  after parity is recorded

#### Scenario: Local policy target matches smoke artifact shape
- **WHEN** `make check-visual-proof-policy` validates local visual-sensitive
  changes
- **THEN** it reads the same visual-geometry summary shape produced by
  `make visual-geometry-smoke`
- **AND** it rejects stale, missing, skipped, unsupported, incomplete, or
  Python-only smoke evidence after the Rust migration point

#### Scenario: Workflow check preserves Python oracle discoverability
- **WHEN** the workflow or Makefile keeps a Python oracle or diagnostic alias
- **THEN** the check allows it only when the alias is documented as non-default
- **AND** the default visual-geometry lane remains Rust-backed after parity
