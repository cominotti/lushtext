## MODIFIED Requirements

### Requirement: Large file installation yields in bounded GTK slices
Installing a decoded document above the synchronous installation threshold SHALL use bounded main-loop slices. Outside a protected paragraph's hidden tail, slice boundaries SHALL align to paragraph ends, as GTK defines them: just after `\n`, `\r`, `\r\n` (never between its CR and LF) or U+2029. The paragraph-boundary function MUST be the single shared one whose agreement with `pango_find_paragraph_boundary` is tested. GTK text layout validates whole paragraphs, so a slice that stops inside a *visible* paragraph forces later slices to re-lay-out everything already installed in that paragraph. That is quadratic total work and can stall recovery of single-line documents for minutes. A visible paragraph longer than the slice byte budget SHALL therefore be installed (and, during clearing, deleted) in one turn. The protected-view policy bounds such a paragraph by its visible-length limit. The editor SHALL remain non-editable, and projections that would amplify each insertion SHALL remain suspended, until the complete current generation is installed or the operation is cancelled.

In a document under Protected View, a paragraph whose tail is folded SHALL be installed as follows:
- its hidden tail in bounded slices of at most the protected tail slice budget, each inserted with the protected tag applied in the same `insert_with_tags` call, so tail text is never untagged across a main-loop dispatch and never laid out untagged, and every LushText `insert-text` handler treats the slice as installation, not as an edit;
- its delimiter before its tail, so the paragraph is never the buffer's last line while the tail grows (ledger A79), and its visible head, up to the fold point, in one turn after the last tail slice, so no tail slice re-lays out the head; when the tail fits one slice, the head immediately before it in the same turn, with no boundary.

A slice MAY end inside such a paragraph only at or after its fold point. A protected body whose hidden text could exceed one tail slice SHALL take this sliced path at any size, and its slices SHALL wait at the paced idle priority, below the view's line validation. Each installation or clearing dispatch of a protected body SHALL stay within the settle budget B_settle on the reference responsiveness lane.

The synchronous installation threshold SHALL be the measured largest size whose insert fits half of `B_SETTLE_US`. A document whose densest validation batch holds more than `V_SLICE` visible characters, a measured constant, SHALL be sliced even when its byte size is below the threshold. Each of its slices SHALL hold at most `V_SLICE` visible characters, and its next slice SHALL NOT be dispatched before the frame that lays out the previous one. The validation that a slice's new lines cause then fits half of `B_SETTLE_US` in either wrap mode. The slice plan is one pure function, shared by load and whole-buffer replacement.

#### Scenario: Large decoded text is installed
- **WHEN** an admitted load returns text above the synchronous installation threshold
- **THEN** GTK inserts the text in bounded slices with scheduling points between them, paragraph-aligned everywhere except inside protected hidden tails
- **AND** syntax, minimap, history, draft, monitor, and modified-state finalization run only after the complete current generation is present

#### Scenario: Giant single-paragraph content avoids quadratic re-layout
- **WHEN** a recovered draft or loaded file that is not under Protected View contains one visible paragraph larger than the slice byte budget
- **THEN** that paragraph is installed in a single turn while any multi-paragraph remainder keeps bounded paragraph-aligned slices
- **AND** previously installed paragraphs are not re-validated by later slices

#### Scenario: Giant protected paragraph is installed in hidden-tail slices
- **WHEN** a document under Protected View contains a paragraph whose hidden tail is larger than the protected tail slice budget
- **THEN** the tail is inserted in tagged slices of at most the protected tail slice budget, each in its own main-loop dispatch, after the paragraph's delimiter is in, and the head is inserted once, untagged, after the last of them
- **AND** the paragraph never shows more than its head at any dispatch boundary, and no dispatch of the install exceeds B_settle on the reference lane at 100, 250, or 500 MB with the accessibility bridge off or on

#### Scenario: CR-only and U+2029-separated documents are sliced
- **WHEN** an admitted document above the synchronous threshold separates its paragraphs only with `\r`, or only with U+2029
- **THEN** installation uses multiple bounded slices, each ending just after a delimiter
- **AND** the installed buffer is byte-identical to the decoded text

#### Scenario: CRLF pairs survive slicing
- **WHEN** a slice's byte budget ends between the `\r` and `\n` of a CRLF pair
- **THEN** the slice ends after the `\n`, so the pair stays one GTK paragraph delimiter

