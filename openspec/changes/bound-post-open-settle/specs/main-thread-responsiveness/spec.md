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
- **AND** every dispatch until the document is settled takes at most `B_SETTLE_US`, or the language-definition parse is recorded as the single accepted residual the maintainer decided

#### Scenario: Typing during settle is not starved
- **WHEN** keys are typed into a document from its load readiness until it is settled
- **THEN** each key is dispatched within `B_INTERACT_US` of its event time

#### Scenario: A dense document of short lines settles within budget
- **WHEN** a document whose lines are dense but shorter than the Protected View entry threshold is opened, with word wrap on or off
- **THEN** no validation dispatch caused by the open takes more than `B_SETTLE_US`

### Requirement: Language attach stays out of the activation dispatch
The language catalog SHALL be built off the GTK thread and handed to it, and every LushText use of a GtkSourceView language manager SHALL go through that catalog. Guessing a document's language from its path SHALL run on a worker and return a language id. Attaching the language to the document's buffer SHALL run in its own dispatch after the document's first editor frame. It SHALL run off the GTK thread only where the ledger's language-parse axiom shows that this is sound.

#### Scenario: The first open of a process does not load the language list
- **WHEN** the first document of a process is opened
- **THEN** the activation dispatch neither loads the language list nor guesses a language

#### Scenario: The language attaches after the first frame
- **WHEN** a document with a recognised language opens
- **THEN** its first editor frame is painted before the language attach stage runs
- **AND** a later load, reload or rename of the same editor replaces any pending attach, so a stale language is never attached

### Requirement: Highlighting attach and background analysis are bounded
Publishing an installed document SHALL NOT turn on syntax highlighting in a dispatch whose cost grows with the document. Above the measured total-size threshold `S_BG_HIGHLIGHT`, if the maintainer adopts it, the document scale SHALL reduce syntax highlighting, with the reduction reason `TotalSize` shown as for the other reductions.

#### Scenario: A large HTML document publishes without a document-sized dispatch
- **WHEN** a 5 MB short-line HTML document finishes its sliced installation
- **THEN** the publish dispatch takes at most `B_SETTLE_US`

#### Scenario: A document above the background-highlight threshold
- **WHEN** an HTML document larger than `S_BG_HIGHLIGHT` is opened, with the threshold adopted
- **THEN** syntax highlighting is reduced with the reason `TotalSize`
- **AND** no background-analysis dispatch runs for it

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
