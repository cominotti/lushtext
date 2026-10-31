## ADDED Requirements

### Requirement: Protected View interactions and settles stay within named budgets
For documents in Protected View, every interaction dispatch SHALL stay within `B_interact`. Interaction dispatches are scroll, key, click, tab switch, toggles, and find navigation. Input-to-presented-frame latency SHALL stay within two reference frames. Every settle dispatch SHALL stay within `B_settle`. Settle dispatches are install, restore, draft recovery, eviction reload, external reload, a word-wrap toggle's re-layout, find-result publication and the clipboard write of a copy. The one declared exception is the single GTK turn that inserts a hidden tail longer than the slice budget ("Large file installation yields in bounded GTK slices" keeps such a paragraph in one turn): until `sliced-protected-install` (C3) is adopted, that turn is a declared envelope whose measured cost the verdict records as C3's gate input, not a `B_settle` guarantee. Its visible head SHALL still be bounded by `head_cap`, so the envelope covers only the copy of the hidden run into the buffer, never its layout. Both budgets SHALL be read from `model/responsiveness_budget.rs` (`B_INTERACT_US`, `B_SETTLE_US`, `REFERENCE_FRAME_US`, `INPUT_TO_FRAME_FRAMES`); Protected View SHALL NOT restate their values. PR-gated widget tests SHALL assert the preconditions that make the expensive toolkit paths unreachable:
- at most `head_cap` visible characters per paragraph, and at most `V_BATCH` per window of `R_fold` paragraphs;
- `editable=false` and `cursor-visible=false`, applied before the first installed byte;
- the `move-cursor` intercept;
- no search context and no minimap attached; syntax as C1's `DocumentScale` allows;
- invisibles off while a fold exists.

#### Scenario: Precondition gates on a protected document
- **WHEN** the widget suite opens each synthetic protected fixture (2k/10k/50k/250k/1.9M-character paragraphs, the dense wrap-off fixture, CR-only, U+2029-separated, and a prose fixture of 600-character lines)
- **THEN** every precondition holds after install, after tab switch, after a word-wrap toggle and after eviction reload

#### Scenario: Recovery of two giant single-line drafts
- **WHEN** startup recovery restores two drafts of 32 MiB each whose last paragraph holds more than 33 million characters
- **THEN** no dispatch other than the declared one-turn hidden-tail inserts exceeds `B_settle`, no paragraph of more than `head_cap` visible characters is ever laid out, and the one-turn inserts are recorded against the envelope

### Requirement: Keystrokes in editable long paragraphs are a measured toolkit-floor exception
Documents whose longest paragraph is at most `L_PROTECT_EDIT` stay editable (maintainer decision, 2026-10-30). A keystroke inside a paragraph longer than `L_EDIT_STALL` exceeds `B_interact` on the reference machine because GtkTextView re-lays out the whole paragraph on every edit (ledger A23) and GtkTextBuffer recomputes its log attrs (A26); no LushText code is on that path. This SHALL be recorded as one explicit toolkit-floor exception, not as a relaxed budget: its record SHALL name the range (`L_EDIT_STALL` exclusive to `L_PROTECT_EDIT` inclusive), the measured per-keystroke costs of task 1.1 (median, with word wrap on: about 5 ms at 5,000 characters, 7 ms at 10,000, 9.6 ms at 20,000 and 18.5 to 21 ms at 50,000; with it off about 1.3 to 5 ms at 10,000 and 7.7 to 11.6 ms at 50,000), the axioms it rests on, and that the optional C5 owns reducing it. `B_INTERACT_US` SHALL NOT change. The responsiveness lane SHALL keep measuring it through its `keystroke` group (one paragraph of a swept length between short paragraphs, keystrokes at its start, middle and end, wrap on and off), whose fixture kind and derivation are covered by the lane's self-tests, and a later verdict that finds the cost above the recorded figures SHALL report it as a regression.

