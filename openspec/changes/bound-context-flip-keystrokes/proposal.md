## Why

C6 gated syntax highlighting by total size per language so that the context
engine's analysis of a whole document never needs a 30 ms worker slice
(ledger A69). C6b then measured what that gate does not bound: **edits**. A
keystroke makes GtkSourceView's context engine re-analyse from the edited line
until its analysis merges with the previous one, in one first update of up to
10 ms at `G_PRIORITY_HIGH_IDLE` and, when that is not enough, in 30 ms worker
slices at `G_PRIORITY_LOW`. On `main`, inside C6's gates:

- on blank-line-free content (the lane's `short-language` fixtures) an
  ordinary key in js (18 KiB gate), typescript (10.5 KiB), css (37 KiB), sh
  (39 KiB) and yaml (144 KiB) costs 9–41 ms;
- a key that flips the syntax context of the rest of the document (a comment,
  string or docstring opener with no closer after it) costs worker-sized
  dispatches in rust, python3, xml and c (11–34 ms), and markdown's own gate
  lets ordinary keys typed inside an open `<!--` cost 11.3–16 ms;
- c at 64–84 KiB shows key-to-frame echo of 16.4–17.2 ms at the start.

C6c (`bound-context-flip-keystrokes`) was queued to fix these without
removing highlighting from ordinary files. Its planned mechanism was a gate on
the longest blank-line-free run plus a runtime downgrade. Measuring real files
first (ledger A85) showed that the blank-line hypothesis does not hold: in real
JavaScript and TypeScript an ordinary key re-analyses across blank lines (a
16 KB file: 14.5 K characters re-analysed by one key, against a longest
blank-line-free run of 9.6 K), real shell scripts with 28 K-character runs cost
3.3 ms per key while the lane's shell fixture cost 40 ms, and a static run gate
at the limits the fixtures imply (2–8 KiB) would remove highlighting from
5–46 % of the real js, ts, sh and yaml files on the reference machine. The cost
of a key is the engine's re-analysis, which only the engine's own progress
reveals.

## What Changes

- **Paced re-analysis of every edit** (design D1). The edit after-handlers arm
  one idle at `G_PRIORITY_HIGH_IDLE - 1`, which runs before the engine's first
  update (A85). Each step calls `gtk_source_buffer_ensure_highlight` on an
  empty range at a bound it chooses: the engine analyses from its invalid
  point to the line after the bound, reports the range in one
  `highlight-updated`, and tags nothing. Steps are sized towards a quarter of
  `B_INTERACT_US` by the measured rate (`model::reanalysis_pacing`, pure,
  Kani-proved). A report short of its bound means the analysis merged; the
  first update and the worker then find nothing.
- **A runtime downgrade before any worker slice** (D2). A pacing session that
  spends `B_SETTLE_US` of re-analysis (the budget of a flipping key) without
  merging removes syntax highlighting for the session: a new `DocumentScale` reduction, `ReductionReason::EditReanalysis`,
  monotone under `join` like every presentation reduction, announced through
  the existing reduced-features notice (status bar, properties row, accessible
  description; never an inline alert). Removing the language removes the
  pending first update and worker (A85) and changes no text.
- **No static run gate** (D3): rejected on the real-file measurements above,
  recorded with them. The profile and the edit-time watch are unchanged.
- **Chunked caps stay dormant** (D4): with pacing every flip at 512 KiB stays
  within `B_INTERACT_US` per dispatch, but such a document would lose its
  highlighting at the first flip near the top, and the chunked install would
  ship for the first time; left to the maintainer with the numbers.
- **A lane group, `reanalysis`**: each language at its gate, ordinary and
  flipping bursts, both fixture families; the interleaved verdict.
- **Ledger A85** with a `gtk-lush-axioms` probe and sample (`--edit-cost`,
  `--flip-cost`, `--paced`) that measured every case above.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: edits in highlighted documents are re-analysed
  in paced steps, and an edit that needs more than the first-update allowance
  removes syntax highlighting before any worker slice.
- `long-line-document-presentation`: the document scale gains the run-time
  `EditReanalysis` reduction and its notice sentence.
- `gtk-axiom-ledger`: A85.
- `responsiveness-measurement`: the lane's `reanalysis` group (each language
  at its gate, ordinary and flipping bursts, both fixture families).

## Impact

- `model/reanalysis_pacing.rs` (new, Kani shard `core-document-scale`),
  `model/document_scale.rs` (reason and reduction),
  `ui/editor_page/reanalysis_pacing.rs` (new), `ui/editor_page/document_scale.rs`
  (hooks), the A85 probe and sample, widget tests, the programme record, the
  ledger, AGENTS.md and the README.
- User-visible: keys in large highlighted documents no longer stall for
  worker slices; a document whose edits the engine cannot re-analyse within
  its first-update allowance loses highlighting for the session, with a
  status-bar notice, instead of stuttering on every key. Reopening it
  highlights it again.
