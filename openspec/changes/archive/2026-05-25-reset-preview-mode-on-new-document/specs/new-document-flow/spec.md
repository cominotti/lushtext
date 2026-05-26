## MODIFIED Requirements

### Requirement: New document creation focuses the editor
The system SHALL move keyboard focus to the source editor for the newly created untitled document whenever the user activates the new document action. If Markdown preview-only mode is active, the system MUST leave preview-only mode, reveal the source editor surface, and clear preview-only action state before focus restoration completes. Focus restoration MUST target the newly selected tab and MUST NOT move focus to another editor if the selected tab changes before the focus handoff completes.

#### Scenario: Shortcut-created document accepts immediate typing
- **WHEN** the main window has focus and the user activates the new document shortcut
- **THEN** the system creates a new untitled document tab
- **AND** the newly created document is the selected tab
- **AND** keyboard focus is inside that document's source editor

#### Scenario: Header or menu-created document focuses its editor
- **WHEN** the user activates the new document action from the header button or primary menu
- **THEN** the system creates a new untitled document tab
- **AND** keyboard focus is inside the newly created document's source editor

#### Scenario: Command palette-created document focuses its editor after palette cleanup
- **WHEN** the user activates the new document command from the command palette
- **THEN** the system creates a new untitled document tab
- **AND** the command palette closes
- **AND** keyboard focus is inside the newly created document's source editor

#### Scenario: Delayed focus does not target a stale tab
- **WHEN** a new untitled document is created
- **AND** the selected tab changes before the focus handoff completes
- **THEN** the system does not move keyboard focus back to the previously created document

#### Scenario: Preview-only mode is cleared for a new document
- **WHEN** Markdown preview-only mode is active for the selected tab
- **AND** the user activates the new document action
- **THEN** the system creates a new untitled document tab
- **AND** Markdown preview-only mode is no longer active
- **AND** the source editor surface for the newly selected tab is visible and focused
