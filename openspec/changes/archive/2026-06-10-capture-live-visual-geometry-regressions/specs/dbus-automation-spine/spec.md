## ADDED Requirements

### Requirement: Visual geometry readiness covers final animated allocations
The automation readiness contract SHALL expose or support deterministic visual-geometry waits that remain blocked until animated shell allocations relevant to the requested workflow have reached their final stable state. For workspace-sidebar transitions, readiness MUST not be considered sufficient for visual proof while the sidebar or editor viewport is still between final allocations.

#### Scenario: Sidebar hide readiness waits for final allocation
- **WHEN** an automation client hides the workspace sidebar and then waits for visual geometry readiness intended for visual proof
- **THEN** readiness remains blocked until the sidebar allocation is fully hidden, the editor viewport has expanded to its final left edge, and relevant visual geometry rows have remained stable across multiple samples
- **AND** the readiness result or snapshot evidence distinguishes final geometry from an intermediate animation frame

#### Scenario: Sidebar show readiness waits for final allocation
- **WHEN** an automation client shows the workspace sidebar and then waits for visual geometry readiness intended for visual proof
- **THEN** readiness remains blocked until the sidebar allocation is fully visible, the editor viewport starts after the sidebar, and relevant visual geometry rows have remained stable across multiple samples
- **AND** a mid-animation sidebar allocation such as a negative `x` while requested visible is not reported as final visual readiness

#### Scenario: Visual readiness timeout exposes blocker detail
- **WHEN** final animated allocations do not settle before the timeout
- **THEN** the readiness wait reports a timeout with bounded blocker detail naming the unsettled surface or relationship
- **AND** it does not return a generic ready status that would allow visual proof to capture stale or transitional geometry

### Requirement: Visual geometry snapshots expose enough state for final-geometry assertions
Automation snapshots SHALL expose bounded visual-geometry state needed by smoke helpers to assert final sidebar/editor/minimap relationships without private widget access or coordinate guesses.

#### Scenario: Snapshot supports sidebar final-state checks
- **WHEN** a visual geometry snapshot is requested during or after a workspace sidebar transition
- **THEN** it includes surface names, visibility, absence reason when any, screen-space rectangles, allocations, scale factor, and requested/visible shell state needed to determine whether the sidebar is fully hidden, fully visible, or transitional
- **AND** it does not require clients to inspect private GTK widgets

#### Scenario: Snapshot supports rendered anchor diagnostics
- **WHEN** the minimap is visible
- **THEN** the snapshot includes bounded app-owned minimap surfaces and pixel-anchor rectangles for the minimap viewport top edge, viewport fill, viewport bottom edge, and first content row when available
- **AND** screenshot-derived pixel comparison remains responsible for proving rendered-pixel stability
