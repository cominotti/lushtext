## ADDED Requirements

### Requirement: The Line Inspector exposes giant-line content in bounded windows
The system SHALL provide an "Inspect Line…" command. It SHALL be reachable from the Protected View context menu, from the fold-marker gutter, from the command palette and from a keyboard shortcut. The command SHALL open a secondary read-only view over a copied window of at most `N_inspect` characters of one paragraph, with word wrap on, where `N_inspect` never exceeds `min(head_cap, L_PROTECT)` (500 with C0's values). The inspector SHALL offer Previous Window, Next Window and Go to Column. It SHALL show the window's column range and the paragraph length. A keyboard-only invocation SHALL default to the first folded paragraph in the viewport and accept a line number.

Window text SHALL be read off GTK from the immutable Protected View text source (`ProtectedTextSource`), which for every whole-content install is the installed body itself, moved into long-lived charged ownership at the install's terminal, and which is captured once only after an edit-growth entry. Copy inside the inspector SHALL be bounded to its window. "Copy Whole Line" SHALL publish a lazy `GdkContentProvider` that writes the paragraph from the immutable source in bounded chunks when a client reads it. No GTK dispatch SHALL copy more than one chunk, and the copied line SHALL never be materialized whole on the GTK thread.

#### Scenario: Reading the middle of a giant line
- **WHEN** the user opens the inspector on a 1,900,000-character paragraph and goes to column 950,000
- **THEN** the inspector shows a window of at most `N_inspect` characters containing column 950,000, and the GTK work in each dispatch stays within `B_interact`

#### Scenario: Whole-line copy streams in bounded chunks
- **WHEN** the user chooses Copy Whole Line and another application reads the clipboard
- **THEN** the content provider writes the paragraph's exact characters in bounded chunks, and no GTK dispatch copies more than one chunk

### Requirement: Secondary views of protected documents follow the same policy
Every secondary view of a protected document SHALL follow the Protected View policy:
- **Local-history preview:** installs the snapshot folded at insertion through the same executor (`ui/protected_fold.rs`, one hidden-tail insert per main-loop iteration), into a preview view that is non-editable and caret-free; its Copy button copies the snapshot from the retained snapshot, never from the folded view.
- **Markdown preview:** renders a paragraph longer than its admission budget as the existing `Simplified` or limited terminal, with the omission announced.
- **File peek:** shows only the bounded sample, and the sample's longest line SHALL NOT exceed `head_cap` visible characters.
- **Notes editors:** a note body whose profile reaches `protected_view` SHALL open read-only and folded at insertion, with a visible notice that its long lines are shortened.

#### Scenario: Local-history preview of a giant-paragraph snapshot
- **WHEN** the user previews a local-history snapshot whose profile reaches `protected_view`
- **THEN** the preview view is non-editable, caret-free and folded at insertion

### Requirement: Folded lines are marked in a gutter
A Protected View gutter renderer SHALL mark each paragraph that has a hidden tail and SHALL expose the hidden-character count in its tooltip; the renderer SHALL carry an accessible label and a description naming the Inspect Line shortcut (a gutter draws its rows and has no per-row accessible object, so the count is also spoken by the Line Inspector's window label). A single primary click in the left gutter beside a folded paragraph SHALL open the Line Inspector on that paragraph. The renderer's per-frame work SHALL be bounded by the number of visible rows, and it SHALL be shown only while the session is protected.

#### Scenario: Activating a fold marker
- **WHEN** the user clicks the fold marker next to a folded paragraph
- **THEN** the Line Inspector opens on that paragraph's first window and the insert mark does not move

### Requirement: Protected heads follow the per-paragraph wrap gate
A protected paragraph's visible head SHALL follow C5's per-paragraph wrap gate like any paragraph longer than `L_WRAP_EDIT`: it is shown unwrapped. Reading a long line wrapped is the Line Inspector's job (its window wraps). Decision of `protected-view-inspector-and-surfaces`, by measurement (2026-11-01, interleaved, plain, 3 + 3 runs each, load 0.8–3.0): with the gate, the worst open-phase dispatch outside the declared hidden-tail insert on the 1.9M wrap-on fixture is 8.7–12.5 ms (head frame paint 3.3–3.7 ms) and the tail insert 13.7–13.9 ms; exempting protected heads from the gate (heads wrapped) raises them to 12.2–16.2 ms (paint 13.3–13.9 ms, one sample over `B_settle`) and 18.7–19.0 ms. The gate keeps C2's accepted 17.6 ms head-frame exception from returning.

#### Scenario: A protected head is unwrapped while word wrap is on
- **WHEN** a document whose longest paragraph exceeds `L_PROTECT_EDIT` opens with word wrap on
- **THEN** the folded paragraph's head carries the no-wrap tag, its hidden tail is never laid out, and every other short paragraph wraps

## MODIFIED Requirements

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

### Requirement: Navigation into protected documents scrolls without a caret
Navigation into a protected document SHALL scroll a LushText-owned navigation mark to the target paragraph and SHALL NOT move the insert mark. This covers session restore position, bookmark activation, local-history targets, workspace-search hits and protected find results. When a navigation carries a target column inside a hidden tail (protected find results), the system SHALL scroll to the paragraph and open the Line Inspector at the window that contains the target column. Workspace-search hits and bookmarks carry a line but no column (workspace-search excerpts are bounded), so they scroll to their paragraph; the in-tab find reaches a match in a hidden tail.

#### Scenario: Workspace search hit in a folded paragraph
- **WHEN** the user activates a workspace-search hit on a folded paragraph of a protected document
- **THEN** the editor scrolls the navigation mark to that paragraph and the insert mark does not move

#### Scenario: Find navigation to a match in a hidden tail
- **WHEN** the user moves to the next protected find match and it lies in a hidden tail
- **THEN** the Line Inspector opens on the window containing the match, with the match selected; the automatic first match of a new query only scrolls, so typing a query never opens a dialog over the search bar

### Requirement: In-tab find in Protected View runs on a worker
No `GtkSourceSearchContext` SHALL ever be attached to a protected buffer. In-tab find SHALL run on a worker over the immutable Protected View text source and SHALL find matches in hidden tails as well as in visible text. Requests SHALL be single-flight: one active request and one latest pending request, with generation freshness. Results SHALL be paragraph-and-column records delivered to GTK in batches bounded by count and bytes, under a total result cap. Activating a result SHALL scroll to it, and SHALL highlight it with a display tag only when the match lies entirely in visible text; otherwise an explicit next or previous step SHALL open the inspector on it (an inspector already open follows every current match). The search counter SHALL report found and capped states. In-tab Replace SHALL be unavailable. The editor-side and window-side selection-prefill sites SHALL both read `visible=true` text under the existing prefill bound.

#### Scenario: Find in a giant line does not block
- **WHEN** the user searches for a string that occurs 100,000 times in a protected document whose paragraph is 1,900,000 characters long
- **THEN** no GTK dispatch exceeds `B_interact`, results arrive in bounded batches, and the counter reports the capped total

#### Scenario: Superseded query is cancelled
- **WHEN** the user types three characters faster than the worker completes
- **THEN** the obsolete requests are cancelled, at most one pending request is retained, and only the latest generation publishes results

### Requirement: Documents over the protected thresholds open in Protected View
The system SHALL present a document in Protected View whenever C1's `DocumentScale::protected_view` holds for it, that is, when its longest paragraph exceeds `L_PROTECT_EDIT` (50,000 characters, the maintainer decision of 2026-10-30: the paragraph length at which the measured median cost of one keystroke inside it reaches about 20 ms with word wrap on; see the modified requirement "One monotone document scale policy governs feature availability"). `L_PROTECT` (500), the measured edit-stall length `L_EDIT_STALL` and the batch density SHALL NOT make a document view-only: a document whose paragraphs are all at most `L_PROTECT_EDIT` characters stays editable under C1's reduced-features policy, and its keystrokes between `L_EDIT_STALL` and `L_PROTECT_EDIT` are the recorded toolkit-floor exception of `main-thread-responsiveness`. For every whole-content install (load, eviction reload, external reload, reopen with encoding, draft recovery, set-aside open, local-history restore and Undo Restore) the decision SHALL be taken from the worker-side profile of the body being installed, before any of its text is inserted into a GtkTextView, and the protected view properties SHALL be applied before the first inserted byte. Entry SHALL also happen when C1's edit-time watch reports that an edit newly sets `protected_view`, inside the same `insert-text` or `delete-range` after-handler emission (a deletion that removes a paragraph delimiter can join two paragraphs into a giant one); in that emission the insert and selection-bound marks SHALL be placed at the start of the paragraph that grew, so that no mark is left inside a hidden tail. Protected View SHALL be monotone for the editor session: no edit, preference change, tab switch, eviction reload, draft recovery, or local-history restore or Undo Restore SHALL return the document to the editable view. There are two exits. A reload of the file from disk (an external-change reload or an explicit reload) SHALL re-derive the decision from the new content's profile, so a file that has shrunk below the thresholds becomes editable again. And when the document entered Protected View through an edit, the system SHALL offer one bounded undo of exactly that edit, from the Protected View alert and through Ctrl+Z, while the edit is still the newest step of the buffer's undo history and undo history is on; the undo SHALL run the buffer's own undo of that step, SHALL restore the effective feature policy recorded immediately before the edit, and SHALL end the Protected View session. The entry undo is the only buffer change Protected View admits besides whole-content installs.

#### Scenario: Giant-paragraph file opens protected
- **WHEN** the user opens a synthetic file whose longest paragraph exceeds `L_PROTECT_EDIT`
- **THEN** the editor installs the document in Protected View, and no text of the file ever exists in that view's buffer without the Protected View fold applied

#### Scenario: Prose with long paragraphs stays editable
- **WHEN** the user opens a Markdown file whose paragraphs are up to 8,000 characters long, each written on one line, all at most `L_PROTECT_EDIT`
- **THEN** the document opens editable with C1's reduced features, and Protected View is not entered

#### Scenario: Entry threshold edge
- **WHEN** the user opens a document whose longest paragraph is exactly `L_PROTECT_EDIT` characters, and another whose longest paragraph is `L_PROTECT_EDIT` + 1
- **THEN** the first opens editable under C1's reduced features and the second opens in Protected View

#### Scenario: Dense short-line file stays editable
- **WHEN** the user opens a synthetic file in which every paragraph is at most `L_PROTECT_EDIT` characters but some window of `R_BATCH` paragraphs exceeds `V_BATCH` characters
- **THEN** the document opens editable, and Protected View is not entered

#### Scenario: Paste creates a giant paragraph
- **WHEN** a paste, undo, move-lines or join-lines in an editable document produces a paragraph longer than `L_PROTECT_EDIT`
- **THEN** the editor enters Protected View within the same `insert-text` or `delete-range` emission, before any GTK validation or highlight idle runs, the insert and selection-bound marks are at the start of the grown paragraph, and the document keeps its modified state

#### Scenario: Recovered draft with a giant paragraph opens protected
- **WHEN** startup recovery restores a draft body whose last paragraph holds more than 33 million characters (the crash-recovery smoke's 32 MiB single-line fixture)
- **THEN** the restored editor is in Protected View, stays modified and draft-restored, and the paragraph's tail was inserted under the fold tag in the same dispatch as its head

#### Scenario: Protected View never reverts within a session
- **WHEN** the user changes preferences, switches tabs, or presses any edit key other than the entry undo in a protected document
- **THEN** the document stays in Protected View for the life of that editor

#### Scenario: Undoing the edit that made the document protected
- **WHEN** a paste makes a paragraph longer than `L_PROTECT_EDIT`, and the user then chooses Undo in the Protected View alert or presses Ctrl+Z
- **THEN** exactly that paste is undone in bounded work, the document returns to the editable view with the effective feature policy it had before the paste, the buffer equals its content before the paste, and the insert mark is where the undo leaves it

#### Scenario: Entry undo is not offered when it cannot be bounded
- **WHEN** a document entered Protected View on load, or through an edit while its undo history was off (C1's undo tier), or the entry edit is no longer the newest undo step
- **THEN** neither the alert nor Ctrl+Z offers the entry undo, and Ctrl+Z publishes the Protected View notice
