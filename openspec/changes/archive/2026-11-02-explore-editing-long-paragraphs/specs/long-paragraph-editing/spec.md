## ADDED Requirements

### Requirement: Typing lays out an edited paragraph at most once
A keystroke that inserts text into an editable paragraph SHALL cause at most one layout of that paragraph across every text view attached to the document's buffer, the one GTK's commit handler builds for the editor view. No other LushText-owned view, live or retired, SHALL lay out the paragraph for that keystroke.

#### Scenario: Minimap preference off
- **WHEN** the user types one character into a 50,000-character paragraph with the minimap preference off
- **THEN** the paragraph is laid out once for that keystroke
- **AND** no `GtkSourceMap` re-validates the paragraph from an idle

#### Scenario: Minimap retired by an edit
- **WHEN** an edit makes a paragraph longer than the minimap paragraph threshold and the minimap is retired
- **THEN** later keystrokes in that paragraph lay it out once each

### Requirement: The responsiveness lane measures per-keystroke cost in long paragraphs
The responsiveness lane SHALL include a keystroke group: synthetic one-paragraph fixtures of swept length between short lines, in a prose and a markup-like shape, with wrap on and off, typed at the paragraph's start, middle and end and in a short paragraph right after it. Each typed burst SHALL be verified to have landed where intended. The report SHALL state, per case and position, the worst stall in each keystroke's window and the summed stall per keystroke, against B_interact.

#### Scenario: A burst lands in the wrong place
- **WHEN** the verification search does not find the typed burst next to its position marker
- **THEN** the report marks that position of the case as not verified and its stalls as excluded

#### Scenario: A second dispatch per keystroke
- **WHEN** a keystroke causes two dispatches each under B_interact whose sum exceeds it
- **THEN** the report shows the summed stall for that keystroke, so the extra work is visible

### Requirement: Only paragraphs above the wrap threshold are shown unwrapped
A paragraph longer than `L_wrap_edit` characters SHALL be shown unwrapped, and every other paragraph SHALL follow the user's word-wrap preference. A paragraph unwrapped this way SHALL wrap again only once it is at most `L_wrap_edit` minus the rewrap margin characters long. The decision SHALL be applied before the paragraph is laid out: at load, on every installation path, and in the same `insert-text` or `delete-range` emission as the edit that changes the paragraph, including an edit before the paragraph's first character, a split, a join, a paste and an undo. The word-wrap preference SHALL NOT be written, and no text, modified state or undo history SHALL change.

#### Scenario: Opening a document with one long paragraph
- **WHEN** a document with one paragraph longer than `L_wrap_edit` between short paragraphs is opened with word wrap enabled
- **THEN** that paragraph is unwrapped and the short paragraphs wrap
- **AND** the word-wrap preference still reads enabled

#### Scenario: Typing past the threshold
- **WHEN** an insertion makes a paragraph longer than `L_wrap_edit`
- **THEN** the paragraph is unwrapped before the insertion's signal emission returns, so GTK lays it out unwrapped for that keystroke

#### Scenario: Typing before the first character
- **WHEN** the user types at the start of an unwrapped paragraph, or deletes its first characters
- **THEN** the paragraph stays unwrapped

#### Scenario: Shrinking around the threshold
- **WHEN** an unwrapped paragraph shrinks to `L_wrap_edit` or below but stays above `L_wrap_edit` minus the rewrap margin
- **THEN** it stays unwrapped, and it wraps again once it is at or below that margin

#### Scenario: A short paragraph next to a long one
- **WHEN** the user types in a short paragraph next to an unwrapped long one
- **THEN** the short paragraph wraps and its keystrokes cost what they cost without the long paragraph's gate

#### Scenario: The reduction is reported
- **WHEN** the gate unwraps a paragraph of a document
- **THEN** the reduced-features status message, the document-properties "Editor Features" row and the editor's accessible description name the word wrap of the longest lines with the "very long lines" reason

### Requirement: The input method gets bounded surrounding text
When the editor's input method asks for the surrounding text, the editor SHALL answer with at most `SURROUNDING_WINDOW_CHARS` characters on each side of the cursor, within the cursor's paragraph, including the selection's other end when the selection is within that paragraph and at most `SURROUNDING_MAX_SELECTION_CHARS` long. The cost of the answer SHALL NOT grow with the paragraph's length. When the text view's own surrounding-text handler cannot be found, the editor SHALL keep GTK's behaviour and log a warning.

#### Scenario: Cursor in the middle of a long paragraph
- **WHEN** the input method asks for surrounding text with the cursor in the middle of a 50,000-character paragraph
- **THEN** it receives `2 × SURROUNDING_WINDOW_CHARS` characters with the cursor in their middle

#### Scenario: Cursor at a paragraph start
- **WHEN** the cursor is at the start of a paragraph
- **THEN** the answer starts at that paragraph's start and contains no text from the paragraph above

#### Scenario: GNOME's input path
- **WHEN** the user types in a 50,000-character paragraph through GTK's Wayland text-input module
- **THEN** no keystroke computes the whole paragraph's word boundaries for the surrounding text

### Requirement: The per-keystroke floor is recorded as a toolkit-floor exception
The long-line programme record SHALL state the per-keystroke floor measured on the reference lane — one layout of the edited paragraph built synchronously by GTK's commit handler — the techniques measured against it with their results, the paragraph lengths at which typing meets B_interact with and without the wrap gate and with each lane input-method variant, and the remaining range covered by the toolkit-floor exception, with an upstream report reference and the re-open triggers.

#### Scenario: A GTK release bounds paragraph layout
- **WHEN** a GTK or Pango release makes the commit handler's layout incremental or bounded
- **THEN** the record's re-open trigger fires and the keystroke group is re-run against the new toolkit before the exception is kept
