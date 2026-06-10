## ADDED Requirements

### Requirement: Visual geometry samples rendered effects during animation frames
The visual geometry system SHALL support scenarios that sample rendered-effect
pixel anchors during an active UI animation. For native minimap/sidebar
animation scenarios, the runner MUST capture a bounded frame burst after the
sidebar action is triggered and before final geometry settles, then evaluate the
declared screenshot-derived pixel anchors for each sampled frame.

#### Scenario: Animation burst records per-frame minimap anchors
- **WHEN** a visual geometry scenario declares native minimap animation coverage
- **AND** the workspace sidebar show/hide action starts
- **THEN** the runner captures a bounded sequence of animation-frame samples
- **AND** each sample records detected native viewport top-edge and first-content-row pixel anchors when visible
- **AND** the scenario fails if any required frame exceeds the declared row-drift tolerance

#### Scenario: Final-state proof does not substitute for animation proof
- **WHEN** a minimap/sidebar scenario passes final settled before/after pixel checks
- **AND** it does not capture or evaluate animation-frame samples
- **THEN** the visual geometry system MUST NOT count the during-animation native minimap invariant as verified
- **AND** proof-policy checks that require animation coverage fail or report missing coverage

### Requirement: Animation-frame reports expose bounded evidence
Animation-frame visual reports SHALL expose bounded evidence that explains
rendered minimap movement without embedding unbounded screenshots or document
content. Reports MUST include scenario id, sampled frame count, elapsed frame
times, sidebar/editor/minimap geometry, native minimap diagnostics, detected row
positions, row deltas, crop paths or frame paths, status, and skip/failure
reason when applicable.

#### Scenario: Passing animation report shows stable rows
- **WHEN** a native minimap animation scenario passes
- **THEN** its summary includes the sampled frame count and maximum rendered row drift for each declared anchor
- **AND** it lists the native minimap animation invariant as pixel-verified for animation coverage

#### Scenario: Failing animation report points to the drifting frame
- **WHEN** a sampled animation frame shows native minimap anchor drift outside tolerance
- **THEN** the report identifies the failing frame index, elapsed time, detected row positions, app geometry, and crop or frame artifact
- **AND** it distinguishes app-vs-rendered disagreement from app geometry that moved with the rendered pixels

#### Scenario: Unsupported animation capture skips explicitly
- **WHEN** the host cannot capture animation frames with the required compositor, screenshot, D-Bus, PipeWire, or image tooling
- **THEN** the animation scenario reports a stable unsupported-host reason
- **AND** skipped animation coverage is not counted as verified
