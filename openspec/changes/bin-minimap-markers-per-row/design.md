## Context

The minimap (`ui/editor_page/minimap/`, WFR minimap row) overlays an 8 px
marker strip (`gtk4::DrawingArea`) on a `GtkSourceMap`. The map shows the
document at a tiny font and **scrolls with the editor**: it does not fit the
whole document into its height. In the 800 px test window, 713 strip rows
show about 142 lines, about 5 px per line (A3: lines 0–141 at the top,
131,054–131,196 in the middle).

Four marker kinds share right-anchored lanes (bookmark 100 %, search 82 %,
modified 64 %, long line 46 %; whole-pixel lanes, `marker_lane_width`).

**Today** `run_minimap_refresh` builds `MinimapState::markers: Vec<MinimapMarker>`
in four steps:

- it collects bookmark lines;
- it walks search matches with `SearchContext::forward` from the buffer start,
  up to `MINIMAP_SEARCH_MATCH_CAP` (2,048);
- it reads `GtkSourceMark`s for modified lines, which
  `record_modified_line_marks` creates up to `MINIMAP_MODIFIED_LINE_MARK_CAP`
  (2,048), deduplicated by a `BTreeSet<u32>` of line numbers that goes stale
  when lines shift, and sampled to 2,048 for a restored draft;
- it reads the long-line numbers the sliced analysis kept, up to
  `MINIMAP_LONG_LINE_MARK_CAP` (2,048).

It then merges contiguous lines into runs. Each draw projects **every** run
through two `line_yrange` calls and `fit_marker_bounds` (`f64`), and draws
the runs inside the band.

PT (`power-of-two-thresholds`, D12) rounded the three caps up from 2,000 to
2,048 after M7 found every phase within `main` + 8.3 ms. M7's worst
dispatches (118–140 ms and 741–823 ms) were recorded as including a
whole-buffer `set_text` and relayout. The programme record queued this change
with the instruction to attribute them first.

## Measured baseline (spec stage, `main` `9b8ba5ae`)

Optimized widget build (`CARGO_PROFILE_DEV_OPT_LEVEL=2`, no debug
assertions), presented, `taskset -c 8-23`, each run under `scripts/measure.sh`
after 30 s at a 1-minute load below 4 (0.27–1.32 at start). Window
destroyed between repetitions. Evidence:
`.claude/handoffs/bin-minimap-markers-per-row/` (`runs.log`,
`measure/evidence.txt`, the leaky-window runs in
`measure/evidence-a1-leaky-windows.txt` and
`measure/evidence-a2-leaky-windows.txt`, the tests in
`measure/editor_page.rs.add`).

**A1, PT M7 replayed with the minimap on and off in each repetition** (worst
single dispatch, 3 repetitions):

| Phase | Minimap on | Minimap off | Minimap share |
|---|---|---|---|
| `set_text` of 2,100 long lines, settle | 9.6–9.7 ms | 3.3–3.5 ms | ≈ 6.3 ms |
| search "needle" (2,048-match walk), settle | 10.6–11.0 ms | 5.2–5.9 ms | ≈ 5 ms |
| 2,100 inserts outside the loop (290–336 ms, both), next dispatches | 114.9–123.9 ms | 103.9–116.0 ms | ≈ 8–11 ms |
| `set_text` of 64 × 8,192 wrapped characters, settle | 646–694 ms | 645–789 ms | none |

With the windows **not** destroyed, as in PT's M7, the long-line phase grows
with each repetition: 9.6, then 432 ms with the minimap, and 3.3, 305, then
727 ms without it. This is the source of PT's 118 and 137 ms in repetitions
1–2, against 17 ms in repetition 0.

**A2, `main` at its caps with no runs merging** (2,048 alternating runs of
search, modified and long lines; 216 inside the band):

- projecting every run for all four kinds (four calls, one per kind; one draw
  projects each run once): 20.3–22.7 ms, so ≈ 5–5.7 ms per draw;
- the search refresh, 10.1–11.7 ms; a forced refresh, 10.4–10.6 ms;
- 20 scroll frames, worst: 13.4–15.9 ms with every kind and 11.3–12.7 ms with
  long lines only.

