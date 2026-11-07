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
with each repetition: 9.6, 10.5, then 432 ms with the minimap, and 3.3, 305,
then 727 ms without it. This is the source of PT's 118 and 137 ms in repetitions
1–2, against 17 ms in repetition 0.

**A2, `main` at its caps with no runs merging** (2,048 alternating runs of
search, modified and long lines; 216 inside the band):

- projecting every run for all four kinds (four calls, one per kind; one draw
  projects each run once): 20.3–22.7 ms, so ≈ 5–5.7 ms per draw;
- the search refresh, 10.1–11.7 ms; a forced refresh, 10.4–10.6 ms;
- 2,100 single-character inserts on alternating lines outside the loop, next
  dispatches: 112.6–125.4 ms, the same shape as A1's modified phase;
- 20 scroll frames, worst: 13.4–15.9 ms with every kind and 11.3–12.7 ms with
  long lines only. These are whole frame dispatches (editor, map and strip);
  the minimap-off share is measured in task 1.1.

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
- the scan itself: 322–323 ms of idle time;
- the worst dispatch in the 2 s after the scan completed: 4.8–11.3 ms.

A4 started timing only after the scan completed. On `main` the 80 ms
refresh debounce fires before this 323 ms scan ends, so its capped walk
meets an unscanned context and its first `forward` scans synchronously, as
the probe above did (68–81 ms). That dispatch was not captured; task 1.4
measures the worst dispatch over the whole search, scan included, on `main`
and on the branch.

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
`i32` constant) grows around its centre: by `(min - height) / 2` above and the
rest below, so an odd remainder goes below. It is then pushed back inside the
rendered-content band `[max(content_top, 0), min(content_bottom, h))`. This is
the integer twin of `expanded_to_min_height`'s rule, so no marker reaches the
EOF overscroll tail. Runs of one kind that overlap or touch after expansion
are merged, so the builder returns disjoint runs in increasing order, and a
lane never draws its translucent colour twice over one pixel (`main` could,
for two expanded runs that met). Lanes keep `marker_lane_width` and `marker_lane_x`. The
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
lost. The cost is that, at a fractional `content_bottom`, a marker on the last
line may cover the one pixel row that line shares with the EOF tail; it never
covers a row wholly inside the tail. The Kani bound `bottom <= upper` is
stated on the integer band.

Pixel effect: today a 1 px run expands to 2 px centred on a half pixel, which
cairo anti-aliases over three rows. Binned runs are crisp whole pixels. The
`minimap-search-marker-row` detector looks for the first row with marker
pixels. `max_delta_change: 0` is judged within one implementation, so the
proof stays valid. Its baseline is re-recorded if the detected row moves by
the removed anti-alias row.

### D4. Modified lines: a property-less `GtkTextTag` with generation swap and sliced retirement

The insert after-handler applies the tag `lushtext-minimap-modified-<gen>`
over `[start of first line, end of last line]` of the inserted run. The
delete handler becomes an **after**-handler too (today it runs before the
default handler and reads the pre-deletion lines): once the default handler
has joined the lines, it tags the joined line at the deletion point. Both
keep the existing `tracking_suspended` early return. Tagging whole lines
merges with any run already on the line, so the tag holds at most one run,
two toggles, per line. A restored draft tags the whole buffer.

A clean transition swaps in a fresh tag. The transitions are every site that
calls `clear_modified_line_marks` or `release_modified_line_marks` today:

- the save terminal (`save/execution.rs`);
- load, at two sites (`load/execution.rs`);
- the buffer-replacement terminal (`buffer_replacement/execution.rs`);
- the local-history restore (`editor_page/local_history.rs`);
- the memory-eviction terminal (`editor_page/mod.rs`);
- the minimap's own `modified-changed` handler (undo to the saved state).

`mark_entire_buffer_modified` (draft restore,
`window/drafts/restore_execution.rs`) swaps in a fresh tag and applies it to
the whole buffer. A swap:

- makes the strip stop reading the old tag at once, so markers clear in the
  same dispatch, as today;
