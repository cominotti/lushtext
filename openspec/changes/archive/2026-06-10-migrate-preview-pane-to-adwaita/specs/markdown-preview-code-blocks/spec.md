## MODIFIED Requirements

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
