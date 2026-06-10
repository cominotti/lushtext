## ADDED Requirements

### Requirement: Animation-frame rendered-effect invariants use timestamp-correlated stream proof
Visual geometry animation-frame rendered-effect scenarios SHALL support timestamp-correlated stream proof. Stream capture MUST record screenshot frames, action trigger
timing, Automation1 geometry sample timing, phase labels, and detector results.
For native or toolkit-owned rendered effects, screenshot-derived pixel anchors
MUST be the pass/fail oracle. App-owned geometry MAY bound crops and explain
failures, but it MUST NOT satisfy the invariant when rendered pixels drift.

#### Scenario: Stream capture proves intermediate animation frames
- **WHEN** a visual geometry scenario declares an animation-frame rendered-effect invariant
- **THEN** the runner captures a bounded stream of PNG frames during the action
- **AND** it records timestamped Automation1 geometry samples for the same time window
- **AND** at least one evaluated PNG frame maps to an intermediate transition phase within the declared skew bound
- **AND** the summary reports first frame time, last frame time, sample count, intermediate sample count, mapped intermediate frame count, and maximum sample skew

#### Scenario: Per-frame pixel anchors gate the invariant
- **WHEN** an animation-frame scenario declares required pixel anchors such as the native minimap viewport top edge
- **THEN** every evaluated frame that maps to a protected phase runs those detectors
- **AND** any required anchor missing or drifting outside the declared tolerance fails the scenario
- **AND** the failure records the frame index, timestamp, mapped sample timestamp, anchor rows, row deltas, crop paths, and failure reason

#### Scenario: Stale frame-to-geometry pairing fails
- **WHEN** a captured frame cannot be matched to a geometry sample inside the declared skew bound
- **THEN** that frame cannot be used as passing animation proof
- **AND** the scenario fails if the remaining evaluated frames do not prove the required intermediate phase

#### Scenario: Final-settle-only proof is insufficient for animation invariants
- **WHEN** a scenario protects a rendered effect during animation
- **AND** the artifacts include only before and after captures after final geometry settles
- **THEN** the visual geometry invariant is incomplete
- **AND** proof policy does not count the scenario as verified

### Requirement: Visual proof policy rejects incomplete animation evidence
The visual proof policy SHALL require animation-frame evidence for changes that
touch native minimap rendering, source-map geometry, editor width reflow,
workspace-sidebar consuming animations, animation capture tooling, or proof
policy itself. The policy MUST reject evidence that lacks stream mode,
intermediate mapped frames, required anchors, per-frame pass/fail results,
timing/skew metadata, or bounded failure artifacts.

#### Scenario: Sensitive diff requires animation proof
- **WHEN** a change modifies minimap rendering, source-map geometry, editor-page width reflow, workspace-sidebar animation coordination, animation capture tooling, or visual proof policy
- **THEN** proof policy requires a passing native-minimap animation-frame artifact for the relevant scenario
- **AND** a final-settle artifact alone does not satisfy the requirement

#### Scenario: Negative self-tests cover escaped failure classes
- **WHEN** visual proof policy self-tests run
- **THEN** they include negative cases for final-settle-only evidence, screenshot sampling without stream mode, no mapped intermediate PNG, stale frame/sample pairing, missing required anchors, and rendered pixel drift hidden by acceptable app geometry
- **AND** each negative case fails with a stable status and bounded diagnostic detail

#### Scenario: Unsupported host does not count as verified
- **WHEN** the host cannot provide compositor, screenshot, stream capture, image decoding, or Automation1 timing support required for animation proof
- **THEN** the scenario reports a distinct unsupported status with the missing capability
- **AND** skipped animation coverage is not counted as verified for sensitive visual changes
