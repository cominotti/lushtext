## MODIFIED Requirements

### Requirement: Protected View pure policies carry Kani proofs over stated domains
Protected View responsiveness and integrity preconditions SHALL be proved by Kani harnesses over production pure cores in `lushtext-model` (`protected_fold/kani_proofs.rs`) and `crates/lushtext-ui/src/ui/editor_page/protected_view/policy/kani_proofs.rs`, with exact proved domains and `#[kani::unwind]` bounds stated in harnesses and rustdoc.

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
- **THEN** every harness above belongs to exactly one shard: `core-protected-fold`, gated `pull-request`, holds the `protected_fold` harnesses in `lushtext-model`, and `core-protected-view`, gated `scheduled`, holds the `protected_view/policy` harnesses in `lushtext-ui`
- **AND** both shards have a runner-measured budget, and every harness has an `ORACLES` entry with its measured local seconds and tier
