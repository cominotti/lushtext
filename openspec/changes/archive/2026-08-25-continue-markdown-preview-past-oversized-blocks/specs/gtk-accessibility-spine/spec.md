## ADDED Requirements

### Requirement: Omitted Markdown preview content is announced rather than silently missing
When Markdown preview replaces content it cannot render with a marker, the marker SHALL be an accessible object with a name or description that identifies it as omitted preview content and states why. This requirement covers user-visible omissions only. An embedded block that the preview replaces with its own in-place fallback presentation MUST be announced through that fallback alone, and MUST NOT also produce an omission marker or contribute to an omission count. A marker replacing a whole top-level block and a marker replacing one unit inside a still-rendered container (a table row, list item, code-block run, quoted paragraph, or definition body) MUST both be reachable and self-describing at their own position. The preview's terminal state SHALL distinguish a complete preview containing omissions from a preview whose rendering stopped at a global budget, and MUST report the number of omissions once rather than announcing each marker as it is projected.

#### Scenario: Marker replaces a whole block the preview cannot render
- **WHEN** Markdown preview omits a top-level block that exceeds its per-slice budgets and has no inline-safe checkpoint
- **THEN** assistive technology reaches a named marker at that position in the document
- **AND** its description states that the block was omitted and why

#### Scenario: Marker replaces one unit inside a rendered container
- **WHEN** Markdown preview omits one row, item, quoted paragraph, or definition body while rendering that container's other units
- **THEN** assistive technology reaches the marker at that unit's position inside the container, as a named object where that container's units are themselves accessible objects and through the preview's text interface where they are rendered as buffer text
- **AND** the container's surrounding units remain readable and correctly ordered
- **AND** the marker names the omitted unit rather than implying the whole container was dropped

#### Scenario: Embedded block resolves to its own in-place fallback
- **WHEN** a table or code block is replaced by the preview's own fallback presentation because it exceeds that block type's widget budget
- **THEN** assistive technology reaches only that fallback, which names the block and its true size
- **AND** no additional omission marker is present for the same block
- **AND** the preview's terminal description still reports a complete preview

#### Scenario: Complete preview containing omissions
- **WHEN** a preview generation finishes with one or more omissions
- **THEN** the preview surface's accessible description reports a complete preview with the number of omissions
- **AND** it does not claim that rendering stopped before the end of the document

#### Scenario: Preview stops at a global budget
- **WHEN** a preview generation stops because a global source, event, retained-byte, embed-descriptor, depth, or inline-footnote budget was exceeded
- **THEN** the accessible description names that stopped state and its reason
- **AND** it is distinguishable from the complete-with-omissions state
