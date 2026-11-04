## Context

C6 (`bound-post-open-settle`) bounded every post-open dispatch. For syntax
highlighting it chose D2 (the language is attached after the text is
installed, from a deferred `G_PRIORITY_LOW` stage after the first frame and
input-quiet) and D4 (a document whose whole analysis needs a 30 ms worker
slice is not highlighted: per-language `S_SYNTAX` gates, ledger A69). The A69
options run also found the alternative the maintainer queued as C6b: with the
language attached to an empty buffer and the text inserted in paragraph-aligned
chunks of at most 8 KiB, each chunk's first update analyses it and no worker
slice runs (html worst 7.7–11.9 ms, json 9.8–10.6 ms; 16 KiB chunks pushed a
first update to 16.1–26.4 ms).

EV (`defer-background-tab-layout`, PR #80) since made a hidden tab's view show
an empty placeholder buffer, paced background installs one priority step below
the visible tab's, and kept only the decoded body in the load permit after the
read. Keep-keystrokes-during-install holds typed text during an install and
replays it.

## Goals / Non-Goals

**Goals:**

- Highlight the languages with no measured per-key cost above their C6 gates, with
  every dispatch of the install, the publish and the settle within
  `B_SETTLE_US`.
- Every path that installs a highlighted body (load, reload, eviction reload,
  draft recovery, local-history restore and undo, save-formatting mirror-back)
  takes the same order; none attaches a language to a whole large body.
- Data safety unchanged: the installed text is the decoded body byte for byte;
  cancellation, supersession and disposal clear or keep exactly what they did.

**Non-Goals:**

- HTML and JSON stay gated (their per-key re-analysis, A69).
- Pastes: a large paste into a highlighted document is analysed by the engine
  as before (worker slices for the pasted text); C6b does not chunk edits. It
  is pre-existing (a paste of the same size into a small document did the same
  on `main`) and recorded as a follow-up.
- The A67 definition parse stays the recorded toolkit-floor exception.

## Decisions

### D1. The chunked languages are gated by `S_SYNTAX`, not by the open limit (coordinator, 2026-11-04)

The queued plan said "any size within the other `DocumentScale` limits". The
chunked install is bounded per dispatch at any size, but **not in time**: it
proceeds at the engine's analysis speed. Measured with the A84 sample's
`--chunked-install` (an editor-like presented view, LushText's pacing,
1 MiB short-line documents, load 4.0–4.8): markdown 0.61–0.65 s, rust 0.70 s,
yaml 0.75–0.80 s, c and xml 0.82–0.84 s, python3 1.13–1.16 s, css 1.26–1.28 s,
sh 1.30 s, js 1.69–1.76 s, typescript 2.28–2.30 s. While it runs the document
is incomplete (the end, search, Save), the caret is hidden and typed keys are
held. At 50 MB that would be half a minute to two minutes.

User experience first: a document must become complete promptly; highlighting
is the feature that yields. The bound is the scale's existing total-size
syntax cutoff `S_SYNTAX` (1 MiB, C1's, GNOME Text Editor's figure), which the
chunked languages now share with plain text: about a second at most (markdown
0.6 s, python3 1.2 s at 1 MiB), every frame of it showing text. `SyntaxLanguage::syntax_size_threshold`
returns `S_SYNTAX` for them; the old per-language values remain as
`attach_after_install_threshold`, the routing threshold of D4. Kani's
monotonicity harnesses are generic over the threshold and unchanged.

*Alternative:* no total-size gate (the plan as written). Rejected for the
install time above; a maintainer decision can raise `S_SYNTAX` for the
chunked languages with these numbers in hand.

### D2. The first chunk goes in before the attach

The plan said "attach the language to the empty buffer". That inverts D2: the
first frame would be the empty editor, and the text would appear only after
input-quiet, the A67 parse (35–41 ms for markdown) and the first chunk. The
install instead inserts the **first chunk unhighlighted**, then requests the
attach (the deferred stage, unchanged: `G_PRIORITY_LOW`, after the editor's
first frame, input-quiet or the deadline), and parks until it resolves. The
attach then meets one chunk, which A84 pins the same way as an insert into an
attached buffer (the attach to a buffer holding one 8 KiB chunk is analysed by
the first update; the probe checks the coverage at the `G_PRIORITY_LOW + 1`
idle). The first frame shows text as on `main`; the rest appears chunk by
chunk.

A language already attached to an **empty** buffer (a path-time attach that
ran before the read finished) is kept: nothing needs clearing, and a second
definition parse is avoided.

### D3. Pacing by the engine's own report (A84)

Each chunk is inserted from the same paced idles as a dense install
(`G_PRIORITY_DEFAULT_IDLE` for the visible tab, one step below for a background
tab, EV): GtkTextView validates the chunk first (`GTK_TEXT_VIEW_PRIORITY_VALIDATE`,
A70), and the engine's first update (`G_PRIORITY_HIGH_IDLE`) analyses it. Before
the next chunk, the install reads how far the engine reported: the largest
`highlight-updated` end since the last insert must reach the end of the last
line's text (A84; the empty line after a final delimiter has no text). If it
has not — the 10 ms first update did not finish the chunk, as for TypeScript
on this machine — the step waits once at `G_PRIORITY_LOW + 1`, which runs only
after the worker (A69), then inserts. A chunk waits at most once, so the
install always progresses. The pure decision is `chunk_gate`
(`model/highlight_chunking.rs`), Kani-proved: after the first chunk, no chunk
goes in with a language attached until the attach resolved and the previous
one was reported analysed or waited once.

*Alternatives:* a fixed `G_PRIORITY_LOW + 1` step for every chunk (rejected:
any `G_PRIORITY_LOW` work in the process, another tab's attach or worker,
would hold every chunk back); trusting the chunk size alone (rejected: A84's
paced runs show TypeScript chunks outrunning the first update every time).

### D4. Only documents above their attach-after-install threshold are chunked

`highlight_install_order(language, total_bytes, syntax_allowed)`: no language
when the incoming scale gates syntax; the chunked order for a chunk-highlighted
language above `attach_after_install_threshold`; C6's order otherwise. Below
the threshold C6 measured the whole analysis within budget, and the whole text
appears in the first frame; above it the C6 order would run worker slices. The
language comes from the planning worker's guess (the catalog's glob table);
GtkSourceView's guess at attach time stays authoritative, and a document the
attach classifies differently is re-gated at publish by the scale.

