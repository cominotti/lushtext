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

### D1. The chunked languages are capped by install time, not by the open limit (coordinator, 2026-11-04; cap revised by D10)

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
is the feature that yields. **Revised after the review (D10):** each chunked
language is capped at the power of two whose install takes about 0.5 s, and no
higher than its flipping-key limit; the text below records the first bound,
`S_SYNTAX`, which the first PR revision used. The bound was the scale's existing total-size
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
it again. A whole-buffer replacement takes the language off at its **first
clear turn**, not when it begins (review S3): a replacement that goes stale
before its first turn then leaves the old text's language and analysis exactly
as they were, instead of re-attaching the language to the whole old body at
its terminal. A chunked body also makes a mirror-back that used to be one
direct `set_text` (a 100 KiB file, say) a visible clear-and-refill of about
0.1 s (review N3).

### D7. A background tab's install does not wait to be shown

C6's attach stage waits for an unmapped editor's first map (R8). A chunked
install parked on it would keep its decoded body charged against the load
budget until the user selects the tab, which EV removed. An attach requested
by a chunked install (`request_language_attach_for_install`) arms at once on
an unmapped editor. It skips input-quiet only in the editor the keys go to:
when the installing editor's view is mapped and has focus, its own typed
keys are held until the install's terminal (S6), so waiting for them to stop
would only lengthen the install. Every other install (a background tab, an
unfocused or hidden editor) waits for input-quiet or the deadline at
`G_PRIORITY_LOW`, because keys arriving in another editor are not held and
the A67 parse would land between them (re-review F2). It also ignores the session scale, which the incoming body's
replaces only at publish (a reload of a document whose previous session was
gated). Whether a stage is the install's is derived from ownership, not kept
as a sticky flag (review S1, S2): any `request_language_attach` while a
chunked install owns the buffer (an identity change: rename, Save As) arms the
stage the install's way, and the flag is cleared when the stage runs or is
withdrawn. Without it, a rename while a background install was parked on its
attach sent the stage back to waiting for a map that might never come, with the
load permit held (`test_a_rename_during_a_background_chunked_install_*`,
failing-first).

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
(it highlights what `main` does not). The exemption is encoded in the lane
(review S4): `c6_verdict` lists an activation-to-laid-out regression of a case
whose `attrs.chunked` is set under `exempt_regressions`, so a combined,
side-labelled run evaluates every other regression clause and does not
contradict this record. The highlight group also runs under the `at` variant
(an assistive technology listening, so hidden views stay attached). The load
is recorded per run and the host is not required to be idle.

### D10. Which languages, and their caps (review M1 and S6)

Three measurements decide, each per language, with `scripts/measure.sh` under
load (ledger A84):

1. **Ordinary keys** (`--typing-limit`): seven keys at the start, the middle
   and inside six lines near the top, after the chunked install, every
   dispatch within `B_INTERACT_US`, on C6's block-structured fixtures and the
   lane's blank-line-free `short-language` ones. On blank-line-free documents
   the engines of js, typescript, css, sh and yaml re-analyse past every key
   until a blank line (a shell document with a blank line every four lines:
   1.2–4.8 ms per key; without: 40–55 ms), and fail at 4–16 KiB. Markdown,
   rust, c, python3 and xml pass at 1 MiB.
2. **Flipping keys** (`--flip-limit`, review M1): a key that changes the
   syntax context of the rest of the document typed at its start, then its
   closer (rust `"`, python3 `"""`, markdown a code fence, c `/*`, xml
   `<!--`), every dispatch within `B_SETTLE_US`, on both fixture families.
   Markdown and c pass at 1 MiB (worst 2.9 and 3.3 ms): their engines
   resynchronise after the flip. Rust passes at 128 KiB (fails at 256 KiB,
   16.6–23.3 ms), python3 fails at 64 KiB (18.5–18.7 ms), xml at 256 KiB
   (37.2–38.4 ms).
3. **Install time** (review S6): the target is a document installed and
   highlighted within about 0.5 s. Markdown 0.61–0.65 s per MiB and c
   0.82 s per MiB give 512 KiB each, rounded down to a power of two.

The cap is the smaller power of two of 2 and 3; a language whose cap does not
exceed its C6 gate keeps the gate (rust 128 KiB against 184 KiB, xml 128 KiB
against 132 KiB, python3 none). The review round gave markdown and c 512 KiB;
the re-review (F1) withdrew both, below.

**Worst case (re-review F1).** Fixtures C6 and the lane generate close every
construct shortly after it opens (c `/* … */` on every line, markdown fenced
blocks), so an opener typed at the start resynchronises at the next closer.
A document with **no closer after the cursor** (`--flavor no-closers`: c `//`
comments only, markdown indented code and no fence) is the worst case. The
criteria are now applied there, at the start and at the middle, with flipping
keys (the opener and its closer) within `B_SETTLE_US` and the ordinary keys
typed after each within `B_INTERACT_US`, reported apart, and markdown also
probes an HTML comment opener `<!--`. Through `scripts/measure.sh`, load
3.3–4.4, two repetitions:

