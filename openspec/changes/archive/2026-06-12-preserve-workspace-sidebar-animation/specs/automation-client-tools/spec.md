## ADDED Requirements

### Requirement: Automation client proves workspace-sidebar animation evidence
The automation client SHALL support capturing, replaying, and summarizing workspace-sidebar animation-frame evidence from live or generated visual-geometry scenarios. The client MUST distinguish animation-frame evidence from final-settle evidence and MUST fail clearly when intermediate frames, timing correlation, required geometry fields, or replay commands are missing.

#### Scenario: Live capture records sidebar animation inputs
- **WHEN** a developer captures a live workspace-sidebar animation repro
- **THEN** the client records the current window size, scale factor, sidebar requested/visible state, selected sidebar width preset when available, minimap visibility, action direction, fixture identity when safely available, stream duration or frame count, and required final-settle follow-up
- **AND** missing required fields are reported as incomplete capture instead of being guessed silently

#### Scenario: Replay command preserves intermediate-width case
- **WHEN** the client writes a generated replay scenario for the reproduced `1100sp` workspace-sidebar animation class
- **THEN** it records the exact visual geometry smoke command needed to replay the case under the supported headless visual runner
- **AND** the generated scenario keeps the intermediate width class, sidebar preset, initial sidebar state, action direction, and final-settle expectations explicit

#### Scenario: Summary separates animation and final-settle lanes
- **WHEN** the client summarizes workspace-sidebar visual geometry artifacts
- **THEN** it reports animation-frame status separately from final-settle status
- **AND** a passing final-settle lane does not mask missing, skipped, stale, or failing animation-frame evidence

#### Scenario: Missing animation evidence fails with stable status
- **WHEN** proof policy or artifact summary evaluates a workspace-sidebar animation-sensitive change
- **AND** artifacts lack mapped intermediate frames, timing correlation, required geometry fields, final-settle follow-up, or bounded failure artifacts
- **THEN** the client reports a stable missing-animation-proof or incomplete-animation-proof status
- **AND** it names the missing evidence category and the scenario or invariant id that required it
