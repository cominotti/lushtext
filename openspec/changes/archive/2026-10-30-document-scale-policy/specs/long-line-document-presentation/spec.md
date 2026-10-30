## ADDED Requirements

### Requirement: Documents carry a paragraph profile computed off GTK without an extra pass
The system SHALL compute a `DocumentProfile` for every document before its content is installed in an editor buffer. The profile holds:
- total bytes;
- the longest paragraph in characters, where a paragraph's length excludes its delimiter;
- per-threshold counts of long paragraphs;
- a bounded list of long-paragraph offsets with a truncation flag;
- the largest character sum of any `R_batch` consecutive paragraphs (wrap-off density).

For file loads, the profile MUST be accumulated inside the existing cancellable decode-analysis pass and MUST NOT add a second traversal of the decoded text. Draft recovery, local-history restore and undo, and save-formatting mirror-back installs MUST compute the profile on the worker that prepares their body. Profiling MUST NOT run on the GTK main thread. The result MUST NOT depend on how the text is divided into analysis slices.

#### Scenario: Load records the longest paragraph without another pass
- **WHEN** a synthetic file whose longest paragraph has 1,894,586 characters is loaded
- **THEN** the load metadata carries a profile whose longest paragraph is 1,894,586 characters
- **AND** the decoded text was traversed once by the analysis pass

#### Scenario: Profile is independent of slicing
- **WHEN** the same text is profiled as one slice and as any sequence of UTF-8-safe slices, including slices that split a CRLF pair and slices that end immediately before or after a U+2029
- **THEN** both profiles are equal

#### Scenario: Offset list stays bounded
- **WHEN** a document contains more long paragraphs than the profile's offset cap
- **THEN** the profile keeps at most the cap of offsets, marks itself truncated, and still counts every long paragraph

#### Scenario: Slice independence is machine-checked
- **WHEN** the Kani lane runs the profile harnesses over every string of at most 6 bytes drawn from `a`, `é`, `\r`, `\n`, U+2028, U+2029 and U+0085, cut into at most 3 slices at any scalar boundaries (empty slices included), with any profile configuration whose thresholds are at most 8, whose offset cap is at most 2 and whose batch row count is 1 to 3
- **THEN** it proves that the profile accumulated slice by slice equals the profile of the whole text in every field: total bytes, longest paragraph, each per-threshold count, the offset list and its truncation flag, and the largest batch sum
- **AND** that the whole-text profile equals the one built from the reference paragraph rule's paragraph lengths
- **AND** the accumulator's rustdoc states this domain and names the property test as the evidence for longer strings and the production configuration

#### Scenario: Profiling slices without the carried state is pinned as wrong
- **WHEN** a `should_panic` Kani harness profiles each slice with a fresh accumulator and combines the slice profiles componentwise
- **THEN** Kani finds a counterexample, such as a CRLF pair or a paragraph split across a cut, and the harness passes

#### Scenario: Cancelled load discards the profile
- **WHEN** a load is cancelled during analysis
- **THEN** no partial profile is published to the editor

### Requirement: Paragraph boundaries match GTK exactly
The system SHALL use one paragraph-boundary function for the profile, the edit-time watch, and every bounded install or replacement slicer. A paragraph ends at `\n`, `\r`, `\r\n` (one delimiter) or U+2029, which is the delimiter set of `pango_find_paragraph_boundary` that GtkTextBuffer uses. The function MUST NOT treat U+2028 or U+0085 as delimiters. A slice boundary it returns MUST lie on a UTF-8 scalar boundary, MUST advance past its start, MUST be either the text end or just after a delimiter, and MUST never fall between `\r` and `\n`. When the byte budget does not reach the text end, the slice MUST end after the last paragraph delimiter that ends within the budget, or, when none does, after the first delimiter beyond it; only text with no further delimiter MAY be taken whole.

#### Scenario: Agreement with Pango
- **WHEN** a property test splits generated strings drawn from an alphabet rich in `\r`, `\n`, U+2028, U+2029 and multibyte scalars
- **THEN** every paragraph split equals the one `pango::find_paragraph_boundary` produces

#### Scenario: CRLF is never split
- **WHEN** the byte budget of a slice ends immediately after a `\r` that is followed by `\n`
- **THEN** the slice ends after the `\n`