### D5. The 8 KiB highlight cut inside the slice plan

`SliceBudget::with_highlight_cut` lowers the byte budget to
`HIGHLIGHT_CHUNK_BYTES` (8 KiB, a power of two as measured) and keeps the
visible-character budget, so a dense highlighted document is bounded by both.
A document the scale highlights has no paragraph longer than `L_SYNTAX`
characters, so no paragraph is larger than `4 · L_SYNTAX + 3` = 8,003 bytes,
under the cut (a compile-time assertion and a Kani harness): every chunk is at
most 8 KiB, and the single-paragraph exception of the slice plan never
applies. A body that crosses the direct-install threshold only by being
chunked is routed to the sliced path (`requires_chunked_install`).

### D6. Clears never run with a language attached to a large text

Deleting slices from the start of an analysed buffer re-analyses the
remainder. So the chunked install removes the language **before** the clear
of the old text (removing one costs nothing, A27); a cancelled or superseded
chunked install removes it before its cancelled clear; and a sliced clear-only
replacement (memory eviction) removes it before clearing, its reload attaching
it again.

### D7. A background tab's install does not wait to be shown

C6's attach stage waits for an unmapped editor's first map (R8). A chunked
install parked on it would keep its decoded body charged against the load
budget until the user selects the tab, which EV removed. An attach requested
by a chunked install (`request_language_attach_for_install`) arms at once on
an unmapped editor; it still waits for input-quiet or the deadline at
`G_PRIORITY_LOW`. It also ignores the session scale, which the incoming body's
replaces only at publish (a reload of a document whose previous session was
gated).

