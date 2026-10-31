## Why

The maintainer chose a **hybrid policy** for long paragraphs (2026-10-30):
documents whose paragraphs reach about `L_protect` ≈ 50,000 characters stay
editable, and every keystroke that misses B_interact (8 ms) in them is
recorded as an explicit **toolkit-floor exception**. C2
(`long-line-protected-view`) measured that miss (`L_EDIT_STALL` sweep, wrap on,
median per key): 5.2 ms at 5k, 6.8 ms at 10k, 9.6 ms at 20k, 20 ms at 50k and
43 ms at 100k characters, about half with wrap off below 10k, and 1.5–2.5 ms
more with a surrounding-text input method. C5's job is to bring that cost back
within budget, or to say on evidence how far it can get.

C5 was first drafted as an evaluation of editing paragraphs **above**
`L_protect` (segmentation with an offset map, a custom widget, windowed
edit-through). Under the hybrid policy that question is deferred (see
"Deferred" below); this revision turns C5 into an implementable change for the
editable range, based on a profile of where a keystroke's time goes.

**Where a keystroke goes** (50,000-character prose paragraph, release
measurement build, headless reference lane, Pango shim timing every Pango call
GTK makes, symbolized against GTK 4.22.5 debuginfo; design "Evidence"):

- **One whole-paragraph Pango layout, 16.5 ms of 16.3–17.4 ms** (wrap on;
  7.6 ms with wrap off). GTK builds it inside the key press, in
  `gtk_text_view_commit_text` → `gtk_text_view_set_virtual_cursor_pos` →
  `gtk_text_layout_get_cursor_locations` → `gtk_text_layout_create_display`.
  The display is cached and reused by validation, the scroll-to-cursor and the
  snapshot: the frame's layout phase then costs 0.1–0.3 ms and its paint
  0–2 ms. There is exactly one layout per key in the editor view, so no
  ordering trick removes it.
