## ADDED Requirements

### Requirement: Rendered effects use screenshot-derived anchors as the oracle
The visual invariant system SHALL use screenshot-derived pixel anchors as the pass/fail oracle for native toolkit-rendered, CSS-rendered, or compositor-rendered visual effects. App-owned geometry MAY define safe bounded crops, readiness metadata, and diagnostics, but it MUST NOT by itself satisfy an invariant for a visible rendered effect.

#### Scenario: Geometry-stable rendered drift fails
- **WHEN** app-owned geometry reports a stable anchor before and after a visual change
- **AND** screenshot-derived pixels for the protected rendered effect move outside the manifest tolerance
- **THEN** the visual comparison fails
- **AND** the report identifies the app-vs-rendered disagreement

#### Scenario: Missing pixel anchor fails rendered-effect coverage
- **WHEN** a manifest declares coverage for a rendered visual effect such as the native minimap viewport highlight
- **AND** the runner cannot detect the required screenshot-derived anchor in before or after captures
- **THEN** the scenario fails or skips with an explicit unsupported reason
- **AND** skipped coverage is not counted as verified

#### Scenario: Crop geometry cannot replace pixel proof
- **WHEN** Automation1 exposes a bounded crop for a native rendered effect
- **THEN** the runner uses that crop only to limit screenshot inspection
- **AND** it still evaluates the declared pixel detector and row relationship before marking the invariant passed

### Requirement: Native minimap threshold coverage is mandatory for visual-sensitive minimap work
The visual invariant system SHALL include targeted native-minimap rendered-highlight scenarios for the reproduced intermediate-size threshold and SHALL require those scenarios for visual-sensitive changes that can affect minimap rendering, source-map geometry, sidebar allocation, editor width reflow, or visual proof tooling.

#### Scenario: Reproduced intermediate case is verified
- **WHEN** the visual geometry smoke lane runs native minimap highlight coverage
- **THEN** it includes the reproduced intermediate-size case around `1822x1272`
- **AND** it verifies sidebar hide and sidebar show directions with screenshot-derived native-highlight anchors

#### Scenario: Passing other sizes does not verify the threshold
- **WHEN** conventional size cases pass
- **AND** the reproduced intermediate-size case is missing, skipped, or filtered out
- **THEN** the native minimap highlight invariant is not counted as verified
- **AND** proof-policy checks fail for changes that require that invariant

#### Scenario: Final rendered frames are stable before comparison
- **WHEN** a sidebar/minimap visual scenario captures before or after screenshots
- **THEN** workflow readiness, final allocation geometry, and native rendered-effect anchor rows have remained stable across the required final samples
- **AND** a mid-animation or stale-frame capture fails with preserved geometry samples and crop artifacts

### Requirement: Visual reports expose rendered-anchor evidence
Visual geometry artifact summaries SHALL expose bounded rendered-anchor evidence for native rendered effects. Reports MUST include scenario id, final geometry, detected anchor rows, row deltas, relationship deltas, app-vs-rendered diagnostics, crop paths, verified invariant ids, and skip or failure reasons.

#### Scenario: Agent can see why native minimap failed
- **WHEN** a native minimap highlight comparison fails
- **THEN** the summary includes before and after screenshot row detections for the native viewport top edge and first minimap content row
- **AND** it includes final sidebar/editor/minimap geometry and app-vs-rendered disagreement details when available

#### Scenario: Passing report proves pixel verification
- **WHEN** a native minimap highlight comparison passes
- **THEN** the summary lists the native minimap invariant id as pixel-verified
- **AND** it records the crop artifacts and detected row relationship used for the pass
