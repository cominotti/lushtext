## ADDED Requirements

### Requirement: Empty Notes browser remains readable
The system SHALL present the empty `Browse Notes...` browser as a readable modal browser state. When there are no bookmarks, document notes, folder notes, workspace folders, or open-tab note rows to show, the empty Notes browser MUST keep a stable compact-browser size, MUST allocate enough width for the empty-state title and description to remain legible, MUST allocate enough height for the empty-state content to fit without vertical scrolling, and MUST NOT collapse to the natural width of the empty-state content. The empty browser MUST continue to avoid creating fake sidebar rows, note sidecars, bookmarks, workspaces, or document-note data merely by being opened.

#### Scenario: Open empty Notes browser from a no-workspace window
- **WHEN** the window has no active editor
- **AND** no workspace folders are restored
- **AND** the user activates `Browse Notes...`
- **THEN** the Notes browser opens an empty state titled `No notes yet`
- **AND** the empty-state modal keeps a readable compact-browser allocation
- **AND** the modal does not materialize a Notes sidebar or fake note rows

#### Scenario: Empty Notes browser sizing does not follow status-page collapse
- **WHEN** the empty Notes browser is presented
- **THEN** the dialog uses its intended compact-browser content dimensions rather than following the natural size of the status page
- **AND** the empty-state title and description remain readable within the dialog bounds
- **AND** the empty-state content fits without a vertical scrollbar

#### Scenario: Empty Notes browser preserves dismissal behavior
- **WHEN** the empty Notes browser is visible
- **THEN** the visible close control dismisses the dialog
- **AND** pressing Escape dismisses the dialog
