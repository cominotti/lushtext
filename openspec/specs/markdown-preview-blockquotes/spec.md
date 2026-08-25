# markdown-preview-blockquotes Specification

## Purpose
TBD - created by archiving change harden-markdown-blockquote-preview. Update Purpose after archive.
## Requirements
### Requirement: Markdown preview renders generic blockquotes with visible rails
The system SHALL render generic CommonMark blockquotes in the Markdown preview as quoted content with visible quote rails. The rendered preview MUST keep the quoted text in source order and MUST NOT display the raw `>` marker syntax as document text.

#### Scenario: Render a simple generic blockquote
- **WHEN** the user previews a Markdown document containing a generic blockquote
- **THEN** the preview shows the quoted text in source order
- **AND** the quoted content has a visible quote rail
- **AND** the raw `>` marker does not appear as rendered document text

#### Scenario: Preserve surrounding content flow around a blockquote
- **WHEN** a Markdown document contains body paragraphs before and after a generic blockquote
- **THEN** the preview keeps the paragraphs and quoted content in source order
- **AND** the quote rail applies only to the quoted block content

### Requirement: Markdown preview preserves nested blockquote hierarchy
The system SHALL render nested generic blockquotes with depth-aware quote rails and indentation so parent and child quote levels remain visually distinguishable. Nested quote depth MUST be preserved for both adjacent marker syntax and spaced marker syntax.

#### Scenario: Render nested blockquotes from adjacent markers
- **WHEN** the user previews Markdown containing nested generic blockquotes written with adjacent greater-than markers such as `>>>`
- **THEN** the preview shows each quoted line in source order
- **AND** each nested quote level has an additional visible quote rail or equivalent depth cue
- **AND** deeper quote content is visually indented relative to its parent quote content

#### Scenario: Render nested blockquotes from spaced markers
- **WHEN** the user previews Markdown containing nested generic blockquotes written with spaced markers such as `> > >`
- **THEN** the preview treats the spaced marker form as the same quote depth as the adjacent marker form
- **AND** each nested quote level remains visually distinguishable from the levels above it

### Requirement: Markdown preview preserves inline formatting inside generic blockquotes
The system SHALL preserve supported inline Markdown formatting inside generic blockquotes. Generic blockquote body text MUST remain distinguishable from inline emphasis so emphasis, strong text, inline code, and supported links keep their normal rendered meaning inside quoted content.

#### Scenario: Render formatted inline content inside a blockquote
- **WHEN** the user previews a generic blockquote containing emphasis, strong text, inline code, and a supported link
- **THEN** the preview shows the quoted content with a visible quote rail
- **AND** each supported inline formatting span remains distinguishable inside the quote
- **AND** the rendered quote remains read-only

#### Scenario: Keep GitHub alert callouts distinct from generic blockquotes
- **WHEN** the user previews a GitHub alert callout such as `[!NOTE]`
- **THEN** the preview renders the typed alert callout using its alert presentation
- **AND** the alert callout remains distinguishable from a generic rail-styled blockquote

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
