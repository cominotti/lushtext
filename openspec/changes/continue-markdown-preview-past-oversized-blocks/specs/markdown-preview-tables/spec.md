## ADDED Requirements

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
