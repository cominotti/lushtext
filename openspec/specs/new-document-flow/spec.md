# new-document-flow Specification

## Purpose
Define how creating a new untitled document selects the editor, handles keyboard focus, and presents the new-document shortcut across user-facing surfaces.

## Requirements
### Requirement: New document creation focuses the editor
The system SHALL move keyboard focus to the source editor for the newly created untitled document whenever the user activates the new document action. Focus restoration MUST target the newly selected tab and MUST NOT move focus to another editor if the selected tab changes before the focus handoff completes.

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

### Requirement: New document shortcut uses Ctrl+N only
The system SHALL expose `Ctrl+N` as the only keyboard shortcut for creating a new untitled document. The previous `Ctrl+T` shortcut MUST NOT create a document, select a new tab, or be advertised as a new document shortcut.

#### Scenario: Ctrl+N creates a new document
- **WHEN** the main window has focus and the user presses `Ctrl+N`
- **THEN** the system creates a new untitled document tab
- **AND** keyboard focus is inside that document's source editor

#### Scenario: Ctrl+T no longer creates a document
- **WHEN** the main window has focus and the user presses `Ctrl+T`
- **THEN** the system does not create a new untitled document tab
- **AND** the selected tab does not change because of `Ctrl+T`

### Requirement: New document surfaces use consistent wording and shortcut metadata
The system SHALL present the untitled-document creation action as `New File` or `New Document` in user-facing surfaces. The command palette, shortcut overlay, primary menu, header tooltip, and README shortcut table MUST advertise `Ctrl+N` and MUST NOT advertise `Ctrl+T` for this action.

#### Scenario: User-facing shortcut surfaces advertise Ctrl+N
- **WHEN** the user opens the command palette, primary menu, shortcut overlay, or README shortcut table
- **THEN** the new document action is described as creating a new file or document
- **AND** the advertised shortcut is `Ctrl+N`
- **AND** `Ctrl+T` is not advertised for creating a new document
