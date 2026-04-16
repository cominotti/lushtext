# invisible-character-visualization Specification

## Purpose
TBD - created by archiving change encoding-toolkit. Update Purpose after archive.
## Requirements
### Requirement: Users can toggle invisible-character visibility modes per document
The system SHALL provide `Off`, `Whitespace Only`, and `All` invisible-character visibility modes for the active editor. Changing modes MUST not modify the document contents or its modified state.

#### Scenario: Enable whitespace-only visibility
- **WHEN** the user enables `Whitespace Only` mode for the active editor
- **THEN** the editor visibly distinguishes ordinary spaces and tabs from surrounding text
- **AND** the underlying buffer text remains unchanged

#### Scenario: Disable invisible-character visibility
- **WHEN** the user returns the active editor to `Off` mode
- **THEN** the editor stops drawing invisible-character hints for that document
- **AND** the document content and saved bytes remain unchanged

### Requirement: All mode reveals encoding-adjacent invisible anomalies
When `All` mode is active, the system SHALL make encoding-adjacent invisible anomalies such as non-breaking spaces, zero-width characters, BOMs, or line-ending boundaries discoverable to the user. Those cues MUST stay visually distinct from ordinary whitespace without injecting synthetic text into the document buffer.

#### Scenario: Reveal a non-breaking space or zero-width character
- **WHEN** the active document contains a non-breaking space or zero-width character and the user enables `All` mode
- **THEN** the editor or its adjacent file-health affordances make that anomaly visibly distinguishable from ordinary spaces
- **AND** the user can inspect the document without altering the stored content

#### Scenario: Reveal line-ending boundaries in all mode
- **WHEN** the active document is shown with `All` mode enabled
- **THEN** the system makes line-ending boundaries discoverable in a way that matches the document's current line-ending state
- **AND** mixed line endings remain distinguishable from uniform line endings

