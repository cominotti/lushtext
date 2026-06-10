## ADDED Requirements

### Requirement: Client summarizes native minimap animation proof
The automation client SHALL summarize visual geometry artifacts that contain
native minimap animation-frame proof. The summary MUST surface animation coverage
separately from final-settle coverage so agents cannot mistake endpoint
correctness for during-animation correctness.

#### Scenario: Artifact summary exposes animation-frame evidence
- **WHEN** the client summarizes a visual geometry artifact with native minimap animation coverage
- **THEN** it reports the scenario id, status, animation invariant ids, sampled frame count, maximum row drift, failing frame details when present, and representative frame/crop artifact paths
- **AND** it preserves final-settle pixel evidence as a separate field when present

#### Scenario: Missing animation proof reports missing coverage
- **WHEN** a visual-sensitive minimap change requires animation-frame coverage
- **AND** the artifact summary only contains final-settle minimap evidence
- **THEN** the client reports that animation coverage is missing
- **AND** it does not mark the animation invariant as verified

### Requirement: Live capture can generate animation-frame replay scenarios
The automation client SHALL help agents capture or generate replay scenarios for
live native minimap/sidebar animation defects. Generated scenarios MUST include
the starting sidebar state, action direction, window size, theme, word-wrap
state, fixture kind or explicit fixture override, animation sampling policy,
native minimap pixel anchors, and animation invariant id.

#### Scenario: Live animation capture records required fields
- **WHEN** the user reproduces a native minimap animation drift in a live window
- **THEN** the client can capture bounded visual geometry state and write a replay scenario with explicit animation sampling fields
- **AND** unknown theme, wrap, fixture, direction, or viewport fields require explicit caller overrides rather than silent guesses

#### Scenario: Replay command is recorded
- **WHEN** the client writes a generated animation replay scenario
- **THEN** it records the exact visual geometry smoke command needed to run that scenario under headless capture
- **AND** the generated artifact summary explains that live screenshots are context while replayed screenshot-derived anchors are proof

### Requirement: Proof policy can require animation coverage
Proof-policy checks SHALL be able to require native minimap animation-frame proof
for changes that affect native minimap rendering, source-map geometry, sidebar
animation/allocation, editor width reflow, or animation proof tooling.

#### Scenario: Relevant files require animation invariant
- **WHEN** a local diff changes minimap native rendering, source-map sync, editor allocation, sidebar animation behavior, or animation-frame visual tooling
- **THEN** proof-policy checks require a passing native minimap animation artifact with the required animation invariant id
- **AND** final-settle-only artifacts do not satisfy that requirement