- queues the old tag for a coordination step (`retirement.rs`) that removes
  its toggles in bounded slices at `G_PRIORITY_LOW` with a generation guard.
  Retiring tags form a FIFO, so saving again while one is still retiring adds
  a second, and dispose cancels the queue and removes every queued tag from
  the table (by then the buffer's text is going away with the editor);
- bounds each slice by **toggles**, not lines: from a `GtkTextMark` cursor
  (edits between slices invalidate iters, not marks), it advances
  `forward_to_tag_toggle` at most `MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES` times
  and removes the tag over that range. The constant is an internal cap
  (`Kind: cap`, a power of two rounded down), derived from A3's 31 ms for
  131,072 toggle pairs so that a slice fits `B_INTERACT_US`, and
  re-measured. A line-sized slice would rely on the one-run-per-line
  invariant, which a later change could break silently;
- removes the old tag from the tag table once it has no toggles, and deletes
  the cursor mark.

Text deleted by a load or buffer replacement takes its toggles with it, so
those swaps leave little or nothing to retire; the replacement's clear turns
now also remove toggles, which task 5.1 re-measures at 131,072 runs.

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

**Memory.** The tag holds at most two toggle segments per line, so it is
bounded by the line count, not by a cap: about 12 MB of segments at 131,072
runs, against `main`'s 2,048 marks. The realistic path to that many runs is
an in-tab Replace All, where every replacement is a delete and an insert.
Task 5.1 measures the peak RSS and the Replace All dispatch at 131,072
replacements on `main` and on the branch.

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
query is unchanged and the bins' draw inputs (D7) still hold, else none.
An edit makes the context report `-1` until its idle rescan of the edited
region ends, usually within one idle cycle; a scroll in that window drops
the search lane until the count returns. Task 3.8 bounds that gap with a
widget test (type, then scroll, with a search active).

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

The decision is taken on **all three** documents, because the maintainer's
standard is no user-visible loss and the dense and sparse documents are where
waiting for the scan is slowest (6.4 s and 0.32 s against `main`'s capped
walk right after the 80 ms debounce). For each document it compares the time
to the first search marker inside the band and the worst dispatch over the
whole search, scan included:

- if the branch is within one reference frame of `main` on every document,
  the completed-scan rule stands alone;
- otherwise the fallback is adopted: a cancellable `forward_async` walk from
  the band's first line, generation-guarded, whose completions fill the bins
  progressively until the scan completes (AX4 must show it never blocks a
  dispatch over `B_INTERACT_US`);
- a document on which `main` is earlier only because its marker came from a
  dispatch over `B_INTERACT_US` (A4's synchronous scan) is recorded as an
  explicit exception: the branch is then bounded by GtkSourceView's
  asynchronous scan speed, with both figures.

The outcome and figures are recorded in this design.

### D6. Long lines keep every line

The analysis policy's `marker_limit` is `usize::MAX` in `PRODUCTION`. The
field stays for the fuzz, bench and property callers. The retained vector
holds at most one `u32` per line of at least 121 characters, so at most
4 bytes per 122 bytes of analysed text, about 3.3 % of the live buffer. The
bound is relative to the live buffer, not to `S_MINIMAP`: the document scale
turns the minimap off from loaded or saved bytes and from paragraph shape,
not from the live byte count, so typing or pasting can grow a buffer past
10,000,000 bytes while the minimap stays on until the next save. At
`S_MINIMAP` the bound is 82,645 entries, about 323 KiB. This is a bound by
construction, recorded in the evidence and `docs/thresholds.md` rather than
as a constant. `MINIMAP_LONG_LINE_MARK_CAP` is deleted.

### D7. The strip redraws on map scroll; bins are cached per draw input

The row table depends on the map's scroll value. The map's vadjustment
`value-changed` queues a strip draw. The draw recomputes the bins only when
these change:

- the map's vertical value or the strip height;
- the map adjustment's `upper` (its laid-out height, which a font zoom,
  style or line-count change moves without moving the value);
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
  row. The runs it returns are disjoint and in increasing order.

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
  - pasting into a buffer with another tag table does not carry it;
  - on a `GtkSourceBuffer` with syntax highlighting and bracket matching on,
    adding a fresh tag to the table, applying it and removing it from the
    table trigger no re-highlight and no relayout;
  - applying it from an `insert-text` or `delete-range` after-handler leaves
    the signal's iterators valid for the handlers after it (the
    document-scale watch is one).
- **AX2:** `gtk_text_view_get_line_at_y` on a `GtkSourceMap` is monotone in
  `y`. It skips zero-height (invisible) lines, which then fall between two
  rows' starts. Its cost is sublinear.
