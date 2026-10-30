## ADDED Requirements

### Requirement: Documents over the protected thresholds open in Protected View
The system SHALL present a document in Protected View whenever C1's `DocumentScale::protected_view` holds for it, that is, when its longest paragraph exceeds `L_EDIT_STALL`, the measured shortest paragraph length at which one keystroke inside the paragraph exceeds `B_interact` on the reference machine (see the modified requirement "One monotone document scale policy governs feature availability"). `L_PROTECT` (500) and the batch density SHALL NOT make a document view-only: a document whose paragraphs are all at most `L_EDIT_STALL` characters stays editable under C1's reduced-features policy. For every whole-content install (load, eviction reload, external reload, reopen with encoding, draft recovery, set-aside open, local-history restore and Undo Restore) the decision SHALL be taken from the worker-side profile of the body being installed, before any of its text is inserted into a GtkTextView, and the protected view properties SHALL be applied before the first inserted byte. Entry SHALL also happen when C1's edit-time watch reports that an edit newly sets `protected_view`, inside the same `insert-text` or `delete-range` after-handler emission (a deletion that removes a paragraph delimiter can join two paragraphs into a giant one); in that emission the insert and selection-bound marks SHALL be placed at the start of the paragraph that grew, so that no mark is left inside a hidden tail. Protected View SHALL be monotone for the editor session: no edit, preference change, tab switch, eviction reload, draft recovery, or local-history restore or Undo Restore SHALL return the document to the editable view. There are two exits. A reload of the file from disk (an external-change reload or an explicit reload) SHALL re-derive the decision from the new content's profile, so a file that has shrunk below the thresholds becomes editable again. And when the document entered Protected View through an edit, the system SHALL offer one bounded undo of exactly that edit, from the Protected View alert and through Ctrl+Z, while the edit is still the newest step of the buffer's undo history and undo history is on; the undo SHALL run the buffer's own undo of that step, SHALL restore the effective feature policy recorded immediately before the edit, and SHALL end the Protected View session. The entry undo is the only buffer change Protected View admits besides whole-content installs.

#### Scenario: Giant-paragraph file opens protected
- **WHEN** the user opens a synthetic file whose longest paragraph exceeds `L_EDIT_STALL`
- **THEN** the editor installs the document in Protected View, and no text of the file ever exists in that view's buffer without the Protected View fold applied

#### Scenario: Prose with long paragraphs stays editable
- **WHEN** the user opens a Markdown file whose paragraphs are up to 8,000 characters long, each written on one line, all at most `L_EDIT_STALL`
- **THEN** the document opens editable with C1's reduced features, and Protected View is not entered

#### Scenario: Dense short-line file stays editable
- **WHEN** the user opens a synthetic file in which every paragraph is at most `L_EDIT_STALL` characters but some window of `R_BATCH` paragraphs exceeds `V_BATCH` characters
- **THEN** the document opens editable, and Protected View is not entered

#### Scenario: Paste creates a giant paragraph
- **WHEN** a paste, undo, move-lines or join-lines in an editable document produces a paragraph longer than `L_EDIT_STALL`
- **THEN** the editor enters Protected View within the same `insert-text` or `delete-range` emission, before any GTK validation or highlight idle runs, the insert and selection-bound marks are at the start of the grown paragraph, and the document keeps its modified state

