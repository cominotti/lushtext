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
The responsiveness lane SHALL include a keystroke group: synthetic one-paragraph fixtures of swept length between short lines, in a prose and a markup-like shape, with wrap on and off, typed at the paragraph's start, middle and end. Each typed burst SHALL be verified to have landed where intended. The report SHALL state, per case and position, the worst stall in each keystroke's window and the summed stall per keystroke, against B_interact.

#### Scenario: A burst lands in the wrong place
- **WHEN** the verification search does not find the typed burst next to its position marker
- **THEN** the case is reported as not verified and its stalls do not count toward a threshold

#### Scenario: A second dispatch per keystroke
- **WHEN** a keystroke causes two dispatches each under B_interact whose sum exceeds it
- **THEN** the report shows the summed stall for that keystroke, so the extra work is visible

### Requirement: Long paragraphs above the wrap threshold are shown unwrapped
When the document scale's `wrap_allowed` feature is off, the editor view SHALL show the document without word wrapping, whatever the word-wrap preference says, for the rest of the editor session. The word-wrap preference SHALL NOT be written. When `wrap_allowed` is on, the view SHALL follow the preference.

#### Scenario: Opening a document with a paragraph over the threshold
- **WHEN** a document whose longest paragraph exceeds `L_wrap_edit` is opened with word wrap enabled in preferences
- **THEN** the view is unwrapped before the paragraph is first laid out
- **AND** the word-wrap preference still reads enabled
- **AND** other open documents keep wrapping

#### Scenario: Typing past the threshold
- **WHEN** an insertion makes a paragraph longer than `L_wrap_edit` in a wrapped view
- **THEN** the view is unwrapped before the insertion's signal emission returns, so GTK's commit handler lays the paragraph out unwrapped
- **AND** the view stays unwrapped for the rest of the session, even if the paragraph shrinks

#### Scenario: The reduction is reported
- **WHEN** the wrap gate turns wrapping off for a document
- **THEN** the reduced-features status message, the document-properties "Editor Features" row and the editor's accessible description name word wrap with the "very long lines" reason

### Requirement: The per-keystroke floor is recorded as a toolkit-floor exception
The long-line programme record SHALL state the per-keystroke floor measured on the reference lane — one layout of the edited paragraph built synchronously by GTK's commit handler — the techniques measured against it with their results, the paragraph lengths at which typing meets B_interact with and without the wrap gate and with each lane input-method variant, and the remaining range covered by the toolkit-floor exception, with an upstream report reference and the re-open triggers.

#### Scenario: A GTK release bounds paragraph layout
- **WHEN** a GTK or Pango release makes the commit handler's layout incremental or bounded
- **THEN** the record's re-open trigger fires and the keystroke group is re-run against the new toolkit before the exception is kept
