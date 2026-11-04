## Why

C6 (`bound-post-open-settle`) had to turn syntax highlighting off for most
source files above a few tens or hundreds of KiB: GtkSourceView's context
engine analyses a whole document in 30 ms worker slices that the application
cannot shorten (ledger A69), so attaching a language to a whole installed
document of more than one first update's worth of text stalls the main loop
for 30–40 ms per slice. The committed per-language gates run from 10.5 KiB
(TypeScript) to 320 KiB (Markdown). A 200 KiB Rust file or a 50 KiB Python
script opens unhighlighted today.

C6 also measured the way out (A69, `--options-s-bg-highlight`, option d):
attach the language while the buffer holds little or nothing and insert the
text in paragraph-aligned chunks of at most 8 KiB, each analysed by its first
update before the next goes in. No worker slice runs, and every dispatch
stayed within `B_SETTLE_US` at every size tried. The maintainer queued that
as C6b (2026-10-31), for the ten languages whose engine C6 measured with no
per-key cost (HTML and JSON re-analyse past a mid-document keystroke).

Measuring C6b found that five of those ten are not free per key after all: on
blank-line-free content (the lane's new `short-language` fixtures) the context
engine of js, typescript, css, sh and yaml re-analyses past every keystroke,
32–41 ms per key at 64 KiB (a blank line is where it resynchronises; C6's
block-structured fixtures had one every few lines). The independent review then
asked for keys that flip the syntax context of the rest of the document (a
string quote, a docstring, a code fence, a comment opener): rust, python3 and
xml re-analyse in worker-sized dispatches above 64–128 KiB. Markdown and c
passed at 1 MiB on fixtures that close every construct soon after it opens;
the re-review's worst case (no closer after the cursor, at the start and the
middle, ordinary keys apart, and markdown's `<!--`) brings c to 64 KiB and
markdown to 128 KiB, each below its C6 gate. **No shipped language is
chunk-highlighted**: the mechanism is kept, proved and tested under a model
cap, for C6c.

## What Changes

- **A chunked highlighted install.** For a language with a chunked cap, a
  document above its language's attach-after-install threshold (C6's measured
  gate, unchanged) installs in a new order: the language comes off before any
  old text is cleared, the first chunk goes in unhighlighted (so the first
  frame still shows text), the deferred attach stage attaches the language to
  that one chunk, and every later chunk of at most `HIGHLIGHT_CHUNK_BYTES`
  (8 KiB) goes in only once the engine has reported the previous chunk
  analysed, or after one `G_PRIORITY_LOW + 1` wait behind its worker. Load,
  reload, eviction reload, draft recovery, local-history restore and undo,
  and save-formatting mirror-back all take it (load and whole-buffer
  replacement share one decision).
- **No gate is lifted** (design D10, re-review F1): a cap is the largest
  power of two whose flipping and ordinary keys stay within budget on a
  worst-case document and whose install takes about 0.5 s; for every
  language it is at or below the C6 gate, so every language keeps its gate.
  Unit tests, Kani and the widget tests give Markdown a 512 KiB model cap
  (`MODEL_MARKDOWN_CAP`); no release, packaging or measurement build does.
- **Queued as C6c** (design D10): on blank-line-free content, C6's own gates
  for js, typescript, css and sh (10.5–39 KiB) and yaml (144 KiB) let
  keystrokes cost 9–41 ms. C6c will gate on the longest blank-line-free run,
  with a carried bound in the edit-time watch, plus a runtime downgrade when an
  edit needs a low-priority worker dispatch.
- **Review fixes:** identity changes keep a background install's attach armed
  (S1, S2); a replacement takes the language off at its first clear turn (S3);
  the chunk decision runs outside the session borrow (N1); the lane encodes
  the D9 exemption; a Kani liveness model; every axiom sample refuses `--help`
  and unknown arguments before GTK starts (a review run had opened a window on
  the live desktop).
- **Documents at or below their threshold keep C6's order** (text first,
  language after the first frame), measured within budget and showing the
  whole text in the first frame.
- **A sliced clear-only replacement (memory eviction)** removes the language
  first, so the clear does not re-analyse the shrinking remainder after every
  slice; a cancelled chunked install clears its partial text the same way.
- **A background tab's chunked install does not wait to be shown**: its attach
  stage arms at once (still deferred to input-quiet or the deadline), so the
  transient load permit is not held until the user selects the tab.
- **A new axiom, A84**: the largest `highlight-updated` end since an insert or
  attach reaches the end of the last line's text exactly when the engine has
  analysed the buffer, and a `G_PRIORITY_LOW + 1` idle always finds it there.
  Probe and sample in `gtk-lush-axioms`; the sample's `--chunked-install`
  mode measures the paced install per language.
- **Pure policy, Kani-proved** (`model/highlight_chunking.rs`, shard
  `core-post-open-settle`): the install order, the chunk gate, the
  `SliceBudget::with_highlight_cut` 8 KiB cut inside the `V_SLICE` slice plan,
  and the fact that a document the scale highlights has no paragraph larger
  than one chunk (`4 · L_SYNTAX + 3 ≤ 8 KiB`).
- **Lane cases** `highlight/<ext>/524288` for the two languages and for the
  ten that keep their gates, with the post-open and typing phases. The
  A84 sample's `--typing-limit` mode measures a language's typing limit with
  its language attached, at the start, the middle and inside lines near the
  top, on a generated or a given document, and `--flip-limit` the same with
  flipping keys.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: the background-highlight and language-attach
  requirements gain the chunked order; a new requirement states the chunked
  highlighted install.
- `gtk-axiom-ledger`: A84.
- `responsiveness-measurement`: the highlight lane group and its interleaved
  verdict.

## Impact

- `crates/lushtext-core/src/model/highlight_chunking.rs` (new),
  `model/document_scale.rs`, `model/buffer_replacement.rs`.
- `ui/editor_page/language_attach.rs`, `load/{execution,policy,retirement,evidence}.rs`,
  `buffer_replacement/execution.rs`, `imp.rs`.
- `crates/gtk-lush/axioms` (A84 probe, sample, fixtures), the ledger, the
  Kani shard table, the lane driver and fixture generator.
- No persisted format, setting, action, D-Bus member or snapshot field
  changes. No byte of any file changes: every path still installs the exact
  decoded body.
