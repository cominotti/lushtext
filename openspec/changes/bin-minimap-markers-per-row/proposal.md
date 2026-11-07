## Why

The maintainer's standard (2026-11-05) is that LushText be extremely
performant and show no user-visible loss. `bound-lists-by-budget-not-count`
(BL), `bound-notes-browser-by-budget` (BN) and
`drill-down-palette-categories` (DP) applied it to workspace search, the
sidebar, Browse Notes and the command palette. This is the fourth slice,
queued by BL's D1: the minimap's semantic markers.

The minimap still drops markers the user should see:

- **Search matches stop at 2,048** (`MINIMAP_SEARCH_MATCH_CAP`). The refresh
  walks matches from the top of the document, so with more than 2,048 matches
  every match below the 2,048th has no marker, wherever the map is scrolled.
- **Modified-since-save lines stop at 2,048 recorded lines**
  (`MINIMAP_MODIFIED_LINE_MARK_CAP`). After the user has edited 2,048 lines,
  later edits get no marker until the next save. A restored draft marks only
  2,048 sampled lines of the whole buffer, so a large restored draft shows
  gaps.
- **Long-line warnings stop at 2,048 lines** (`MINIMAP_LONG_LINE_MARK_CAP`).
- `modified_lines_cache` remembers line numbers at the time of the edit. When
  lines shift, a later edit on a line that now has the same number as an
  earlier edit gets no marker.

The caps exist because the strip's cost grows with the number of markers.
Each refresh enumerates every match, and each draw projects every marker run
through two `GtkSourceMap` line lookups, including runs that are scrolled out
of view. Measured on `main` at its caps (2,048 separate runs of each kind),
projecting every run costs about 5 ms per draw (20.3–22.7 ms for four calls,
one per kind; spec measurement A2). Without the caps that cost would grow
linearly, to 64 times as much at 131,072 markers per kind.

## What Changes

- **Markers are binned per pixel row of the strip, with no count cap.** Each
  draw reads which document lines each strip row shows (one
  `line_at_y` per row). It then asks each marker kind's own index which rows
  hold a marker of that kind. A row that holds any marker of a kind draws that
  kind's lane. Drawing costs O(strip height) whatever the marker count:
  - rows that show the same lines are grouped;
  - each kind's walk jumps from one marker to the next, so it needs at most
    one probe per row group.

  A prototype on `main` at 131,072 search matches and 131,072 modified lines,
  with 713 strip rows, spent:
  - 0.09–0.64 ms on the row table;
  - 1.2–4.7 ms on a search probe per row (before grouping);
  - 0.2–0.5 ms on a modified-line probe;
  - 0.11–0.12 ms on long lines.

  (A3, A3L.)
- **The markers are drawn on whole pixels.** Row runs, their minimum height
  and the lanes are integers in the pure policy. They are converted to
  floating point once, at the cairo call.
- **Modified-since-save lines are tracked by a property-less `GtkTextTag`,
  with no count.** The tag replaces the 2,048 capped `GtkSourceMark`s and the
  stale line-number cache, and it moves with the text.
  - Tagging an edit costs about 1.1 µs (A3: 131,072 separate applications in
    140–144 ms, with no relayout).
  - A restored draft tags the whole buffer in 10–12 µs, so every line shows
    as modified.
  - On save the tag is swapped for a fresh one, so markers clear at once. The
    old tag's toggles are removed in bounded low-priority slices, each a
    power-of-two count of toggles: removing 131,072 toggle pairs in one call
    costs 30.6–32.4 ms.
  - One `GtkSourceMark` per line was measured and rejected: 768–784 ms to
    create 131,072 marks, and 147–156 ms to remove them.
- **Search markers are read from the search context's scanned matches, and
  are never forced by a synchronous scan.** On `main` a `forward` call from
  an unscanned position scans synchronously up to the next match: 68–81 ms
  for 262,144 lines with one match at the end (A4). After the scan completes,
  the same probe takes 1.1–1.2 ms. Waiting for the scan can show markers
  later than `main`'s capped walk (6.4 s at 131,072 matches), so time to the
  first in-band search marker is measured on an ordinary, a sparse and a
  dense document, and a cancellable asynchronous band-first fill is adopted
  wherever the branch is later than `main` by more than one reference frame
  (design D5).
- **The long-line analysis keeps every long line.** At most one entry per
  121-character line is kept, so the list stays below about 323 KiB under
  `S_MINIMAP`.
