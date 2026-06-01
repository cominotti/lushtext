## ADDED Requirements

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
