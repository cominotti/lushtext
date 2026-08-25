## ADDED Requirements

### Requirement: Markdown preview renders definition lists larger than one projection slice
The system SHALL render a Markdown definition list completely even when it needs more render events or bytes than one projection slice permits. Every title and every definition body MUST appear once, in source order, with the same indentation and paragraph flow a definition list that fits one slice would show. Content after the definition list MUST remain rendered.

#### Scenario: Render a definition list with more entries than one projection slice allows
- **WHEN** the user previews a Markdown document whose definition list exceeds one projection slice
- **THEN** the preview shows every title and definition body once, in source order
- **AND** definition indentation and paragraph flow are preserved across entries projected in different turns

#### Scenario: Keep document content after an oversized definition list
- **WHEN** a Markdown document contains an oversized definition list followed by further blocks
- **THEN** the preview renders that following content
- **AND** the preview does not report that rendering stopped at the definition list

#### Scenario: One definition body exceeds a projection slice
- **WHEN** one definition body's inline content exceeds one projection slice and has no interior inline-safe checkpoint
- **THEN** only that definition body is replaced by one accessible omission entry at its own position
- **AND** the other titles and definitions in the list are still rendered
- **AND** the rest of the document is still rendered