#### Scenario: Recovered draft with a giant paragraph opens protected
- **WHEN** startup recovery restores a draft body whose last paragraph holds more than 33 million characters (the crash-recovery smoke's 32 MiB single-line fixture)
- **THEN** the restored editor is in Protected View, stays modified and draft-restored, and the paragraph's tail was inserted under the fold tag in the same dispatch as its head

#### Scenario: Protected View never reverts within a session
- **WHEN** the user changes preferences, switches tabs, or presses any edit key other than the entry undo in a protected document
- **THEN** the document stays in Protected View for the life of that editor

#### Scenario: Undoing the edit that made the document protected
- **WHEN** a paste makes a paragraph longer than `L_EDIT_STALL`, and the user then chooses Undo in the Protected View alert or presses Ctrl+Z
- **THEN** exactly that paste is undone in bounded work, the document returns to the editable view with the effective feature policy it had before the paste, the buffer equals its content before the paste, and the insert mark is where the undo leaves it

#### Scenario: Entry undo is not offered when it cannot be bounded
- **WHEN** a document entered Protected View on load, or through an edit while its undo history was off (C1's undo tier), or the entry edit is no longer the newest undo step
- **THEN** neither the alert nor Ctrl+Z offers the entry undo, and Ctrl+Z publishes the Protected View notice

### Requirement: Protected View never modifies the buffer
While Protected View is active, no interaction, keybinding, action, gesture, drop, menu entry, save path or other LushText operation, and no reachable GTK or GtkSourceView default handler, SHALL insert, delete or replace text in the document's buffer. The only exceptions are whole-content installs that the user explicitly requested or that reload the file from disk (load, eviction reload, external-change reload, reopen with encoding, draft recovery, set-aside open, and local-history restore and Undo Restore), and each of them SHALL go through the fold-at-insertion executor, plus the one bounded entry undo of an edit-growth entry, which ends the Protected View session. Save paths SHALL NOT mirror formatted text back into a protected buffer. A document that was installed protected SHALL keep `modified == false`. Tag application and removal for the fold and for the current find match SHALL be the only buffer-level changes, and they SHALL NOT emit `changed`, set `modified` or record undo history (ledger A27).

#### Scenario: Interaction leaves an installed protected document unmodified
- **WHEN** the user scrolls, clicks, drags, types printable keys, presses Backspace, Delete or Enter, invokes paste or cut by shortcut or menu, or drops data onto a protected document
- **THEN** the buffer's character count and content generation are unchanged and `modified` stays false

#### Scenario: No draft or history capture for an installed protected document
- **WHEN** a protected document that was installed unmodified stays open across autosave ticks and the periodic local-history interval
- **THEN** no draft body is written and no local-history snapshot is captured for it

### Requirement: Protected View folds giant paragraphs for display only, at insertion
In Protected View every paragraph SHALL show at most `head_cap` characters, where `head_cap = min(N_VISIBLE, N_HEAD, A28_SAFE_CHARS)`: `N_HEAD` is the largest visible head whose first layout, in either wrap mode, and whose re-layout on a word-wrap toggle each fit `B_settle` per dispatch on the reference lane, and `A28_SAFE_CHARS` keeps every visible head below Pango's `int32` x-position overflow (ledger A28). In addition, a paragraph that belongs to any window of `R_fold` consecutive paragraphs whose `head_cap`-capped character sum exceeds `V_BATCH` SHALL show at most `N_DENSE = V_BATCH / R_BATCH` characters. No other paragraph SHALL be folded: a paragraph of at most `head_cap` characters that lies in no over-budget window keeps all its characters visible. `R_fold` is C1's `R_BATCH`, plus one if C0's value does not count the partially covered edge row of a 2000 px validation batch. Every window of `R_fold` consecutive paragraphs, and therefore every 2000 px validation batch in either wrap mode, SHALL hold at most `V_BATCH` visible characters. The fold plan SHALL NOT depend on the wrap mode, so toggling word wrap never re-folds. A folded paragraph's tail SHALL carry exactly one Protected View fold tag with `invisible=true` and `editable=false`. Only tails SHALL be hidden: a head, any paragraph start, and any paragraph delimiter SHALL never be hidden, and a non-empty paragraph SHALL keep at least `min(len, N_DENSE)` visible characters.

The fold SHALL be applied at insertion. Every install path SHALL insert a folded tail already tagged, in the same dispatch as its head, so that no paragraph with more visible characters than the plan allows exists in any LushText `GtkTextView` at any dispatch boundary. The install paths are initial load, buffer replacement (draft recovery, set-aside open, local-history restore and Undo Restore, eviction reload), external reload, reopen with encoding and the local-history preview. The fold placement SHALL be decided by the pure GTK-free streaming policy `model/protected_fold.rs` and applied by one cross-cutting GTK executor. The placement SHALL be computed off GTK from the complete paragraph sequence of the content being installed, in the worker pass that runs C1's `ParagraphProfileAccumulator` over the body, not from the profile's bounded long-paragraph offset list. Its state SHALL be bounded by `R_fold`. The resulting plan SHALL be compact (one range per folded paragraph) and its retained size SHALL be charged to the install's transient admission. Fold boundaries SHALL fall on character boundaries.

#### Scenario: Head shown, tail hidden
- **WHEN** a protected document contains a paragraph of 1,900,000 characters between short paragraphs
- **THEN** at most `head_cap` of its characters are visible, never more than `A28_SAFE_CHARS`, and the remaining characters are present in the buffer under the fold tag

#### Scenario: A protected document with moderately long lines is not folded
- **WHEN** a document is protected only because some of its paragraphs are 600 characters long, and no window of `R_fold` paragraphs exceeds `V_BATCH` characters
- **THEN** no paragraph is folded and every character is visible

#### Scenario: Dense windows fold to N_DENSE
- **WHEN** a protected document holds `2 · R_fold` consecutive paragraphs of 700 characters
- **THEN** each of them shows exactly `N_DENSE` characters, and paragraphs outside every over-budget window keep their full length up to `head_cap`

#### Scenario: Fold exists from the first inserted slice
- **WHEN** a widget test inspects the protected buffer after every install turn
- **THEN** no paragraph has more visible characters than the fold plan allows at any turn boundary

#### Scenario: Validation batches stay within V_BATCH
- **WHEN** the dense fixture is installed protected and laid out with word wrap off and then on
- **THEN** every 2000 px window of laid-out rows holds at most `V_BATCH` visible characters, and toggling wrap changes no fold

#### Scenario: Fold placement properties are machine-checked
- **WHEN** `make kani` runs the `protected_fold` harnesses
- **THEN** they prove the properties and domains stated in "Protected View pure policies carry Kani proofs over stated domains"

### Requirement: Protected View offers no caret, selection or edit paths
In Protected View the main source view SHALL have `editable=false` and `cursor-visible=false`, set before the first byte of an install (the `editable` setter invalidates every line, ledger A37), and SHALL have no minimap (`GtkSourceMap`) for the rest of the session, whatever C1 allows. Syntax highlighting and bracket matching SHALL follow C1's `DocumentScale`. The view SHALL handle `move-cursor` itself and map every movement step to a viewport movement, even when `gtk-keynav-use-caret` is enabled; this covers arrows, Page Up/Down, Home/End and Ctrl+Home/End. A capture-phase gesture SHALL claim every primary, middle and secondary press and drag sequence over the view and its gutters, including double- and triple-click, before GtkTextView's own gestures run. Drag-and-drop source and target SHALL be disabled. No LushText or toolkit input path SHALL move the insert or selection-bound mark while Protected View is active. Assistive-technology caret and selection writes (`GtkAccessibleText` `set_caret_position`/`set_selection`, gtktextview.c:11074-11119) are a toolkit path LushText cannot pre-empt; their cost on a folded paragraph SHALL be pinned by a ledger axiom and the lane's AT case, and the Protected View evidence SHALL count any insert-mark move it observes.

GtkTextView recomputes the per-instance enablement of its `clipboard.*` and `selection.*` actions on every popup and selection change (gtktextview.c:9317-9348), so disabling them per instance is not a guard. Protected View SHALL instead pre-empt, before the default handler runs, every default key binding and keybinding signal of GtkTextView and GtkSourceView that can move a mark or change the buffer, and SHALL publish the single bounded Protected View notice for each pre-empted user request. This includes at least:
- GtkTextView's `select-all` signal (Ctrl+A, Ctrl+/, and their Shift variants), which selects the whole buffer through `gtk_text_buffer_select_range` regardless of `editable` (gtktextview.c:9206-9224);
- GtkTextView's `toggle-cursor-visible` signal (F7), which would turn the caret back on;
- GtkTextView's `menu.popup` bindings (Shift+F10, Menu), which open the default context menu containing Select All;
- GtkTextView's `cut-clipboard`, `copy-clipboard`, `paste-clipboard`, `backspace`, `delete-from-cursor`, `insert-at-cursor`, `toggle-overwrite`, `set-anchor`, `extend-selection` and `insert-emoji` signals, and the `text.undo`/`text.redo` bindings;
- GtkSourceView's `change-number` (Ctrl+Shift+A, Ctrl+Shift+X), `change-case` and `join-lines`, whose default handlers change the buffer without checking `editable` (gtksourceview.c:434-560);
- GtkSourceView's `move-to-matching-bracket` (Ctrl+%), which places the cursor, and `move-lines`, `move-words`, `show-completion` and `push-snippet`;
- buffer-mutating line-ending normalization;
- toggle-bookmark-at-cursor.

A widget test SHALL enumerate the view's class shortcut controllers on the pinned toolkit versions and assert that every bound action and signal is classified by the pure `action_disposition` policy as either viewport-safe or pre-empted, so a binding added by a toolkit update fails the test instead of reaching the buffer.

The secondary-press context menu and the keyboard menu bindings SHALL open a Protected View menu that offers Inspect Line, Copy Whole Line and Find, and that shows the disabled entries as insensitive. GtkTextView's default context menu SHALL never be shown.

#### Scenario: Arrow keys scroll under caret browsing
- **WHEN** `gtk-keynav-use-caret` is enabled and the user presses Down, Page Down and Ctrl+End in a protected document
- **THEN** the viewport scrolls, the insert mark offset is unchanged, and no IM reset is requested

#### Scenario: Click does not place a caret
- **WHEN** the user single-, double- or triple-clicks or drags inside a giant paragraph's head
- **THEN** the capture-phase gesture claims the sequence, the insert and selection-bound marks do not move, and no selection exists

#### Scenario: Disabled edit action shows a notice
- **WHEN** the user presses Ctrl+V, Ctrl+X, Ctrl+A, Ctrl+Shift+A, Ctrl+%, F7, Shift+F10, Ctrl+Z or Alt+Up in a protected document
- **THEN** nothing in the buffer changes, the insert and selection-bound marks do not move, `cursor-visible` stays false, and the Protected View notice explains that the document is view-only and how to inspect a line

#### Scenario: Every toolkit binding is classified
- **WHEN** the widget test enumerates the class shortcuts of the protected GtkSourceView
- **THEN** every bound action and signal has an `action_disposition`, and none classified as pre-empted reaches its default handler

### Requirement: The Line Inspector exposes giant-line content in bounded windows
The system SHALL provide an "Inspect Line…" command. It SHALL be reachable from the Protected View context menu, from the fold-marker gutter, from the command palette and from a keyboard shortcut. The command SHALL open a secondary read-only view over a copied window of at most `N_inspect` characters of one paragraph, with word wrap on, where `N_inspect` never exceeds `min(head_cap, L_PROTECT)` (500 with C0's values). The inspector SHALL offer Previous Window, Next Window and Go to Column. It SHALL show the window's column range and the paragraph length. A keyboard-only invocation SHALL default to the first folded paragraph in the viewport and accept a line number.

Window text SHALL be read off GTK from the immutable Protected View text source (`ProtectedTextSource`), which for every whole-content install is the installed body itself, moved into long-lived charged ownership at the install's terminal, and which is captured once only after an edit-growth entry. Copy inside the inspector SHALL be bounded to its window. "Copy Whole Line" SHALL publish a lazy `GdkContentProvider` that writes the paragraph from the immutable source in bounded chunks when a client reads it. No GTK dispatch SHALL copy more than one chunk, and the copied line SHALL never be materialized whole on the GTK thread.

#### Scenario: Reading the middle of a giant line
- **WHEN** the user opens the inspector on a 1,900,000-character paragraph and goes to column 950,000
- **THEN** the inspector shows a window of at most `N_inspect` characters containing column 950,000, and the GTK work in each dispatch stays within `B_interact`

#### Scenario: Whole-line copy streams in bounded chunks
- **WHEN** the user chooses Copy Whole Line and another application reads the clipboard
- **THEN** the content provider writes the paragraph's exact characters in bounded chunks, and no GTK dispatch copies more than one chunk

### Requirement: Navigation into protected documents scrolls without a caret
Navigation into a protected document SHALL scroll a LushText-owned navigation mark to the target paragraph and SHALL NOT move the insert mark. This covers session restore position, bookmark activation, local-history targets, workspace-search hits and protected find results. When the target column lies inside a hidden tail, the system SHALL scroll to the paragraph and open the Line Inspector at the window that contains the target column.

#### Scenario: Workspace search hit inside a hidden tail
- **WHEN** the user activates a workspace-search hit at a column inside the hidden tail of a folded paragraph
- **THEN** the editor scrolls to the paragraph and the Line Inspector opens on the window containing that column

### Requirement: In-tab find in Protected View runs on a worker
No `GtkSourceSearchContext` SHALL ever be attached to a protected buffer. In-tab find SHALL run on a worker over the immutable Protected View text source and SHALL find matches in hidden tails as well as in visible text. Requests SHALL be single-flight: one active request and one latest pending request, with generation freshness. Results SHALL be paragraph-and-column records delivered to GTK in batches bounded by count and bytes, under a total result cap. Activating a result SHALL scroll to it, and SHALL highlight it with a display tag only when the match lies entirely in visible text; otherwise activation SHALL open the inspector. The search counter SHALL report found and capped states. In-tab Replace SHALL be unavailable. The editor-side and window-side selection-prefill sites SHALL both read `visible=true` text under the existing prefill bound.

#### Scenario: Find in a giant line does not block
- **WHEN** the user searches for a string that occurs 100,000 times in a protected document whose paragraph is 1,900,000 characters long
- **THEN** no GTK dispatch exceeds `B_interact`, results arrive in bounded batches, and the counter reports the capped total

#### Scenario: Superseded query is cancelled
- **WHEN** the user types three characters faster than the worker completes
- **THEN** the obsolete requests are cancelled, at most one pending request is retained, and only the latest generation publishes results

### Requirement: Print is refused in Protected View
Invoking `win.print` on a protected document SHALL NOT start a print operation. It SHALL publish a notice that printing view-only long-line documents is not supported. Fold-aware pagination is out of scope.

#### Scenario: Print request on a protected document
- **WHEN** the user presses Ctrl+P with a protected document active
- **THEN** no print dialog opens and the status lane explains why

### Requirement: Secondary views of protected documents follow the same policy
Every secondary view of a protected document SHALL follow the Protected View policy:
- **Local-history preview:** installs the snapshot protected and folded through the same executor, and the preview view is itself non-editable and caret-free, with the same capture gesture and key pre-emption.
- **Markdown preview:** renders a paragraph longer than its admission budget as the existing `Simplified` or limited terminal, with the omission announced.
- **File peek:** shows only the bounded sample, and the sample's longest line SHALL NOT exceed `head_cap` visible characters.
- **Notes editors:** a note body whose profile reaches `protected_view` SHALL open read-only with the same notice.

#### Scenario: Local-history preview of a giant-paragraph snapshot
- **WHEN** the user previews a local-history snapshot whose profile reaches `protected_view`
- **THEN** the preview view is non-editable, caret-free and folded at insertion

### Requirement: Folded lines are marked in a gutter
A Protected View gutter renderer SHALL mark each paragraph that has a hidden tail and SHALL expose the hidden-character count in its tooltip and accessible label. Activating the marker SHALL open the Line Inspector on that paragraph. The renderer's per-frame work SHALL be bounded by the number of visible rows.

#### Scenario: Activating a fold marker
- **WHEN** the user clicks the fold marker next to a folded paragraph
- **THEN** the Line Inspector opens on that paragraph's first window and the insert mark does not move

### Requirement: Protected View pure policies carry Kani proofs over stated domains
The responsiveness and integrity preconditions of Protected View SHALL be proved by Kani harnesses over the production pure cores, following `formal-verification-kani`. The fold placement and its row model SHALL be checked in `crates/lushtext-core/src/model/protected_fold/kani_proofs.rs`. The action-admission table, the entry state, navigation targets, Line Inspector windows and find batches SHALL be checked in `crates/lushtext-core/src/ui/editor_page/protected_view/policy/kani_proofs.rs`. The proved domain of each property SHALL be stated in its harness and in the checked function's rustdoc, as follows.

- **Fold thresholds.** The fold policy takes its thresholds as one validated `FoldThresholds { head_cap, n_dense, v_batch, r_fold }` value. Its preconditions are `n_dense ≥ 1`, `n_dense ≤ head_cap`, `r_fold ≥ 1` and `r_fold · n_dense ≤ v_batch` (checked multiplication). A harness SHALL prove that the production constants satisfy them, and the generic harnesses SHALL range over every threshold tuple that satisfies them, with `r_fold` from 1 to 3.
- **Paragraph sequences.** The generic harnesses SHALL range over every sequence of up to six paragraphs whose lengths are arbitrary `u64` values. A window-lemma harness SHALL prove the per-window decision for any `r_fold` lengths, and the checked function's rustdoc SHALL state the argument that extends the bound to sequences of any length.
- **Wrap mode.** The fold plan SHALL be independent of the wrap mode. The extension from paragraph windows to row windows rests on every paragraph occupying at least one row, which the rustdoc SHALL state and the PR-gated widget test SHALL check against GtkTextView's laid-out rows in both modes.
- **UTF-8.** The byte-boundary property SHALL be proved for every valid UTF-8 string of up to 8 bytes that contains no paragraph delimiter.
- **Session generation.** The entry-state harness SHALL range over every start generation up to `u64::MAX − 6`, and the transition's rustdoc SHALL state that bound.
- **Unwind.** Each harness SHALL declare the `#[kani::unwind]` bound its loops need, and SHALL compute its own row positions and window sums without overflow.

#### Scenario: Every window holds at most V_BATCH visible characters
- **WHEN** the density harness runs the production `fold_step` and `fold_finish` over any paragraph sequence in the stated domain
- **THEN** every window of `r_fold` consecutive paragraphs (the whole sequence when it is shorter) holds at most `v_batch` visible characters
- **AND** every paragraph shows at most `head_cap` visible characters

#### Scenario: An over-budget window fits once its paragraphs are dense-capped
- **WHEN** the window-lemma harness takes any `r_fold` capped lengths
- **THEN** if their sum exceeds `v_batch`, the sum of their dense-capped counts is at most `v_batch`

#### Scenario: The density bound fails without the density fold
- **WHEN** a `should_panic` harness runs only the production length-cap stage over the same domain, and asserts the window bound
- **THEN** Kani finds a sequence whose window exceeds `v_batch`, and the harness passes

#### Scenario: Only tails are hidden
- **WHEN** the fold-shape harness folds any paragraph sequence in the stated domain
- **THEN** the head plus the tail equals the paragraph length, a non-empty paragraph keeps at least `min(len, n_dense)` visible characters starting at its first character, a paragraph of at most `head_cap` characters that lies in no over-budget window is never folded, and no fold range covers the paragraph delimiter

#### Scenario: A fold boundary never splits a UTF-8 sequence
- **WHEN** the byte-boundary harness converts any head length up to the string's character count into a byte offset with the production `fold_byte_offset`
- **THEN** the offset is a scalar boundary, and the prefix before it holds exactly that many characters

#### Scenario: The column mapping is total and round-trips
- **WHEN** the mapping harness applies the production `FoldRange::locate` to every `u64` column of any folded or unfolded paragraph
- **THEN** it returns without panicking exactly one of visible, hidden at a tail offset, or past the end
- **AND** mapping the location back with `column_of` returns the original column for every column up to the paragraph length
- **AND** `navigation_target` returns a scroll target for a visible column, or an inspector window that contains a hidden column

#### Scenario: Fold placement does not depend on install slicing
- **WHEN** the slicing harness folds the paragraph sequence in one run, and again in two runs split at any index with the state carried across
- **THEN** both plans are identical

#### Scenario: No buffer-mutating action is admitted
- **WHEN** the admission harness classifies every `ProtectedAction` with the production `action_disposition`
- **THEN** no action whose `toolkit_effect` mutates the buffer or moves a mark is admitted, where admitted means classified viewport-safe, the only disposition that lets the default handler run
- **AND** the entry undo is reached only through its own disposition, never by admitting the default undo handler
- **AND** each action the harness names independently is not admitted, and the table records it as mutating: insert, delete, backspace, cut, paste, middle-click paste, move-lines, move-words, join-lines, change-case, change-number, insert-emoji, push-snippet, undo, redo, DnD drop, the search bar's replace and replace-all, and line-ending normalization

#### Scenario: Entry is monotone for the session
- **WHEN** the entry harness applies any sequence of up to six session events to a protected session with the production transition `ProtectedSession::on_event`
- **THEN** the session stays protected through every event except a reload from disk, which may return it to editable only when the new decision says so, and the entry undo, which may return it to editable only when the session was entered through an edit and the undone step is the entry edit
- **AND** its generation never decreases and advances on every whole-content install

#### Scenario: Re-deriving on eviction reload breaks monotonicity
- **WHEN** a `should_panic` harness routes an eviction reload through the from-disk re-derivation
- **THEN** Kani finds a sequence that leaves Protected View without a reload from disk, and the harness passes

#### Scenario: Inspector windows are bounded and tile the paragraph
- **WHEN** the inspector harness takes any paragraph length in `u64` of at least 1 (an empty paragraph has no window), any `N_inspect` from 1 up to `min(head_cap, L_PROTECT)`, and any window index below the window count
- **THEN** each window holds between 1 and `N_inspect` characters, the first starts at column 0, each later window starts where the previous ended, and the last ends at the paragraph length
- **AND** the window that `inspector_window_containing` returns for any column below the length contains that column

#### Scenario: Find batches are bounded and partition the results
- **WHEN** the find-batch harness drives the production `next_find_batch` over any queue of up to six admitted result records, with any per-turn count and byte caps and any result cap
- **THEN** every batch delivers at least one and at most `FIND_RESULTS_PER_TURN` records and at most `FIND_BYTES_PER_TURN` bytes, the batches deliver the first `min(n, FIND_RESULT_CAP)` records exactly once and in order, and the planner finishes in at most `n` turns

#### Scenario: An unadmitted record breaks the byte bound
- **WHEN** a `should_panic` harness feeds the planner a record that did not pass the production record admission and is larger than `FIND_BYTES_PER_TURN`
- **THEN** Kani finds a batch over the byte bound, and the harness passes

#### Scenario: Harnesses are placed in shards and oracles
- **WHEN** `make check-kani-shards` runs
- **THEN** every harness above belongs to exactly one shard: `core-protected-fold`, gated `pull-request`, holds the `model/protected_fold` harnesses, and `core-protected-view`, gated `scheduled`, holds the `protected_view/policy` harnesses
- **AND** both shards have a runner-measured budget, and every harness has an `ORACLES` entry with its measured local seconds and tier

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
- the `protected_view` flag.

Each threshold MUST be a named policy constant. `L_syntax`, `L_minimap`, `L_invisibles`, `L_search_highlight`, `L_protect`, `L_edit_stall`, `N_visible`, `V_batch`, the batch row count and the syntax total-size cutoff MUST take the values that the `probe-long-line-toolkit-costs` measurement records; a threshold that measurement does not derive MUST say in its documentation which recorded value it defaults to. The total-size open, undo and local-history tiers and the minimap total-size tier MUST keep their current values (500 MB refuse, 50 MB undo off and history unavailable, 10 MB save-only history, 10 MB minimap). `protected_view` MUST hold exactly when the longest paragraph exceeds `L_edit_stall`, the shortest paragraph length at which one keystroke inside the paragraph exceeds `B_interact` on the reference machine, measured by `long-line-protected-view` under the reference precondition. `L_protect` (500) remains a profile threshold (the long-paragraph offsets and counts) and MUST NOT decide `protected_view`; the batch density MUST NOT decide it either, so a document whose paragraphs are all at most `L_edit_stall` characters stays editable under the reduced-features policy.

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
- **WHEN** an edit makes a paragraph longer than `L_edit_stall` and Protected View is not yet implemented
- **THEN** `protected_view` becomes true in the effective policy and the reduced-features notice reports it
- **AND** the buffer contents and modified state are unchanged by the downgrade
