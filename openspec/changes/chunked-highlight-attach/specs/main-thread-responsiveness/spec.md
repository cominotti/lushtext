## ADDED Requirements

### Requirement: Highlighted documents above their attach threshold install chunk by chunk
A document whose incoming scale allows syntax highlighting, in one of the five languages with no measured per-key cost (markdown, rust, c, python3, xml), and larger than that language's attach-after-install threshold SHALL be installed with its language attached, in this order: a language attached to old text is removed before that text is cleared; the first paragraph-aligned chunk is inserted before the attach stage runs (a language already attached to the empty buffer may stay); the deferred attach stage attaches the language to that one chunk; and every later chunk, of at most `HIGHLIGHT_CHUNK_BYTES` (8 KiB) and ending on a paragraph boundary, is inserted only once the context engine has reported the buffer analysed to its end (ledger A84) or after one wait at `G_PRIORITY_LOW + 1`. Every installer of a whole body SHALL take this order: file load and reload, eviction reload, draft recovery, local-history restore and undo, and save-formatting mirror-back. A cancelled, superseded or disposed chunked install SHALL remove the language before clearing its partial text, and a sliced clear-only replacement SHALL remove it before clearing. Publication SHALL NOT toggle highlighting on a chunked document. The installed text SHALL be the decoded body byte for byte.

#### Scenario: The first frame shows text
- **WHEN** a 400 KiB Markdown document opens
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

## MODIFIED Requirements

### Requirement: Language attach stays out of the activation dispatch
The language catalog SHALL be built off the GTK thread and handed to it, and every LushText use of a GtkSourceView language manager SHALL go through that catalog. Guessing a document's language from its path SHALL run on a worker and return a language id. The catalog SHALL be started at application startup, so that it is ready before the activation's opens. The worker's path guess SHALL agree with GtkSourceView's `guess_language` for every glob of every shipped language, and at attach time GtkSourceView's guess is authoritative.

Every site that sets a document's language (path changes, load publication, the buffer-replacement terminal and document-scale changes) SHALL request one attach stage instead of attaching in its own dispatch. The stage SHALL run in its own dispatch after the editor's first frame; for an editor that is not mapped, after it is first mapped, except a stage a chunked highlighted install requested, which SHALL NOT wait for the editor to be mapped. While the attach is pending, the document's language id SHALL already be reported to every consumer that asks for it, including the Markdown preview. The attach SHALL run off the GTK thread only where the ledger's language-parse axiom shows that this is sound, and then only on a language object that no GTK-thread code could reach before the handoff.

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
- **WHEN** session restore creates an editor page that is not selected, for a document that does not take the chunked highlighted install
- **THEN** its language attach runs only after it is first mapped, and its language id is reported meanwhile

#### Scenario: The language attaches after the first frame
- **WHEN** a document with a recognised language opens
- **THEN** its first editor frame is painted before the language attach stage runs
- **AND** a later load, reload or rename of the same editor replaces any pending attach, so a stale language is never attached

### Requirement: Highlighting attach and background analysis are bounded
Publishing an installed document SHALL NOT turn on syntax highlighting in a dispatch whose cost grows with the document. The document scale's total-size syntax threshold `S_SYNTAX` SHALL be the measured `S_BG_HIGHLIGHT`: the largest short-line total size whose background analysis keeps every dispatch within `B_SETTLE_US`. Above it, syntax highlighting is reduced with the existing reduction reason `TotalSize`, shown as for the other reductions, except for the five languages whose context engine showed no per-key cost at 1 MiB on block-structured and on blank-line-free documents (markdown, rust, c, python3, xml): a document of one of them above its measured threshold and within C1's total-size cutoff (1 MiB, the constant `S_SYNTAX` in `model/document_scale.rs`) takes the chunked highlighted install instead, and is reduced with `TotalSize` only above `S_SYNTAX`. The budgets are not relaxed for background analysis.

#### Scenario: A large HTML document publishes without a document-sized dispatch
- **WHEN** a 5 MB short-line HTML document finishes its sliced installation
- **THEN** the publish dispatch takes at most `B_SETTLE_US`

#### Scenario: A document above the background-highlight threshold
- **WHEN** an HTML document larger than `S_BG_HIGHLIGHT` is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize`
- **AND** no background-analysis dispatch runs for it

#### Scenario: A large Rust document keeps its highlighting
- **WHEN** a short-line Rust document larger than its measured threshold and at most `S_SYNTAX` is opened
- **THEN** syntax highlighting stays on, the document takes the chunked highlighted install, and no dispatch of the install, publish or settle exceeds `B_SETTLE_US`

#### Scenario: HTML, JSON, JavaScript, TypeScript, CSS, shell and YAML stay gated
- **WHEN** a document of one of those languages larger than its measured threshold is opened
- **THEN** syntax highlighting is reduced with the reason `TotalSize` as before, and no language is attached to it
