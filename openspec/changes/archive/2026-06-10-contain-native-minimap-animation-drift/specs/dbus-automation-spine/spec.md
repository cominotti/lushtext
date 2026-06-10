## ADDED Requirements

### Requirement: Automation exposes bounded animation geometry diagnostics
Automation1 SHALL expose bounded visual geometry diagnostics that allow visual
smoke tooling to correlate animation frames with application state. The
diagnostics MUST include named surface rectangles, visibility, transition phase,
timing information, readiness blockers, and minimap/source-map diagnostic fields
needed to explain rendered drift. The diagnostics MUST NOT expose document text,
note bodies, draft bodies, local-history contents, full search result text, or
private persistence identifiers.

#### Scenario: Animation sample includes phase and surface geometry
- **WHEN** a visual smoke helper samples Automation1 during workspace-sidebar animation
- **THEN** each sample includes a monotonic timestamp or equivalent timing field
- **AND** it identifies the workspace sidebar, editor viewport, source view, minimap shell, source map, marker strip, and status bar rectangles when present
- **AND** it identifies whether the transition phase is settled, showing, hiding, or intermediate by documented fields

#### Scenario: Minimap diagnostics explain rendered anchors without text
- **WHEN** the native minimap is visible during an animation sample
- **THEN** Automation1 reports bounded minimap diagnostics such as allocation, top inset policy, adjustment values, anchor state, refresh blockers, and detector crop bounds
- **AND** it does not report minimap-rendered text or document body content

#### Scenario: Readiness distinguishes animation sampling from final settle
- **WHEN** a smoke helper starts animation-frame capture
- **THEN** it can begin from a settled baseline without waiting through the action being sampled
- **AND** after stream capture it can wait for the existing final visual geometry readiness predicate to prove endpoint stability

#### Scenario: Missing animation diagnostics fail clearly
- **WHEN** an animation-frame visual invariant requires Automation1 timing or geometry fields
- **AND** the running app does not expose those fields
- **THEN** the smoke helper reports a distinct contract failure
- **AND** it does not silently downgrade to fixed sleeps or final-settle-only proof
