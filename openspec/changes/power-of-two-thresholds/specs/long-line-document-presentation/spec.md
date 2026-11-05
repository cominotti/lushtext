## MODIFIED Requirements

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
- word wrapping (`wrap_allowed`);
- the `protected_view` flag.

Each threshold MUST be a named policy constant. `L_syntax`, `L_minimap`, `L_invisibles`, `L_search_highlight`, `L_protect`, `L_edit_stall`, `L_protect_edit`, `N_visible`, `V_batch`, the batch row count and the syntax total-size cutoff MUST take the values that the `probe-long-line-toolkit-costs` measurement records; a threshold that measurement does not derive MUST say in its documentation which recorded value it defaults to. Since `power-of-two-thresholds`, every threshold MUST follow the `threshold-constants` capability: a cap is that measured value rounded down to a power of two (`N_visible` 262,144), the batch row count is a coverage window rounded up (`R_batch` 256), and `L_syntax` (2,000), `L_minimap` (22,000), `L_invisibles` (50,000), `L_search_highlight` (10,000), `L_protect_edit` (50,000), `V_batch` (56,000) and every per-language total-size cutoff whose rounding down loses highlighting in the corpus (all but CSS, which becomes 32 KiB) are recorded exceptions whose documentation gives the measurement that keeps them exact (the corpus count of `power-of-two-thresholds` task 4.5 found a loss above the `threshold-constants` materiality floor for each of the three paragraph caps: 0.73 %, 0.085 % and 0.31 % of the files that have the feature). The total-size tiers MUST be powers of two in IEC units: the minimap and save-only local history turn off above 8 MiB, undo turns off and local history becomes unavailable above 64 MiB, and a file above 512 MiB is refused (the exclusive-load memory measurement of `power-of-two-thresholds` admitted it: the peak stayed within 1.074 times the peak at 500,000,000 bytes). Every message and accessible text that names a tier MUST print it in IEC units. `L_edit_stall` is the shortest paragraph length at which one keystroke inside the paragraph exceeds `B_interact` on the reference machine, as `long-line-protected-view` measured it (task 1.1: about 3,500 to 5,000 characters with word wrap on, 10,000 to 14,000 with it off); it is recorded, and it decides nothing. `protected_view` MUST hold exactly when the longest paragraph exceeds `L_protect_edit` = 50,000, the maintainer's choice of 2026-10-30, at which the measured median keystroke cost inside the paragraph reaches about 20 ms with word wrap on; the constant's documentation MUST cite that decision and the measurement. `L_protect` (512, equal to `N_inspect`: `power-of-two-thresholds` measured dense 512-character lines within the stall basis 500 was measured on) remains a profile threshold (the long-paragraph offsets and counts) and MUST NOT decide `protected_view`; neither `L_edit_stall` nor the batch density may decide it, so a document whose paragraphs are all at most `L_protect_edit` characters stays editable under the reduced-features policy. `L_wrap_edit` MUST take the value the responsiveness lane's keystroke group derives (the largest paragraph length whose wrapped keystroke meets B_interact by the lane's rule); `wrap_allowed` MUST be false exactly when the longest paragraph exceeds `L_wrap_edit`.

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
- **AND** it proves agreement of the open, undo, multiplier and local-history outputs with the byte-tier reference table (8 MiB, 64 MiB and the open tier) for every byte count

#### Scenario: Replacing the policy on save is pinned as wrong
- **WHEN** a `should_panic` Kani harness replaces the effective policy with the total-size-only policy of the saved bytes instead of joining it
- **THEN** Kani finds a presentation feature that comes back on, and the harness passes

#### Scenario: Byte-only callers keep their tiers
- **WHEN** file peek, a bookmark excerpt, draft availability or the local-history journal classifies a file it knows only by byte size
- **THEN** it uses the total-size-only constructor and gets the open and history decisions of the byte-tier table, the same tiers a profiled load of the same bytes gets

#### Scenario: A paragraph over the wrap threshold disables wrapping only
- **WHEN** a document whose longest paragraph exceeds `L_wrap_edit` but not `L_protect_edit` is loaded
- **THEN** `wrap_allowed` is false and the document stays editable
- **AND** the monotonicity and meet-law harnesses cover `wrap_allowed` like every other presentation feature

