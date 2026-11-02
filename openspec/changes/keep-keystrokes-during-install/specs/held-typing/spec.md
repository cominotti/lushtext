## ADDED Requirements

### Requirement: Text typed while a workflow owns the buffer is held and inserted
While a document's buffer is owned by a workflow (a load installation, a whole-buffer replacement or a save holds the edit lock), while its load is in flight, or while held text waits for its insertion, an insertion the user types SHALL be held instead of refused, when it is inside a user action, at the caret, in a user action that deleted nothing, and fits the held buffer whole. Held text SHALL NOT change the buffer, its modified state, its undo history or its draft while held. Once no workflow owns the buffer and no load is in flight, one dispatch after the releasing terminal has finished SHALL insert the held text, in the order it was typed, at the caret, as one user action, through the ordinary interactive insert, when the editor's document is whole and editable. Text typed after the release and before that dispatch SHALL be held behind it. The insertion SHALL be one undo step, SHALL mark the document modified and SHALL be captured by the next draft autosave.

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

#### Scenario: A draft holds the replayed text
- **WHEN** held text has been inserted into a file-backed document
- **THEN** the document is modified and draft-dirty, and the next autosave writes a draft containing the text

#### Scenario: Backspace corrects held text
- **WHEN** the user types characters and then presses Backspace while the text is held
- **THEN** the last held character is removed and the rest is inserted

### Requirement: Held text goes only into the document it was typed into
Held text SHALL belong to the editor it was typed into. It SHALL be inserted only when the editor's buffer holds a whole document: every lock release in the episode that left the buffer partial or emptied SHALL be followed by a release that published a whole document, the load SHALL NOT have failed or been cancelled, the installation SHALL be complete and the tab SHALL NOT be evicted. Otherwise the held text SHALL be discarded and a status warning SHALL say how many typed characters were not inserted. When the document is in Protected View, or its view is not editable, at that dispatch, the held text SHALL be discarded with a Protected View notice that the typed text was not inserted.

#### Scenario: A cancelled install does not receive the text
- **WHEN** the user types while a load installs and the load is then cancelled
- **THEN** the cleared buffer does not receive the held text
- **AND** a status warning says the typed characters were not inserted

#### Scenario: A superseded install gives the text to the document that replaces it
- **WHEN** the user types while an install runs and a newer load of the same tab supersedes it and completes
- **THEN** the held text is inserted into the newer document, once, and into no other tab

#### Scenario: Typing into a document that becomes protected
- **WHEN** the user types while a document is loading and the document is shown in Protected View
- **THEN** the typed text is not inserted
- **AND** a Protected View notice says the text typed while it was loading was not inserted

### Requirement: Edits that are not held are refused with feedback
While a document's buffer is held, a deletion of document text, undo, redo, an insertion away from the caret, any insertion in a user action that also deleted (typing over a selection, Replace, moving lines), and an insertion that does not fit the held buffer whole SHALL be refused and SHALL publish a rate-limited status warning. None of them SHALL be applied later. The held buffer SHALL be capped at `HELD_TEXT_CAP_BYTES`, 4 KiB of UTF-8, a power of two; an insertion that would exceed it SHALL be refused whole, never split. The first held character of an episode SHALL publish one status message that the typed text will appear when the document is ready, and the editor's accessible description SHALL say so while text is held.

#### Scenario: Deleting during an install is reported
- **WHEN** the user presses Delete or Ctrl+Z while an install holds the buffer
- **THEN** the document is unchanged by it after the install
- **AND** a status warning says the edit was not applied

#### Scenario: A paste larger than the cap is refused whole
- **WHEN** the user pastes more than 4 KiB while an install holds the buffer
- **THEN** none of the pasted text is inserted, text typed before it still is, and a status warning says it was not kept