#### Scenario: Load is cancelled during installation
- **WHEN** the tab closes, reloads, or advances generation between installation slices, including between two hidden-tail slices of one paragraph
- **THEN** remaining slices stop without applying final loaded state, and partial content is cleared through the bounded clear plan
- **AND** admission ownership and retained decoded text are released

#### Scenario: Small load remains direct
- **WHEN** decoded text is below the synchronous installation threshold, its densest validation batch holds at most `V_SLICE` visible characters, and it hides at most one tail slice of text
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

## ADDED Requirements

### Requirement: Protected tail slices never merge into a growing segment
The sliced protected install SHALL keep one zero-width text-buffer boundary between every two consecutive hidden-tail slices of a paragraph, so that GTK never merges the newly inserted run with the already installed tail by copying both (ledger A36, A76). The boundary is an anonymous left-gravity mark; alternating protected tags with identical properties is the recorded fallback if that axiom stops holding. The install also marks each protected paragraph's start, where its head goes, and its tail end, so the record of ranges covers the whole paragraph content. A tail that fits one slice needs no boundary: it goes in right after its head, in the same dispatch. Boundaries SHALL persist until their range is deleted or the buffer is disposed. They SHALL NOT be removed while char segments remain on both sides of them. The total work to install a protected tail SHALL be linear in the tail's size.

#### Scenario: Consecutive tail slices stay separate segments
- **WHEN** a hidden tail is installed in K slices
- **THEN** K − 1 boundaries separate them, the paragraph start and the tail end carry one boundary each, and no later slice's insertion copies the bytes of an earlier slice
- **AND** the whole tail still reads as one protected-tag range

#### Scenario: Boundaries do not outlive their content
- **WHEN** a buffer that held protected tails is evicted, reloaded, replaced, or closed
- **THEN** no boundary mark created by the install remains in the buffer after the clear completes

### Requirement: Protected tails are cleared in recorded slices
Clearing a buffer that holds protected tails SHALL first delete each protected paragraph's content through the boundary ranges recorded at install time, paragraph by paragraph, each from the paragraph's start forwards, as many consecutive ranges per main-loop dispatch as fit the protected clear slice budget and at least one (a deletion costs time growing with the text before it in its paragraph, ledger A76). The record of those ranges SHALL outlive the install and SHALL hold marks and byte counts only, never text. The boundaries a deleted range leaves collapsed SHALL be deleted in the same dispatch, where they no longer separate two char segments. Visible content SHALL then keep the paragraph-aligned clear slices, which no longer meet a giant paragraph. A buffer emptied another way (a direct replacement) SHALL drop its record after the text is gone. No clear dispatch SHALL exceed B_settle on the reference lane.

#### Scenario: Eviction clears a giant protected tail
- **WHEN** memory policy evicts a clean protected editor whose tail spans many recorded ranges
- **THEN** each clear dispatch deletes the first remaining recorded ranges of a paragraph that fit the clear slice budget, from its first range to its last
- **AND** residency is released only after the clear session completes

#### Scenario: A partial install is cancelled
- **WHEN** a protected install is cancelled after some tail slices were inserted
- **THEN** the cancellation clear removes exactly the installed ranges and boundaries, and no partial body becomes editable, saveable, or visible as a completed load

### Requirement: Sliced protected installation preserves document bytes exactly
Slicing SHALL only choose offsets and SHALL never transform text. After a completed sliced install, the buffer's document text, hidden text included, SHALL equal the decoded document byte for byte, for every slice budget and every placement of paragraph delimiters and multibyte characters. Saving or Save As after a sliced install SHALL write exactly the source bytes. The planner that chooses the offsets SHALL be GTK-free and Kani-proved, as the requirement "The protected install slice plan is proved over a stated domain" states.

#### Scenario: Mixed delimiters and multibyte text round-trip
- **WHEN** a synthetic protected document with CR-only, `\r\n` straddling a budget, U+2029, and multibyte characters at every byte phase is installed through the sliced path
- **THEN** reading the buffer's full text including hidden text returns the decoded document unchanged

#### Scenario: An undelimited folded last paragraph is never the last line
- **WHEN** a protected body whose last paragraph is folded and has no delimiter is installed, directly or sliced
- **THEN** the buffer ends with one stand-in `\n` under an invisible, non-editable terminator tag, so a point below the text maps to an iterator without walking the hidden run (ledger A79)
- **AND** the stand-in is not document text: every snapshot of the buffer's document text (save, draft, local history, preview) and the document statistics stop before it, and every clear deletes it

