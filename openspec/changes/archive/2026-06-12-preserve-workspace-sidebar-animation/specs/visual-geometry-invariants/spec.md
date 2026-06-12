## ADDED Requirements

### Requirement: Workspace sidebar animation proof includes intermediate frames
The visual invariant system SHALL include workspace-sidebar animation-frame scenarios that prove the sidebar moves through observable intermediate states during show and hide transitions. Final-settle screenshots alone MUST NOT satisfy workspace-sidebar animation coverage. The scenarios MUST cover collapsed overlay, reproduced intermediate, and wide desktop width classes, and MUST preserve bounded artifacts for both passing and failing animation evidence.

#### Scenario: Intermediate frame is required for sidebar show
- **WHEN** a visual geometry scenario verifies showing the workspace sidebar
- **THEN** the runner captures a bounded stream of frames during the toggle action
- **AND** at least one evaluated frame maps to an intermediate sidebar transition phase before final visible geometry
- **AND** the summary reports intermediate frame count, geometry sample timing, maximum frame/sample skew, final settle status, and the invariant id verified by the run

#### Scenario: Intermediate frame is required for sidebar hide
- **WHEN** a visual geometry scenario verifies hiding the workspace sidebar
- **THEN** the runner captures a bounded stream of frames during the toggle action
- **AND** at least one evaluated frame maps to an intermediate sidebar transition phase before final hidden geometry
- **AND** the summary reports intermediate frame count, geometry sample timing, maximum frame/sample skew, final settle status, and the invariant id verified by the run

#### Scenario: Reproduced width class is covered
- **WHEN** the workspace-sidebar animation scenario matrix runs
- **THEN** it includes an intermediate desktop case around `1100sp` with the `Comfy` workspace sidebar preset
- **AND** the case exercises a toggle where adaptive secondary-surface coordination can change the document-properties presentation or breakpoint guard
- **AND** passing narrower or wider cases alone does not mark the `1100sp` class as verified

#### Scenario: Final-settle-only evidence is rejected
- **WHEN** proof policy evaluates workspace-sidebar animation evidence
- **AND** the artifacts include only before and after captures taken after final visual geometry settles
- **THEN** the animation invariant is incomplete
- **AND** the proof result names missing intermediate-frame evidence rather than marking sidebar animation as verified

#### Scenario: Warning scan remains part of animation proof
- **WHEN** a workspace-sidebar animation visual scenario runs
- **THEN** unexpected GTK, Libadwaita, renderer, application, automation, or capture warnings fail the affected case
- **AND** failure artifacts include bounded log excerpts, frame paths, geometry samples, and the scenario identity
