## ADDED Requirements

### Requirement: Markdown preview renders lists larger than one projection slice
The system SHALL render a Markdown list completely even when the list needs more render events or bytes than one projection slice permits. Every item MUST appear once, in source order, with its nesting depth, marker style, ordered-list numbering, and indentation identical to a list that fits one slice. Content after the list MUST remain rendered.

#### Scenario: Render a list with more items than one projection slice allows
- **WHEN** the user previews a Markdown document whose bullet or ordered list exceeds one projection slice
- **THEN** the preview shows every list item once, in source order
- **AND** ordered-list numbering continues across items projected in different turns without restarting

#### Scenario: Render nested items across a projection boundary
- **WHEN** an oversized list contains nested sublists whose items land in different projection turns
- **THEN** each item keeps its nesting depth, marker, and indentation
- **AND** task-list checkbox state remains attached to the correct item

#### Scenario: Keep document content after an oversized list
- **WHEN** a Markdown document contains an oversized list followed by further headings or paragraphs
- **THEN** the preview renders that following content
- **AND** the preview does not report that rendering stopped at the list

#### Scenario: A single list item's paragraph exceeds a projection slice
- **WHEN** one list item's body paragraph exceeds one projection slice and has no interior inline-safe checkpoint
- **THEN** that item still renders as an item, with only its overflowing paragraph replaced by one accessible omission marker inside that item
- **AND** every other item in the list is still rendered with its correct marker, numbering, and depth
- **AND** the rest of the document is still rendered
