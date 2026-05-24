# markdown-preview-code-blocks Specification

## Purpose

Render Markdown fenced and indented code blocks in the native preview as readable, syntax-aware block surfaces that preserve source text, avoid false horizontal overflow, and match the active editor palette.

## Requirements

### Requirement: Markdown preview renders code blocks as continuous padded blocks
The system SHALL render Markdown fenced and indented code blocks in the preview
as single read-only code block surfaces with visible interior padding. A single
Markdown code block MUST preserve its source text, including blank lines, inside
one continuous visual block and MUST NOT split into multiple highlighted
regions.

#### Scenario: Render a fenced code block with an empty line
- **WHEN** the user previews a Markdown document containing one fenced code block with an empty line between code lines
- **THEN** the preview shows one continuous code block surface for the entire fenced block
- **AND** the empty line remains inside that same code block surface
- **AND** the code text remains in source order

#### Scenario: Render an indented code block
- **WHEN** the user previews a Markdown document containing an indented code block
- **THEN** the preview shows the code as one padded read-only code block surface
- **AND** the rendered code block remains distinct from surrounding prose

### Requirement: Markdown preview syntax-highlights supported fenced languages
The system SHALL use fenced code block info strings to apply GtkSourceView syntax
highlighting when the language can be resolved locally. The system MUST treat
the first word of the fenced info string as the language hint and MUST fall back
to readable plain monospaced code when no supported language is available.

#### Scenario: Highlight a supported fenced language
- **WHEN** the user previews a Markdown document containing a fenced code block marked with a supported language such as `js`
- **THEN** the preview renders the block with syntax highlighting for that language
- **AND** the highlighted code remains inside one continuous padded code block surface

#### Scenario: Fall back for an unsupported fenced language
- **WHEN** the user previews a Markdown document containing a fenced code block marked with an unsupported language
- **THEN** the preview renders the code as readable monospaced text
- **AND** the unsupported language does not prevent the surrounding Markdown preview from rendering

### Requirement: Markdown preview keeps inline code on the inline path
The system SHALL continue rendering Markdown inline code spans as inline content
within the surrounding paragraph. Inline code MUST NOT become a block-level
surface and MUST remain visually distinct from normal prose.

#### Scenario: Render inline code beside prose
- **WHEN** the user previews a Markdown paragraph containing an inline code span
- **THEN** the preview keeps the code span on the same rendered line as the surrounding prose
- **AND** the inline code span remains visually distinct from normal prose

### Requirement: Markdown preview sizes code blocks to the available text column
The system SHALL size each embedded Markdown code block to the available preview
text column width. A horizontal scrollbar MUST NOT appear merely because the
anchored code-block widget received a narrow natural allocation. Horizontal
scrolling MAY appear only when a code line is wider than the available preview
text column after padding is accounted for.

#### Scenario: Suppress false horizontal overflow when code fits
- **WHEN** the user previews a Markdown document containing a code block whose longest line fits within the visible preview text column
- **THEN** the rendered code block fills the available preview text column
- **AND** the code text is legible without horizontal scrolling
- **AND** the code block does not allocate as a narrow clipped box

#### Scenario: Allow horizontal overflow only for genuinely long code
- **WHEN** the user previews a Markdown document containing a code block with a line wider than the visible preview text column
- **THEN** the rendered code block still fills the available preview text column
- **AND** horizontal scrolling is available inside that code block for the long line
- **AND** vertical scrolling remains owned by the parent Markdown preview

#### Scenario: Update code block width after preview layout changes
- **WHEN** the Markdown preview width or readable-column margins change after a code block has been rendered
- **THEN** the code block width is recomputed from the current visible text column
- **AND** the code block remains legible without stale narrow allocation when the preview becomes wider

### Requirement: Markdown preview uses one code block background
The system SHALL render each Markdown code block as a single visual surface. The
outer code-block container and the inner source text area MUST use the same
background color so the code text does not appear inside a second mismatched
rectangle.

#### Scenario: Render code text on the same background as the block
- **WHEN** the user previews a Markdown document containing a code block
- **THEN** the code block container and the source text area share one background color
- **AND** syntax-highlighted tokens remain readable against that background
