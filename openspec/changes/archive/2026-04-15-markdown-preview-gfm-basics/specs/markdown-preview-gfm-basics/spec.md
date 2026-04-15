## ADDED Requirements

### Requirement: Markdown preview renders task list state
The system SHALL render GitHub-flavored task list items in the Markdown preview with distinct checked and unchecked markers instead of showing raw source markers or flattening them into plain list text.

#### Scenario: Render checked and unchecked task list items
- **WHEN** the user previews a Markdown document containing checked and unchecked task list items
- **THEN** the preview shows those items with distinct checked and unchecked markers
- **AND** the task item text remains in the correct list order

### Requirement: Markdown preview renders GitHub alert callouts
The system SHALL render GitHub alert callouts in the Markdown preview as visually distinct callout blocks that preserve the alert kind and the block's body content.

#### Scenario: Render a note callout without raw marker syntax
- **WHEN** the user previews Markdown containing a GitHub alert callout such as `[!NOTE]`
- **THEN** the preview shows a note-style callout title and body
- **AND** the raw `[!NOTE]` marker text does not appear in the rendered preview

#### Scenario: Preserve inline formatting inside a callout
- **WHEN** the body of an alert callout contains supported inline formatting such as emphasis, links, or inline code
- **THEN** the preview keeps that inline content in the rendered callout body
- **AND** the callout remains distinguishable from a generic blockquote

### Requirement: Markdown preview renders footnote references and definitions
The system SHALL render supported footnote references and definitions in the Markdown preview so the relationship between inline references and their definition blocks remains readable.

#### Scenario: Render a referenced footnote
- **WHEN** the user previews a Markdown document with an inline footnote reference and a matching definition
- **THEN** the preview shows an inline footnote reference marker in place of the raw `[^label]` syntax
- **AND** the matching definition renders as a footnote block with the same reference number

#### Scenario: Preserve footnote content flow
- **WHEN** a footnote definition contains one or more paragraphs or supported nested Markdown content
- **THEN** the preview keeps that footnote content readable inside the rendered definition block
- **AND** surrounding document content remains in its original order
