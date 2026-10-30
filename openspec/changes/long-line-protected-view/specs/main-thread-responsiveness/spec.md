## ADDED Requirements

### Requirement: Protected View interactions and settles stay within named budgets
For documents in Protected View, every interaction dispatch SHALL stay within `B_interact`. Interaction dispatches are scroll, key, click, tab switch, toggles, find navigation, inspector navigation and fold-marker activation. Input-to-presented-frame latency SHALL stay within two reference frames. Every settle dispatch SHALL stay within `B_settle`. Settle dispatches are install, restore, draft recovery, eviction reload, external reload, a word-wrap toggle's re-layout, find-batch publication and whole-line clipboard writes. The one declared exception is the single GTK turn that inserts a hidden tail longer than the slice budget ("Large file installation yields in bounded GTK slices" keeps such a paragraph in one turn): until `sliced-protected-install` (C3) is adopted, that turn is a declared envelope whose measured cost the verdict records as C3's gate input, not a `B_settle` guarantee. Its visible head SHALL still be bounded by `head_cap`, so the envelope covers only the copy of the hidden run into the buffer, never its layout. Both budgets SHALL be read from `model/responsiveness_budget.rs` (`B_INTERACT_US`, `B_SETTLE_US`, `REFERENCE_FRAME_US`, `INPUT_TO_FRAME_FRAMES`); Protected View SHALL NOT restate their values. PR-gated widget tests SHALL assert the preconditions that make the expensive toolkit paths unreachable:
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

### Requirement: Protected View never snapshots a protected buffer on the GTK thread
LushText-owned Protected View work SHALL stay within declared chunk sizes, and SHALL NOT read a protected buffer's text through a GTK snapshot whose per-chunk cost grows with the chunk's depth inside a paragraph:
- find, the Line Inspector, whole-line copy and every save of a protected document SHALL read `ProtectedTextSource`, which for every whole-content install is the installed body itself, retained;
- after an edit-growth entry the source SHALL be captured at most once; the capture SHALL use a positioning whose per-chunk cost the ledger shows to be independent of depth (P8), or, if no such positioning exists, SHALL be recorded in the verdict as a declared envelope owned by C3;
- find SHALL deliver at most a declared number of results and bytes per GTK turn under a total result cap;
- the inspector SHALL read at most `N_inspect` characters per window, and whole-line copy SHALL write at most one chunk per dispatch;
- notice and accessible text stay within the inline-alert bounds.

None of this work SHALL copy a whole paragraph or the whole buffer on the GTK thread.

#### Scenario: First find on a large protected document
- **WHEN** the user opens find on a 100 MB protected document that was installed from disk
- **THEN** the search runs on the retained source with no snapshot turn on the GTK thread, and every result batch is published within `B_settle`

#### Scenario: Save As of a giant protected document
- **WHEN** the user saves a protected document holding a 1.9M-character paragraph with Save As
- **THEN** no GTK dispatch reads the buffer's text, and the write runs on the worker from the retained source

### Requirement: The Protected View verdict is measured under the reference precondition
The Protected View budget verdict SHALL be measured with C0's responsiveness lane as it now stands (presented frames only; a run in which a measured case wrote no `frame` record fails), against both `main`'s and the change's release responsiveness binaries measured by the change's lane driver, each in its own hold of the shared measurement lock. Every kept run SHALL meet the reference precondition: before the lock, 300 s in which the 1-minute load stays below 4, no process except the agent orchestrator uses more than 25 % CPU, and no `cargo`, `rustc`, `nextest`, `kani`, `cbmc` or private `mutter --headless` process runs; a 30 s re-check inside the lock; a sampler for the whole run that records any violation outside the lane's own process tree; and a 60 s check afterwards. A run that breaks it SHALL be discarded and retried. Each case SHALL run 7 repetitions in the lane's three variants (plain, AT, probe IM); cases that start a process (open, three-document restore, draft recovery) SHALL also run in the four construction profiles (fresh or steady-state, keyfile or dconf). The verdict, its run ids, the precondition evidence, `btrfs-stall-report` for each run and the toolkit versions SHALL be recorded in `docs/next/long-line-documents.md`.

#### Scenario: Responsiveness lane run
- **WHEN** the lane runs the protected cases (open, scroll, Page Down, caret keys, typing, click and double-click, find, inspector open and navigation, Copy Whole Line, Save As, tab switch, word-wrap toggle, eviction and reselection, restore of three protected documents, recovery of two 32 MiB single-line drafts, the AT bridge with a listener)
- **THEN** the largest interaction dispatch is at most `B_interact`, the largest settle dispatch other than a declared one-turn hidden-tail insert is at most `B_settle`, the one-turn inserts are recorded against the envelope, and the input-to-frame latency is at most two reference frames

#### Scenario: A run on a busy machine is not kept
- **WHEN** the sampler records a process outside the lane over 25 % CPU during a measured run
- **THEN** the run is discarded and retried, and no figure from it enters the verdict