### D8. Publish toggles nothing

The chunked install keeps highlighting on throughout; publication's
`apply_document_scale_gates` finds the language attached and highlighting on,
so the attach request settles and `set_highlight_syntax(true)` is a no-op. The
whole-buffer replacement's guard records highlighting as on for a chunked body,
so its terminal does not toggle it either (A68: a toggle over a whole buffer is
O(document)).

### D9. The verdict

Interleaved through `scripts/measure.sh`, `main` and the branch, at least three
repetitions per side: the `highlight/*` group (the five chunked languages and
the seven gated ones at 1 MiB), the five construction profiles of `baseline/small` (first frame and
fill-in), and C6's `total-size/*` cases. Per-dispatch stages are gated against
`B_SETTLE_US`/`B_INTERACT_US`; activation to laid out on the highlight group
is reported, not gated, because the branch does strictly more work there
(it highlights what `main` does not). The load is recorded per run and the
host is not required to be idle.

### D10. Which languages: the five whose typing stays free on both fixture families

The plan listed ten languages because C6 measured no per-key cost for them at
their attach thresholds, on its block-structured fixtures (blank lines between
blocks). The first verdict run of C6b, on the lane's new `short-language`
fixtures (one statement per line, no blank lines), measured keys of 31–39 ms
for js, typescript, css, sh and yaml at 1 MiB with the language attached,
against 1.4–8.5 ms for the other five. The A84 sample's `--typing-limit`
(seven keys at the start, the middle and inside six lines near the top, after
the chunked install; every dispatch within `B_INTERACT_US`; 2–3 repetitions,
load 1.7–8.3) reproduced it on the lane's fixtures and located the cause: the
same shell document with a blank line every four lines types in 1.2–4.8 ms
per key, without them in 40–55 ms; the context engine re-analyses past an edit
until a blank line. Limits on blank-line-free documents: markdown, rust, c,
python3 and xml pass at 1 MiB (worst key 2.2, 5.9, 4.6, 3.2 and 7.3 ms); css
and sh fail at 8 KiB, yaml at 16 KiB, js and typescript already at 4 KiB.

So only markdown, rust, c, python3 and xml are chunk-highlighted
(`SyntaxLanguage::chunk_highlighted`). The other five keep their C6 gates.
Those gates are themselves above the blank-line-free typing limits (js 18 KiB,
typescript 10.5 KiB, css 37 KiB, sh 39 KiB, yaml 144 KiB): a pre-existing
budget miss on `main`, reachable only with blank-line-free content. Lowering
them to the measured limits would remove highlighting from almost every
JavaScript and TypeScript file, which user experience first does not justify
without the maintainer; it is recorded and queued (record, "C6b").

## Risks / Trade-offs

- **Install time** (D1): up to about 2.3 s at 1 MiB for TypeScript, with the
  text appearing chunk by chunk. Bounded by `S_SYNTAX`.
- **The A67 parse** now also runs during an install, once per language per
  process, still deferred to input-quiet (toolkit-floor exception).
- **An eviction reload** of a highlighted document may parse its language
  twice when no other buffer holds it (D6 removes it before the clear).
- **A rename or scale change during a chunked install** re-requests a normal
  attach, which for a background tab waits for its map again; the install then
  waits too (bounded by the user showing the tab). Rare, and data-safe.
- **Large pastes** (Non-Goals).

## Migration Plan

No persisted state changes. Rollback is a revert.

## Open Questions

- Whether to raise `S_SYNTAX` for the chunked languages (D1) is the maintainer's.
- Whether to enforce `B_INTERACT_US` on blank-line-free js, typescript, css,
  sh and yaml (D10), at the cost of their highlighting, is the maintainer's.
