## ADDED Requirements

### Requirement: Automation client summarizes animation-frame visual proof
The automation client SHALL summarize animation-frame visual geometry artifacts
through its stable result envelope. Summaries MUST distinguish stream
animation-frame evidence from final-settle evidence and MUST report whether the
required animation invariant, mapped intermediate frames, per-frame anchors,
timing/skew metadata, warning scans, and bounded failure artifacts are present.

#### Scenario: Passing animation proof is summarized
- **WHEN** a developer runs artifact-summary on a passing animation-frame visual geometry artifact directory
- **THEN** the client reports the scenario id, invariant id, capture mode, frame count, geometry sample count, intermediate sample count, mapped intermediate frame count, maximum row drift, maximum sample skew, final-settle status, and representative artifact paths
- **AND** it exits successfully through the documented result envelope

#### Scenario: Failing animation proof points to frame evidence
- **WHEN** an animation-frame visual geometry artifact records native minimap anchor drift, missing anchors, stale frame/sample pairing, missing intermediate frames, readiness timeout, or warning-scan failure
- **THEN** artifact-summary exits nonzero with a stable status
- **AND** it reports the most useful frame report, crop, screenshot, geometry sample, warning log, and manifest paths without embedding image data or unbounded logs

#### Scenario: Final-settle proof is labeled separately
- **WHEN** an artifact directory contains both animation-frame evidence and final-settle evidence
- **THEN** the client reports both lanes separately
- **AND** a passing final-settle lane does not mask a failing or missing animation-frame lane

### Requirement: Automation client generates replayable animation scenarios from live captures
The automation client SHALL support generating or preserving replayable visual
geometry scenarios from a live window state that reproduces animation-sensitive
rendered-effect bugs. Generated scenarios MUST include explicit window size,
theme/wrap/minimap/sidebar state, action direction, stream capture settings,
required anchors, tolerances, and final-settle follow-up requirements.

#### Scenario: Live capture preserves animation settings
- **WHEN** a developer captures a live minimap/sidebar animation repro
- **THEN** the generated scenario records the current window size, visible surface state, minimap state, top-of-document or scroll anchor state, theme, wrap mode, action direction, stream frame count or duration, sample cadence, required anchors, and row tolerances
- **AND** the generated scenario can be replayed in an isolated smoke session without depending on private user document contents

#### Scenario: Missing live prerequisites fail explicitly
- **WHEN** live capture cannot determine a required field such as window size, minimap visibility, sidebar state, action direction, or screenshot-stream capability
- **THEN** the client reports a stable incomplete-capture status
- **AND** it does not generate a scenario that could be mistaken for verified animation proof

### Requirement: Automation client enforces animation proof policy for sensitive changes
The automation client and proof-policy checks SHALL reject sensitive visual
changes unless the artifact set contains valid animation-frame evidence for the
declared invariant.

#### Scenario: Sensitive visual change without stream evidence fails policy
- **WHEN** proof policy evaluates a minimap/source-map/editor-width/sidebar-animation sensitive diff
- **AND** the provided artifacts lack stream-mode animation proof for the native minimap invariant
- **THEN** the policy fails with a stable missing-animation-proof status
- **AND** it names the required scenario or invariant id

#### Scenario: Stale or incomplete animation evidence fails policy
- **WHEN** proof policy evaluates an animation artifact with no mapped intermediate PNG, stale frame/sample pairings, missing required anchors, or missing per-frame pass/fail rows
- **THEN** the policy fails even if final-settle evidence passes
- **AND** the failure summary points to the incomplete evidence fields