#### Scenario: The exception is recorded with its measurements
- **WHEN** a reader looks up why typing in a 20,000-character line misses `B_interact`
- **THEN** `docs/next/long-line-documents.md` and the ledger rows of A23 and A26 name the exception's range, its measured costs, the maintainer decision and C5 as its owner

#### Scenario: Keystrokes below the exception stay within budget
- **WHEN** the lane's `keystroke` group types into a paragraph of at most `L_EDIT_STALL` characters
- **THEN** every keystroke dispatch stays within `B_interact`

#### Scenario: The keystroke sweep is self-tested
- **WHEN** `make check-responsiveness-lane` runs
- **THEN** it checks the `keystroke` fixture's markers, paragraph length and delimiters, and the driver's case grid for the group

### Requirement: Protected View never snapshots a protected buffer on the GTK thread
LushText-owned Protected View work SHALL stay within declared chunk sizes, and SHALL NOT read a protected buffer's text through a GTK snapshot whose per-chunk cost grows with the chunk's depth inside a paragraph:
- find, copy and every save of a protected document SHALL read `ProtectedTextSource`, which for every whole-content install is the installed body itself, retained;
- after an edit-growth entry the source SHALL be captured at most once; the capture SHALL run in the shared bounded chunked buffer snapshot after the entering emission returns, while the buffer cannot change;
- find SHALL deliver at most `FIND_RESULT_CAP` records, moved to GTK in one dispatch;
- copy SHALL cut the selected range from the source on a worker and hand GTK the bytes without copying them.

None of this work SHALL copy a whole paragraph or the whole buffer on the GTK thread.

#### Scenario: First find on a large protected document
- **WHEN** the user opens find on a 100 MB protected document that was installed from disk
- **THEN** the search runs on the retained source with no snapshot turn on the GTK thread, and the result set is published within `B_settle`

#### Scenario: Save As of a giant protected document
- **WHEN** the user saves a protected document holding a 1.9M-character paragraph with Save As
- **THEN** no GTK dispatch reads the buffer's text, and the write runs on the worker from the retained source

### Requirement: The Protected View verdict is measured under the reference precondition
The Protected View budget verdict SHALL be measured with C0's responsiveness lane as it now stands (presented frames only; a run in which a measured case wrote no `frame` record fails), against both `main`'s and the change's release responsiveness binaries measured by the change's lane driver, each in its own hold of the shared measurement lock. Every run SHALL hold the measurement lock exclusively for itself alone (lock protocol rev 2: the intent lock first, so no new build starts, then the measurement lock; one run per hold), and SHALL record the load before the lock, inside it and after the run; load is accepted and recorded, not gated (maintainer decision, 2026-10-30, as for C1's lane verdict). `main` and the change SHALL be measured interleaved, so both sides see the same load. Each case SHALL run 7 repetitions in the lane's three variants (plain, AT, probe IM); cases that start a process (open, three-document restore, draft recovery) SHALL also run in the four construction profiles (fresh or steady-state, keyfile or dconf). The verdict, its run ids, the precondition evidence, `btrfs-stall-report` for each run and the toolkit versions SHALL be recorded in `docs/next/long-line-documents.md`.

#### Scenario: Responsiveness lane run
- **WHEN** the lane runs the protected cases (open, scroll, Page Down, caret keys, typing, click and double-click, find, Save As, tab switch, word-wrap toggle, eviction and reselection, restore of three protected documents, recovery of two 32 MiB single-line drafts, the AT bridge with a listener)
- **THEN** the largest interaction dispatch is at most `B_interact`, the largest settle dispatch other than a declared one-turn hidden-tail insert is at most `B_settle`, the one-turn inserts are recorded against the envelope, and the input-to-frame latency is at most two reference frames

#### Scenario: Load is recorded with each run
- **WHEN** a verdict run is measured
- **THEN** its record carries the load before, inside and after the lock, and `main` and the change alternate run by run
