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

### Requirement: Minimap preferences stay grouped on the Editor page
The system SHALL expose minimap-specific preferences in a dedicated `Minimap` group on the existing `Editor` preferences page. The system MUST NOT add a top-level `Minimap` preferences page while the minimap preference surface contains only the minimap visibility control and the long-line marker visibility control.

#### Scenario: Editor page shows minimap controls together
- **WHEN** the user opens Preferences and views the `Editor` page
- **THEN** the page contains a `Minimap` group
- **AND** that group contains the `Show Minimap` control
- **AND** that group contains the `Show Long-Line Markers` control

#### Scenario: Long-line marker preference defaults off
- **WHEN** LushText starts with default settings
- **THEN** the `Show Long-Line Markers` preference is disabled
- **AND** enabling the minimap does not automatically enable long-line markers

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
The system SHALL render visually distinguishable semantic markers in the minimap for editor-local navigation signals. The default supported marker set SHALL include bookmarks, active in-tab search matches, and modified-since-save line regions. Lines above the minimap warning threshold SHALL be represented by long-line warning markers only when the user enables the long-line marker preference. Semantic marker positions SHALL be projected from the same rendered `GtkSourceMap` geometry used by the minimap content, including top margin, dynamic EOF overscroll, and allocation changes. Semantic markers MUST NOT be spread across blank EOF overscroll tail space when their corresponding document lines render above that tail.

#### Scenario: Bookmarks appear in the minimap
- **WHEN** the active editor page contains bookmarks
- **THEN** the minimap shows bookmark markers at the corresponding document regions
- **AND** removing a bookmark removes its minimap marker

#### Scenario: Active search matches appear in the minimap
- **WHEN** the user has an active in-tab search with one or more matches in the editor
- **THEN** the minimap shows search-match markers at the corresponding regions
- **AND** clearing or closing the in-tab search removes those search markers

#### Scenario: Search markers respect EOF overscroll geometry
- **WHEN** the active editor page has dynamic EOF overscroll
- **AND** an active in-tab search produces markers for document lines above the final rendered content line
- **THEN** search-match markers align with the corresponding rendered minimap content regions
- **AND** search-match markers do not extend into the blank EOF overscroll tail below the rendered document content

#### Scenario: All semantic marker categories share source-map projection
- **WHEN** the active editor page shows bookmark, search-match, modified-since-save, and enabled long-line warning markers
- **THEN** each marker category is positioned using the same source-map layout geometry
- **AND** resizing or reallocation refreshes marker positions without changing which semantic lines are marked

#### Scenario: Modified-since-save markers clear after save
- **WHEN** the user edits a document and then saves it successfully
- **THEN** the minimap removes modified-since-save markers for those saved changes
- **AND** later unsaved edits create new modified markers again

#### Scenario: Long-line markers stay hidden by default
- **WHEN** the active document contains lines longer than the minimap warning threshold
- **AND** the minimap is enabled
- **AND** the long-line marker preference is disabled
- **THEN** the minimap does not show long-line warning markers for those regions

#### Scenario: Enabled long-line markers flag long lines
- **WHEN** the active document contains lines longer than the minimap warning threshold
- **AND** the minimap is enabled
- **AND** the long-line marker preference is enabled
- **THEN** the minimap shows long-line warning markers for those regions
- **AND** shortening those lines removes the corresponding warnings

#### Scenario: Disabling long-line markers removes existing warnings
- **WHEN** long-line warning markers are visible in the minimap
- **AND** the user disables the long-line marker preference
- **THEN** the minimap removes long-line warning markers
- **AND** bookmark, search-match, and modified-since-save markers remain governed by their existing behavior

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

### Requirement: Minimap viewport follows settled editor geometry after width changes
The system SHALL keep the minimap viewport overlay synchronized with the active editor's settled visible buffer range after layout-driven width changes. Sidebar show/hide, compact surface arbitration, editor width-only allocation changes, word-wrap reflow, and end-of-file overscroll recalculation MUST NOT leave the minimap viewport overlay using stale pre-transition geometry. The minimap's wrapping and viewport projection policy MUST be explicit enough that the overlay corresponds to the main editor viewport even when the minimap content uses a different wrap mode from the editor view.

#### Scenario: Sidebar toggle preserves viewport correspondence
- **WHEN** the minimap is enabled for the active document
- **AND** word wrap is enabled
- **AND** the workspace sidebar is shown or hidden at a width where it changes the editor allocation
- **THEN** after layout settles, the minimap viewport overlay corresponds to the active editor's visible buffer range
- **AND** the overlay is not positioned from stale pre-toggle adjustment geometry

