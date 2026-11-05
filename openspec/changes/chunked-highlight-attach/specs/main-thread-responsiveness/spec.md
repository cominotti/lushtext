## ADDED Requirements

### Requirement: Highlighted documents above their attach threshold install chunk by chunk
A document whose incoming scale allows syntax highlighting, in a language with a chunked cap (`document_scale::chunked_threshold`: the largest power of two at which, on a worst-case document with no closer after the cursor, a key that changes the syntax context of the rest of the document and its closer keep every dispatch within `B_SETTLE_US` and ordinary keys stay within `B_INTERACT_US`, at the start and the middle, and which the install fits in about 0.5 s; a language whose cap would not exceed its C6 gate has none, and no shipped language has one today), larger than that language's attach-after-install threshold and at most its cap, SHALL be installed with its language attached, in this order: a language attached to old text is removed before that text is cleared; the first paragraph-aligned chunk is inserted before the attach stage runs (a language already attached to the empty buffer may stay); the deferred attach stage attaches the language to that one chunk; and every later chunk, of at most `HIGHLIGHT_CHUNK_BYTES` (8 KiB) and ending on a paragraph boundary, is inserted only once the context engine has reported the buffer analysed to its end (ledger A84) or after one wait at `G_PRIORITY_LOW + 1`. Every installer of a whole body SHALL take this order: file load and reload, eviction reload, draft recovery, local-history restore and undo, and save-formatting mirror-back. A cancelled, superseded or disposed chunked install SHALL remove the language before clearing its partial text, and a sliced clear-only replacement SHALL remove it before clearing; a whole-buffer replacement SHALL remove it at its first clear turn, so one that ends before any turn leaves the language as it was. An attach stage requested while a chunked install owns the buffer, including by an identity change, SHALL NOT wait for the editor to be mapped. Publication SHALL NOT toggle highlighting on a chunked document. The installed text SHALL be the decoded body byte for byte.

#### Scenario: The first frame shows text
- **WHEN** a 400 KiB document opens in a language with a chunked cap above it (the chunked tests give Markdown 512 KiB)
- **THEN** the language is attached while the buffer holds at most one chunk, after the first chunk was inserted
- **AND** the document ends highlighted, its text equal to the file

#### Scenario: Each chunk is analysed before the next
- **WHEN** a chunk has gone in and the engine has not reported the buffer analysed to its end
- **THEN** the next chunk waits once at `G_PRIORITY_LOW + 1`, behind the engine's worker, and then goes in

#### Scenario: A background tab installs without being shown
- **WHEN** a large Markdown document is opened into a tab that is not selected
- **THEN** its chunked install completes with the language attached while the tab stays hidden

#### Scenario: A cancelled chunked install leaves no language behind
- **WHEN** a chunked install is cancelled after several chunks
- **THEN** its partial text is cleared with no language attached, and the editor is marked incomplete so nothing can be saved over the file

#### Scenario: A reload removes the language before clearing
- **WHEN** a highlighted large Markdown document is reloaded
- **THEN** its language is removed while the old text is still whole, and the reloaded document ends highlighted

#### Scenario: A local-history restore installs chunk by chunk
- **WHEN** a 400 KiB Markdown body is restored from local history into a Markdown editor
- **THEN** the replacement inserts it in chunks of at most 8 KiB and the language is attached while the buffer holds at most one chunk

#### Scenario: A rename while a background install waits for its attach
- **WHEN** a hidden tab's chunked install waits for its attach stage and the file is renamed
- **THEN** the install still finishes without the tab being shown

#### Scenario: A chunk the engine does not report waits once
- **WHEN** a chunk goes in and the engine reports no analysed range
- **THEN** the next chunk waits once at `G_PRIORITY_LOW + 1` and goes in, and the install finishes

#### Scenario: A replacement that ends before its first turn keeps the language
- **WHEN** a chunked whole-buffer replacement becomes stale before its first turn
- **THEN** the old text keeps its language and highlighting, and nothing re-analyses it

## MODIFIED Requirements

### Requirement: Language attach stays out of the activation dispatch
The language catalog SHALL be built off the GTK thread and handed to it, and every LushText use of a GtkSourceView language manager SHALL go through that catalog. Guessing a document's language from its path SHALL run on a worker and return a language id. The catalog SHALL be started at application startup, so that it is ready before the activation's opens. The worker's path guess SHALL agree with GtkSourceView's `guess_language` for every glob of every shipped language, and at attach time GtkSourceView's guess is authoritative.

Every site that sets a document's language (path changes, load publication, the buffer-replacement terminal and document-scale changes) SHALL request one attach stage instead of attaching in its own dispatch. The stage SHALL run in its own dispatch after the editor's first frame; for an editor that is not mapped, after it is first mapped, except a stage a chunked highlighted install requested, which SHALL NOT wait for the editor to be mapped. While the attach is pending, the document's language id SHALL already be reported to every consumer that asks for it, including the Markdown preview. The attach SHALL run off the GTK thread only where the ledger's language-parse axiom shows that this is sound, and then only on a language object that no GTK-thread code could reach before the handoff.

