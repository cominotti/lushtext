## ADDED Requirements

### Requirement: Post-open settle fits the settle budget per dispatch
Every main-loop dispatch from the activation turn that creates an editor page until that document is settled SHALL take at most `B_SETTLE_US`, on every opening path:
- activation, the Open dialog, the recent-documents popover and the sidebar;
- session restore and draft recovery.

A document is settled when all of the following hold:
- its load is installed and published;
- its language is attached;
- no post-open slice, debounce, worker completion or publish step is pending;
- GtkSourceView highlighting and GtkTextView validation have been quiet for the named settle-quiet period.

An input event delivered during settle SHALL be dispatched within `B_INTERACT_US` of its event time. A dispatch recorded as a toolkit-floor exception against a ledger axiom is exempt from `B_SETTLE_US`, and the input events whose dispatch overlaps it are exempt from `B_INTERACT_US`; both are reported, never dropped. Debounces started by user edits during settle are not post-open work. Dispatches that lay out a single paragraph longer than the Protected View entry threshold are excluded; they are owned by the protected-view and sliced-protected-install changes. The budgets are read from `model/responsiveness_budget.rs`.

#### Scenario: A small document settles within budget
- **WHEN** a 400-byte plain-text document is opened in any of the four construction profiles
- **THEN** every dispatch from the activation turn until the document is settled takes at most `B_SETTLE_US`

#### Scenario: An HTML document opens without a language-definition stall
- **WHEN** the first HTML document of a process is opened
- **THEN** the activation dispatch does not parse the language definition
- **AND** every dispatch until the document is settled takes at most `B_SETTLE_US`, except a language-definition parse recorded as a toolkit-floor exception against its ledger axiom

#### Scenario: Typing during settle is not starved
- **WHEN** keys are typed into a document from its load readiness until it is settled
- **THEN** each key is dispatched within `B_INTERACT_US` of its event time

#### Scenario: A dense document of short lines settles within budget
- **WHEN** a document whose lines are dense but shorter than the Protected View entry threshold is opened, with word wrap on or off
- **THEN** no validation dispatch caused by the open takes more than `B_SETTLE_US`

### Requirement: Language attach stays out of the activation dispatch
The language catalog SHALL be built off the GTK thread and handed to it, and every LushText use of a GtkSourceView language manager SHALL go through that catalog. Guessing a document's language from its path SHALL run on a worker and return a language id. The catalog SHALL be started at application startup, so that it is ready before the activation's opens. The worker's path guess SHALL agree with GtkSourceView's `guess_language` for every glob of every shipped language, and at attach time GtkSourceView's guess is authoritative.

Every site that sets a document's language (path changes, load publication, the buffer-replacement terminal and document-scale changes) SHALL request one attach stage instead of attaching in its own dispatch. The stage SHALL run in its own dispatch after the editor's first frame; for an editor that is not mapped, after it is first mapped. While the attach is pending, the document's language id SHALL already be reported to every consumer that asks for it, including the Markdown preview. The attach SHALL run off the GTK thread only where the ledger's language-parse axiom shows that this is sound, and then only on a language object that no GTK-thread code could reach before the handoff.

Where it cannot, the attach SHALL run from a `G_PRIORITY_LOW` idle, and only after the named input-quiet interval has passed with no key press, button press, scroll start or touch begin on the window, or once the named attach deadline has passed. Its cost SHALL be recorded as a toolkit-floor exception against that axiom, with measured numbers and a drafted upstream report.

#### Scenario: The first open of a process does not load the language list
- **WHEN** the first document of a process is opened
- **THEN** the activation dispatch neither loads the language list nor guesses a language

#### Scenario: The language parse cannot leave the GTK thread
- **WHEN** the ledger's language-parse axiom shows that the definition parse must run on the GTK thread, and an HTML document is opened while the user is typing
- **THEN** the attach waits until no input event has arrived for the input-quiet interval, or until the attach deadline, and runs from a low-priority idle
- **AND** the programme record lists the parse as a toolkit-floor exception with its axiom id, cost and upstream report

#### Scenario: A Markdown document is recognised before its language attaches
- **WHEN** a Markdown document opens with the side-by-side preview shown, and its language attach is still pending
- **THEN** the preview treats it as Markdown, and never shows the not-a-Markdown placeholder for it

