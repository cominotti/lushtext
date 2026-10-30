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

An input event delivered during settle SHALL be dispatched within `B_INTERACT_US` of its event time. Dispatches that lay out a single paragraph longer than the Protected View entry threshold are excluded; they are owned by the protected-view and sliced-protected-install changes. The budgets are read from `model/responsiveness_budget.rs`.

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
The language catalog SHALL be built off the GTK thread and handed to it, and every LushText use of a GtkSourceView language manager SHALL go through that catalog. Guessing a document's language from its path SHALL run on a worker and return a language id. Attaching the language to the document's buffer SHALL run in its own dispatch after the document's first editor frame. It SHALL run off the GTK thread only where the ledger's language-parse axiom shows that this is sound.

Where it cannot, the attach SHALL run from a `G_PRIORITY_LOW` idle, and only after the named input-quiet interval has passed with no input event on the window. Its cost SHALL be recorded as a toolkit-floor exception against that axiom, with measured numbers and a drafted upstream report.

#### Scenario: The first open of a process does not load the language list
- **WHEN** the first document of a process is opened
- **THEN** the activation dispatch neither loads the language list nor guesses a language

#### Scenario: The language parse cannot leave the GTK thread
- **WHEN** the ledger's language-parse axiom shows that the definition parse must run on the GTK thread, and an HTML document is opened while the user is typing
- **THEN** the attach waits until no input event has arrived for the input-quiet interval, and runs from a low-priority idle
- **AND** the programme record lists the parse as a toolkit-floor exception with its axiom id, cost and upstream report

#### Scenario: The language attaches after the first frame
- **WHEN** a document with a recognised language opens
- **THEN** its first editor frame is painted before the language attach stage runs
- **AND** a later load, reload or rename of the same editor replaces any pending attach, so a stale language is never attached

### Requirement: Highlighting attach and background analysis are bounded
Publishing an installed document SHALL NOT turn on syntax highlighting in a dispatch whose cost grows with the document. Above the measured total-size threshold `S_BG_HIGHLIGHT`, the document scale SHALL reduce syntax highlighting, with the reduction reason `TotalSize` shown as for the other reductions. The budgets are not relaxed for background analysis.

#### Scenario: A large HTML document publishes without a document-sized dispatch
- **WHEN** a 5 MB short-line HTML document finishes its sliced installation
- **THEN** the publish dispatch takes at most `B_SETTLE_US`

#### Scenario: A document above the background-highlight threshold
- **WHEN** an HTML document larger than `S_BG_HIGHLIGHT` is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize`
- **AND** no background-analysis dispatch runs for it

### Requirement: User-started invalidations on dense documents are paced
On a document where some window of consecutive paragraphs that one validation batch can cover holds more than `V_SLICE` visible characters, a user-started change of word wrap, zoom, font or tab width SHALL NOT leave more than `V_SLICE` visible characters of invalid lines for one validation dispatch.

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

## MODIFIED Requirements

### Requirement: Large file installation yields in bounded GTK slices
Installing a decoded document above the synchronous installation threshold SHALL use bounded main-loop slices whose boundaries align to paragraph ends (just after a newline). GTK text layout validates whole paragraphs, so a slice that stops inside a paragraph forces later slices to re-lay-out everything already installed in that paragraph — quadratic total work that can stall recovery of single-line documents for minutes. A single paragraph longer than the slice byte budget SHALL be installed (and, during clearing, deleted) in one turn, because GTK cannot lay out a partial paragraph incrementally regardless of how the mutation is sliced. The editor SHALL remain non-editable and projections that would amplify each insertion SHALL remain suspended until the complete current generation is installed or the operation is cancelled.

Each slice SHALL also hold at most `V_SLICE` visible characters, a measured constant. The validation that a slice's new lines cause then fits half of `B_SETTLE_US` in either wrap mode. The synchronous installation threshold SHALL be the measured largest size whose insert fits half of `B_SETTLE_US`. A document above `V_SLICE` visible characters SHALL be sliced even when its byte size is below the threshold. The slice plan is one pure function, shared by load and whole-buffer replacement.

#### Scenario: Large decoded text is installed
- **WHEN** an admitted load returns text above the synchronous installation threshold
- **THEN** GTK inserts the text in bounded paragraph-aligned slices with scheduling points between them
- **AND** syntax, minimap, history, draft, monitor, and modified-state finalization run only after the complete current generation is present

#### Scenario: Giant single-paragraph content avoids quadratic re-layout
- **WHEN** a recovered draft or loaded file contains one paragraph larger than the slice byte budget
- **THEN** that paragraph is installed in a single turn while any multi-paragraph remainder keeps bounded newline-aligned slices
- **AND** previously installed paragraphs are not re-validated by later slices

#### Scenario: Load is cancelled during installation
- **WHEN** the tab closes, reloads, or advances generation between installation slices
- **THEN** remaining slices stop without applying final loaded state
- **AND** admission ownership and retained decoded text are released

#### Scenario: Small load remains direct
- **WHEN** decoded text is below the synchronous installation threshold and holds at most `V_SLICE` visible characters
- **THEN** the existing direct installation path may run in one GTK turn
- **AND** it observes the same generation and finalization rules as chunked installation

#### Scenario: Dense short-line text is paced by visible characters
- **WHEN** an admitted load returns text below the byte threshold whose visible characters exceed `V_SLICE`
- **THEN** it is installed in paragraph-aligned slices of at most `V_SLICE` visible characters
- **AND** each slice's validation runs in the next frame's layout, as A37 requires, within half of `B_SETTLE_US`

#### Scenario: Post-open formatting does not invalidate every line
- **WHEN** EditorConfig resolves a tab width for a document being opened, or the end-of-document overscroll is updated after a viewport resize
- **THEN** the tab width is applied before the text is installed
- **AND** the overscroll space is applied to the last paragraph only, so no default-style property of the view changes after the document is installed