### Requirement: Edits downgrade features synchronously and never re-enable them
The system SHALL update the editor's live profile from buffer `insert-text` and `delete-range` signals in after-handlers, during the same emission as the edit, and SHALL apply any resulting feature downgrade before the handler returns, so no GTK validation or highlight idle can process a newly giant paragraph with that feature on. Each update MUST be bounded by the inserted run plus a constant number of line-length reads whose cost does not grow with line length. Within one document session, the effective presentation features MUST be the meet of every policy computed so far: edits, saves and buffer-replacement terminals MUST NOT re-enable a presentation feature. The one exception is the bounded undo of the edit that set `protected_view` (`long-line-protected-view`): it MUST restore the effective policy recorded immediately before that edit, because the undo removes exactly the content that edit added. The byte-only outputs MUST follow the latest loaded or saved byte count, as before this change. The watch MUST measure the buffer's own lines, which depend on its edit history: an insertion splits only its own text at the paragraph delimiters and never joins with or splits around the characters beside it, and a deletion joins exactly the lines whose starts it removes (GTK axiom A42). An edit that changes the buffer's line count MUST be treated as able to raise both the longest paragraph and the batch density. A fast path that skips the density recomputation MUST carry its upper bound across consecutive edits. The watch MUST stand down while a load installation or buffer-replacement session is active, because those installers supply their profile from the worker.

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
- **WHEN** a document larger than 8 MiB is edited below 8 MiB and saved
- **THEN** local history becomes fully available for it
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
- **THEN** the keystroke that makes the sum exceed `V_batch` takes the window path and records the crossing in the watch state, without setting `protected_view`

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
- **WHEN** an edit makes a paragraph longer than `L_protect_edit` and Protected View is not yet implemented
- **THEN** `protected_view` becomes true in the effective policy and the reduced-features notice reports it
- **AND** the buffer contents and modified state are unchanged by the downgrade

### Requirement: The editor's accessible description names reduced features
The system SHALL describe reduced features in the editor's accessible description using the policy's reduced-features list and reason classes, bounded by the existing accessible-text limits, in place of the former byte-tier sentences. The list MUST include undo history and local history when the total-size tiers reduce them. A document refused by open admission MUST keep its disabled accessible state and its statement that it is too large to open.

#### Scenario: Screen reader learns why highlighting is off
- **WHEN** an assistive technology reads the editor of a document whose syntax highlighting is off because of a long paragraph
- **THEN** the description states that syntax highlighting is off because of very long lines
- **AND** the description contains no document content

#### Scenario: Huge document still names undo
- **WHEN** an assistive technology reads the editor of a document larger than 64 MiB
- **THEN** the description states that undo history is off because of the document's total size
- **AND** any size it names is in IEC units

### Requirement: The Line Inspector exposes giant-line content in bounded windows
The system SHALL provide an "Inspect Line…" command. It SHALL be reachable from the Protected View context menu, from the fold-marker gutter, from the command palette and from a keyboard shortcut. The command SHALL open a secondary read-only view over a copied window of at most `N_inspect` characters of one paragraph, with word wrap on, where `N_inspect` never exceeds `min(head_cap, L_PROTECT)` and equals `L_PROTECT`, both the same power of two, 512 (`power-of-two-thresholds` design D7: the stall basis holds at 512). The inspector SHALL offer Previous Window, Next Window and Go to Column. It SHALL show the window's column range and the paragraph length. A keyboard-only invocation SHALL default to the first folded paragraph in the viewport and accept a line number.

Window text SHALL be read off GTK from the immutable Protected View text source (`ProtectedTextSource`), which for every whole-content install is the installed body itself, moved into long-lived charged ownership at the install's terminal, and which is captured once only after an edit-growth entry. Copy inside the inspector SHALL be bounded to its window. "Copy Whole Line" SHALL publish a lazy `GdkContentProvider` that writes the paragraph from the immutable source in bounded chunks when a client reads it. No GTK dispatch SHALL copy more than one chunk, and the copied line SHALL never be materialized whole on the GTK thread.

#### Scenario: Reading the middle of a giant line
- **WHEN** the user opens the inspector on a 1,900,000-character paragraph and goes to column 950,000
- **THEN** the inspector shows a window of at most `N_inspect` characters containing column 950,000, and the GTK work in each dispatch stays within `B_interact`

#### Scenario: Whole-line copy streams in bounded chunks
- **WHEN** the user chooses Copy Whole Line and another application reads the clipboard
- **THEN** the content provider writes the paragraph's exact characters in bounded chunks, and no GTK dispatch copies more than one chunk