**A3, an O(rows) binning prototype over public GTK API**, at 131,072 search
matches and 131,072 tagged lines (262,144 lines, 2.1 MB), 713 rows, top,
middle and end of the map:

- `line_at_y` row table: 0.09–0.64 ms;
- `forward` probe per row: 1.2–4.7 ms;
- tag probe per row (`has_tag` or `forward_to_tag_toggle`): 0.2–0.5 ms;
- long lines, 65,536 alternating in 9.6 MB (`S_MINIMAP` admits it), binary
  search per row: 0.11–0.12 ms.

The scan of 131,072 matches took 6.35–6.39 s of GtkSourceView idle time.

The tag and mark operations measured in the same run:

| Operation | Time |
|---|---|
| tag 131,072 separate lines | 140–144 ms (≈ 1.1 µs each), next frame: no dispatch |
| untag them in one call | 30.6–32.4 ms |
| tag the whole buffer | 10–12 µs, next frame 5.8–9.5 ms |
| untag the whole buffer | 53–57 µs |
| 131,072 `GtkSourceMark`s, create | 768–784 ms |
| 131,072 `GtkSourceMark`s, remove | 147–156 ms |

**A4, sparse search** (262,144 lines, one match on the last line):

- per-row `forward` probe after the scan completed: 1.1–1.2 ms;
- the same probe on a context that had not scanned: 68–81 ms, because the
  first `forward` scans synchronously to the match;
- the scan itself: 322–323 ms of idle time.

## Goals / Non-Goals

**Goals:**

- Every marker of every kind on a line the strip shows is drawn, with no
  count cap.
- Drawing and refreshing cost O(strip height), not O(markers).
- Every minimap dispatch fits `B_INTERACT_US` (8 ms) or `B_SETTLE_US` (16 ms),
  measured up to 131,072 markers of each kind and documents up to
  `S_MINIMAP`, or is a measured toolkit-floor exception with an axiom.
- Keep the whole-pixel geometry policy: no new `#[expect(float_arithmetic)]`,
  and the ceiling of 8 is never raised.
- Keep the lanes, colours, minimum height, the EOF-tail rule, accessibility
  (the strip stays decorative, `can-target=false`), and the
  `native-minimap-*-anchors` and `minimap-search-marker-row` rendered-pixel
  proofs.

**Non-Goals:**

- The `GtkSourceMap`'s own relayout after a burst of edits (A1: 8–11 ms of the
  modified phase).
- The editor's wrapped long-line layout (A1 wrapped phase).
- `MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET` (an availability analysis
  threshold, not a marker cap), `S_MINIMAP` and `L_MINIMAP`.
- Showing markers for lines scrolled out of the map: like `main`, the strip
  shows the band the map shows.

## Decisions

### D1. Attribute M7 first; no minimap blocker is claimed there

A1 is the basis.

- **Long-line 118–140 ms.** These came from leaked test windows. Owner: PT's
  measurement harness. The leak does not affect the product. The
  `test_measure_*` helpers this change adds destroy their windows.
- **Wrapped 741–823 ms.** This is the editor's layout of wrapped
  8,192-character lines, reached through `set_text`, which skips the
  load-path gates. Owner: DL `bound-dense-long-line-editing`. It is recorded
  in the programme record as a known test shape.
- **Modified 126–138 ms.** The editor's validation of 2,100 lines edited
  outside the main loop accounts for 104–116 ms (owner: GtkTextView, A37).
  The minimap accounts for 8–11 ms: the map's own relayout (toolkit) plus
  the marker refresh. The marker refresh share is re-measured on the branch.

The only minimap-owned cost that grows with the marker count is the
per-run projection (A2) and the match walk. That is what this change
removes. *Alternative rejected:* treating M7's figures as a minimap blocker.
Without the minimap the dispatches are the same or worse.

### D2. Bin per strip pixel row, probing per row group

The draw builds a **row table**:

- for each strip row `r` in the band `[lower, upper)`, it reads
  `B[r] = line_at_y(map_visible_y + r - map_offset)`. The map is
  `GtkTextView::line_at_y`; integer `y`, monotone (axiom AX2);
- `B[upper]` is one past the line at the band's last row;
- row `r` shows the lines `[B[r], max(B[r+1], B[r]+1))`.

