# markdown-preview-tables Specification

## Purpose
Render Markdown table syntax in the native preview as readable table blocks that preserve headers, column structure, and alignment cues within the surrounding document flow.
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

### Requirement: Markdown preview renders tables larger than one projection slice
The system SHALL render a Markdown table completely, within the preview's table-cell widget budget, even when the table needs more render events or bytes than one projection slice permits. Such a table MUST appear as one continuous table block with every row present in source order, and MUST NOT appear as several separate stacked tables. A table beyond the table-cell widget budget MUST keep its existing single in-place fallback presentation rather than truncating the document. Content after the table MUST remain rendered in every case.

#### Scenario: Render a table with more rows than one projection slice allows
- **WHEN** the user previews a Markdown document whose table exceeds one projection slice but stays within the preview's table-cell widget budget
- **THEN** the preview shows one table containing every header and body row in source order
- **AND** column structure and alignment cues are preserved across rows that were projected in different turns

#### Scenario: Keep document content after an oversized table
- **WHEN** a Markdown document contains an oversized table followed by further headings, paragraphs, or tables
- **THEN** the preview renders that following content
- **AND** the preview does not report that rendering stopped at the table

#### Scenario: One table row exceeds a projection slice
- **WHEN** a single table row, such as one containing a cell with more inline events than one projection slice permits, cannot be fitted
- **THEN** only that row is replaced by one accessible omission row inside the same table
- **AND** the table's other rows and its column structure are still rendered
- **AND** the rest of the document is still rendered

#### Scenario: Large-byte table within the table-cell widget budget
- **WHEN** a table has fewer cells than the table-cell widget budget but its total cell text is large, including well past the byte ceiling that bounds retained code-block text
- **THEN** the preview still renders that table in full
- **AND** no retention ceiling reduces, truncates, or replaces any row

#### Scenario: Table exceeds the table-cell widget budget
- **WHEN** a table has more cells than the preview's table-cell widget budget permits
- **THEN** that table keeps its existing in-place fallback presentation naming the cell count and the budget
- **AND** the content before and after the table is still rendered
