## ADDED Requirements

### Requirement: End-user visual geometry smoke uses the extracted toolchain
The end-user visual-geometry smoke lane SHALL continue to run the same
LushText visual invariants while the extracted proof toolchain takes over
schema, corpus, and policy validation. The Python visual runner SHALL remain
the live execution path until Rust live-runner parity is recorded. The
scheduled/manual smoke workflow, Makefile target, artifact directory,
pass/fail/skip semantics, and reviewable evidence layout MUST remain stable or
provide documented compatibility aliases.

#### Scenario: Scheduled visual-geometry lane still uploads evidence
- **WHEN** the scheduled or manually dispatched end-user smoke workflow runs
  the visual-geometry matrix lane
- **THEN** it executes the stable visual-geometry Makefile or script wrapper
- **AND** documentation identifies which evidence is produced by the Python
  live runner and which validation surfaces are owned by `cargo-gtk-proof`
- **AND** it uploads the same bounded artifact root expected by existing
  maintainers and agents

#### Scenario: Unsupported host does not count as coverage
- **WHEN** the smoke host lacks required compositor, screenshot, D-Bus,
  PipeWire, GStreamer, or image tooling
- **THEN** the lane reports a distinct skipped or unsupported status with
  bounded diagnostics
- **AND** proof policy and smoke summaries do not count the skipped invariant
  as verified

### Requirement: Smoke artifacts remain bounded and schema-valid
Visual smoke artifacts produced through the extracted toolchain SHALL remain
bounded, privacy-preserving, and schema-valid where Rust descriptors already
cover the artifact type. They MUST include enough evidence for review: root
summary, per-case summaries, scenario manifests, screenshots or generated
fixtures, crop reports, animation reports when required, warning scans,
environment details, skip/failure reasons, and engine metadata.

#### Scenario: Passing smoke has reviewable proof
- **WHEN** the visual-geometry smoke lane passes
- **THEN** the artifact root contains schema-valid summaries that identify
  verified invariant IDs, capture modes, rendered-anchor evidence,
  animation-frame evidence when required, warning-scan status, and engine
  metadata
- **AND** the terminal output points to artifact paths rather than embedding
  large screenshots or logs

#### Scenario: Failing smoke points to diagnosis
- **WHEN** a visual-geometry smoke case fails
- **THEN** the artifact root preserves the relevant scenario manifest,
  screenshots or frames, crops, comparison reports, geometry samples, warning
  scan, and failure reason
- **AND** none of those summaries expose user document text, note bodies,
  draft bodies, local-history contents, or private persistence identifiers

### Requirement: Smoke workflow checks cover the new proof tool
Workflow and policy checks SHALL be updated so the end-user smoke workflow,
Makefile, and proof-policy gates cannot drift away from the extracted
toolchain. The same guard that checks the smoke matrix MUST recognize the
documented visual-geometry command, artifact path, and staged Rust proof-tool
boundary.

#### Scenario: Smoke workflow matrix remains documented
- **WHEN** `make check-end-user-smoke-workflow` runs
- **THEN** it accepts the visual-geometry lane command only if it matches the
  documented wrapper or cargo-command boundary for the current phase
- **AND** the lane still names the expected artifact path

#### Scenario: Local policy target matches smoke artifact shape
- **WHEN** `make check-visual-proof-policy` validates local visual-sensitive
  changes
- **THEN** it reads the same visual-geometry summary shape produced by
  `make visual-geometry-smoke`
- **AND** it rejects stale, missing, skipped, unsupported, or incomplete smoke
  evidence
