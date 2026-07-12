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
The system SHALL size each embedded Markdown code block to the effective
available preview text column for the Markdown block context where the code
block appears. A horizontal scrollbar MUST NOT appear merely because the
anchored code-block widget received a narrow natural allocation, because it was
inserted inside a nested block context such as a definition body, list item,
blockquote, alert body, or footnote definition, or because the preview shell was
hidden or moving between Adwaita-native presentations when the code block was
rendered. Horizontal scrolling MUST appear only when a code line is wider than
the effective available preview text column after padding and nested context
margins are accounted for.

#### Scenario: Suppress false horizontal overflow when code fits
- **WHEN** the user previews a Markdown document containing a code block whose longest line fits within the visible preview text column
- **THEN** the rendered code block fills the available preview text column
- **AND** the code text is legible without horizontal scrolling
- **AND** the code block does not allocate as a narrow clipped box

#### Scenario: Suppress false horizontal overflow inside a definition body
- **WHEN** the user previews a Markdown document containing a pulldown-cmark definition-list definition with a nested code block whose longest line fits within the definition body's effective visible text column
- **THEN** the rendered code block fills the definition body's effective available code-block column
- **AND** the code text is legible without horizontal scrolling
- **AND** the code-block surface remains visually nested under the definition rather than escaping to the root preview column
- **AND** the code block does not allocate as a narrow clipped box

#### Scenario: Suppress false horizontal overflow after entering preview-only mode
- **WHEN** the user opens Markdown Preview in preview-only mode for a Markdown document containing the screenshot-style pulldown-cmark definition-list sample with `{ some code, part of Definition 2 }`
- **THEN** the nested code block settles to the definition body's effective available code-block column after the preview shell becomes visible
- **AND** the code block does not allocate as a tiny natural-width clipped box
- **AND** the code text is legible without horizontal scrolling

#### Scenario: Suppress false horizontal overflow after opening the side-by-side preview surface
- **WHEN** the user opens the side-by-side Markdown preview surface for a Markdown document containing a definition-list definition with a nested code block whose longest line fits the preview's visible definition-body column
- **THEN** the nested code block settles to the definition body's effective available code-block column after the side-by-side preview surface becomes visible
- **AND** the code block does not allocate as a tiny natural-width clipped box
- **AND** the code text is legible without horizontal scrolling

#### Scenario: Preserve nested code-block width after late preview allocation
- **WHEN** the Markdown preview receives its width allocation after rendering a definition-list code block
- **THEN** the nested code-block width is recomputed from the current visible preview text column and its nested block context
- **AND** the code block remains legible without stale narrow allocation

#### Scenario: Allow horizontal overflow only for genuinely long code
- **WHEN** the user previews a Markdown document containing a code block with a line wider than the visible preview text column
- **THEN** the rendered code block still fills the available preview text column
- **AND** horizontal scrolling is available inside that code block for the long line
- **AND** vertical scrolling remains owned by the parent Markdown preview

#### Scenario: Allow horizontal overflow only for genuinely long nested code
- **WHEN** the user previews a Markdown document containing a nested definition-list code block with a line wider than the definition body's effective visible text column
- **THEN** the rendered code block fills the definition body's effective available code-block column
- **AND** horizontal scrolling is available inside that nested code block for the long line
- **AND** vertical scrolling remains owned by the parent Markdown preview

#### Scenario: Update code block width after preview layout changes
- **WHEN** the Markdown preview width, preview shell presentation, preview visibility, preferred side-by-side preview width, or readable-column margins change after a code block has been rendered
- **THEN** the code block width is recomputed from the current visible text column and the block's captured Markdown layout context
- **AND** the code block remains legible without stale narrow allocation when the preview becomes wider or moves from hidden to visible

### Requirement: Markdown preview uses one code block background
The system SHALL render each Markdown code block as a single visual surface. The
outer code-block container and the inner source text area MUST use the same
background color so the code text does not appear inside a second mismatched
rectangle.

#### Scenario: Render code text on the same background as the block
- **WHEN** the user previews a Markdown document containing a code block
- **THEN** the code block container and the source text area share one background color
- **AND** syntax-highlighted tokens remain readable against that background

### Requirement: Code-block width repair skips unchanged embed sets
The system SHALL preserve all deferred Markdown code-block geometry repair passes while avoiding repeated embed traversal when the effective text-column width and rendered-embed generation are unchanged. Render, clear, placeholder, and embed membership changes MUST invalidate the cached decision.

#### Scenario: Deferred passes see unchanged layout
- **WHEN** immediate, idle, and timed repair passes observe the same valid text-column width and embed generation
- **THEN** code-block widgets are traversed only for the first required pass
- **AND** the final timed pass still releases waiting visual-readiness callbacks

#### Scenario: Preview rerenders at the same width
- **WHEN** the document rerenders a different set of code blocks while the preview width stays constant
- **THEN** the embed-generation change invalidates the fast path
- **AND** every new nested and root code block receives its correct width request

#### Scenario: Nested context width changes
- **WHEN** a layout change alters a code block's effective nested column width
- **THEN** the changed valid width triggers a complete code-block refresh
- **AND** the existing nested overflow and no-false-scrollbar requirements remain satisfied
