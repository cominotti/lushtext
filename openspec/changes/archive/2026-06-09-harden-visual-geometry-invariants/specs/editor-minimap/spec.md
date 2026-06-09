## ADDED Requirements

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
