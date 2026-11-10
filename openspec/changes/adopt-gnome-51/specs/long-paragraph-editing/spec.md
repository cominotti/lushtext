## ADDED Requirements

### Requirement: Arrow-key steps in long left-to-right paragraphs lay out no cursor list
A Left or Right key press in an editable paragraph longer than `L_LOGICAL_CURSOR_STEP` characters that holds no right-to-left text SHALL move the cursor by the buffer's cursor positions, not by GTK's visual step, whose cost is quadratic in the display line's length (ledger A107). The cursor SHALL land where GTK's visual step lands, with Shift extending the selection as before. A paragraph holding right-to-left text, a step that leaves the paragraph, a step without a visible cursor, and the collapse of a selection without Shift SHALL keep GTK's own step.

#### Scenario: Right arrow in a 10,000-character paragraph
- **WHEN** the user presses Right in an editable 10,000-character left-to-right paragraph shown unwrapped
- **THEN** the cursor moves one position right
- **AND** the key press does not block the GTK thread for a cursor-list layout of the line (1.2 s before this change)

#### Scenario: Shift+Right extends the selection
- **WHEN** the user presses Shift+Right in such a paragraph
- **THEN** the selection grows by one position from the same anchor

#### Scenario: A paragraph with right-to-left text
- **WHEN** the cursor's paragraph holds a Hebrew or Arabic run and the user presses Right inside it
- **THEN** the cursor moves the way GTK's visual step moves it
