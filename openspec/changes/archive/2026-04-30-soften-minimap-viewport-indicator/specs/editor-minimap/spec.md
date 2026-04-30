## MODIFIED Requirements

### Requirement: The minimap shows a clear viewport overlay
The system SHALL render a rectangular semi-transparent viewport overlay inside the minimap that clearly indicates which portion of the document is currently visible in the main editor. That overlay SHALL remain visible whenever the minimap itself is visible. The overlay MUST use a neutral editor-chrome treatment instead of the system accent color, and it MUST remain visually subordinate to semantic markers such as bookmarks, search matches, modified lines, and long-line warnings.

#### Scenario: Viewport overlay tracks the visible region during scrolling
- **WHEN** the user scrolls the active editor page while the minimap is visible
- **THEN** the minimap updates the viewport overlay position to the corresponding document region
- **AND** the overlay remains visually distinct from semantic markers behind it
- **AND** the overlay does not use system accent color styling

#### Scenario: Viewport overlay fills the minimap when the full document is visible
- **WHEN** the active document fits entirely within the editor viewport and the minimap is enabled
- **THEN** the minimap remains visible
- **AND** the viewport overlay expands to indicate that the full document is currently visible
- **AND** the expanded overlay remains neutral and visually calm rather than appearing as an accent-colored block
