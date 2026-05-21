## MODIFIED Requirements

### Requirement: Non-content sidebar affordances remain symbolic
The system SHALL keep symbolic icons for sidebar controls and non-content status rows. The regular themed icon behavior MUST apply only to actual filesystem content rows in the file tree.

#### Scenario: Sidebar controls remain symbolic
- **WHEN** the sidebar renders controls such as New Workspace, Refresh, drill-down back, or Focus Folder
- **THEN** those controls continue to use symbolic icons
- **AND** their actions, tooltips, visibility, and placement remain unchanged

#### Scenario: Placeholder rows remain symbolic status rows
- **WHEN** the file tree renders a synthetic placeholder or informational row instead of a real filesystem path
- **THEN** that row continues to use a symbolic status/information icon
- **AND** it is not treated as a file or directory content row for regular icon selection