Rows with an identical range form a **row group**. A line taller than one
pixel spans several rows of one group. A run of hidden lines (zero height,
Protected View folds) lies inside one group, so it is still represented.

Each kind gives a probe, "first marker line at or after `l`":

- **search:** `SearchContext::forward` from the start of line `l`, on a
  completed scan (D5);
- **modified:** the tag (D4): `has_tag` at `l`'s start, else
  `forward_to_tag_toggle`;
- **long lines:** `partition_point` on the sorted line vector;
- **bookmarks:** `partition_point` on the sorted bookmark lines.

The pure walker probes from the first group's start line. It maps the
returned line to its group by binary search over group starts. If that group
is past the band, it stops. Otherwise it marks the group and continues at the
next group's start. So it calls the probe at most `groups + 1` times, and
`groups ≤ band rows`. The cost is O(height) whatever the marker count, and it
only touches lines the strip shows.

Grouping cuts the search probes from one per row (A3: up to 4.7 ms at 713
rows) to one per group with a marker plus one. That is about 142 in A3's
window, an estimated 1 ms.

**The brief's "every marker lands in exactly one in-band row" is refined.**
Each marker line lands in exactly one row group. That group is drawn on every
pixel row the line covers, as `main` draws a run from the line's top to its
bottom. A line no taller than one pixel lands in exactly one row. Kani proves
the group form (D8).

*Alternatives rejected:*

- **Bin all markers once per refresh into buffer-y pixels.** That is
  O(document height in map pixels): 262,144 lines at about 5 px is 1.3 M
  rows.
- **Keep runs and project only those inside the band.** That still enumerates
  every match or mark to find them, O(markers).
- **One probe per pixel row.** That is 5 times the probes of grouping in
  A3's window.

### D3. Row runs, minimum height and drawing are integers

Per kind, adjacent marked rows coalesce into runs `[top, bottom)` in strip
pixels. A run shorter than `MINIMAP_MARKER_MIN_HEIGHT` (2 px, which becomes an
`i32` constant) grows around its centre, then is pushed back inside the
rendered-content band `[max(content_top, 0), min(content_bottom, h))`. This is
the integer twin of `expanded_to_min_height`'s rule, so no marker reaches the
EOF overscroll tail. Lanes keep `marker_lane_width` and `marker_lane_x`. The
cairo call converts each edge with `f64::from` once.

`fit_marker_bounds` and `MarkerProjectionSpace` are retired, with their three
Kani harnesses (`marker_fit_never_panics`,
`marker_bounds_stay_in_content_for_finite_f64` and
`marker_min_height_fails_for_general_f64`), replaced by D8's. The minimap
policy's admitted-function ceiling stays at most 8. It is lowered if a
retired function was one of the eight, which task 2.6 checks.

The band edges come from GTK as `f64` (`compute_bounds`) and `i32`
(`buffer_to_window_coords`). The adapter rounds the band **outward** to whole
pixels: `floor` the top, `ceil` the bottom, then clamp to the strip. That
matches today's clamp, so a marker on the first or last content line is not
lost.

Pixel effect: today a 1 px run expands to 2 px centred on a half pixel, which
cairo anti-aliases over three rows. Binned runs are crisp whole pixels. The
`minimap-search-marker-row` detector looks for the first row with marker
pixels. `max_delta_change: 0` is judged within one implementation, so the
proof stays valid. Its baseline is re-recorded if the detected row moves by
the removed anti-alias row.

### D4. Modified lines: a property-less `GtkTextTag` with generation swap and sliced retirement

The insert after-handler applies the tag `lushtext-minimap-modified-<gen>`
over `[start of first line, end of last line]` of the inserted run. The
delete handler does the same over the joined line. A restored draft tags the
whole buffer. A clean transition (save, undo to the saved state, a clean
restore) swaps in a fresh tag:

- the strip stops reading the old tag at once, so markers clear in the same
  dispatch, as today;
- a coordination step (`retirement.rs`) removes the old tag's toggles in
  bounded slices at `G_PRIORITY_LOW` with a generation guard;
- each slice removes the tag over a line range sized so it fits
  `B_INTERACT_US`. The size is derived from A3's 31 ms for 131,072 toggle
  pairs and re-measured;
- once the old tag has no toggles, it is removed from the tag table.

