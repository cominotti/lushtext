## Why

The long-line stream (`docs/next/long-line-responsiveness.md`, C0–C5) makes
documents with giant paragraphs instantly responsive by opening them in
**Protected View** (C2): the buffer is the decoded file and is never modified,
giant paragraphs are folded for display only, there is no caret, and the
content of a giant line is read through the bounded Line Inspector. The
maintainer accepted that trade-off on 2026-10-11 ("Protected View now,
editing those paragraphs deferred to an optional C5").

Protected View is view-only because stock GtkTextView pays whole-paragraph
costs on every caret and edit path, and folding cannot remove them:

- `compute_log_attrs` slices the whole logical paragraph, hidden text
  included, and its cache is cleared by any character change
  (gtktextbuffer.c:4469-4503, 4532-4537). It measured 92–99 ms per call on a
  1,894,586-character line.
- Visible-cursor iteration across hidden runs is linear in the hidden length;
  IM retrieve-surrounding slices the caret's paragraph; AT-SPI offsets and
  TextChanged payloads cover the paragraph; typing measured 300–440 ms per
  character unfolded.

The recorded budgets are stricter than the draft: B_interact = 8 ms per
interaction dispatch, input-to-presented-frame ≤ 2 frames, B_settle = 16 ms
per settle dispatch, as policy constants whose final values C0 records. An
editing mode that misses them is not acceptable, and data integrity is
absolute: no approach may change a byte the user did not edit.

So whether giant paragraphs can become editable is an open engineering
question with three plausible answers, each touching every place that stores
or exchanges a document coordinate (save, drafts, bookmarks, local history,
session restore, search). This change answers it with measurement and ends in
a recorded maintainer decision. **It does not ship editing.**

## What Changes

- Write an evaluation record, `docs/next/long-paragraph-editing.md`, that
  compares three candidates against one set of criteria:
  - **A. In-buffer segmentation with an offset map.** Giant paragraphs are
    split in the GtkSourceBuffer by synthetic, tagged paragraph breaks so that
    every logical paragraph GTK sees stays under the protected threshold. A
    pure offset map translates buffer coordinates to file coordinates for
    save, drafts, bookmarks, local history, session restore, workspace and
    in-tab search, go-to-line, clipboard, undo, and automation.
  - **B. A custom virtualized text widget** for protected documents only: a
    LushText-owned text model (rope or piece table), viewport-only Pango
    layout, its own caret, selection, IM (bounded surrounding text) and
    `GtkAccessibleText` implementation, with GtkSourceView kept for every
    other document.
  - **C. Windowed edit-through** (found while grounding this change): the C2
    Line Inspector becomes editable over a copied, grapheme-safe window, and a
    commit splices the window back into the unchanged-identity main buffer as
    one bounded delete-and-insert, while the main view stays caret-free. The
    buffer stays equal to the file text, so no offset map exists.
- Inventory every consumer of buffer coordinates or buffer text, with the
  obligation each candidate imposes on it.
- Run a **measured spike** for each candidate that survives the paper
  analysis, in a disposable package outside every production crate, build,
  lint and CI gate, driven by C0's synthetic fixture generator, in-process
  watchdog and frame-clock timing on the reference lane (private headless
  Mutter, `GSK_RENDERER=cairo`, ≤ 16 cores), with the AT bridge and a
  surrounding-text IM, and calibrated against C0 by re-measuring one stock
  case C0 already recorded.
- Prove the data-integrity core of each surviving candidate on paper and in
  the spike: byte-identity round trip with no edit, edits change exactly the
  edited file range, and seeded defects (a synthetic break leaking into saved
  bytes, an off-by-one splice) are rejected by property tests and, where the
  core is pure integer arithmetic, a Kani harness.
- Record any toolkit behaviour the decision relies on as a GTK axiom ledger
  entry with a `gtk-lush-axioms` probe and sample, unless C0 already did.
- End with a **Decision** section recorded by the maintainer: adopt one
  candidate (carried by a separate follow-up change), keep Protected View
  view-only, or defer with named re-open triggers. Update the long-line
  programme record accordingly.

## Capabilities

### New Capabilities
- `long-paragraph-editing-evaluation`: how LushText decides whether, and how,
  paragraphs above the Protected View threshold may become editable — the
  gate that keeps them view-only until a recorded decision, the candidate and
  consumer coverage the evaluation must have, the measured spike against the
  recorded budgets, the data-integrity obligations, the spike's isolation, and
  the decision record.

### Modified Capabilities
<!-- None. Protected View's capability (`long-line-document-presentation`)
     is introduced by C1 (`document-scale-policy`) and extended by C2
     (`long-line-protected-view`); it is not yet in openspec/specs/, and this
     change does not alter its requirements. An adopted candidate would modify
     it in its own follow-up change. GTK axiom ledger entries added under
     design D8 extend the ledger's content without changing the requirements
     of `gtk-axiom-ledger` or `gtk-lush-axioms`. -->

## Impact

- **Depends on** C2 (`long-line-protected-view`: Protected View, the fold,
  the Line Inspector, worker find) and therefore on C0 (budgets, fixture
  generator, watchdog, `make responsiveness-smoke`, axioms A23–A36) and C1
  (`DocumentScale`, the paragraph profile, the GTK-matching paragraph
  boundary function).
- **New, documentation:** `docs/next/long-paragraph-editing.md` (evaluation
  and decision); the long-line programme record created by C0 (C5 posture).
- **New, disposable:** a standalone spike package under
  `spikes/long-paragraph-editing/` with its own `[workspace]`, listed in the
  root `Cargo.toml` `exclude`, building into the gitignored `build/` tree, with
  a local-only make target. Synthetic fixtures only; the user's file is never
  copied or referenced in the repository.
- **Possibly new, maintained:** GTK axiom ledger entries with
  `gtk-lush-axioms` probes and samples for toolkit facts the decision relies
  on that C0 did not already record.
- **Unchanged:** every LushText application crate (`lushtext-core`,
  `lushtext`) and all behaviour. Protected View stays
  view-only. No new production dependency, GSettings key, action, D-Bus member
  or persisted format.