- **AX3:** `GtkSourceSearchContext::forward` costs O(log n) inside a completed
  scan. From an unscanned position it scans synchronously to the next match.
  It honours `visible-only` (A33): folded text is not searched, so its
  markers follow the search bar. `occurrences-count` reads `-1` from an edit
  until the rescan of the edited region ends, and `notify::occurrences-count`
  fires when any scan ends, an edit's rescan included.
- **AX4** (only if D5's fallback is adopted): `forward_async` never blocks a
  dispatch over `B_INTERACT_US` on an unscanned `S_MINIMAP` document.
- **AX5:** a `GtkOverlay` sibling is not re-snapshotted when the
  `GtkSourceMap` scrolls (its render node is cached).

### D10. Budgets and measurement

Two scopes, so the budget names what the minimap controls:

- **Dispatches the minimap owns** (the refresh debounce, a scan-completion
  re-arm, each retirement slice, the tag application inside an edit's
  handlers) fit `B_INTERACT_US`. So does the minimap's own marker work
  inside a frame: the strip's draw function, row table and bins included,
  timed on its own.
- **The frame dispatch that shows the change** (editor, map and strip
  together) fits `B_SETTLE_US` and stays within `main` + one reference
  frame. A2's scroll frames are 11.3–15.9 ms on `main`, so the toolkit's
  share is measured with the minimap off (task 1.1) before a frame is judged.

A miss with a toolkit cause is an explicit exception with its axiom and its
figures; a miss in the minimap's own work is a defect.

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

### D11. Readiness and the test seams follow the new state

- The `minimap-refresh` readiness blocker (`minimap_work_pending`) also
  covers search bins waiting for a scan and queued tag retirement, so an
  automation client or widget test that waits for idle sees settled markers.
  `docs/automation.md` and `docs/automation-reference.md` describe the
  blocker, so they are updated and `make check-automation-docs` runs.
- `minimap_marker_count` and `minimap_marker_bounds` now report the runs
  drawn in the band, not every run in the document. Their 37 uses in
  `crates/lushtext/tests/widget/editor_page.rs` (16) and `window.rs` (21) are
  reviewed one by one. An assertion about a marker outside the band scrolls
  the map to it first; none is weakened or dropped.

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
- **Toggle memory** grows with the number of separately edited lines, up to
  two segments per line (D4). Measured in task 5.1.
- **Search lane gap after an edit.** A scroll inside an edit's rescan drops
  the search lane until the count returns (D5). Bounded by task 3.8.
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

## Implementation record (stage 3, 2026-11-07)

### Toolkit facts (task 1.3)

The four axioms hold on GTK 4.22.5, Libadwaita 1.9.3, GtkSourceView 5.20.0
and Pango 1.57.1 (sample `--check` under private headless Mutter), numbered
from the next free id:

- **A98 (AX1).** Tagging 1,024 separate lines costs 1.4 ms in a
  262,144-line buffer and 0.46 ms in a 1,024-line one; a property-less tag
  over a presented 20,000-line view leaves 0 µs of main-loop work where a
  `scale`-only tag costs 100 ms; no undo entry; it moves with its text; a
  cross-table paste carries no tag; no `highlight-updated`; after-handler
  iterators stay valid. **One correction to D4:** text inserted *strictly
  inside* a tagged range does carry the tag (inserted at either end it does
  not). It changes nothing: an insertion is tagged as modified anyway, and a
  swapped-out tag inherited by new text is removed by its retirement. The AX1
  gate passed, so D4 stood.
- **A99 (AX2).** `line_at_y` on a `GtkSourceMap` is monotone, never returns
  a hidden line, and costs 0.73 µs near the end of 262,144 lines (0.76 µs at
  1,024).
- **A100 (AX3).** `forward` costs 3 µs inside a completed scan and 74.5 ms
  outside one (262,144 lines, one match on the last line); hidden text is
  skipped; `occurrences-count` reads -1 after an edit and notifies when the
  rescan ends.
- **A101 (AX5).** An overlay sibling of the map is not redrawn when the map
  scrolls (0 draws over five scrolls; one `queue_draw` draws once).

- **A102 (AX4).** `forward_async` never blocks a dispatch (83 ms to a match
  262,143 lines away with no dispatch over 0.1 ms), **but** a pending
  request superseded by a query change never calls back and GIO logs a
  CRITICAL (`GTask ... finalized without ever returning`). The context's own
  scan of that buffer ends in 70 ms when nothing else runs; in a presented
  editor it waits behind the views' layout validation.

### D5 decided (task 1.4)

Measured interleaved (optimized widget build, load 0.7–1.0, `main` pass then
the completed-scan-only branch pass; first in-band search marker / scan end /
worst dispatch over the whole search, three repetitions):

| Document | `main` | Completed scan only |
| --- | --- | --- |
| ordinary (10,000 lines, 100 matches, map at top) | 259–296 ms / same / 6.4–7.9 ms | 381–439 ms / same / 3.9–5.7 ms |
| sparse (262,144 lines, one match at the end, map at end) | 330–345 ms / same / 75.6–85.6 ms | 5,295–5,402 ms / same / 10.0–40.6 ms |
| dense (131,072 matches, map at top) | 268–269 ms / 6,116–6,182 ms / 11.0–18.7 ms | 6,134–6,197 ms / same / 10.9–18.4 ms |

(That branch pass is the superseded completed-scan-only build; the shipped
figures are in "Measured verdicts".)

`main` was earlier on every document, and on the **ordinary** one without a
dispatch over `B_INTERACT_US`, so D5's rule required the fallback. The
scans are far slower in a presented editor than in isolation (A102: 70 ms
alone, 5.3 s here), because GtkSourceView's scan runs behind the editor's and
the map's layout validation; `main` hid that by scanning synchronously in its
refresh. The fallback D5 named, `forward_async` walks, was implemented and
dropped: a query change supersedes a pending request, which then never
returns and GIO logs a CRITICAL (A102), and every keystroke in the search
entry is such a change. **Adopted instead:** while the scan runs, the band's
own text (at most `MINIMAP_INTERIM_SEARCH_CHARS`, 32,768 characters) is
copied into a private buffer and searched synchronously with the same query
and options; once the live scan completes, the bins come from it. The copy
carries no tags, so text hidden by a fold can show an interim mark until the
scan ends. The re-measured figures are in "Measured verdicts" below.

### Decisions taken while implementing

- **The scan's end redraws the strip directly.** The `notify::occurrences-count`
  handler invalidates the bins and queues the strip draw instead of re-arming
  the 80 ms refresh debounce, which would delay the switch from the interim
  marks to the scan's by another 80 ms.
- **The bins are computed in the draw function, cached per draw input.** The
  refresh only advances the marker generation and queues the draw. The test
  seams `minimap_marker_count` and `minimap_marker_bounds` read the same
  cache through the same compute-or-reuse path, so they report exactly what
  the strip draws (D11).
- **The row table reads one `line_at_y` per line, not per row:** the rows a
  line covers (its `line_yrange`) reuse its answer.
- **An empty last line has no character to carry the tag;** its terminator
  above is tagged instead, so the edit shows on the line above (one row
  away). `main` marked it with a source mark.
- **Dispose cancels retirement without untagging.** It removes the slice
  source and deletes the cursor marks; the swapped-out tags stay in the table
  of a buffer that is finalized with the editor, rather than being removed in
  one O(toggles) call (30.6–32.4 ms at 131,072 runs) during teardown. Task 3.9
  asserts no slice source and no queued tag remain.
- **`MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES` is 4,096.** The design's 16,384
  (about 2 ms at A3's 0.12 µs per toggle) measured 6.9–9.1 ms per slice in a
  presented editor: a slice walks the toggles one by one and then removes the
  tag over the range, about 0.5 µs per toggle. 4,096 is a quarter of that.
- **Kani shard.** The `marker_rows_*` harnesses got their own shard,
  `core-minimap-binning` (6.31 local minutes, peak 11.64 GiB): added to
  `core-geometry-policies` they would break its pull-request margin (fit
  order: split first). It is gated `scheduled` with derived figures until a
  dispatched CI run measures it; promoting it to `pull-request` is an open
  decision for the maintainer.
- **Kani domain (a reduced bound, recorded).** The end-to-end harnesses
  (row groups, walker, run builder) prove bands of one to three rows; at four
  rows the walker and the run builder exhausted the 14 GiB address-space cap
  in CBMC's propositional reduction. Two narrower harnesses widen the run
  builder's guarantees: `marker_rows_expansion_*` proves the integer growth
  on every whole-pixel band up to 2^20 (minimum heights up to 2^16), and
  `marker_rows_merge_*` proves two runs that meet after growing merge into
  disjoint, ordered runs on bands up to 16 rows. Three solver-cost findings,
  each fixed in the harness or in a Kani-neutral way in the code: a symbolic
  band start (one row already exhausted the cap; the grouping is
  translation-invariant, so the start is fixed), a symbolic `Vec` capacity
  (the group vector is sized from the table's length), and `i64` arithmetic in
  the run builder (now `i32` with saturating operations). The unit property
  tests cover the same properties on bands of up to 53 rows.
- **D7: `main` is not stale in task 3.4's scenario.** The rendered-pixel test
  passes on `main`: scrolling a 3,000-line document validates new lines, which
  changes the editor adjustment's `upper`, whose `changed` handler re-arms
  `main`'s refresh, and the test reads the pixels 300 ms later. A101 still
  shows the strip is not redrawn by the scroll itself; on `main` it is
  redrawn only by that later refresh (80 ms debounce, and only when `upper`
  moved). The test stays as a guard: on the branch the redraw happens on the
  scroll's own `value-changed`.

### Two pre-existing defects found and fixed in this work stream

- **`gtk_lush_settle::Debounce` kept every superseded timeout source.** Each
  `schedule` added a GLib source holding a weak reference to its target and
  never removed the previous one, so 131,072 edits in one dispatch (an in-tab
  Replace All, or the measurement's edit loop) left 393,216 pending sources on
  the minimap refresh debounce (three arms per edit) and passed GLib's
  per-object weak-reference limit: `GLib-GObject-CRITICAL: Too many GWeakRef
  registered`, and `main` stalled for over 27 minutes in the 131,072-run
  fixture. `Debounce::schedule` and `invalidate` now remove the superseded
  source (as `SupersedingTimer` already did); `gtk-lush-settle` unit tests and
  the widget test `test_minimap_replace_all_of_many_matches_keeps_one_pending_refresh`
  are red on `main` and green here.
- **The modified-line source marks collapsed onto line 0 on `set_text`**
  (2,048 of them), which made `main`'s A1 wrapped phase 612–799 ms even with
  the minimap off. The tag has no marks: 80.7–81.0 ms here with the minimap
  off, 135.8–146.4 ms with it on (the map's own wrapped layout).

### Measured verdicts (task 5.1)

Optimized widget build, presented, `taskset -c 8-23`, each run after the
quiet gate (1-minute load 0.38–1.36 at every start), `main` (`9b8ba5ae`,
worktree `bmm-main`) and the branch (worktree `bmm-branch`) interleaved,
every window destroyed between repetitions; `runs.log` and
`measure/evidence3.txt` in the handoff hold every figure.

**Search latency (task 1.4), first in-band marker / scan end / worst dispatch
over the whole search, two passes of three repetitions each:**

| Document | `main` | Branch |
| --- | --- | --- |
| ordinary | 261–313 ms / same / 7.2–9.3 ms | **150.7–151.4 ms** / 369–437 ms / 3.8–7.0 ms |
| sparse | 331–348 ms / same / **75.7–85.8 ms** | **150.4–152.2 ms** / 5,263–5,426 ms / 8.7–43.7 ms |
| dense | 266–273 ms / 6,112–6,166 ms / 10.7–18.7 ms | **150.6–152.5 ms** / 6,159–6,203 ms / 10.9–18.6 ms |

The first marker is 110–195 ms **earlier** than `main` on every document, so
no latency exception is needed. The worst dispatches are not the minimap's:
the minimap's own marker work stays below 0.11 ms (below), the sparse
document's 43.7 ms and the dense one's 18.6 ms come with the search itself
(`main`'s are 85.8 and 18.7 ms).

**The minimap's own work at 131,072 markers of each kind**
(`test_minimap_marker_work_fits_the_budgets_at_131072_markers`, enforced on
the optimized build, two passes, 713- and 1,425-row strips):

| Work | 713 rows | 1,425 rows | Budget |
| --- | --- | --- | --- |
| bins of all four kinds after a map scroll | 0.07–0.10 ms | 0.10–0.11 ms | `B_INTERACT_US` |
| refresh dispatch | 3.5–3.6 ms | 6.2–6.4 ms | `B_INTERACT_US` |
| the clean transition (save) | 0.10–0.13 ms | 0.10–0.13 ms | `B_INTERACT_US` |
| one retirement slice (4,096 toggles), 65 slices | 3.0–3.2 ms | 4.0–4.3 ms | `B_INTERACT_US` |
| scroll frames | 3.4–3.5 ms | 5.4–5.9 ms | `B_SETTLE_US` |

`main` projected every run on every draw: 16.8–22.5 ms for the four kinds at
its caps (A2), against 0.016–0.05 ms on the branch.

**A1 (PT's M7 attribution), worst dispatch per phase, minimap on / off:**

| Phase | `main` on / off | Branch on / off |
| --- | --- | --- |
| `set_text` of 2,100 long lines | 9.6–9.7 / 3.3–3.4 ms | 9.6–9.7 / 3.1–3.2 ms |
| search "needle" | 10.4–11.1 / 5.7–7.1 ms | 9.9–10.7 / 3.6–6.1 ms |
| 2,100 inserts outside the loop | 114.0–124.5 / 101.7–116.8 ms | 87.0–90.2 / 87.1–90.3 ms |
| `set_text` of 64 × 8,192 wrapped characters | 627–723 / 612–799 ms | 136–146 / 80.7–81.0 ms |

The minimap's share of the modified phase is now nil (8–11 ms on `main`).

**A2 (2,048 alternating runs of each kind):** projection 0.016–0.05 ms
(`main` 16.8–22.5 ms); search 6.4–8.9 ms (`main` 9.2–11.1); refresh
6.7–8.9 ms (`main` 7.2–11.0); 2,100 inserts 96.7–108.8 ms (`main`
111.9–125.3); scroll frames 11.4–16.7 ms (`main` 11.4–16.8).

**Scroll frames, worst (mean), minimap on / off (tasks 1.1, 1.2):**

| Fixture, strip | `main` on | `main` off | Branch on | Branch off |
| --- | --- | --- | --- | --- |
| caps (2,100 pairs, 302 KB), 713 rows | 15.3–17.0 (11.1–11.9) | 8.2–8.5 | 11.7–17.7 (8.9–9.9) | 8.5–9.1 |
| caps, 1,425 rows | 14.0–16.2 (11.2–12.2) | 11.5–12.3 | 14.8–16.4 (11.5–12.4) | 10.4–13.7 |
| 10 MB (66,000 pairs, 9.5 MB), 713 rows | 13.4–17.5 (10.9–11.0) | 8.2–8.6 | 12.7–17.9 (9.7–10.4) | 7.8–8.9 |
| 10 MB, 1,425 rows | 17.2–17.8 (11.9–12.1) | 10.2–13.0 | 18.7–20.8 (15.1–15.8) | 10.1–13.3 |

**131,072-run fixture (branch only; `main` could not complete it, see the
`Debounce` defect above):** 131,072 single-character edits 20.6 s (157 µs
each, the toolkit's per-edit cost); worst dispatch after them 3.1–6.0 ms;
clean transition 0.10–0.14 ms with the markers cleared in that dispatch; an
in-tab Replace All of 131,072 matches 43.4–43.7 s inside GtkSourceView's own
`replace_all`, peak RSS growth 14.2–14.6 MiB, worst dispatch after it
9.4–58.1 ms; a load over the buffer 142–438 ms, worst dispatch 16.3–146.9 ms
and 11.8–80.3 ms after it (these used the first, 16,384-toggle slices).

### Exceptions (recorded)

- **Scroll frames over `B_SETTLE_US` (toolkit floor).** With the minimap on,
  `main`'s frames reach 17.0–17.8 ms and the branch's 17.7–20.8 ms; with the
  minimap off both stay at 7.8–13.7 ms, and the branch's own marker work is
  at most 0.11 ms. The excess is the `GtkSourceMap`'s own repaint of a
  scrolled map, present on `main`. Every branch frame is within `main` + one
  reference frame (the 10 MB, 1,425-row case: 20.8 against 17.8 + 8.3 ms);
  the 1,425-row 10 MB mean is 3–4 ms above `main`'s because the branch
  redraws the strip on every map scroll (A101) where `main` left it stale
  until its next refresh. No axiom pins the map's repaint cost yet (open
  item).
- **The A1 search phase** (9.9–10.7 ms, minimap off 3.6–6.1 ms) is the map
  repainting the search highlights it draws itself; the minimap's own marker
  work is in microseconds. Within `B_SETTLE_US`.
- **2,100 edits outside the main loop** (87–109 ms) and **Replace All of
  131,072 matches** (43.4 s in one GtkSourceView call): the toolkit's
  validation and replace (D1); the same with the minimap off.
- **Load over a buffer of 131,072 modified runs** (worst dispatch up to
  146.9 ms): measured, not attributed; `main` has no figure to compare
  (it cannot reach that state). Open item for the maintainer.

