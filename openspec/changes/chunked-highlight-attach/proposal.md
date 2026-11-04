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
block-structured fixtures had one every few lines). Chunking does not change
that, so C6b lifts the gate for the other five only.

## What Changes

- **A chunked highlighted install.** For the five languages with no measured
  per-key cost at 1 MiB on either fixture family (markdown, rust, c, python3,
  xml), a
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
- **The gate is lifted to `S_SYNTAX` for those five languages** (1 MiB, C1's
  total-size cutoff), not to the open limit: the chunked install takes as
  long as the engine's analysis, 0.6–1.2 s per MiB for them on the reference
  machine (A84), during which the document is incomplete, the caret hidden and
  typed keys held. Coordinator decision of 2026-11-04 (design D1); raising it
  is a maintainer call with the measured install times in hand. HTML, JSON,
  js, typescript, css, sh, yaml, the unmeasured languages and plain text keep
  their gates (design D10).
- **A finding for the maintainer** (design D10): on blank-line-free content,
  C6's own gates for js, typescript, css and sh (10.5–39 KiB) and yaml
  (144 KiB) let keystrokes cost 9–41 ms. Enforcing `B_INTERACT_US` there
  would turn highlighting off for nearly every JavaScript and TypeScript file,
  so it is recorded and queued, not changed here.
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
- **Lane cases** `highlight/<ext>/1048576` for the five languages and for the
  seven that keep their gates, with the post-open and typing phases. The
  A84 sample's `--typing-limit` mode measures a language's typing limit with
  its language attached, at the start, the middle and inside lines near the
  top, on a generated or a given document.

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