#### Scenario: Slice safety is machine-checked
- **WHEN** the Kani lane runs the paragraph-boundary harnesses
- **THEN** it proves, for every valid UTF-8 string of at most 8 bytes, every start on a scalar boundary that does not split a CRLF pair (every production start is the text start or a previous slice end) and every byte budget, that each returned boundary is a scalar boundary, advances, ends at a delimiter or the text end, and never separates CR from LF
- **AND** that the returned boundary is the slice end the reference rule chooses for that budget
- **AND** the rustdoc of the boundary functions states this 8-byte domain and names the Pango property test as the evidence for longer strings

#### Scenario: Agreement with the delimiter rule is machine-checked
- **WHEN** the Kani lane runs the paragraph-boundary harnesses
- **THEN** it proves, for every valid UTF-8 string of at most 8 bytes, that the paragraph breaks equal those of the reference rule: a paragraph ends at `\n`, at U+2029, or at a `\r` not followed by `\n`; `\r\n` is one delimiter; U+2028 and U+0085 never end a paragraph
- **AND** that converting a GTK line's character count to a paragraph length subtracts exactly its delimiter (one character, two for `\r\n`, none on the last line) and never underflows

#### Scenario: The retired newline-only slicer is pinned as wrong
- **WHEN** a `should_panic` Kani harness compares the transcribed retired `'\n'`-only slicer, with its byte budget made symbolic, with the slice end the reference rule chooses
- **THEN** Kani finds a CR-only or U+2029 counterexample and the harness passes

### Requirement: One monotone document scale policy governs feature availability
The system SHALL derive every size- and length-dependent feature decision from one GTK-free `DocumentScale` policy computed from the profile's total bytes, longest paragraph, and batch density. That policy SHALL replace the byte-only size classification.

The policy outputs are:
- open admission;
- undo;
- the live-memory buffer multiplier;
- local-history availability;
- syntax highlighting;
- bracket matching;
- minimap availability;
- invisible-character drawing;
- automatic search highlighting;
- the `protected_view` flag.

Each threshold MUST be a named policy constant. `L_syntax`, `L_minimap`, `L_invisibles`, `L_search_highlight`, `L_protect`, `N_visible`, `V_batch`, the batch row count and the syntax total-size cutoff MUST take the values that the `probe-long-line-toolkit-costs` measurement records; a threshold that measurement does not derive MUST say in its documentation which recorded value it defaults to. The total-size open, undo and local-history tiers and the minimap total-size tier MUST keep their current values (500 MB refuse, 50 MB undo off and history unavailable, 10 MB save-only history, 10 MB minimap). `protected_view` MUST hold exactly when the longest paragraph exceeds `L_protect` or the batch density exceeds `V_batch`.

The policy's outputs are of two kinds. The open, undo, buffer-multiplier and local-history outputs depend on total bytes alone and describe the document's latest loaded or saved bytes. The other outputs are presentation features.

The policy MUST be monotone: if one profile is componentwise no larger than another, the larger profile MUST NOT enable any feature that the smaller one disables, MUST NOT grant better local-history availability, MUST NOT clear `protected_view`, and MUST NOT raise the buffer multiplier. Callers that know only a byte count MUST use an explicit total-size-only constructor.

#### Scenario: Short-line document under the syntax size keeps every feature
- **WHEN** a 2 MB document whose longest paragraph and batch density are below every threshold is loaded
- **THEN** syntax highlighting, bracket matching, the minimap, invisibles and search highlighting remain available

#### Scenario: One giant paragraph disables syntax although the file is small
- **WHEN** a 5 MB document whose longest paragraph exceeds `L_syntax` is loaded
- **THEN** the language is none and syntax highlighting is off before the buffer is installed into a view that could highlight it
- **AND** no GtkSourceView context-engine timeout warning is emitted

#### Scenario: Threshold edges
- **WHEN** a profile component equals a threshold and when it exceeds that threshold by one
- **THEN** the feature stays enabled at the threshold and is disabled one above it

#### Scenario: Monotonicity is machine-checked
- **WHEN** the Kani lane runs the document-scale harnesses
- **THEN** it proves, for every pair of profiles over every `u64` component where the first is componentwise no larger than the second, that every presentation feature enabled for the second is enabled for the first, that the second has no better local-history availability and no larger buffer multiplier, and that `protected_view` of the first implies `protected_view` of the second
- **AND** it proves the meet laws of the session join on the presentation features (commutative, associative, idempotent, never enabling a feature either argument disables), that the join's byte-only outputs are its second argument's, and that the presentation features of the componentwise maximum of two profiles equal the join of their two policies
- **AND** it proves agreement of the open, undo, multiplier and local-history outputs with the retired byte tiers for every byte count

