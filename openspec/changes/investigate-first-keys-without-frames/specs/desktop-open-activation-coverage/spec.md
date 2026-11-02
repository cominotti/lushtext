## ADDED Requirements

### Requirement: An opened document takes keyboard focus
A document that desktop, CLI or file-manager activation opens, or that the startup session restore selects, SHALL take keyboard focus once its page is selected. The first characters typed after it opens then go into that document. The handoff SHALL apply only while the window's focus is unset or on the window chrome (header bar, tab strip, status bar), and SHALL NOT take focus from a dialog, a dismissible transient surface, or a widget the user moved focus to.

#### Scenario: Typing after a command-line open goes into the document
- **WHEN** the application receives an open activation for a supported file and the window composes
- **THEN** the window's keyboard focus is the opened document's text view, not GTK's initial pick on the chrome

#### Scenario: Typing after a session restore goes into the document
- **WHEN** a window restores a session with a document tab at startup
- **THEN** the restored, selected document's text view has keyboard focus

#### Scenario: The handoff leaves a user's focus alone
- **WHEN** a dialog or a transient surface is up, or the focus is in the sidebar, the properties panel or an editor
- **THEN** opening or restoring a document does not move the focus