| Language, size | Flip worst (us) | Ordinary worst (us) | Verdict |
|---|---|---|---|
| c 64 KiB | 11678 | 3017 | pass |
| c 84 KiB (C6 gate) | 11507 | 3005 | pass |
| c 128 KiB | 20152 (closer, start) | 3655 | fail |
| markdown 128 KiB | 9244 (`<!--`) | 6253 | pass |
| markdown 256 KiB | 11743 | 11858 (inside `<!--`, start) | fail |
| markdown 320 KiB (C6 gate) | 16145 | 11503 | fail |
| markdown 512 KiB, fence only | 7861 | 6104 | pass |

C's largest passing power of two is 64 KiB and markdown's 128 KiB, each below
its C6 gate (84 and 320 KiB): **no shipped language is chunk-highlighted**
(`document_scale::chunked_threshold` returns `None` for every language) and
each keeps its C6 gate. The mechanism stays, for C6c, whose runtime
downgrade would bound the flips that cap every language here; unit tests,
Kani and the widget tests (`test-utils`) prove it with a model cap of
512 KiB for Markdown (`MODEL_MARKDOWN_CAP`), which no release, packaging or
measurement build enables. Markdown's own C6 gate fails the ordinary-key
criterion inside an open HTML comment (11.3–11.5 ms at 320 KiB): a
pre-existing miss on `main`, added to C6c.

The other languages' C6 gates are themselves above their blank-line-free
typing limits (js 18 KiB, typescript 10.5 KiB, css 37 KiB, sh 39 KiB, yaml
144 KiB), and rust's and python3's are above their flip limits only where
C6's gate is (they keep it): a pre-existing budget miss on `main`, reachable
with blank-line-free content or a flipping key. It is queued as **C6c**: gate
on the longest blank-line-free run, with a carried bound in the edit-time
watch, plus a runtime downgrade when an edit needs a low-priority worker
dispatch. This change adds no runtime downgrade.

### D11. Review fixes (2026-11-04)

- **N1:** the chunk decision runs outside the session's `RefCell` borrow; the
  `HighlightChunks` value is copied out and written back.
- **N2:** `MAX_SYNTAX_PARAGRAPH_BYTES` names its kind (external bound).
- **N4:** the redundant waiter wake after an install request is gone.
- **Liveness** (Kani, `a_chunked_install_finishes_even_without_coverage_or_a_language`):
  a step model of the chunk gate and the attach stage, where the coverage
  report may never arrive, the stage may leave no language, and one identity
  change may re-arm the stage at any step, finishes within
  `2·chunks + (deadline + 2)·(1 + renames) + 1` steps, provided a pending
  stage resolves by its deadline (`ATTACH_DEADLINE_US`).
- **Failing-first against the mechanisms** (S5): forcing the coverage
  predicate to `false` fails `test_most_chunks_go_in_on_the_engines_coverage_report`;
  forcing it to `true` fails `test_a_chunk_without_a_coverage_report_waits_once_and_goes_in`;
  removing the language removal from `begin_load_installation` fails the
  reload test, and from `retirement::abort_installation` the cancel test.
- **Re-review (2026-11-04).** F1 above. **F2:** only the editor the keys go
  to (mapped and focused) skips input-quiet; D7. **S-b:** `for_install` is
  cleared on the request's no-path, Plain and settled early returns, so a
  later ordinary request is never armed the install's way. **S-c:** whether
  a chunked install owns the buffer is an explicit fact the load and the
  replacement set and clear (`set_load_chunked_owner`,
  `set_replacement_chunked_owner`), not a `try_borrow` of their sessions.
  **S3 (cancel):** a cancelled replacement re-arms the chunked order only if
  its mutation had started. **S-a:** `c6_verdict` compares a stage only over
  dispatches starting by the baseline's median settle plus one frame
  (self-tested), so a later settle no longer counts a floor the baseline
  also has but outside its window.
- **Samples:** every `gtk-lush-axioms` sample prints its usage and exits on
  `--help`/`-h`, and refuses an unknown argument, before GTK starts
  (`classify_sample_args`, unit-tested; `make check-gtk-axioms` checks every
  flag-reading sample calls `support::guard` first). A review run of
  `a84 --help` had opened its interactive window on the live desktop.

## Risks / Trade-offs

- **Install time** (D10): a language given a cap would show its text chunk
  by chunk for up to about 0.5 s, its typed keys held until the install's
  terminal; no shipped language has a cap (F1).
- **Dormant mechanism** (F1): shipped builds never take the chunked order;
  the model builds keep it proved and tested until C6c can give a language a
  cap.
- **The A67 parse** now also runs during an install, once per language per
  process; deferred to input-quiet unless the installing editor is the shown,
  focused one whose keys the install holds (D7; toolkit-floor exception).
- **An eviction reload** of a highlighted document may parse its language
  twice when no other buffer holds it (D6 removes it before the clear).
- **A rename during a chunked install** reports the tab `Loaded` at once
  (`set_file_path`, pre-existing, unchanged); saves stay refused by the
  install's ownership until it ends.
- **Large pastes** (Non-Goals).

## Migration Plan

No persisted state changes. Rollback is a revert.

## Open Questions

- Whether to accept install times above 0.5 s for a higher cap (D10) is the maintainer's.
- Whether to enforce `B_INTERACT_US` on blank-line-free js, typescript, css,
  sh and yaml (D10), at the cost of their highlighting, is the maintainer's.
