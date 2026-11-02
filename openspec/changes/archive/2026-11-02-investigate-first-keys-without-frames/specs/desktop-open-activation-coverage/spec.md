## ADDED Requirements

### Requirement: An opened document takes keyboard focus
A document that desktop, CLI or file-manager activation opens or selects in a window, or that the startup session restore creates and selects, SHALL take keyboard focus once its page is selected. The first characters typed after it opens then go into that document. The handoff SHALL apply only when the activation added or selected a page in that window (not when another window owned the path and was presented instead), and only while the window's focus is unset or on the window's own chrome (header bar, tab strip, status bar). A widget inside a popover or menu the chrome owns SHALL NOT count as chrome. The handoff SHALL NOT take focus from a dialog, a dismissible transient surface, a popover, or a widget the user moved focus to, and SHALL NOT move focus at all once the window has received a key or button press since it was mapped.

#### Scenario: Typing after a command-line open goes into the document
- **WHEN** the application receives an open activation for a supported file and the window composes
- **THEN** the window's keyboard focus is the opened document's text view, not GTK's initial pick on the chrome

#### Scenario: Typing after a session restore goes into the document
- **WHEN** a window restores a session with a document tab at startup
- **THEN** the restored, selected document's text view has keyboard focus

#### Scenario: The handoff leaves a user's focus alone
- **WHEN** a dialog, a transient surface or a popover (the Open popover, a tab menu) is up, or the focus is in the sidebar, the properties panel or an editor
- **THEN** opening or restoring a document does not move the focus

#### Scenario: Input before the document opens keeps the user's focus
- **WHEN** the user presses a key or a mouse button in the window after it is mapped and before an activation's or a restore's document is selected, even with the focus still on the chrome
- **THEN** opening or restoring the document does not move the focus

#### Scenario: A path another window owns hands off nothing here
- **WHEN** an activation names a file another window of the application has open, and that window is presented instead
- **THEN** the activated window's focus does not move
