# markdown-preview-tables Specification

## Purpose
TBD - created by archiving change render-markdown-tables. Update Purpose after archive.
## Requirements
### Requirement: Markdown preview renders table blocks
The system SHALL render Markdown table syntax in the Markdown preview pane as a readable table block instead of omitting the table structure. Table header rows MUST remain visually distinct from body rows.

#### Scenario: Render a Markdown table with header and body rows
- **WHEN** the user opens or previews a Markdown document that contains a valid table with a header row and one or more body rows
- **THEN** the preview shows the table as visible rows and columns
- **AND** the header row remains distinguishable from the body rows

#### Scenario: Keep surrounding document flow around a rendered table
- **WHEN** a Markdown document contains paragraphs or other supported blocks before and after a valid table
- **THEN** the preview keeps those surrounding blocks in their original order
- **AND** the rendered table appears between them instead of replacing or collapsing adjacent content

### Requirement: Markdown preview preserves readable columns and alignment cues
The system SHALL preserve each table cell's text content and MUST render columns so tables remain readable when rows contain uneven cell widths or blank cells. The system MUST honor left, center, and right alignment markers from Markdown table syntax in the rendered preview.

#### Scenario: Render a table with uneven widths and blank cells
- **WHEN** the user previews a Markdown table whose rows contain short cells, long cells, and blank cells
- **THEN** the preview keeps each cell in the correct row and column position
- **AND** blank cells remain visible as empty table cells rather than disappearing

#### Scenario: Render alignment markers from Markdown table syntax
- **WHEN** the user previews a Markdown table that declares left-, center-, and right-aligned columns
- **THEN** the preview pads the rendered cells so those alignment cues remain visible in their respective columns
- **AND** the table stays readable without collapsing all columns to the same left alignment

