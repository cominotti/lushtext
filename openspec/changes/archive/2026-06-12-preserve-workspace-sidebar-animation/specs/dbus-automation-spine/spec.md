## ADDED Requirements

### Requirement: Automation distinguishes workspace-sidebar animation phases
Automation1 SHALL expose bounded visual-geometry state that allows automation and visual tooling to distinguish fully hidden, fully visible, and intermediate workspace-sidebar transition phases for both show and hide actions. The diagnostic state MUST remain privacy-preserving and MUST support final readiness after animation capture without forcing animation-frame sampling to wait for final geometry.

#### Scenario: Snapshot exposes intermediate sidebar state
- **WHEN** an automation snapshot is requested while the workspace sidebar is between hidden and visible endpoints
- **THEN** the visual geometry payload identifies the workspace sidebar as transitional or intermediate through documented bounded fields
- **AND** it includes enough geometry to determine whether the sidebar is moving toward shown or hidden state
- **AND** it does not expose document text, note bodies, draft bodies, local-history contents, full search result text, or private persistence identifiers

#### Scenario: Final readiness waits after show
- **WHEN** an automation client shows the workspace sidebar and then waits for final visual geometry readiness
- **THEN** readiness remains blocked until the workspace sidebar reaches fully visible geometry, relevant editor/minimap geometry is stable across required samples, and any app-owned minimap transition work has settled
- **AND** the readiness result distinguishes final geometry from any sampled intermediate frame

#### Scenario: Final readiness waits after hide
- **WHEN** an automation client hides the workspace sidebar and then waits for final visual geometry readiness
- **THEN** readiness remains blocked until the workspace sidebar reaches fully hidden geometry, relevant editor/minimap geometry is stable across required samples, and any app-owned minimap transition work has settled
- **AND** the readiness result distinguishes final geometry from any sampled intermediate frame

#### Scenario: Animation sampling starts from a known baseline
- **WHEN** a visual runner prepares to capture workspace-sidebar animation frames
- **THEN** Automation1 can confirm the app has a loaded baseline state, the requested initial sidebar state, and any required minimap or content fixture state
- **AND** the runner can trigger the sidebar action and sample frames before final sidebar geometry settles