#### Scenario: A restored tab that is not selected
- **WHEN** session restore creates an editor page that is not selected
- **THEN** its language attach runs only after it is first mapped, and its language id is reported meanwhile

#### Scenario: The language attaches after the first frame
- **WHEN** a document with a recognised language opens
- **THEN** its first editor frame is painted before the language attach stage runs
- **AND** a later load, reload or rename of the same editor replaces any pending attach, so a stale language is never attached

### Requirement: Highlighting attach and background analysis are bounded
Publishing an installed document SHALL NOT turn on syntax highlighting in a dispatch whose cost grows with the document. The document scale's total-size syntax threshold `S_SYNTAX` SHALL be the measured `S_BG_HIGHLIGHT`: the largest short-line total size whose background analysis keeps every dispatch within `B_SETTLE_US`. Above it, syntax highlighting is reduced with the existing reduction reason `TotalSize`, shown as for the other reductions. The budgets are not relaxed for background analysis.

#### Scenario: A large HTML document publishes without a document-sized dispatch
- **WHEN** a 5 MB short-line HTML document finishes its sliced installation
- **THEN** the publish dispatch takes at most `B_SETTLE_US`

#### Scenario: A document above the background-highlight threshold
- **WHEN** an HTML document larger than `S_BG_HIGHLIGHT` is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize`
- **AND** no background-analysis dispatch runs for it

### Requirement: User-started invalidations on dense documents are paced
On a document where some window of consecutive paragraphs that one validation batch can cover holds more than `V_SLICE` visible characters (the document profile's densest batch), a user-started change of word wrap, zoom, font or tab width SHALL NOT leave more than `V_SLICE` visible characters of invalid lines for one validation dispatch. A style that a buffer tag carries reaches every view of the buffer, so it SHALL NOT change what the minimap or printing shows; word wrap SHALL therefore not be carried by a buffer tag. When a change applies to several editors, the selected editor SHALL be updated first, and every other dense editor when it is next mapped.

The interaction dispatch that handles the change SHALL take at most `B_INTERACT_US`. The visible paragraphs SHALL be updated first, in a frame whose dispatch takes at most `B_SETTLE_US`. The rest of the document SHALL be updated outward, in paragraph-aligned ranges, in low-priority dispatches of at most `B_SETTLE_US` each. A newer change SHALL supersede the running one and restart from the visible paragraphs.

The mechanism is the one the ledger's restyle axiom selects. If none passes, the cost SHALL be recorded as a toolkit-floor exception against that axiom.

#### Scenario: Toggling word wrap on a dense document
- **WHEN** the user toggles word wrap on a document of 710-character lines
- **THEN** the toggle's dispatch takes at most `B_INTERACT_US`, the visible paragraphs show the new wrapping in the next frame, and every later dispatch until the document is settled takes at most `B_SETTLE_US`

#### Scenario: Zooming a dense document
- **WHEN** the user zooms in on a document of 710-character lines
- **THEN** the zoom's dispatch takes at most `B_INTERACT_US`, the visible paragraphs show the new size first, and every later dispatch until the document is settled takes at most `B_SETTLE_US`

#### Scenario: Changing the font of a dense document
- **WHEN** the user changes the editor font while a document of 710-character lines is open
- **THEN** no dispatch caused by the change in that editor takes more than `B_SETTLE_US`, and the visible paragraphs are updated first

#### Scenario: A second change during a paced update
- **WHEN** the user toggles word wrap again before the previous toggle has reached every paragraph
- **THEN** the earlier update stops without applying its remaining ranges, and the newest setting is applied from the visible paragraphs outward

#### Scenario: A document that is not dense keeps the direct path
- **WHEN** the user toggles word wrap on a document with no window over `V_SLICE` visible characters
- **THEN** the view property changes directly, as today

#### Scenario: Zooming with several dense documents open
- **WHEN** the user zooms while two dense documents are open in different tabs
- **THEN** the selected document is updated first, outward from its visible paragraphs, and the other document is updated only when it is next shown
- **AND** neither minimap is drawn at the document's font size

## MODIFIED Requirements

### Requirement: Large file installation yields in bounded GTK slices
Installing a decoded document above the synchronous installation threshold SHALL use bounded main-loop slices whose boundaries align to paragraph ends, as GTK defines them: just after `\n`, `\r`, `\r\n` (never between its CR and LF) or U+2029. The paragraph-boundary function MUST be the single shared one whose agreement with `pango_find_paragraph_boundary` is tested. GTK text layout validates whole paragraphs, so a slice that stops inside a paragraph forces later slices to re-lay-out everything already installed in that paragraph. That is quadratic total work and can stall recovery of single-line documents for minutes. A single paragraph longer than the slice byte budget SHALL be installed (and, during clearing, deleted) in one turn, because GTK cannot lay out a partial paragraph incrementally regardless of how the mutation is sliced. The editor SHALL remain non-editable, and projections that would amplify each insertion SHALL remain suspended, until the complete current generation is installed or the operation is cancelled.

The synchronous installation threshold SHALL be the measured largest size whose insert fits half of `B_SETTLE_US`. A document whose densest validation batch holds more than `V_SLICE` visible characters, a measured constant, SHALL be sliced even when its byte size is below the threshold. Each of its slices SHALL hold at most `V_SLICE` visible characters, and its next slice SHALL NOT be dispatched before the frame that lays out the previous one. The validation that a slice's new lines cause then fits half of `B_SETTLE_US` in either wrap mode. The slice plan is one pure function, shared by load and whole-buffer replacement.

#### Scenario: Large decoded text is installed
- **WHEN** an admitted load returns text above the synchronous installation threshold
- **THEN** GTK inserts the text in bounded paragraph-aligned slices with scheduling points between them
- **AND** syntax, minimap, history, draft, monitor, and modified-state finalization run only after the complete current generation is present

#### Scenario: Giant single-paragraph content avoids quadratic re-layout
- **WHEN** a recovered draft or loaded file contains one paragraph larger than the slice byte budget
- **THEN** that paragraph is installed in a single turn while any multi-paragraph remainder keeps bounded paragraph-aligned slices
- **AND** previously installed paragraphs are not re-validated by later slices

#### Scenario: CR-only and U+2029-separated documents are sliced
- **WHEN** an admitted document above the synchronous threshold separates its paragraphs only with `\r`, or only with U+2029
- **THEN** installation uses multiple bounded slices, each ending just after a delimiter
- **AND** the installed buffer is byte-identical to the decoded text

#### Scenario: CRLF pairs survive slicing
- **WHEN** a slice's byte budget ends between the `\r` and `\n` of a CRLF pair
- **THEN** the slice ends after the `\n`, so the pair stays one GTK paragraph delimiter

#### Scenario: Load is cancelled during installation
- **WHEN** the tab closes, reloads, or advances generation between installation slices
- **THEN** remaining slices stop without applying final loaded state
- **AND** admission ownership and retained decoded text are released

#### Scenario: Small load remains direct
- **WHEN** decoded text is below the synchronous installation threshold and its densest validation batch holds at most `V_SLICE` visible characters
- **THEN** the existing direct installation path may run in one GTK turn
- **AND** it observes the same generation and finalization rules as chunked installation

#### Scenario: Dense short-line text is paced by visible characters
- **WHEN** an admitted load returns text below the byte threshold whose densest validation batch holds more than `V_SLICE` visible characters
- **THEN** it is installed in paragraph-aligned slices of at most `V_SLICE` visible characters, each dispatched only after the frame that lays out the previous one
- **AND** each slice's validation runs in that frame's layout, as A37 requires, within half of `B_SETTLE_US`

#### Scenario: A short-line document with many characters stays direct
- **WHEN** an admitted load below the byte threshold has more than `V_SLICE` visible characters in total but no validation batch over `V_SLICE`
- **THEN** it is installed directly in one GTK turn

#### Scenario: Post-open formatting does not invalidate every line
- **WHEN** EditorConfig resolves a tab width for a document being opened or reloaded, or the end-of-document overscroll of a dense document is updated after a viewport resize
- **THEN** the tab width is applied after any clear of old text and before the first insert
- **AND** no default-style property of a dense document's view changes in a dispatch after the install that is not paced or recorded as a toolkit-floor exception