Where it cannot, the attach SHALL run from a `G_PRIORITY_LOW` idle, and only after the named input-quiet interval has passed with no key press, button press, scroll start or touch begin on the window, or once the named attach deadline has passed. The one exception is a stage a chunked highlighted install requested in an editor that is mapped and has the keyboard focus of the active window: that install holds the editor's typed keys until its terminal, so the stage SHALL NOT wait for input-quiet there; a chunked install in any other editor, including a background tab and the focus child of an inactive window, SHALL wait for input-quiet or the deadline. Its cost SHALL be recorded as a toolkit-floor exception against that axiom, with measured numbers and a drafted upstream report.

#### Scenario: The first open of a process does not load the language list
- **WHEN** the first document of a process is opened
- **THEN** the activation dispatch neither loads the language list nor guesses a language

#### Scenario: The language parse cannot leave the GTK thread
- **WHEN** the ledger's language-parse axiom shows that the definition parse must run on the GTK thread, and an HTML document is opened while the user is typing
- **THEN** the attach waits until no input event has arrived for the input-quiet interval, or until the attach deadline, and runs from a low-priority idle
- **AND** the programme record lists the parse as a toolkit-floor exception with its axiom id, cost and upstream report

#### Scenario: A background chunked install waits for input-quiet
- **WHEN** a large Markdown document installs chunk by chunk in a tab that is not shown, while the user types in the shown editor
- **THEN** its language does not attach until no input has arrived for the input-quiet interval, or until the attach deadline

#### Scenario: An inactive window's install waits for input-quiet
- **WHEN** a chunked install runs in an editor that is its window's focus child, while the user types in another, active window
- **THEN** its language does not attach until no input has arrived for the input-quiet interval, or until the attach deadline

#### Scenario: A Markdown document is recognised before its language attaches
- **WHEN** a Markdown document opens with the side-by-side preview shown, and its language attach is still pending
- **THEN** the preview treats it as Markdown, and never shows the not-a-Markdown placeholder for it

#### Scenario: A restored tab that is not selected
- **WHEN** session restore creates an editor page that is not selected, for a document that does not take the chunked highlighted install
- **THEN** its language attach runs only after it is first mapped, and its language id is reported meanwhile

#### Scenario: The language attaches after the first frame
- **WHEN** a document with a recognised language opens, other than into a tab that is not shown while a chunked highlighted install owns its buffer
- **THEN** its first editor frame is painted before the language attach stage runs
- **AND** a later load, reload or rename of the same editor replaces any pending attach, so a stale language is never attached

### Requirement: Highlighting attach and background analysis are bounded
Publishing an installed document SHALL NOT turn on syntax highlighting in a dispatch whose cost grows with the document. The document scale's total-size syntax threshold `S_SYNTAX` SHALL be the measured `S_BG_HIGHLIGHT`: the largest short-line total size whose background analysis keeps every dispatch within `B_SETTLE_US`. Above it, syntax highlighting is reduced with the existing reduction reason `TotalSize`, shown as for the other reductions, except for a language with a chunked cap (`document_scale::chunked_threshold`; none shipped): a document of one of them above its measured threshold and at most its cap takes the chunked highlighted install instead, and is reduced with `TotalSize` only above the cap. The budgets are not relaxed for background analysis.

#### Scenario: A large HTML document publishes without a document-sized dispatch
- **WHEN** a 5 MB short-line HTML document finishes its sliced installation
- **THEN** the publish dispatch takes at most `B_SETTLE_US`

#### Scenario: A document above the background-highlight threshold
- **WHEN** an HTML document larger than `S_BG_HIGHLIGHT` is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize`
- **AND** no background-analysis dispatch runs for it

#### Scenario: A document with a chunked cap keeps its highlighting
- **WHEN** a short-line document larger than its language's measured threshold and at most its chunked cap is opened
- **THEN** syntax highlighting stays on, the document takes the chunked highlighted install, and no dispatch of the install, publish or settle exceeds `B_SETTLE_US`

#### Scenario: A chunked cap holds for flipping keys on a worst-case document
- **WHEN** a key that changes the syntax context of the rest of a document at its language's chunked cap (a code fence or `<!--` in Markdown, `/*` in C) is typed at its start or its middle, in a document with no closer after the cursor, then its closer
- **THEN** every dispatch until quiet stays within `B_SETTLE_US`, and every ordinary key typed meanwhile within `B_INTERACT_US`

#### Scenario: A shipped Markdown document above its gate is not chunked
- **WHEN** a 400 KiB short-line Markdown document opens in a shipped build
- **THEN** syntax highlighting is reduced with the reason `TotalSize`, no chunked install runs, and no language is attached

#### Scenario: Markdown, C, Rust, Python and XML keep their gates
- **WHEN** a Markdown, C, Rust, Python or XML document larger than its measured threshold is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize` as before: the largest power of two that passes the flipping-key criteria is 128 KiB for markdown (an ordinary key inside an open `<!--` fails at 256 KiB), 64 KiB for c (a closing `/*` fails at 128 KiB), 128 KiB for rust and xml, and none for python3, so no cap exceeds that language's gate

#### Scenario: HTML, JSON, JavaScript, TypeScript, CSS, shell and YAML stay gated
- **WHEN** a document of one of those languages larger than its measured threshold is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize` as before, and no language is attached to it
