## ADDED Requirements

### Requirement: Client captures and summarizes native minimap rendered-proof scenarios
The automation client SHALL help agents capture, replay, summarize, and policy-check native minimap rendered-proof scenarios. The client MUST surface whether the required screenshot-derived native minimap invariant was verified, skipped, or failed, and it MUST NOT count geometry-only evidence as a pass.

#### Scenario: Live capture emits native minimap rendered-proof fields
- **WHEN** the client generates a live visual-geometry scenario from a visible minimap/sidebar state
- **THEN** the generated manifest includes native minimap pixel anchors, final sidebar geometry requirements, source window size, sidebar direction, theme or requested color scheme, word-wrap state, fixture kind, and native minimap invariant id
- **AND** unknown fields are reported as missing-field or require explicit caller overrides rather than being guessed silently

#### Scenario: Artifact summary exposes native minimap pixel status
- **WHEN** the client summarizes a visual geometry artifact containing native minimap highlight coverage
- **THEN** the summary reports scenario id, status, pixel-verified invariant ids, native minimap anchor rows, row deltas, relationship deltas, crop paths, app-vs-rendered diagnostics, and final geometry
- **AND** a rendered-anchor failure exits with a stable nonzero status

#### Scenario: Proof policy rejects geometry-only native minimap evidence
- **WHEN** files that can affect native minimap rendering, source-map geometry, sidebar/editor allocation, or visual proof tooling change
- **THEN** proof-policy checks require a passing native minimap rendered-proof artifact for the reproduced intermediate-size invariant
- **AND** artifacts without screenshot-derived pixel anchors or without the required invariant id do not satisfy the policy

#### Scenario: Filtered runs do not overclaim coverage
- **WHEN** a developer runs a filtered visual geometry case that excludes the reproduced native minimap threshold scenario
- **THEN** the client summary reports the filtered result accurately
- **AND** it does not mark the full native minimap rendered invariant as verified unless the required scenario actually ran and passed
