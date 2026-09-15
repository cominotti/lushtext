## ADDED Requirements

### Requirement: Workspace-section scroll area renders every tree row
The scrollable workspace-section area below the fixed workspace-scope row SHALL render every row of every visible section's tree model. Dense-row scrolling MUST reach the last row of the last visible section, and no section may leave an allocated but unrendered band.

#### Scenario: Dense file rows reach the final row
- **WHEN** the visible sections together hold more than 200 tree rows and the outer sidebar scroller is moved to its lower edge
- **THEN** the final tree row in the last visible section is rendered with its label
- **AND** the fixed workspace-scope row remains visible above the scroll area
- **AND** the sidebar preserves its no-horizontal-scrollbar contract
