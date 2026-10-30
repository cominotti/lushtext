## ADDED Requirements

### Requirement: Protected View's toolkit assumptions are ledger axioms
Every GTK or GtkSourceView behaviour that Protected View relies on SHALL be a ledger axiom, unless C0 or C1 already pins it. Each axiom SHALL have an isolated headless probe and a `gtk-lush-axioms` sample, except where a probe cannot observe it in-crate; that axiom SHALL be pinned by the named lane case instead, and its ledger row SHALL say so, as C0 does for A35. The axioms are numbered after the last applied ledger entry (provisionally P1–P8, which `sliced-protected-install` cites). The entries SHALL cover:
- P1: a capture-phase gesture that claims a press sequence on a GtkTextView prevents GtkTextView's click gesture from moving the insert mark, resetting the IM context, pasting the primary selection on middle click, opening its default context menu, or starting a drag; an unclaimed touchscreen drag scrolls the view without moving a mark;
- P2: a non-editable GtkTextView never calls `gtk_im_context_focus_in`, and its key controller carries no IM context;
- P3: a handled and stopped `move-cursor` emission with `cursor-visible=false` moves only the viewport, including under `gtk-keynav-use-caret`, extending C0's A34;
- P4: for a paragraph whose tail is invisible, the AT-SPI text contents and offsets; the payload and cost of text queries at character, word, sentence and line granularity; the `TextChanged` payload when the hidden tail is inserted; and the cost of an AT-initiated caret or selection write into it, extending C0's A35. Like A35 it is pinned by the lane's AT case, not by an in-crate probe;
- P5: disabling the text view's DnD source and target prevents drop insertion on a non-editable view;
- P6: GtkTextView recomputes the per-instance enablement of its `clipboard.*` and `selection.*` actions, so it is not a guard; and a key binding pre-empted before its default handler never reaches `select-all`, `toggle-cursor-visible`, `menu.popup`, `change-number`, `change-case`, `join-lines` or `move-to-matching-bracket`;
- P7: `insert_with_tags` of an invisible run never lays out the run, and its cost grows linearly with the run (recorded per MiB, the input to C3's gate);
- P8: the cost of reading one fixed-size chunk of a single-segment paragraph at increasing depths, positioned by character offset (`iter_at_mark` and `forward_chars`) and by line and byte index; cost evidence like A36, recorded with the probe's figures.

A probe that fails SHALL block the dependent Protected View task until the design adds the missing interception or the view is made non-focusable.

#### Scenario: Axiom gate before relying on an assumption
- **WHEN** `make gtk-axioms` runs on the toolkit versions in the ledger
- **THEN** every Protected View axiom probe passes, or the ledger records the axiom as falsified and the corresponding Protected View task stays open

#### Scenario: Lane-pinned axiom
- **WHEN** the ledger lists P4
- **THEN** its row names the lane AT case that pins it and states that no in-crate probe exists

#### Scenario: The snapshot-depth cost is recorded before the source design relies on it
- **WHEN** the P8 probe reads chunks at increasing depths of a two-million-character single-line buffer
- **THEN** the ledger records the per-chunk time for both positionings, and the edit-growth source capture uses a positioning whose recorded time does not grow with depth, or is declared an envelope
