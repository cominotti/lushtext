## ADDED Requirements

### Requirement: Automation snapshots support animation-frame correlation
Automation1 visual geometry snapshots SHALL expose bounded frame-phase geometry
needed to correlate sampled sidebar animation frames with native minimap
rendering. The snapshot MUST remain content-safe and MUST NOT expose document
text, note bodies, draft bodies, local-history contents, complete search result
text, or private persistence identifiers.

#### Scenario: Snapshot reports sidebar animation geometry
- **WHEN** an automation snapshot is requested while the workspace sidebar is animating
- **THEN** the visual geometry payload includes bounded workspace-sidebar, editor viewport, minimap shell, source-map, and native minimap diagnostic geometry for the current frame
- **AND** it reports enough state to distinguish fully shown, fully hidden, and intermediate sidebar positions

#### Scenario: Snapshot reports native minimap frame inputs
- **WHEN** an automation snapshot is requested while the minimap is visible during sidebar animation
- **THEN** the native minimap diagnostics include bounded source-map visible state, source-map adjustment state, editor visible state, document-height ratio inputs, compensation margin or equivalent top-inset diagnostic, and estimated native slider rect when available
- **AND** absent or unprojectable diagnostics use stable absence reasons

### Requirement: Visual readiness distinguishes settled state from animation capture
Automation readiness SHALL keep the existing `visual-geometry-settled` predicate
for final-state proof while allowing animation-frame capture to observe
intermediate geometry intentionally. Animation capture MUST NOT wait for final
sidebar geometry before sampling frames, but it MUST still wait for the initial
document, minimap, and action state needed to start from a known baseline.

#### Scenario: Animation capture starts from ready baseline
- **WHEN** a visual runner prepares a native minimap animation scenario
- **THEN** Automation1 readiness confirms the file is loaded, the minimap is visible, the editor starts at the requested scroll position, and the sidebar starts in the requested shown or hidden state
- **AND** frame sampling starts immediately after the sidebar action rather than after final sidebar geometry settles

#### Scenario: Final readiness remains available after animation capture
- **WHEN** animation-frame sampling finishes
- **THEN** visual tooling can still wait for `visual-geometry-settled`
- **AND** final settled before/after assertions continue to use the existing readiness and geometry predicates
