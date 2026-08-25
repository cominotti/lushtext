## ADDED Requirements

### Requirement: Code block size never truncates the Markdown preview
A code block's size SHALL never stop the preview or discard content after it. A code block within the preview's code-block byte budget MUST render whole as one continuous padded surface with source text, blank lines, and line order preserved, and MUST NOT split into multiple highlighted regions or multiple code surfaces. A code block beyond that budget MUST resolve to the existing single in-place fallback presentation reporting its true source size, even when the planner stopped retaining the block's text before its end. Content after the code block MUST remain rendered in every case.

#### Scenario: Render a code block within the code-block byte budget
- **WHEN** the user previews a Markdown document containing a fenced or indented code block whose source stays within the preview's code-block byte budget
- **THEN** the preview shows one continuous padded code block surface containing every line in source order
- **AND** syntax highlighting for a resolvable language applies to the whole surface
- **AND** no omission marker appears for that block
- **AND** this holds whether the block fits one projection turn or is projected across several turns, as an indented block with more lines than the per-slice event budget must be

#### Scenario: Code block exceeds the code-block byte budget
- **WHEN** a code block's source bytes exceed the preview's code-block byte budget, including when planning stopped retaining its text at the carried embedded-block byte ceiling
- **THEN** that code block resolves to the existing single in-place fallback presentation naming its true byte count and the budget
- **AND** the preview does not show a partially rendered code surface for that block
- **AND** the preview reports a complete render, without an added omission marker or omission count for that block

#### Scenario: Keep document content after a very large code block
- **WHEN** a Markdown document contains a code block far larger than any preview budget, followed by further prose or blocks
- **THEN** the preview renders that following content
- **AND** the preview does not report that rendering stopped at the code block
