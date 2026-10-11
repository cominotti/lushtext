# Long-line documents — programme record (C0–C5)

> Planned-work record for the long-line responsiveness programme. The origin
> proposal, with its review and the reasoning behind the split, is
> [`long-line-responsiveness.md`](long-line-responsiveness.md); this record is the
> one that tracks the programme's state. `probe-long-line-toolkit-costs` (C0)
> created it and filled in the measured values; every later change updates it in
> the same change that advances it.

LushText must stay instantly responsive (open, scroll, every interaction) with
documents of any admitted size, and must never change a file's bytes. A 5 MB
HTML file with a 1.9-million-character line froze the app for seconds. The cost
that freezes it is **paragraph length**, not file size.

## Sequence and gates

| # | Change | Delivers | Gate before the next change |
|---|---|---|---|
| C0 | `probe-long-line-toolkit-costs` | Axioms A23–A36, the synthetic fixture generator, the smoke-only in-process probe, `make responsiveness-smoke`, the budget constants, and the measured values below | Values and the budget verdict recorded here (this record). |
| C1 | `document-scale-policy` | Paragraph profile on the worker, one GTK-matching paragraph-boundary function (fixes the `\n`-only slicer), `DocumentScale` replacing `FileSizeCheck`, gates for syntax/minimap/invisibles/search highlighting at load and on edits, the minimap-detach CRITICAL fix | The context-engine stall and both CRITICALs gone, verified on the lane. |
| C2 | `long-line-protected-view` | Protected View (view-only, caret-free) for giant-paragraph documents, the display-only fold, the Line Inspector, find on a worker | Every interaction offered on these documents within the budgets. |
| C3 | `sliced-protected-install` (conditional) | Sliced install of a giant protected paragraph | Adopted only if the lane's `install` case, run against C2, exceeds B_settle. |
| C4 | `bound-window-construction-dispatch` | Removes the window-construction stall | Window construction within B_settle per dispatch. |
| C5 | `explore-editing-long-paragraphs` (optional) | Exploration of editing giant paragraphs | Maintainer decision. |

C1–C4 are drafted; none may be **implemented** before this record holds the
values they use (`responsiveness-measurement` spec).

## Maintainer decisions (2026-10-11)

1. Protected View now: giant-paragraph documents are view-only; editing them is
   deferred to an optional C5.
2. The syntax total-size cutoff S_syntax is measured by C0, not fixed in
   advance.
3. Budgets, stricter than the origin proposal's 16/50 ms: **B_interact = 8 ms**
   per interaction dispatch, **input-to-presented-frame ≤ 2 frames**, and
   **B_settle = 16 ms** per settle dispatch. They are read against **main-loop
   stalls** (the longest run of dispatches without returning to poll).
4. A **fixed 120 Hz reference frame** (8,333 µs) for every frame-relative limit:
   input-to-frame ≤ 16,666 µs, scroll p95 frame gap ≤ 8,333 µs.
5. The **reference machine** is the maintainer's desktop (`sophia`, inside its
   toolbox, headless, at most 16 CPUs); CI runs are trend evidence only and never
   fail on a budget.
6. L_protect is a fixed constant (not a GSettings key, for now).
7. Fixtures are synthetic only; the reproducer file is never committed or copied.

The constants live once in `crates/lushtext-core/src/model/responsiveness_budget.rs`
(`B_INTERACT_US`, `B_SETTLE_US`, `REFERENCE_FRAME_US`, `INPUT_TO_FRAME_FRAMES`,
`SCROLL_P95_FRAME_GAP_US`), with Kani-proved classification arithmetic. Later
changes read them and never restate the numbers.

## Measured values (reference run)

_Pending: the `full` reference run of `make responsiveness-smoke` (task 7.1) fills this section with each derived value, its rule, deciding cases, variants, repetitions and run id, and the budget verdict._

## Axioms each change cites

| Change | Axioms |
|---|---|
| C1 `document-scale-policy` | A27 (tag changes do not dirty the buffer), A29 (context engine cut-off), A31 (2000 px validation batches, the source of R_batch), A32 (source-map detach CRITICAL) |
| C2 `long-line-protected-view` | A23, A24, A25, A26, A28, A30, A33, A34, A35 |
| C3 `sliced-protected-install` | A24, A35, A36 |

Two probes corrected the designs that cite them, and C1/C2 must read the
ledger rows rather than their own earlier text:

- **A33.** GtkSourceView's `SearchSettings:visible-only` defaults to **true**:
  both the plain and the regex search skip invisible text by default. With it
  off, the plain search sees hidden text and the regex search still does not.
  C2 must set `visible-only` explicitly for whichever behaviour it relies on.
- **A29.** The context engine's 2000 ms cut-off fires at tens of thousands of
  characters, not near two million. On the reproducer's own lines (a
  single-line prefix highlighted inside its `<style>`/`<script>` context):
  CSS lines 8k → 128–131 ms, 16k → 280–430 ms, 32k → 0.96–1.58 s,
  64k → 4.3–5.4 s, 128k → cut-off; a script line 8k → 685 ms, 32k → cut-off;
  a markup line 8k → 345 ms, 32k → 2.5 s. Without its `<style>` context the
  longest line highlights in under 1 ms (it is then plain text). The synthetic
  `minified-html` longest line matches the markup and script lines (8k → 337 ms,
  32k → cut-off); the lane's `paragraph` kind has a `script` flavour calibrated
  to be at least as costly (~0.75 s at 8k, cut-off by 16k), and L_syntax takes
  the worst flavour.

Other recorded probe values (GTK 4.22.5, Adw 1.9.3, Pango 1.57.1,
GtkSourceView 5.20.0): A23 first layout of a 200k-character paragraph
177–217 ms wrapped, 34–42 ms unwrapped; A24 hiding a 200k tail 1.6 ms, showing
it 202–211 ms; A26 one log-attrs recompute in a 300k paragraph ≈ 17 ms; A28
Pango x positions overflow int32 at 238,319 monospace characters; A30 the
surrounding text is the whole paragraph (50,000 characters), requested on caret
moves, `move-cursor`, typing and scroll; A31 about 2016 px per validation step,
112 rows per batch at the default font with wrap off; A33 plain search
18–19 ms per match at 200k.

## Pre-existing blockers reproduced by C0

None is allowlisted; each is reported by the lane as a finding with its owner.

| Blocker | Owner |
|---|---|
| `GtkSourceMap` detach with a pending update tick emits `Gtk-CRITICAL` and `GLib-GObject-CRITICAL` (axiom A32) | C1 |
| The install slicer splits only on `\n`, so CR-only and U+2029 documents install in one GTK turn | C1 |
| Window construction stalls the main loop for about 190 ms on every open | C4 |



## Headless measurement notes (task 6.0 spike)

- Mutter's `org.gnome.Mutter.RemoteDesktop` works inside the private
  `dbus-run-session`: keysym, relative motion, axis and button injection reach
  GTK with no screencast session. The first keyboard and pointer event after
  `Start` can be dropped, so the driver warms both devices up.
- Keys reach the window only after a click **inside the text area** (a
  header-bar click starts a compositor grab), so every case clicks first.
- `gdk_event_get_time()` is `CLOCK_MONOTONIC` in milliseconds, so the
  compositor timestamp is a valid latency origin (it lies between the injection
  and the handler).
- `--virtual-monitor 1280x800@120` is accepted. With `GSK_RENDERER=cairo` the
  frame clock presents at 8,333 µs; with the GL renderer the client got no
  presentation times and ran at 60 Hz, so the GL variant is optional.
- `ibus-daemon` is not installed on the reference host: the IBus variant is
  recorded as skipped, and the probe input method stands in for it (it requests
  the surrounding text at every entry point an IBus engine with that capability
  does). GTK 4.22 renamed the `ngl` renderer to `gl`.
- The probe input method needed two workarounds for gtk4-rs subclassing gaps:
  its `get_preedit_string` trampoline writes NULL out-parameters (GtkText passes
  NULL while realizing, which segfaulted), and `set_surrounding_with_selection`
  is not overridable, so GtkIMContext's own implementation swallowed the answer.

## Carried-forward decisions

- L_protect as a GSettings key (decision 6 keeps it a constant for now).
- "Copy whole line" from the Line Inspector.
- Upstream reports: the `GtkSourceMap` `update_id` that survives
  `set_view(NULL)` (A32), Pango's int32 width overflow (A28), log attrs over
  invisible text (A26), the IM spot updated on every scroll (A30), search
  copying the line per match (A33), and the gtk4-rs `IMContextImpl`
  `get_preedit_string` trampoline that writes NULL out-parameters.
- Validation in the user's session (an isolated-XDG launch with consent) is a
  C2 precondition, not a C0 deliverable.
- Whether C0 may be archived while the reproduced blockers stay open with their
  owners (design Open Question 6).
