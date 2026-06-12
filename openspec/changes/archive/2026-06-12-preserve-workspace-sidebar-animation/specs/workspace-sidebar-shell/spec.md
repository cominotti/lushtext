## ADDED Requirements

### Requirement: Workspace sidebar toggles visibly animate across adaptive widths
The system SHALL show an observable workspace-sidebar transition whenever the user toggles the workspace panel between shown and hidden states. The transition MUST remain visible at collapsed overlay widths, intermediate desktop widths where adaptive secondary-surface decisions can change, and wide desktop widths where the sidebar consumes layout width. Requested visibility, rendered visibility, action state, focus restoration, minimap protection, and persisted settings MUST remain consistent before, during, and after the transition.

#### Scenario: Intermediate desktop show transition animates
- **WHEN** the window is approximately `1100sp` wide, the workspace sidebar is hidden, the selected workspace sidebar preset is `Comfy`, and the user toggles the workspace panel on
- **THEN** the workspace sidebar becomes requested visible
- **AND** the user can observe the sidebar moving from hidden toward fully visible rather than appearing only at the final `x == 0` position
- **AND** final geometry settles with the workspace sidebar fully visible at the expected `Comfy` width for that window
- **AND** document-properties presentation reconciliation does not collapse the workspace-sidebar transition into an immediate jump

#### Scenario: Intermediate desktop hide transition animates
- **WHEN** the window is approximately `1100sp` wide, the workspace sidebar is shown, the selected workspace sidebar preset is `Comfy`, and the user toggles the workspace panel off
- **THEN** the workspace sidebar becomes requested hidden
- **AND** the user can observe the sidebar moving from fully visible toward hidden rather than disappearing only after an immediate endpoint jump
- **AND** final geometry settles with the workspace sidebar hidden and the editor/content area in its expected final allocation

#### Scenario: Collapsed overlay transition remains visible
- **WHEN** the window is at or below the workspace collapsed-width breakpoint and the user explicitly toggles the workspace panel
- **THEN** the workspace panel uses the collapsed overlay presentation
- **AND** the show or hide transition remains visible
- **AND** persistent chrome, the status-bar toggle, and any focused editor content remain reachable after the transition settles

#### Scenario: Wide desktop transition remains visible
- **WHEN** the window is wide enough for both the workspace sidebar and document properties to use side-by-side desktop presentation
- **AND** the user toggles the workspace panel
- **THEN** the workspace sidebar transition remains visible while layout width is consumed or released
- **AND** the document-properties pane keeps its requested visibility and final allocation policy
- **AND** no unrelated secondary surface opens or closes solely to make the workspace animation possible

#### Scenario: Sidebar content extremes do not suppress animation
- **WHEN** the workspace sidebar contains no workspaces, one representative workspace, or many workspaces with long folder names and deep visible trees
- **AND** the user toggles the workspace panel
- **THEN** the show or hide transition remains visible
- **AND** the fixed workspace scope row, workspace creation affordance, section headers, and item-region scrolling contracts remain intact after the transition settles
- **AND** the sidebar does not gain an unintended horizontal scrollbar, fake row, or overlapping controls because of the animation path