- **A second, unwrapped layout of the same paragraph, 7.5 ms, in its own
  dispatch, on every keystroke at every paragraph length.** It belongs to a
  **retired `GtkSourceMap` that is still alive and still bound to the document
  buffer** (no parent, one surviving reference, `incremental_validate_callback`
  queued by the buffer's own insertion). The minimap retirement exists to stop
  exactly this ("a hidden GtkSourceMap that remains bound still performs text
  layout"), and the `editor-minimap` spec requires that a retired map "never
  lays out the new paragraph". Every editor whose minimap is not visible —
  the default — pays it. This is a pre-existing defect.
- Everything LushText does per keystroke (draft dirty, document-scale watch,
  memory estimate, A37 allocation queue, status projections) is under 1 ms.
  Buffer log attributes are not computed while typing. The AT-SPI bridge with
  a listener changed nothing measurable at 20k or 50k.

**What the measured candidates do** (median of the per-key worst stall /
summed stall per key, 50k prose, 90 keystrokes per cell; design D2):

| Candidate | Wrap on | Wrap off | Verdict |
|---|---|---|---|
| Baseline | 16.3 / 22.0 ms | 7.4 / 21.3 ms | — |
| Release the retired minimap's buffer | 15.9 / 15.9 ms | 7.4 / 19.2 ms | adopt (defect fix; at 100k: summed 56 → 41 ms, stalls > 8 ms per key 2 → 1) |
| Word wrap off | 7.4 ms | — | adopt as a scale gate above `L_wrap_edit` |
| `WrapMode::Char` | Pango 34.9 ms vs 16.1 | — | reject (2× worse) |
| `WrapMode::WordChar` | Pango 15.9 ms | — | reject (no gain) |
| Font features `calt`/`liga`/`kern` off | 16.2 ms | 7.5 ms | reject (no gain) |
| Pango fallback off, hyphens off | Pango 15.9 / 16.2 ms | — | reject (no gain) |
| Current-line highlight off | 17.6 ms | 8.2 ms | reject (no gain) |
| Drop the A37 allocation queue | 17.4 ms | 8.1 ms | reject (no gain; the lane fails its A37 fixed-defect check) |
| Pre-building the full display before validation | — | — | moot: the display is already built once and reused |
| Batching keystrokes in a frame | — | — | moot: GTK already coalesces queued keys; the layout is inside GTK's commit handler |
| Deferring accessibility events | — | — | moot: no measurable AT cost |
| Display-only soft segmentation (invisible tags, anchors) | — | — | defer: tags do not split a paragraph's PangoLayout; a moving "edit window" fold is the only route to wrapped 50k ≤ 8 ms and is not prototyped |

**How far it gets.** With wrap on, the floor is one wrapped layout of the
paragraph: 16.1 ms at 50k in an isolated Pango benchmark. No measured
technique lowers it, so wrapped paragraphs meet B_interact up to about 20,000
characters and no further. The retired-map fix plus word wrap off for long
paragraphs reaches **7.4 ms median, 8.2 ms p90 at 50,000 characters** with
plain input, i.e. at the budget.

**A surrounding-text input method is the larger floor.** With the lane's probe
IM (it asks for the surrounding text wherever an IBus engine with that
capability does), GTK's `retrieve-surrounding` handler moves iterators three
words back and forward, which computes the **whole paragraph's log
attributes** (A26: 2.7 ms at 50k) about four times per keystroke, then slices
the whole paragraph. Measured with the map fix: wrap on 2.7 ms at 5k, 5.3 ms at
10k, 11.6 ms at 20k, 32 ms at 50k (worst-stall medians, heavy host load during
these runs). LushText cannot reach that handler (the text view's IM context is
private), so above about 10,000 characters it stays in the toolkit-floor
exception with an upstream report.

## What Changes

- **Fix the retired minimap (pre-existing defect).** A retired
  `GtkSourceMap` releases the document buffer and is finalized; find and
  remove the reference that keeps it alive. No retired map lays out a
  paragraph again. Failing-first widget test and a GTK axiom for the toolkit
  behaviour the fix relies on.
- **Gate word wrap by paragraph length (`L_wrap_edit`).** `DocumentScale`
  gains a presentation feature, `wrap_allowed`, off when the longest paragraph
  exceeds `L_wrap_edit`. The editor then shows the document unwrapped while
  the user's word-wrap preference is unchanged, with the same monotone
  session join, synchronous edit-time downgrade, reduced-features notice,
  document-properties row and accessible description as the other gates.
  `L_wrap_edit` is measured by the lane (the wrapped-keystroke boundary,
  expected near 20,000 characters).
- **Keystroke group in the responsiveness lane.** C2's measurement-only
  keystroke sweep becomes a maintained lane group (fixtures with start/middle/
  end markers, `place-*`/`key-*`/`verify` phases), so `L_wrap_edit` and every
  later claim are derived on the reference lane.
- **Record the floor.** The long-line programme record states the per-key
  floor (one paragraph layout in GTK's commit handler), what was tried, and
  the remaining toolkit-floor exception: wrapped paragraphs between
  `L_wrap_edit` and `L_protect` when the gate is declined, and typing with a
  surrounding-text input method above about 10,000 characters.
- **Upstream reports** (GTK): the commit handler lays out the whole paragraph
  synchronously to place the virtual cursor, and GtkTextLayout has no partial
  paragraph layout; `retrieve-surrounding` computes whole-paragraph log
  attributes several times per keystroke and returns the whole paragraph. A
  GTK release that bounds either re-opens this change's floor.

## Deferred

Editing paragraphs **above** `L_protect` (the three candidates this change
first proposed: in-buffer segmentation with an offset map, a custom
virtualized widget, windowed edit-through via the Line Inspector) is deferred
to a later change, with re-open triggers in the programme record: a
maintainer request to edit Protected View documents, or a toolkit change that
bounds paragraph layout. The analysis stays in this change's history.

## Capabilities

### New Capabilities
- `long-paragraph-editing`: the per-keystroke cost of typing in long editable
  paragraphs — the budget it is measured against, the lane group that
  measures it, the long-paragraph word-wrap gate, and the recorded
  toolkit-floor exception.

### Modified Capabilities
- `editor-minimap`: a retired native map releases the document buffer and is
  finalized, so it never lays out document text again.
- `long-line-document-presentation`: the document scale policy gains the
  `wrap_allowed` presentation feature and the `L_wrap_edit` threshold.

## Impact

- **Code:** `ui/editor_page/minimap/retirement.rs` (and whatever holds the
  retired map), `model/document_scale.rs` (+ its Kani harnesses),
  `ui/editor_page/document_scale.rs` and `imp.rs` (the wrap projection),
  `scripts/responsiveness-smoke-driver.py`,
  `scripts/generate-long-line-fixtures.py`.
- **Proofs and tests:** Kani `core-document-scale` (monotonicity and meet laws
  over the new feature), a failing-first widget test for the retired map, a
  widget test for the gate's edit-time downgrade, a GTK axiom probe and sample
  for the retired-map behaviour, the lane's keystroke group.
- **Docs:** `docs/next/long-line-documents.md` (C5 posture, floor, exception),
  `docs/next/long-line-evolution.md`, the GTK axiom ledger, AGENTS.md
  (document scale policy), README (reduced features), accessibility docs (the
  new reduced-feature wording).
- **No** new dependency, action, D-Bus member, snapshot field, GSettings key
  or persisted format. The word-wrap preference is not written by the gate.
- **Depends on** C1 (`DocumentScale`, archived) and C2
  (`long-line-protected-view`: the hybrid `L_protect` and the keystroke sweep).
