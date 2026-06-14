## ADDED Requirements

### Requirement: Document properties uses a readable side-rail inspector presentation
The system SHALL render the document properties surface as a GNOME-like inspector on top of the shared side-rail tone. Its grouped document and health rows MUST remain visually distinct from the side-rail background in both the spacious right-pane presentation and the compact bottom-sheet presentation. The surface MUST preserve the existing document-properties scope and MUST NOT add document type, language picker, duplicate encoding or line-ending controls, or app-wide Preferences controls as part of the styling change.

#### Scenario: Empty properties state remains readable
- **WHEN** no document is selected and the document properties surface is open
- **THEN** the Document and Health groups remain visible and readable against the side-rail background
- **AND** unavailable fields use explicit empty-state copy instead of stale metadata
- **AND** the surface does not introduce document type, language, encoding, line-ending, or app-wide Preferences controls

#### Scenario: File-backed metadata is visually separated
- **WHEN** a file-backed document is active and the document properties surface is open as a right-side pane
- **THEN** path or location, file size, formatting source, statistics, and file-health rows remain grouped and visually distinct from the surrounding side rail
- **AND** long location or formatting text remains readable without overlapping adjacent rows or controls

#### Scenario: Health findings do not flatten the inspector hierarchy
- **WHEN** a file-backed document has multiple file-health findings
- **THEN** the health summary row and dynamic finding rows remain inside the inspector presentation
- **AND** each row stays readable and separated from the side-rail background
- **AND** the Review action remains visible and reachable when findings exist

#### Scenario: Compact bottom sheet keeps the same readable styling
- **WHEN** the window is too narrow for the right-side pane and document properties open as a bottom sheet
- **THEN** the bottom sheet uses the same readable inspector relationship between side-rail surface and grouped rows
- **AND** the document properties content keeps its existing categories instead of switching to a reduced or unrelated layout

#### Scenario: Styling works in light and dark schemes
- **WHEN** the user switches between light and dark style preferences
- **THEN** the document properties side rail and grouped rows preserve clear contrast using theme-appropriate colors
- **AND** tab-content transparency does not make the document properties surface translucent
