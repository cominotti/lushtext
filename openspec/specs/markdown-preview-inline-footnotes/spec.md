# markdown-preview-inline-footnotes Specification

## Purpose
Define native Markdown preview support for markdown-it-style inline footnotes so preview rendering can show generated footnote markers and definitions without mutating the source document text.

## Requirements

### Requirement: Markdown preview renders inline footnote definitions
The system SHALL render markdown-it-style inline footnote definitions written as `^[...]` in the native Markdown preview. The rendered preview MUST replace the raw inline footnote source with a footnote reference marker and MUST render the captured inline body as a matching footnote definition.

#### Scenario: Render a simple inline footnote
- **WHEN** the user previews a Markdown document containing `Body text^[Inline note].`
- **THEN** the preview shows `Body text` followed by a rendered footnote reference marker instead of the raw `^[Inline note]` source
- **AND** the preview renders a footnote definition containing `Inline note` with the same reference number

#### Scenario: Preserve supported inline formatting inside the generated definition
- **WHEN** the user previews an inline footnote body containing supported inline Markdown such as emphasis, links, or inline code
- **THEN** the generated footnote definition preserves that supported inline formatting through the same rendering path used by reference-style footnote definitions
- **AND** the inline footnote marker remains in the original prose position

### Requirement: Markdown preview preserves mixed footnote numbering
The system SHALL preserve readable numbering when inline footnote definitions appear in the same document as reference-style footnote references and definitions. Each rendered reference marker MUST match the number shown by its corresponding rendered definition.

#### Scenario: Render inline and reference-style footnotes together
- **WHEN** the user previews a Markdown document containing both `^[Inline note]` and reference-style footnotes such as `[^label]` with `[^label]: Reference note`
- **THEN** each inline and reference-style footnote marker has a matching rendered definition number
- **AND** the existing reference-style footnote source does not render as raw `[^label]` syntax
- **AND** generated internal labels for inline footnotes do not appear in the preview text

#### Scenario: Preserve source document content
- **WHEN** the user previews a document containing inline footnotes
- **THEN** the system renders the inline footnotes without modifying the source buffer text
- **AND** saving the document preserves the user's original `^[...]` source syntax

### Requirement: Markdown preview limits inline footnote recognition to prose text
The system SHALL recognize inline footnote definitions only in Markdown prose text contexts. The system MUST NOT create footnote markers or generated definitions from escaped inline-footnote syntax, inline code, fenced code blocks, indented code blocks, raw HTML, or other non-prose parser regions.

#### Scenario: Leave escaped inline footnote syntax literal
- **WHEN** the user previews Markdown containing escaped syntax such as `\^[Not a footnote]`
- **THEN** the preview does not create a footnote marker or definition from that escaped syntax
- **AND** the rendered prose keeps the literal `^[Not a footnote]` text according to the existing escape rendering behavior

#### Scenario: Ignore inline footnote syntax inside code
- **WHEN** the user previews Markdown containing `^[Not a footnote]` inside an inline code span, a fenced code block, or an indented code block
- **THEN** the preview does not create a footnote marker or definition from those code occurrences
- **AND** the code content remains on the existing code rendering path

#### Scenario: Ignore inline footnote syntax inside raw HTML
- **WHEN** the user previews Markdown containing `^[Not a footnote]` inside raw HTML content
- **THEN** the preview does not create a footnote marker or generated definition from the raw HTML occurrence
- **AND** raw HTML continues to follow the preview's existing raw-HTML handling