#### Scenario: Replacing the policy on save is pinned as wrong
- **WHEN** a `should_panic` Kani harness replaces the effective policy with the total-size-only policy of the saved bytes instead of joining it
- **THEN** Kani finds a presentation feature that comes back on, and the harness passes

#### Scenario: Byte-only callers keep their tiers
- **WHEN** file peek, a bookmark excerpt, draft availability or the local-history journal classifies a file it knows only by byte size
- **THEN** it uses the total-size-only constructor and gets the same open and history decisions as before this change

### Requirement: Edits downgrade features synchronously and never re-enable them
The system SHALL update the editor's live profile from buffer `insert-text` and `delete-range` signals in after-handlers, during the same emission as the edit, and SHALL apply any resulting feature downgrade before the handler returns, so no GTK validation or highlight idle can process a newly giant paragraph with that feature on. Each update MUST be bounded by the inserted run plus a constant number of line-length reads whose cost does not grow with line length. Within one document session, the effective presentation features MUST be the meet of every policy computed so far: edits, saves and buffer-replacement terminals MUST NOT re-enable a presentation feature. The byte-only outputs MUST follow the latest loaded or saved byte count, as before this change. The watch MUST measure the buffer's own lines, which depend on its edit history: an insertion splits only its own text at the paragraph delimiters and never joins with or splits around the characters beside it, and a deletion joins exactly the lines whose starts it removes (GTK axiom A42). An edit that changes the buffer's line count MUST be treated as able to raise both the longest paragraph and the batch density. A fast path that skips the density recomputation MUST carry its upper bound across consecutive edits. The watch MUST stand down while a load installation or buffer-replacement session is active, because those installers supply their profile from the worker.

#### Scenario: Paste creates a giant paragraph
- **WHEN** the user pastes text that makes one paragraph longer than `L_syntax` into a highlighted document
- **THEN** the language is cleared and syntax highlighting is off before the paste's signal emission returns
- **AND** the inserted paragraph never receives context-class tags

#### Scenario: Joining lines creates a giant paragraph
- **WHEN** a deletion removes the delimiter between two paragraphs whose combined length exceeds a feature threshold
- **THEN** that feature is downgraded in the same `delete-range` emission

#### Scenario: Deleting the giant line does not restore features
- **WHEN** the user deletes the paragraph that caused a downgrade and saves the file
- **THEN** the downgraded features stay off for the rest of the document session

#### Scenario: Save cannot re-enable syntax
- **WHEN** a document whose syntax highlighting was turned off is saved at a size below every threshold
- **THEN** syntax highlighting stays off until the document is reopened

#### Scenario: Saving a shrunk document keeps the local-history tiers
- **WHEN** a document larger than 10 MB is edited below 10 MB and saved
- **THEN** local history becomes fully available for it, as before this change
- **AND** no presentation feature that was off comes back on

#### Scenario: Edit watch and load profile agree at a threshold
- **WHEN** an edit makes a paragraph exactly as long as a feature threshold, with a `\n`, `\r`, `\r\n` or U+2029 delimiter
- **THEN** the feature stays enabled, exactly as the load profile would decide for the same text
- **AND** one more character disables it

#### Scenario: Ordinary typing stays on the fast path
- **WHEN** the user types characters without delimiters into a paragraph well below every threshold, while the watch's carried density bound plus each keystroke stays within `V_batch`
- **THEN** the watch performs no density-window recomputation
- **AND** the number of line-length reads per keystroke is bounded by a constant that a widget test asserts

#### Scenario: Text typed between a CR and an LF stays in its line
- **WHEN** the user inserts characters without a delimiter between the `\r` and the `\n` of a CRLF pair
- **THEN** the buffer gains no line, but the line's `\r\n` ending becomes `\n` and its paragraph grows by one more than the typed characters, so the watch takes the window path and measures the grown line, its embedded `\r` included

#### Scenario: Deleting the text between a CR and an LF removes no line
- **WHEN** a deletion removes everything between a `\r` and a following `\n`
- **THEN** the buffer keeps the `\r`-ended line and the `\n` line apart, no paragraph grows, and the watch does no work

