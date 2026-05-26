## MODIFIED Requirements

### Requirement: Markdown preview sizes code blocks to the available text column
The system SHALL size each embedded Markdown code block to the effective
available preview text column for the Markdown block context where the code
block appears. A horizontal scrollbar MUST NOT appear merely because the
anchored code-block widget received a narrow natural allocation or because it
was inserted inside a nested block context such as a definition body, list item,
blockquote, alert body, or footnote definition. Horizontal scrolling MUST appear
only when a code line is wider than the effective available preview text column
after padding and nested context margins are accounted for.

#### Scenario: Suppress false horizontal overflow when code fits
- **WHEN** the user previews a Markdown document containing a code block whose longest line fits within the visible preview text column
- **THEN** the rendered code block fills the available preview text column
- **AND** the code text is legible without horizontal scrolling
- **AND** the code block does not allocate as a narrow clipped box

#### Scenario: Suppress false horizontal overflow inside a definition body
- **WHEN** the user previews a Markdown document containing a definition-list definition with a nested code block whose longest line fits within the definition body's effective visible text column
- **THEN** the rendered code block fills the definition body's effective available code-block column
- **AND** the code text is legible without horizontal scrolling
- **AND** the code-block surface remains visually nested under the definition rather than escaping to the root preview column
- **AND** the code block does not allocate as a narrow clipped box

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
- **WHEN** the Markdown preview width or readable-column margins change after a code block has been rendered
- **THEN** the code block width is recomputed from the current visible text column
- **AND** the code block remains legible without stale narrow allocation when the preview becomes wider
