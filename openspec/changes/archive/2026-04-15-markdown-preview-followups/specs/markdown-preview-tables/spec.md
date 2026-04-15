## ADDED Requirements

### Requirement: Markdown preview preserves supported links inside table cells
The system SHALL keep supported Markdown links inside rendered table cells as recognizable links instead of flattening them to plain cell text. Link activation MUST use the same external launch path as other preview links and MUST preserve the table's readable column layout.

#### Scenario: Render a supported link inside a table cell
- **WHEN** the user previews a Markdown table containing a supported link inside a cell
- **THEN** the rendered cell shows the link text as a dedicated link instead of plain text
- **AND** the row and column alignment remain readable

#### Scenario: Activate a supported link inside a table cell
- **WHEN** the user activates a supported link inside a rendered table cell
- **THEN** the system launches the target externally with the default desktop handler
- **AND** the table remains read-only after activation