- **The strip redraws when the map scrolls.** The row bins depend on the
  map's scroll position.
- **The three caps leave the code and `docs/thresholds.md`.**
  `make check-thresholds` keeps the inventory honest.
- **Kani** proves the binning walk:
  - the row-to-line table is monotone;
  - every marker inside the band lands in the row group that shows its line,
    and nowhere else;
  - no row outside the band is marked;
  - the walk makes at most one probe per row group plus one, which is the
    O(height) bound.
- **GTK axioms** (probes and samples) for the toolkit behaviour the design
  relies on:
  - a property-less tag's cost, and that it causes no relayout and no undo
    entry;
  - `line_at_y` is monotone, including over hidden lines;
  - `forward` costs O(log n) after a completed scan, and scans synchronously
    before it;
  - overlay siblings are not re-snapshotted when the map scrolls.

## Attribution of PT's M7 worst dispatches (no minimap blocker)

Measured on `main` `9b8ba5ae` at a 1-minute load of 0.3–1.3. The runs were
interleaved, with the minimap on and off in each repetition and the window
destroyed between repetitions (A1).

- **Long lines, 118–140 ms (PT reps 1–2):** a measurement artefact. PT's M7
  never destroyed its windows, so each repetition also relaid out the
  previous ones. Its rep 0 was 17 ms. A run with leaked windows reproduces
  the growth, from 0.4 s to 0.7 s. With the windows destroyed, the worst
  dispatch is 9.6–9.7 ms with the minimap and 3.3–3.5 ms without it. The
  minimap's share is 6.3 ms.
- **Wrapped lines, 741–823 ms:** not the minimap. The worst dispatch is
  646–694 ms with the minimap and 645–789 ms without it. It is the editor's
  wrapped layout of 64 lines of 8,192 characters. The test installs them with
  `set_text`, which bypasses the load path's long-line gates. Owner: the
  long-line programme (DL `bound-dense-long-line-editing`).
- **Modified lines, 126–138 ms:** the minimap owns 8–11 ms of it. The worst
  dispatch is 115–124 ms with the minimap and 104–116 ms without it. It is
  the relayout after 2,100 inserts made outside the main loop, a test shape
  rather than a user path. Owner: GtkTextView validation (A37). The
  minimap's share is the `GtkSourceMap`'s relayout of the same lines, which
  is toolkit work this change does not alter, plus the marker refresh.

This change targets only the marker cost, which grows with the marker count.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `editor-minimap`: markers are drawn per pixel row with no count cap, and
  modified lines are tracked by a tag with no count. The fit-function proof
  requirement now covers the integer row binning instead of
  `fit_marker_bounds`.
- `main-thread-responsiveness`: the minimap marker dispatches fit the
  interaction budgets at any marker count.

## Impact

- **Code:** `crates/lushtext-core/src/ui/editor_page/minimap/`:
  - `policy.rs`: row binning, integer row runs and minimum height, the Kani
    harnesses; the caps and `modified_line_mark_samples` are removed;
  - `projection_execution.rs`: the gatherer and the draw;
  - `watch.rs`: the tag tracking and the map scroll redraw;
  - `retirement.rs`: tag swap and sliced retirement;
  - `evidence.rs` and `imp.rs` (`MinimapState`).
- **Tests:** failing-first unit, property and widget tests; widget
  measurements interleaved with `main`; lane cases; the `make
  visual-geometry-smoke` rendered-pixel proof (`native-minimap-*-anchors`,
  `minimap-search-marker-row`).
- **Kani:** the minimap policy harnesses in `core-geometry-policies` and
  `core-minimap-fit`, rerun and re-measured, and the minimap entry of the
  proof-strength oracle in `scripts/kani-shards.py` (its harness list and
  `functions=`).
- **Callers outside the minimap** that clear or mark modified lines: save,
  load (two sites), buffer replacement, local-history restore, memory
  eviction and draft restore.
- **Mutation testing:** the `fit_marker_bounds` notes in
  `.cargo/mutants.toml` and `docs/mutation-testing.md`.
- **Docs:**
  - `docs/thresholds.md` and `make check-thresholds`;
  - the WFR minimap row in `docs/workflow-readability-matrix.md`;
  - `AGENTS.md`;
  - the GTK axiom ledger;
  - `docs/next/long-line-documents.md`;
  - `docs/automation*.md`: the `minimap-refresh` readiness blocker's
    meaning (it now also covers a pending scan wait and tag retirement), and
    evidence fields if any reach the snapshot;
  - the release notes draft.