#### Scenario: Deleting the LF of a CRLF joins two lines
- **WHEN** a deletion removes the `\n` of a CRLF pair, so the following line's start is removed
- **THEN** the two lines join and the watch recomputes the longest paragraph and the density window in the same emission

#### Scenario: Repeated keystrokes cannot cross the density limit unseen
- **WHEN** the user types into one paragraph keystroke by keystroke, each keystroke small enough for the fast path, until a window's sum exceeds `V_batch`
- **THEN** the keystroke that makes the sum exceed `V_batch` takes the window path and sets `protected_view`

#### Scenario: Incremental update equals recomputation is machine-checked
- **WHEN** the Kani lane runs the edit-watch harnesses as an inductive step: from every buffer of at most 2 symbols from `a`, `\r`, `\n` and U+2029 with any line structure the history-dependent rule (A42) can reach, and every watch state related to it by the invariant, one edit inserting at most 2 symbols at any offset or deleting any range of at most 2 symbols, every batch row count of 1 or 2, and every `V_batch`, state value and high-water of at most 16 (weakened from the designed bounded replay of 3 edits from a 4-symbol start, which did not fit the lane)
- **THEN** it proves that the step preserves the invariant, which holds after a load, so for every edit sequence within that domain the watched longest paragraph equals the largest longest paragraph recomputed from scratch, by the production length fold over the model's line lengths, over every document so far
- **AND** that the watch's density bound is never below the current document's recomputed largest batch sum, that its batch sum never exceeds the recomputed high-water and equals it whenever no step took the fast path, and that it exceeds `V_batch` exactly when the recomputed high-water does
- **AND** that every step asks for at most `2·R_batch + 2` line-length reads, and a fast-path step for one
- **AND** that the same holds for deleting the symbol between `\r` and `\n` in every three-symbol buffer `\r`, any symbol, `\n` with any such line structure and state, a case the 2-symbol step cannot reach
- **AND** the watch core's rustdoc states this domain

#### Scenario: The uncarried fast-path bound is pinned as wrong
- **WHEN** a `should_panic` Kani harness runs the watch with the fast-path bound reset to the batch sum before each step
- **THEN** Kani finds a missed density crossing and the harness passes

#### Scenario: Install slices do not re-profile on GTK
- **WHEN** a large load or buffer replacement inserts its slices
- **THEN** the edit-time watch does no profiling work for those insertions

#### Scenario: Crossing the protected threshold before Protected View exists
- **WHEN** an edit makes a paragraph longer than `L_protect` and Protected View is not yet implemented
- **THEN** `protected_view` becomes true in the effective policy and the reduced-features notice reports it
- **AND** the buffer contents and modified state are unchanged by the downgrade

### Requirement: Scale-gated features are applied before content reaches them
The system SHALL apply syntax highlighting, bracket matching, invisible-character drawing and automatic search highlighting from the effective policy at load publication, at document-identity republication and at every edit-time downgrade. A disabled feature MUST NOT be re-applied by a later language detection, rename, Save As, preference change or search session in the same document session. Gating MUST change only presentation and MUST NOT change buffer text, file bytes or the modified state.

#### Scenario: Rename does not reapply a gated language
- **WHEN** a document with syntax gated off is renamed or saved with a new extension
- **THEN** language detection is skipped and highlighting stays off

#### Scenario: Search session respects gated highlighting
- **WHEN** the in-tab search bar opens on a document whose policy disables search highlighting
- **THEN** the search context is created with highlighting off
- **AND** match navigation and counts keep their existing behaviour

#### Scenario: Bracket matching is off for gated documents
- **WHEN** the policy disables bracket matching for the active document
- **THEN** the buffer's matching-bracket highlighting is off

### Requirement: The editor's accessible description names reduced features
The system SHALL describe reduced features in the editor's accessible description using the policy's reduced-features list and reason classes, bounded by the existing accessible-text limits, in place of the former byte-tier sentences. The list MUST include undo history and local history when the total-size tiers reduce them. A document refused by open admission MUST keep its disabled accessible state and its statement that it is too large to open.

#### Scenario: Screen reader learns why highlighting is off
- **WHEN** an assistive technology reads the editor of a document whose syntax highlighting is off because of a long paragraph
- **THEN** the description states that syntax highlighting is off because of very long lines
- **AND** the description contains no document content

#### Scenario: Huge document still names undo
- **WHEN** an assistive technology reads the editor of a document larger than 50 MB
- **THEN** the description states that undo history is off because of the document's total size
