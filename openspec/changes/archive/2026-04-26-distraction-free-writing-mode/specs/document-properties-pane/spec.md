## ADDED Requirements

### Requirement: Focus Mode suppresses document properties without discarding requested state
The system SHALL keep `F9` and the document-properties action owned by the document properties surface while Focus Mode is active. Focus Mode MUST suppress the rendered document-properties surface without overwriting the user's requested document-properties visibility state.

#### Scenario: Existing open properties are restored after focus
- **WHEN** document properties are visible in a spacious layout
- **AND** the user enters Focus Mode
- **THEN** document properties are no longer rendered
- **AND** the document-properties requested visibility remains open
- **WHEN** the user exits Focus Mode without explicitly changing document-properties state
- **THEN** document properties render again according to the current adaptive layout

#### Scenario: F9 changes requested properties state while focused
- **WHEN** Focus Mode is active and document properties were requested open before entry
- **AND** the user presses `F9`
- **THEN** the document-properties requested state changes to closed
- **AND** the document-properties surface remains suppressed while Focus Mode is active
- **WHEN** the user exits Focus Mode
- **THEN** document properties remain closed

#### Scenario: F9 can request properties for after focus
- **WHEN** Focus Mode is active and document properties are not requested open
- **AND** the user presses `F9`
- **THEN** the document-properties requested state changes to open
- **AND** the document-properties surface remains suppressed while Focus Mode is active
- **WHEN** the user exits Focus Mode
- **THEN** document properties render according to the current adaptive layout
