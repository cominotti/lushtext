## Why

The maintainer chose a **hybrid policy** for long paragraphs (2026-10-30):
documents whose paragraphs reach about `L_protect_edit` ≈ 50,000 characters stay
editable, and every keystroke that misses B_interact (8 ms) in them is
recorded as an explicit **toolkit-floor exception**. C2
(`long-line-protected-view`) measured that miss (`L_EDIT_STALL` sweep, wrap on,
median per key): 5.2 ms at 5k, 6.8 ms at 10k, 9.6 ms at 20k, 20 ms at 50k and
43 ms at 100k characters. C5's job is to bring that cost back within budget, or
to say on evidence how far it can get.

C5 was first drafted as an evaluation of editing paragraphs **above**
`L_protect_edit`. Under the hybrid policy that question is deferred (see
"Deferred"); this change lowers the per-keystroke cost in the editable range,
from a profile of where a keystroke's time goes (design "Evidence").

**Where a keystroke goes** (50,000-character prose paragraph, reference lane):

- **One whole-paragraph Pango layout**, 16.5 ms wrapped, 7.6 ms unwrapped, built
  inside the key press by GTK's commit handler (`gtk_text_view_commit_text` →
  `set_virtual_cursor_pos` → `create_display`) and reused by validation, the
  scroll and the paint. Exactly one per keystroke in the editor view: the
  floor of a stock `GtkTextView`.
- **A second, unwrapped layout of the same paragraph, 7.5 ms per keystroke,
  from a retired `GtkSourceMap` that stays alive** and bound to the buffer: the
  minimap's reflow freeze (`gtk-lush-widgets`' `RenderHoldOverlay`) kept a
  strong reference to the map it covers. Every editor with the minimap off —
  the default — paid it. A pre-existing defect (ledger A52).
- **With GNOME's input path, the whole paragraph's word boundaries about five
  times per keystroke** (2.6 ms each at 50k): GNOME delivers keys to GTK 4
  through the Wayland text-input protocol, GTK's text-input module asks the
  view for its surrounding text on every committed character, and the view's
  answer walks the paragraph's word boundaries from its start (ledger A54).
  Every GNOME user goes through this path; the lane's `wayland` variant
  measures it.
- LushText's own per-keystroke work is under 1 ms.

## What Changes

- **Release the retired minimap (pre-existing defect).** `RenderHoldOverlay`
  refers to its live child weakly, so a retired map is finalized and no longer
  lays out document text. Failing-first widget tests; ledger A52.
- **Bounded surrounding text.** The editor answers its input method's
  `retrieve-surrounding` with at most 1,024 characters on each side of the
  cursor, within its paragraph, instead of GTK's whole-paragraph walk (the
  protocol sends at most 4,000 bytes around the cursor anyway). GTK offers no
  hook, so the view's own handler is blocked by its data; ledger A54 pins that
  wiring, and a missing handler leaves GTK's behaviour in place.
- **Per-paragraph wrap gate (`L_wrap_edit`).** A paragraph longer than
  `L_wrap_edit` characters is shown unwrapped, by a no-wrap tag that GTK reads
  from its first character (ledger A53); every other paragraph keeps the
  user's wrap setting.
  The decision has hysteresis (`REWRAP_MARGIN`), runs in the same
  `insert-text`/`delete-range` emission as the edit (before GTK lays the
  paragraph out) and during installation, and never writes the preference or
  the text. `DocumentScale` reports it (`wrap_allowed`, the reduced-features
  notice, the properties row, the accessible description).
- **Keystroke group in the responsiveness lane.** C2's keystroke sweep becomes
  a maintained lane group: start, middle, end and a short paragraph next to the
  long one, verified placement, and per-keystroke worst and summed stalls in
  the report.
- **Record the floor** and the remaining toolkit-floor exception in the
  programme record, with the upstream reports (GTK's commit-handler layout of
  the whole paragraph; the whole-paragraph surrounding-text walk).

**Measured** (design D2; reference lane, per-key worst-stall median / p90):
see design D2 for the full table. In short: the retired-map fix removes one
paragraph layout per keystroke; the per-paragraph gate costs what the
view-wide one did (one unwrapped layout per keystroke, a 70–90 µs tag
application when a paragraph crosses the threshold) and leaves a short
paragraph next to a long one at its usual cost; bounded surrounding text takes
GNOME's input path at 50k from 19–25 ms to 8–11 ms. Rejected for no gain:
char and word-char wrap, font features, Pango fallback and hyphens, the
current-line highlight, the A37 allocation queue, display pre-building,
batching, accessibility deferral.

## Deferred

Editing paragraphs **above** `L_protect_edit` (in-buffer segmentation with an
offset map, a custom virtualized widget, windowed edit-through via the Line
Inspector) is deferred to a later change, with re-open triggers in the
programme record. Display-only soft segmentation (a moving fold) is deferred
with them.

## Capabilities

### New Capabilities
- `long-paragraph-editing`: the per-keystroke cost of typing in long editable
  paragraphs — the lane group that measures it, the per-paragraph wrap gate,
  bounded surrounding text, and the recorded toolkit-floor exception.

### Modified Capabilities
- `editor-minimap`: a retired native map is released, so it never lays out
  document text again.
- `long-line-document-presentation`: the document scale policy gains the
  `wrap_allowed` presentation feature and the `L_wrap_edit` threshold.

## Impact

- **Code:** `gtk-lush-widgets` `render_hold.rs` (the live child is weak;
  `live_child()` returns `Option`), `ui/editor_page/minimap/` (retirement and
  evidence), `model/document_scale.rs` (+ Kani harnesses), new
  `model/paragraph_wrap.rs`, `model/surrounding_text.rs`,
  `ui/editor_page/paragraph_wrap.rs`, `ui/editor_page/surrounding_text.rs`,
  `scripts/responsiveness-smoke-driver.py`,
  `scripts/generate-long-line-fixtures.py`.
- **Axioms:** A52, A53, A54 (probes and samples in `gtk-lush-axioms`).
- **Docs:** `docs/next/long-line-documents.md`,
  `docs/next/long-line-evolution.md`, the GTK axiom ledger, AGENTS.md,
  README, the workflow matrix rows `WFR-MINIMAP` and `WFR-DOCUMENT-SCALE`.
- **No** new dependency, action, D-Bus member, snapshot field, GSettings key
  or persisted format. The word-wrap preference is never written by the gate.
- **Depends on** C1 (`DocumentScale`, archived); independent of C2's code.
