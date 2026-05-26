# markdown-preview-definition-lists Specification

## Purpose

Render pulldown-cmark definition-list events in the native Markdown preview while
preserving source order, inline formatting, nested block behavior, and parser
boundaries.

## Requirements

### Requirement: Markdown preview renders pulldown-cmark definition lists
The system SHALL render pulldown-cmark 0.13 definition-list events in the native Markdown preview as structured term and definition content. Rendered definition lists MUST preserve source order, MUST make terms visually distinguishable from definitions, and MUST NOT show raw colon definition markers as document text.

#### Scenario: Render a simple definition list
- **WHEN** the user previews a Markdown document containing `Term` followed by `: Definition`
- **THEN** the preview renders `Term` as a definition-list term
- **AND** the preview renders `Definition` as that term's definition
- **AND** the raw `:` definition marker is not shown as document text

#### Scenario: Render multiple terms and definitions in source order
- **WHEN** the user previews a Markdown document containing multiple pulldown-cmark definition-list terms and definitions
- **THEN** each term and definition appears in the same order as the source document
- **AND** repeated definitions for the same term remain separate readable definition entries

### Requirement: Markdown preview preserves inline formatting inside definition lists
The system SHALL preserve supported inline Markdown formatting inside definition-list terms and definitions through the same inline rendering path used by ordinary Markdown prose. Supported inline formatting MUST include emphasis, strong text, strikethrough, inline code, links, and footnote references where pulldown-cmark emits the corresponding events.

#### Scenario: Render inline markup in a definition-list term
- **WHEN** the user previews a Markdown document containing a definition-list term with supported inline formatting
- **THEN** the rendered term keeps the supported inline formatting
- **AND** the term remains visually distinguishable from its definition body

#### Scenario: Render inline markup in a definition body
- **WHEN** the user previews a Markdown document containing a definition body with supported inline formatting
- **THEN** the rendered definition keeps the supported inline formatting
- **AND** the definition indentation and wrapping remain readable

### Requirement: Markdown preview preserves nested blocks inside definitions
The system SHALL render supported nested Markdown blocks inside pulldown-cmark definition-list definitions without flattening them into plain text. Nested paragraphs, ordered and unordered lists, blockquotes, and fenced or indented code blocks MUST remain readable inside the definition body and MUST preserve their existing Markdown preview behavior.

#### Scenario: Render multiple paragraphs in a definition
- **WHEN** the user previews a Markdown document containing a definition with multiple paragraphs
- **THEN** each paragraph appears inside the definition body in source order
- **AND** paragraph spacing remains readable without duplicating blank rows

#### Scenario: Render a blockquote inside a definition
- **WHEN** the user previews a Markdown document containing a blockquote inside a definition
- **THEN** the blockquote renders with the existing Markdown preview blockquote styling
- **AND** the blockquote remains visually nested under the definition rather than escaping into a top-level paragraph

#### Scenario: Render a code block inside a definition without false horizontal overflow
- **WHEN** the user previews a Markdown document containing a definition with a code block whose longest line fits within the visible preview text column
- **THEN** the code block renders as one embedded Markdown code-block surface inside the definition
- **AND** the code block does not show a horizontal scrollbar merely because it is nested inside the definition list

### Requirement: Markdown preview follows pulldown-cmark definition-list boundaries
The system SHALL recognize definition lists only through pulldown-cmark's enabled definition-list parser events. Markdown syntax that pulldown-cmark does not emit as a definition list MUST remain readable ordinary Markdown output and MUST NOT be reinterpreted by LushText as a definition list.

#### Scenario: Leave markdown-it compact tilde syntax as ordinary text
- **WHEN** the user previews Markdown containing compact markdown-it-style definition syntax such as `Term ~ Definition`
- **THEN** the preview does not render that line as a definition list
- **AND** the text remains readable according to pulldown-cmark's emitted events

#### Scenario: Preserve ordinary colon prose outside definition-list events
- **WHEN** the user previews Markdown containing ordinary prose with a colon that pulldown-cmark does not emit as a definition-list marker
- **THEN** the preview does not render that prose as a definition list
- **AND** the colon remains part of the rendered text
