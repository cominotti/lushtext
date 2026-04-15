## ADDED Requirements

### Requirement: Markdown preview preserves nested list hierarchy
The system SHALL render nested ordered and unordered Markdown lists with depth-aware indentation and correct marker sequencing so parent-child relationships remain readable in the native preview.

#### Scenario: Render nested unordered list items
- **WHEN** the user previews a Markdown document containing unordered list items nested beneath a parent item
- **THEN** the preview keeps child items visually indented beneath their parent item
- **AND** each list level remains distinguishable instead of flattening into one margin

#### Scenario: Render mixed ordered and unordered nesting
- **WHEN** the user previews a Markdown document that mixes ordered and unordered lists across multiple nesting levels
- **THEN** the preview preserves each item's source order and marker type at every level
- **AND** ordered items keep readable numbering without collapsing nested levels into plain paragraphs