#### Scenario: Width-only allocation refreshes the minimap viewport
- **WHEN** the active editor's width changes without a corresponding editor height change
- **AND** that width change can alter wrapping, visible-line geometry, or scroll adjustment ranges
- **THEN** the minimap viewport overlay is refreshed after the new editor and source-map allocations settle
- **AND** semantic minimap markers continue to use the refreshed source-map geometry

#### Scenario: Wrapping policy does not create viewport drift
- **WHEN** the main editor and minimap use different wrap modes
- **AND** the window width changes enough to reflow wrapped editor lines
- **THEN** the minimap viewport overlay still represents the main editor's settled visible buffer range under the chosen projection policy
- **AND** the user does not see the viewport indicator jump solely because the minimap retained a stale logical-to-visual mapping

#### Scenario: Word-wrap-disabled control remains stable
- **WHEN** word wrap is disabled for the active editor
- **AND** the workspace sidebar is shown or hidden while the minimap is visible
- **THEN** the minimap viewport overlay remains aligned with the active editor's visible buffer range after layout settles
- **AND** any residual viewport drift is not hidden by word-wrap-specific assumptions

### Requirement: Minimap top content remains visible after shell reflow
The minimap SHALL keep the first rendered minimap content visible and unclipped when the active editor is at the top of the document. Width-only shell reflow, workspace sidebar show/hide, document-properties presentation changes, word-wrap changes, style changes, and dynamic overscroll refreshes MUST NOT leave the minimap's top content hidden under its border or clipped by stale source-map geometry.

#### Scenario: Sidebar hide keeps minimap top line visible
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** the workspace sidebar is hidden at a width where the editor allocation changes
- **THEN** the minimap's first rendered content line remains visible below the minimap's top edge
- **AND** the main editor's first visible line remains line one

#### Scenario: Sidebar show keeps minimap top line visible
- **WHEN** the minimap is enabled for a supported document
- **AND** the active editor is scrolled to the top of the document
- **AND** the workspace sidebar is shown at a width where the editor allocation changes
- **THEN** the minimap's first rendered content line remains visible below the minimap's top edge
- **AND** the minimap viewport overlay corresponds to the main editor's top visible range after layout settles

#### Scenario: Width-only reflow refreshes top anchoring
- **WHEN** the active editor's width changes without a corresponding height change
- **AND** the editor was already vertically anchored at the top
- **THEN** the editor and minimap refresh top anchoring after the new allocation settles
- **AND** no stale vertical adjustment or source-map projection clips the minimap's top content

#### Scenario: Theme and wrap controls do not hide top content
- **WHEN** the minimap is visible at the top of the document
- **AND** the user switches between light and dark style preference or enables/disables word wrap
- **THEN** the minimap top content remains visible and unclipped after the style and layout settle
- **AND** semantic markers continue to project from the refreshed source-map geometry

### Requirement: Minimap geometry exposes bounded visual anchors
The minimap SHALL provide bounded geometry anchors for automation and visual smoke without exposing document contents. Anchors MUST include enough information to prove minimap shell bounds, source-map bounds, top content inset, viewport overlay bounds, marker-strip bounds, and scroll/top state.

#### Scenario: Automation can identify minimap bounds
- **WHEN** Automation1 visual geometry state is requested while the minimap is visible
- **THEN** it reports bounded minimap shell, source-map, viewport-overlay, and marker-strip rectangles in the documented coordinate space
- **AND** it does not expose minimap-rendered text or document body content

#### Scenario: Hidden minimap reports absence explicitly
- **WHEN** Automation1 visual geometry state is requested while the minimap is disabled, too large, evicted, or focus-suppressed
- **THEN** the minimap geometry entry records the unavailable state
- **AND** visual smoke can distinguish intentional absence from missing geometry data

### Requirement: Minimap widget tests cover logical top-edge invariants
The project SHALL provide widget-level regression coverage for minimap top anchoring and width reflow. These tests SHALL assert logical geometry, allocation, scroll adjustment, and marker projection state without depending on compositor pixels.

#### Scenario: Top scroll stays anchored across sidebar toggle
- **WHEN** a widget test creates a real window with minimap enabled and the active editor at the top of a long document
- **AND** it toggles workspace sidebar visibility
- **THEN** the source view vertical adjustment remains at its lower bound after layout settles
- **AND** the minimap source-map allocation remains positive and refreshed

#### Scenario: Mid-file reflow does not jump document range
- **WHEN** a widget test scrolls a minimap-enabled editor to a representative middle line
- **AND** shell reflow changes the editor width
- **THEN** the visible start line remains within the existing accepted tolerance
- **AND** minimap marker bounds remain projectable through the source-map geometry
