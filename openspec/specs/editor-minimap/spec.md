# editor-minimap Specification

## Purpose
Provide a toggleable editor minimap that mirrors document state, shows semantic navigation markers, and supports viewport-aware navigation without changing the surrounding window layout.
## Requirements
### Requirement: Users can toggle the editor minimap
The system SHALL provide a persistent minimap preference and window action that show or hide a minimap on eligible editor pages. When enabled, the minimap SHALL appear on the right edge of supported editor pages without changing the outer workspace or properties sidebar visibility model.

#### Scenario: Enable the minimap for a supported document
- **WHEN** the user enables the minimap while the active editor page supports it
- **THEN** the active editor page shows a minimap on its right edge
- **AND** later editor pages in the same enabled state also show the minimap when they support it

#### Scenario: Minimap preference persists across sessions
- **WHEN** the user closes LushText with the minimap enabled and later reopens the app
- **THEN** supported editor pages restore with the minimap visible
- **AND** unsupported pages keep the preference enabled without forcing the minimap to appear

### Requirement: The minimap stays synchronized with the active document
The system SHALL render the minimap from the same active editor buffer and SHALL keep its viewport indication synchronized with document scrolling and editing. The minimap MUST remain read-only and MUST NOT become a second editable text surface.

#### Scenario: Scrolling the editor updates the minimap viewport
- **WHEN** the user scrolls the active editor page
- **THEN** the minimap updates its viewport indicator to the corresponding document region
- **AND** the minimap continues to represent the same document content as the editor

#### Scenario: Editing the document refreshes the minimap
- **WHEN** the user changes the active document text
- **THEN** the minimap refreshes to reflect the updated document shape
- **AND** the user cannot place a text cursor or type into the minimap itself

### Requirement: The minimap shows a clear viewport overlay
The system SHALL render a rectangular semi-transparent viewport overlay inside the minimap that clearly indicates which portion of the document is currently visible in the main editor. That overlay SHALL remain visible whenever the minimap itself is visible.

#### Scenario: Viewport overlay tracks the visible region during scrolling
- **WHEN** the user scrolls the active editor page while the minimap is visible
- **THEN** the minimap updates the viewport overlay position to the corresponding document region
- **AND** the overlay remains visually distinct from semantic markers behind it

#### Scenario: Viewport overlay fills the minimap when the full document is visible
- **WHEN** the active document fits entirely within the editor viewport and the minimap is enabled
- **THEN** the minimap remains visible
- **AND** the viewport overlay expands to indicate that the full document is currently visible

### Requirement: Users can navigate from the minimap
The system SHALL let users navigate the active document from the minimap by clicking or dragging inside it. Minimap navigation SHALL move the main editor viewport to the corresponding document position and SHALL keep editor focus behavior consistent with normal document interaction.

#### Scenario: Clicking the minimap jumps to a document region
- **WHEN** the user clicks a position in the minimap
- **THEN** the active editor scrolls to the corresponding document region
- **AND** the editor remains the primary editing surface after the jump

#### Scenario: Dragging inside the minimap scrolls continuously
- **WHEN** the user drags through the minimap
- **THEN** the active editor viewport follows that drag through the document
- **AND** the minimap viewport indicator updates continuously during the drag

### Requirement: The minimap shows semantic navigation markers
The system SHALL render visually distinguishable semantic markers in the minimap for editor-local navigation signals. The first supported marker set SHALL include bookmarks, active in-tab search matches, modified-since-save line regions, and long lines above the minimap warning threshold.

#### Scenario: Bookmarks appear in the minimap
- **WHEN** the active editor page contains bookmarks
- **THEN** the minimap shows bookmark markers at the corresponding document regions
- **AND** removing a bookmark removes its minimap marker

#### Scenario: Active search matches appear in the minimap
- **WHEN** the user has an active in-tab search with one or more matches in the editor
- **THEN** the minimap shows search-match markers at the corresponding regions
- **AND** clearing or closing the in-tab search removes those search markers

#### Scenario: Modified-since-save markers clear after save
- **WHEN** the user edits a document and then saves it successfully
- **THEN** the minimap removes modified-since-save markers for those saved changes
- **AND** later unsaved edits create new modified markers again

#### Scenario: Long lines are flagged in the minimap
- **WHEN** the active document contains lines longer than the minimap warning threshold
- **THEN** the minimap shows long-line warning markers for those regions
- **AND** shortening those lines removes the corresponding warnings

### Requirement: Minimap availability adapts to high-cost documents without hiding enabled supported tabs
The system SHALL keep the minimap visible on supported editor pages whenever the user's minimap preference is enabled, including when the full document already fits inside the editor viewport. The system SHALL suppress the per-tab minimap instance only when the active document exceeds the file-size tier that supports minimap rendering or another unsupported editor state prevents a safe minimap presentation. Suppressing the minimap for one document MUST NOT silently disable the user's saved minimap preference for other eligible documents.

#### Scenario: Fully visible document still keeps the minimap visible
- **WHEN** the active document fits entirely inside the current editor viewport and the minimap preference is enabled
- **THEN** the editor page still renders the minimap for that document view
- **AND** the saved minimap preference remains enabled for other supported documents

#### Scenario: Large document keeps the preference but disables the minimap
- **WHEN** the user opens or focuses a document that exceeds the minimap-supported size tier
- **THEN** the editor page does not render the minimap for that document
- **AND** the user receives feedback that the minimap is unavailable for the current document

### Requirement: Focus Mode temporarily hides the minimap
The system SHALL suppress editor minimap rendering while Focus Mode is active, regardless of the user's saved minimap preference. Focus Mode MUST NOT change the saved minimap preference, and normal minimap availability MUST resume when Focus Mode exits.

#### Scenario: Enabled minimap hides while focused
- **WHEN** the user's minimap preference is enabled and a supported editor page shows the minimap
- **AND** the user enters Focus Mode
- **THEN** the minimap is hidden
- **AND** the saved minimap preference remains enabled

#### Scenario: Minimap restores after focus
- **WHEN** Focus Mode is active after hiding an enabled minimap
- **AND** the user exits Focus Mode
- **THEN** the minimap renders again for supported editor pages
- **AND** unsupported editor pages continue to follow normal minimap availability rules

#### Scenario: Disabled minimap remains disabled after focus
- **WHEN** the user's minimap preference is disabled
- **AND** the user enters and exits Focus Mode
- **THEN** the minimap remains disabled