`MinimapState::modified_marks`, `modified_lines_cache`,
`MINIMAP_MODIFIED_LINE_MARK_CAP` and `modified_line_mark_samples` are
deleted.

**Why:**

- a tag moves with its text;
- a run of edited lines is one toggle pair;
- applying one costs O(log n);
- a property-less tag queues no relayout (A3: no pending dispatch after
  131,072 applications; a whole-buffer application cost a 5.8–9.5 ms frame,
  a redraw);
- undo history records text, not tags (AX1 pins all of this).

*Alternatives rejected:*

- **Uncapped per-line marks:** 768–784 ms to create 131,072 marks, and
  147–156 ms to remove them.
- **Start/end mark pairs per run:** ordering breaks when an insertion lands
  where one run's end and the next run's start coincide, and repairing it
  needs a merge structure the tag gives for free.
- **Removing the tag over the whole buffer in one call on save:** 30.6–32.4 ms
  at 131,072 toggles, over `B_SETTLE_US`.

**Risk:** text pasted from the same buffer carries the tag through
`gtk_text_buffer_insert_range`. That text is a modification anyway, and the
insert handler tags it. AX1's probe also pins that cross-buffer paste does
not create a stray tag in another tab's table.

### D5. Search markers come from a completed scan; a dispatch never scans synchronously

On `main`, `forward` from an unscanned position scans synchronously until it
finds a match (A4: 68–81 ms for 262,144 lines). The refresh's walk today only
escapes that because the 80 ms debounce usually lands after GtkSourceView's
idle scan.

The search probe of D2 therefore runs only when the search context reports a
completed scan (`occurrences-count >= 0`). On the notify that completes it,
the refresh re-arms. A completed scan's `forward` walks the context's match
tag (A4: 1.1–1.2 ms for 713 sparse probes). AX3 pins both costs.

While the scan runs, the strip shows the previous search bins only if the
query is unchanged, else none.

**Latency:** on `main`, a query with more than 2,048 matches showed the first
2,048 (the top of the document) after the debounce. The branch shows all of
them when the occurrence count appears. That is the moment the search bar
shows "N of M", and 6.4 s at 131,072 matches in 2.1 MB (A3), which is
GtkSourceView's own scan speed.

Task 1.4 measures time to the first search marker on `main` and on the
branch for three documents:

- ordinary: 10,000 lines;
- sparse: 262,144 lines with one match;
- dense: 131,072 matches.

If the branch is later than `main` by more than one reference frame on the
ordinary document, the fallback is adopted: a cancellable
`forward_async` walk from the band's first line, generation-guarded, whose
completions fill the bins progressively (AX4 must show it never blocks a
dispatch over `B_INTERACT_US`). Either outcome is recorded in this design.

### D6. Long lines keep every line

The analysis policy's `marker_limit` is `usize::MAX` in `PRODUCTION`. The
field stays for the fuzz, bench and property callers. The retained vector
holds at most one `u32` per line of at least 121 characters. Analysis runs
only while the minimap is available (at most `S_MINIMAP` = 10,000,000 bytes),
so it is bounded by the document: at most 82,645 entries, about 323 KiB. This
is a bound by construction, recorded in the evidence and `docs/thresholds.md`
rather than as a constant. `MINIMAP_LONG_LINE_MARK_CAP` is deleted.

### D7. The strip redraws on map scroll; bins are cached per draw input

The row table depends on the map's scroll value. The map's vadjustment
`value-changed` queues a strip draw. The draw recomputes the bins only when
these change:

- the map's vertical value or the strip height;
- the band;
- a per-kind marker generation.

Each generation advances on refresh, tag apply or swap, analysis publish,
bookmark change, or scan completion.

`main` connects only the adjustment's `changed` signal. A2's scroll frames
cost about the same with every marker kind and with long lines only, which
suggests the strip is not redrawn on scroll and its markers go stale. Task
3.4 confirms or rules this out with a rendered-pixel widget test, failing
first. If `main` is stale, this fixes a pre-existing defect (AX5 pins the
toolkit's render-node caching).

### D8. Kani over the pure binning

`policy.rs` gains the GTK-free, integer-only walker and run builder, under the
module's `deny`, with no new `expect`. Harnesses (`marker_rows_*`, so the
`core-geometry-policies` prefix `kani_proofs::marker_` picks them up),
bounded at a small number of rows and lines, prove these properties:

- **Monotone.** For any row table built from a monotone `line_at_y` model,
  group starts are strictly increasing and every row is in exactly one group.
- **Sound and complete.** For any marker-line set, the walker marks exactly
  the groups whose line range contains a marker line. Each marker line inside
  the band lands in exactly one group. No row outside `[lower, upper)` is
  marked.
- **O(height).** The walker calls the probe at most `groups + 1 ≤ rows + 1`
  times. Runs number at most `ceil(rows / 2)` per kind.
- **Runs.** Run building plus minimum-height expansion keep every run inside
  the band, at least `min(MIN, band height)` tall, and covering every marked
  row.

The affected shards are `core-geometry-policies` (prefix `marker_`, where the
retired harnesses lived) and `core-minimap-fit`. Both are rerun whole, and
their `ci_minutes` and `ci_peak_gib` are re-measured.

### D9. GTK axioms the design relies on

Each axiom gets a probe in `gtk-lush-axioms`, an interactive/`--check` sample
and a ledger row, numbered from the next free id at implementation:

- **AX1:** a property-less `GtkTextTag`:
  - applying or removing it costs O(toggles · log n), not O(lines);
  - it queues no relayout of either view, at most a redraw;
  - it creates no undo entry;
  - it moves with its text, and inserted text does not inherit it;
  - pasting into a buffer with another tag table does not carry it.
- **AX2:** `gtk_text_view_get_line_at_y` on a `GtkSourceMap` is monotone in
  `y`. It skips zero-height (invisible) lines, which then fall between two
  rows' starts. Its cost is sublinear.
- **AX3:** `GtkSourceSearchContext::forward` costs O(log n) inside a completed
  scan. From an unscanned position it scans synchronously to the next match.
  It honours `visible-only` (A33): folded text is not searched, so its
  markers follow the search bar.
- **AX4** (only if D5's fallback is adopted): `forward_async` never blocks a
  dispatch over `B_INTERACT_US` on an unscanned `S_MINIMAP` document.
- **AX5:** a `GtkOverlay` sibling is not re-snapshotted when the
  `GtkSourceMap` scrolls (its render node is cached).

### D10. Budgets and measurement

Every minimap dispatch fits `B_INTERACT_US`. A refresh, a scroll frame with
markers, and a scan-completion refresh fit `B_SETTLE_US` and stay within
`main` + one reference frame.

**Fixtures** (in the strip at 713 and 1,425 rows):

- 2,048 and 131,072 markers of each kind;
- the long-line maximum under `S_MINIMAP` (65,536 alternating, 9.6 MB);
- a sparse document;
- a 10,000,000-byte document.

**Measurements:** optimized widget build, interleaved with `main`, load below
4, recorded before and after. Lane cases `minimap-markers/2048` and
`minimap-markers/131072` (open, find, Page Down, scroll, type, save) measure
the real process. A dispatch that misses its budget for a toolkit reason is
recorded as an exception with its axiom.

## Risks / Trade-offs

- **Search markers appear when the scan completes (D5).** This is later than
  `main`'s partial walk on documents whose scan outlasts the debounce.
  Mitigation: measured, with the `forward_async` fallback ready.
- **Tag interplay with other buffer consumers.** Draft, save and local
  history read text only. Search with `visible-only` and Protected View's
  invisible tag are unaffected, because a property-less tag has no
  invisibility. AX1 and a widget test cover undo and paste.
- **Taller windows.** The row-table lookups are O(rows), 0.64 ms worst at 713
  rows. The 1,425-row measurement bounds a 4K-tall window.
- **Pixel baseline shift** from crisp whole-pixel runs (D3), proven by `make
  visual-geometry-smoke`.
- **Facade budget.** The minimap `mod.rs` is at 366 of 370 lines. New stage
  narration replaces the retired modified-mark wording, and the facade stays
  within budget, checked by `make check-workflow-boundaries`.

## Migration Plan

There is no persisted format. Tags and marks live in the buffer only.
Rollback is a revert.

## Open Questions

None blocking. D5's fallback and D7's staleness are settled by tasks 1.4 and
3.4.
