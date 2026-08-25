## ADDED Requirements

### Requirement: Markdown preview renders blockquotes larger than one projection slice
The system SHALL render a Markdown blockquote completely even when the quote needs more render events or bytes than one projection slice permits. Every quoted paragraph MUST appear once, in source order, with the same rail glyphs, nesting depth, and indentation a quote that fits one slice would show. Content after the blockquote MUST remain rendered.

#### Scenario: Render a blockquote with more paragraphs than one projection slice allows
- **WHEN** the user previews a Markdown document whose blockquote exceeds one projection slice
- **THEN** the preview shows every quoted paragraph once, in source order
- **AND** each paragraph keeps its depth-aware rail prefix across projection turns

#### Scenario: Render nested quote depth across a projection boundary
- **WHEN** an oversized blockquote contains nested quotes whose paragraphs land in different projection turns
- **THEN** the rendered rail depth matches the source nesting for every paragraph
- **AND** a typed GFM alert callout keeps its card rendering rather than degrading to generic rails

#### Scenario: Keep document content after an oversized blockquote
- **WHEN** a Markdown document contains an oversized blockquote followed by further blocks
- **THEN** the preview renders that following content
- **AND** the preview does not report that rendering stopped at the blockquote