#### Scenario: Save As after a sliced install
- **WHEN** the user saves a protected document to a new path after a sliced install
- **THEN** the new file's bytes equal the original file's bytes

### Requirement: The protected install slice plan is proved over a stated domain
The sliced protected install SHALL take its slices from one GTK-free, I/O-free planner, and Kani harnesses SHALL check that production planner, C1's paragraph-boundary function and C2's fold placement step directly, never a copy. Each hidden-tail slice SHALL end at the previous slice's end plus `tail_slice_bytes`, or at the tail end if that is nearer, rounded down to the nearest boundary that is a char boundary and not between `\r` and `\n`; `tail_slice_bytes` SHALL be at least 4, so every slice advances. The tail and clear slice budgets SHALL be derived from B_settle and the measured per-KiB costs by one pure, integer-only function. The harnesses SHALL apply only when the adoption gate adopted this change, and only for the adopted half.

The proofs hold on these domains, which the harnesses and the planner's rustdoc SHALL state:
- the **document domain**: every valid UTF-8 string of at most 10 bytes over `a`, `é`, `€`, `𝄞`, `\r`, `\n` and U+2029, a tail slice budget and a clear slice budget of 4 to 7 bytes, `N_visible` of 1 to 3 characters, and small symbolic density parameters within the fold placement step's preconditions;
- the **giant-paragraph domain**: one paragraph whose content is at most 12 bytes of `a`, `é`, `€` and `𝄞`, ending with no delimiter, `\n`, `\r`, `\r\n` or U+2029, with the same budgets and `N_visible`, and density parameters under which the density rule does not shorten a first paragraph's head;
- every `u64` input of the budget derivation.

Larger sizes SHALL be covered by property tests of the same assertions, and no rustdoc or spec text SHALL claim the Kani result beyond these domains.

#### Scenario: The slices reproduce the exact bytes
- **WHEN** Kani plans any input of the document domain
- **THEN** every step advances, and the slices concatenated in order equal the input byte for byte

#### Scenario: No boundary splits a scalar or a CRLF pair
- **WHEN** Kani plans any input of the document domain
- **THEN** every slice end is a char boundary of the input and never falls between the `\r` and `\n` of a pair

#### Scenario: Every slice fits the budget derived from B_settle
- **WHEN** Kani derives the slice budget from any `u64` settle budget, per-KiB cost and margin
- **THEN** the derivation never overflows or panics, and an accepted budget is at least 4 bytes, at most the stated maximum slice, the largest size whose predicted cost, margin included, is within the settle budget, while an unattainable budget is reported as an error
- **AND** when Kani plans any input of the document domain with such a budget, every hidden-tail slice is at most that budget and every slice is wholly visible or wholly hidden

#### Scenario: Boundaries and the clear plan are placed exactly
- **WHEN** Kani plans any input of the document domain
- **THEN** a boundary follows a slice exactly when the next slice is a hidden-tail slice of the same paragraph
- **AND** the clear plan deletes each paragraph's recorded ranges from its first, each turn within the clear budget, and with the ordinary clear slices covers the buffer exactly once

#### Scenario: The tag boundary falls exactly at N_visible
- **WHEN** Kani plans one giant paragraph of the giant-paragraph domain
- **THEN** if the paragraph is longer than `N_visible` characters, the switch from visible to hidden slices falls at the end of the paragraph's `N_visible`-th character
- **AND** every byte before it is in the paragraph's head slice and every byte from it to the tail end of C2's fold range is in a hidden-tail slice, and the executor inserts the head untagged and hidden-tail slices with the protected tag

#### Scenario: The slice count of one giant paragraph is bounded
- **WHEN** Kani plans one giant paragraph whose content is `len` bytes, delimiter excluded, with tail slice budget `B`
- **THEN** the paragraph's content takes at most ⌈len / (B − 3)⌉ + 1 slices: one head slice and at most ⌈tail bytes / (B − 3)⌉ tail slices, each tail slice but the last being at least `B − 3` bytes

#### Scenario: A boundary choice that splits UTF-8 is caught
- **WHEN** the `should_panic` harness runs the production stepper with the rounding guard removed, so a tail slice ends at the raw previous end plus `B`
- **THEN** Kani finds a slice end inside a multibyte character and the harness passes as `should_panic`
