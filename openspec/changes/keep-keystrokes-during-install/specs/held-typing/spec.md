## ADDED Requirements

### Requirement: Text typed while a workflow owns the buffer is held and inserted
While a document's buffer is owned by a workflow (a load installation, a whole-buffer replacement or a save holds the edit lock), while its load is in flight, or while held text waits for its insertion, an insertion the user types SHALL be held instead of refused, when it is inside a user action, at the caret, in a user action that deleted nothing, and fits the held buffer whole. Held text SHALL NOT change the buffer, its modified state, its undo history or its draft while held. Once no workflow owns the buffer, no load is in flight and no draft restore is pending for the editor, one dispatch after the releasing terminal has finished SHALL insert the held text, in the order it was typed, as one user action, through the ordinary interactive insert, when the editor's document is whole and editable: at the caret the published document is shown with when a release in the episode published a whole document, otherwise where the caret was when the episode's first character was held. A reload (Reload, Replace All's reload, Discard Changes) SHALL keep the caret's line and column. Text typed after the release and before that dispatch SHALL be held behind it. The insertion SHALL be one undo action, like typing (typing that follows it may coalesce with it), SHALL mark the document modified and SHALL be captured by the next draft autosave. An input-method composition SHALL hold the dispatch back for at most 1,024 ms, after which the input method is reset. While text is held, the input method's surrounding text SHALL include the held text before the cursor, and a `delete-surrounding` that falls inside the held text SHALL edit the held text. The view's drop targets SHALL be off while a load is in flight or text is held.

#### Scenario: Typing during a dense install keeps every character
- **WHEN** the user types characters into a document while its sliced install holds the edit lock
- **THEN** none of them changes the buffer before the install completes
- **AND** after the install every character is in the document, in order, at the caret the install left

#### Scenario: Typing while the file is still being read
- **WHEN** the user types into an editor whose load is still being read and planned
- **THEN** the typed text is not deleted by the install that follows
- **AND** it is in the document at the caret after the install completes

#### Scenario: The replay is one undo step
- **WHEN** held text has been inserted after an install
- **THEN** one undo removes all of it and nothing that was installed

#### Scenario: Typing right after the replay never undoes installed text
- **WHEN** the user types right after held text was inserted, then undoes until nothing is left to undo
- **THEN** at most two undo steps remove the held and the following text, and the installed document is unchanged

#### Scenario: A save-only episode replays where the first character was held
- **WHEN** the user types during a save and then moves the caret before the save ends
- **THEN** the held text is inserted where the caret was when the first character was held

#### Scenario: An input method edits held text
- **WHEN** an input method reads the surrounding text and deletes the last character while text is held
- **THEN** it sees the held text before the cursor, and the deletion removes the held character, not document text

#### Scenario: A drop is refused while a load is in flight
- **WHEN** text is dropped on an editor whose load is still being read
- **THEN** the drop is not reported done, so a move drag's source keeps its text

#### Scenario: A draft holds the replayed text
- **WHEN** held text has been inserted into a file-backed document
- **THEN** the document is modified and draft-dirty, and the next autosave writes a draft containing the text

#### Scenario: Backspace corrects held text
- **WHEN** the user types characters and then presses Backspace while the text is held
- **THEN** the last held character is removed and the rest is inserted

### Requirement: Held text goes only into the document it was typed into
Held text SHALL belong to the editor it was typed into. It SHALL be inserted only when the editor's buffer holds a whole document: every lock release in the episode that left the buffer partial or emptied SHALL be followed by a release that published a whole document, the load SHALL NOT have failed or been cancelled, the installation SHALL be complete and the tab SHALL NOT be evicted. Otherwise, and when the document is in Protected View or its view is not editable at that dispatch, or the insert is refused, the held text SHALL NOT be discarded: it SHALL open in a new Untitled tab, modified and draft-dirty, and a status warning SHALL say how many typed characters could not go into the document, why (view-only, unloaded from memory, not finished loading, or replaced), and that they are in a new Untitled tab. A tab holding typed text SHALL NOT be evicted. A tab moved to another window SHALL keep its load and its held text, and the receiving window SHALL adopt it as one of its own tabs.

#### Scenario: A cancelled install does not receive the text
- **WHEN** the user types while a load installs and the load is then cancelled
- **THEN** the cleared buffer does not receive the held text
- **AND** the text is in a new Untitled tab, modified and draft-dirty, and a status warning says so

#### Scenario: A superseded install gives the text to the document that replaces it
- **WHEN** the user types while an install runs and a newer load of the same tab supersedes it and completes
- **THEN** the held text is inserted into the newer document, once, and into no other tab

#### Scenario: Typing into a document that becomes protected
- **WHEN** the user types while a document is loading and the document is shown in Protected View
- **THEN** the typed text is not inserted into it
- **AND** the text is in a new Untitled tab and a status warning says the document is view-only and where the text went

#### Scenario: A tab moved to another window keeps its held text
- **WHEN** the user types while a tab installs and the tab is moved to another window before the install ends
- **THEN** the load completes, the held text is inserted, and the receiving window knows the tab's path

### Requirement: Edits that are not held are refused with feedback
While a document's buffer is held, a deletion of document text, undo, redo, an insertion away from the caret, any insertion in a user action that also deleted (typing over a selection, Replace, moving lines), and an insertion that does not fit the held buffer whole SHALL be refused and SHALL publish a rate-limited status warning. None of them SHALL be applied later. The held buffer SHALL be capped at `HELD_TEXT_CAP_BYTES`, 4 KiB of UTF-8, a power of two; a user action whose insertions would exceed it SHALL be refused whole, never split, including the segments of it already held, and the warning SHALL say the clipboard still has the text. The refusal warning SHALL say typed text is kept only while text is held. The first held character of an episode SHALL publish one status message that the typed text will appear when the document is ready, and the editor's accessible description SHALL say so while text is held.

#### Scenario: Deleting during an install is reported
- **WHEN** the user presses Delete or Ctrl+Z while an install holds the buffer
- **THEN** the document is unchanged by it after the install
- **AND** a status warning says the edit was not applied

#### Scenario: A paste larger than the cap is refused whole
- **WHEN** the user pastes more than 4 KiB while an install holds the buffer
- **THEN** none of the pasted text is inserted, text typed before it still is, and a status warning says it was not kept

### Requirement: A close never drops held text
Every close collector (closing a tab, closing other or several tabs, closing the window, quitting) SHALL count an editor with held text as having unsaved changes. Before it collects, a close SHALL settle held text that no workflow owns any more (a composition is reset), so the Save Changes dialog sees the document modified and its Save writes the text; when a workflow still owns the buffer of an editor with held text (a load, a replacement or a save), the close SHALL be refused with a status warning that typed text is waiting for the document. `app.quit` (Ctrl+Q) SHALL close every window through its own close confirmation and close-safety path.

#### Scenario: Closing a tab in the gap between the release and the replay
- **WHEN** a save that held typed text has released the buffer and the user closes the tab before the replay
- **THEN** the Save Changes dialog appears, and its Save writes the typed text

#### Scenario: Closing the window in the gap between the release and the replay
- **WHEN** the same happens and the user closes the window
- **THEN** the Save Changes dialog appears, and its Save writes the typed text

#### Scenario: Closing a loading tab with held text is refused
- **WHEN** the user closes a tab, or its window, while its install owns the buffer and typed text is held
- **THEN** the close is refused without a dialog, and the text is inserted once the install completes

#### Scenario: Quit asks before dropping unsaved documents
- **WHEN** the user quits with a modified document open
- **THEN** the Save Changes dialog appears, and its Save writes the document
