## ADDED Requirements

### Requirement: Markdown preview renders ATX heading hierarchy
The system SHALL render ATX Markdown headings from level 1 through level 6 in the Markdown preview as heading text with visible hierarchy. The rendered preview MUST apply the matching heading-level style to each heading's text and MUST NOT display the leading `#` marker sequence as rendered document text.

#### Scenario: Render all ATX heading levels
- **WHEN** the user previews a Markdown document containing ATX headings from `#` through `######`
- **THEN** the preview shows each heading's text in source order
- **AND** each heading's text is styled with its matching heading level from H1 through H6
- **AND** the leading `#` marker sequence does not appear as rendered document text

#### Scenario: Preserve content flow around ATX headings
- **WHEN** the user previews Markdown containing paragraphs before, between, and after ATX headings
- **THEN** the preview keeps those paragraphs and headings in source order
- **AND** each heading remains visually distinct from surrounding body text

### Requirement: Markdown preview renders Setext heading hierarchy
The system SHALL render valid Setext H1 and H2 Markdown headings in the Markdown preview as heading text with visible hierarchy. The rendered preview MUST apply the matching H1 or H2 style to the heading text and MUST NOT display the Setext underline marker as rendered document text.

#### Scenario: Render Setext H1 and H2 headings
- **WHEN** the user previews a Markdown document containing a Setext H1 underline using `===` and a Setext H2 underline using `---`
- **THEN** the preview shows each heading's text in source order
- **AND** the Setext H1 text is styled as H1
- **AND** the Setext H2 text is styled as H2
- **AND** the underline marker lines do not appear as rendered document text

### Requirement: Markdown preview is discoverable from the primary menu
The system SHALL expose rendered Markdown preview through a visible primary-menu action labeled `Markdown Preview` in addition to the existing `Alt+P` keyboard shortcut. Activating the visible action MUST render the active Markdown document's current buffer content in preview-only mode.

#### Scenario: Activate Markdown preview from the primary menu
- **WHEN** a Markdown document is active and the user activates the primary-menu `Markdown Preview` action
- **THEN** the window enters preview-only mode
- **AND** the preview renders the active document's current buffer content
- **AND** Markdown headings in that buffer render according to the heading hierarchy requirements

#### Scenario: Preserve explicit preview activation on startup
- **WHEN** LushText starts with a Markdown document open or restores a session containing Markdown documents
- **THEN** the source editor remains the default visible document surface
- **AND** rendered Markdown preview appears only after the user activates the visible Markdown Preview action, the existing shortcut, or another explicit preview control

#### Scenario: Show placeholder for non-Markdown documents
- **WHEN** a non-Markdown document is active and the user activates the primary-menu `Markdown Preview` action
- **THEN** the window enters preview-only mode
- **AND** the preview shows the non-Markdown placeholder instead of rendering the document as Markdown

### Requirement: Markdown source headings are visually emphasized
The system SHALL make Markdown heading lines visibly distinct in the editable source editor while preserving the raw heading marker syntax as editable text. The source editor MUST NOT hide heading markers or automatically replace source lines with rendered preview blocks.

#### Scenario: Edit Markdown headings with visible hierarchy cues
- **WHEN** the user opens or edits a Markdown document containing heading lines
- **THEN** the editor keeps the raw heading marker syntax visible
- **AND** heading lines use a larger, bold heading style distinct from body text
- **AND** the rendered preview remains an explicit action rather than replacing the source editor automatically
