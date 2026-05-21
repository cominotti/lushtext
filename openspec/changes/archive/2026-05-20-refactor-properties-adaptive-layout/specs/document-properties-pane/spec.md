## ADDED Requirements

### Requirement: Adaptive document properties transitions preserve one logical surface
The system SHALL treat the right-side document-properties pane and the compact document-properties bottom sheet as two presentations of one logical surface. Switching between those presentations MUST preserve the user's requested open or closed state, the active document's displayed properties, explicit empty or unavailable states, and focus-restoration behavior. The system MUST NOT expose stale metadata from a previous document, duplicate independent properties content, or lose the user's explicit visibility intent during pane-to-sheet or sheet-to-pane transitions.

#### Scenario: Open right pane becomes open bottom sheet
- **WHEN** document properties are open as a right-side pane for the active document
- **AND** the window becomes too narrow for the right-side pane according to the existing dynamic editor-width guard
- **THEN** document properties remain requested open and render as a compact bottom sheet
- **AND** the bottom sheet shows the same active document properties that were visible in the pane

#### Scenario: Open bottom sheet becomes open right pane
- **WHEN** document properties are open as a compact bottom sheet for the active document
- **AND** the window becomes wide enough for the right-side pane according to the existing dynamic editor-width guard
- **THEN** document properties remain requested open and render as a right-side pane
- **AND** the pane shows the same active document properties that were visible in the bottom sheet

#### Scenario: Closed requested state survives adaptive transitions
- **WHEN** document properties are explicitly closed
- **AND** the window crosses between the compact and spacious document-properties layouts
- **THEN** document properties remain closed
- **AND** neither the right-side pane nor the bottom sheet opens merely because the presentation changed

#### Scenario: Focus restoration is presentation-independent
- **WHEN** keyboard focus is inside the document-properties surface
- **AND** document properties are closed, suppressed, or replaced by the other adaptive presentation
- **THEN** focus is restored using the same fallback order regardless of whether the previous presentation was the right-side pane or the bottom sheet
