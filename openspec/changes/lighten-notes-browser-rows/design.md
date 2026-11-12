## Context

BN (`bound-notes-browser-by-budget`) replaced the notes browser's `AdwSidebar`
with a virtualized `GtkListView`. Its row factory builds this tree once per
realized row:

```
ListItemWidget
└── Box
    ├── Image
    ├── Box
    │   ├── Label   (title)
    │   └── Label   (subtitle: caption, dim)
    └── Button      (Open; hidden unless the row is a bookmark)
        └── Image
```

That is 8 widgets per row, about 1,640 for the 205 rows GTK realizes (A1). A
republish retargets the realized row objects in place (`content-changed`), so
a query rebinds rows rather than rebuilding them. The command palette's
grouped and category views share one factory
(`ui/command_palette/row_factory.rs`):

```
ListItemWidget
└── Box
    ├── Label      (name; middle-ellipsized)
    ├── Separator  (headers only)
    └── Label      (subtitle: caption, dim)
```

That is 5 widgets per row.

BN recorded E1–E3 and DP recorded X1 and the deferred clear as toolkit-floor
exceptions, both blaming "the about 205 realized rows". Neither separated the
row count from the row tree, nor layout from paint. The queue entry made that
separation this change's first job. The measurements below come before every
decision.

## Measured baseline (`main` `23518e53`, 2026-11-12)

Conditions:

- **Toolkit and host**: GTK 4.24.1 / libadwaita 1.10.0, toolbox
  `hypatia-f45`, CPUs 8–23.
- **Build**: the optimized widget build (`opt-level 2`, no debug assertions),
  headless Mutter.
- **Load**: every run behind `scripts/measure.sh` and a 1-minute load below 4
  for 30 s, recorded before and after in
  `.claude/handoffs/lighten-notes-browser-rows/runs.log` (0.7–2.1).
- **Evidence**: `.claude/handoffs/lighten-notes-browser-rows/`. `main/` holds
  the widget runs, `rowcost-*.txt` the probe, `perf-*.script.gz` the profiles,
  and `zz_*.rs` the uncommitted measurement code.

### B1. The committed bound tests, harness renderer (`GSK_RENDERER=cairo`), 3 runs

| Dispatch | `main` (16,384 rows) | BN's record (GTK 4.22) |
|---|---|---|
| `show-notes` action | 4.6–4.7 ms | 4.0–11.7 ms |
| until rows | 19.4–19.5 ms | 16.9–19.9 ms |
| first frames (worst) | 61.3–63.2 ms | 60–65 ms (E1) |
| keystroke | 0.15–0.31 ms | 0.17–0.40 ms |
| query publish | 2.2–2.6 ms | 2.2–2.8 ms |
| query frames (worst) | 20.2–20.8 ms | 22.3–22.9 ms (E2) |
| close (worst, 1.5 s after) | 25.1–28.1 ms | 26.7–34.4 ms (E3) |

| Palette dispatch (131,072 files) | `main` |
|---|---|
| Enter on a More row | 0.12–0.17 ms |
| first category publish | 7.1–7.2 ms |
| the frame after it (X1) | 15.7–15.8 ms |
| Escape | 0.9–1.1 ms |
| leave, worst (deferred clear) | 10.6–11.6 ms |

### B2. Frame phases inside the real dialog (measurement-only test, 3 rounds in one process)

The test records every dispatch over 2 ms, splitting it at the end of the
frame clock's `layout` and `paint` phases.

| Phase | 16,384 rows, cairo | 8 rows, cairo | 16,384 rows, GL (rounds 2–3) |
|---|---|---|---|
| first frame with rows: layout / paint | 36.4–38.2 / 21–23 ms | 3.0–7.4 / 0.1–17 ms | 36.4 / 2.2 ms |
| open-animation frames (each) | 25.8–30.6 ms, all paint (9 frames) | 17.9–28.3 ms, all paint | 4.1–5.4 ms |
| query frame: layout / total | 18.7–19.3 / 20.5–21.2 ms | 1.1–3.9 / 2.9–6.5 ms | 18.4 / 20.8 ms |
| close input dispatch (`close()` call) | 13.3–14.5 ms | 0.25–0.62 ms | 14.2 ms |
| close-animation frames (each) | 15.5–26.9 ms, all paint | 11.0–34.3 ms, all paint | 2.8–5.4 ms |

- **The first GL dialog of a process** paints for 49 ms in its first frame and
  takes 81 ms to close. That is the GL renderer compiling its pipelines once.
  It recurs in no later dialog. The committed bound test opens one dialog per
  process, so under GL it measured that warm-up: first frames 85–86 ms, close
  19–81 ms. The test failed its bound; the warm-up, not the rows, caused it.
- **Palette under GL**: X1 16.3–16.6 ms, deferred clear 7.9–10.6 ms. Like
  cairo, so both are layout and teardown, not paint.

### B3. `perf` profiles (cycles:u, DWARF call graphs, cairo)

- **Query frame (E2)**: `gtk_list_view_measure` 88 %, `gtk_label_measure`
  82 %, `pango_layout_check_lines` 81 %, `gtk_list_view_measure_across` 64 %,
  `pango_shape` 35 %. Every realized label is measured for the list's natural
  width and again for its height at the allocated width.
- **Close input dispatch**: `gtk_list_item_manager` 86 %, dispose 61 %,
  finalize 49 %, unparent 26 %, `gtk_accessible` 17 %. The realized rows are
  torn down inside the call.
- **Animation frames**: `gsk` 97–99 %, pixman 85–88 %, mostly bilinear
  `repeat`. Cairo composites the dialog's scaled, faded node in software.

### B4. Isolated probe

The probe varies only the row tree and the model size. A presented 1000×760
window shows a 640 px `navigation-sidebar` list. The Rust is a debug build;
GTK's C code is the system's. Each figure is the median of 7 runs. E1 is the
frame that maps the list; E2 is the frame after every bound row's text
changes; E3 is removing the list (the remove call).

| Row tree | rows realized | widgets | setup | E1 frame (layout) | E2 frame (layout) | E3 |
|---|---|---|---|---|---|---|
| notes row (`main`) | 16 | 129 | 2.3 ms | 5.2 (3.6) ms | 7.8 (5.1) ms | 2.6 ms |
| notes row (`main`) | 205 | 1,641 | 4.8 ms | 20.5 (18.9) ms | 19.2 (17.6) ms | 11.4 ms |
| notes row, no Open button | 205 | 1,231 | 3.2 ms | 20.3 (18.7) ms | 19.3 (17.8) ms | 10.3 ms |
| two labels, no icon or button | 205 | 821 | 2.2 ms | 15.7 (14.2) ms | 17.9 (16.5) ms | 9.0 ms |
| one two-line label | 205 | 411 | 1.2–1.7 ms | 11.7 (10.6) ms | 12.1–13.0 (11.0–12.1) ms | 6.4–6.7 ms |
| notes row, `GtkInscription` | 205 | 1,641 | **19.2 ms** | 14.6 (12.9) ms | 5.9 (0.0) ms | 10.3 ms |
| **notes row, measure-free text** | 205 | 1,641 | 3.9 ms | **12.3 (10.5) ms** | **6.4 (0.0) ms** | 10.7 ms |
| palette row (`main`) | 205 | 1,026 | 3.4 ms | 17.6 (15.9) ms | 18.9 (17.0) ms | 9.0 ms |
| **palette row, measure-free text** | 205 | 1,026 | 2.0 ms | **9.3 (7.5) ms** | **6.3 (0.0) ms** | 8.2 ms |

With the measure-free text, the query frame is pure paint of the visible rows.
It costs the same at 205 realized rows as at 16. `GtkInscription`'s setup
column comes from `gtk_inscription_init` calling
`gtk_widget_create_pango_layout`. That computes a style for a node with no
parent yet, which the style cache cannot share: 28 % of that run's samples.

## Goals / Non-Goals

**Goals**

- Every notes browser and palette list dispatch fits its budget on the
  renderer users get: input dispatches (the browser's close included)
  `B_INTERACT_US`, everything else `B_SETTLE_US`. A dispatch that cannot fit
  stays a measured toolkit-floor exception, citing an axiom and bounded by a
  widget test.
- No user-visible loss:
  - the same rows, icons, typography, colours, ellipsis and row heights;
  - the bookmark row's Open button;
  - both browser modes and the palette's grouped and category views;
  - keyboard paths and accessible names and descriptions.
- Correct attribution in the records: which costs are rows, which are the
  harness renderer, which are GL warm-up.

**Non-Goals**

- Fewer realized rows through GTK. There is no API for it (A1). A windowed
  model that reports a false length would break the scrollbar, `scroll_to`
  and accessible set sizes.
- The cairo renderer's animation cost. It belongs to the test harness, not to
  the product.
- The other lists (workspace sidebar, search results, local history). They are
  candidates once `LushtextRowText` exists, recorded as follow-ups.
- Publishing `LushtextRowText` in GTK Lush. It is recorded as an extraction
  candidate.

## Decisions

### D1. Attribution: the realized rows, mostly through text measurement; the harness renderer, not rows, for the animation frames

B2 separates the costs. The row-dependent parts are the first frame's layout
(+33 ms from 8 to 205 rows), the query frame's layout (+15–17 ms), and the
close input dispatch (+13–14 ms). The animation frames cost the same at 8 rows
as at 205, are 97–99 % cairo paint, and fall to 3–5 ms under GL.

B3 and B4 separate the row costs:

- **Relayout (E2) is text measurement.** A row tree whose size does not depend
  on its text removes it entirely.
- **The first frame is text measurement plus style and box measurement of new
  widgets.** The measure-free text removes about half.
- **Teardown (E3, the deferred clear) is per widget, not per text.** Lighter
  text does not change it. Moving it out of the input dispatch does.

The cairo animation paint is recorded as A111, a test-renderer property. It
is not a product exception: BN's E1 "first frames" and E3 "close" worst values
were cairo animation frames.

### D2. A measure-free row text widget, `LushtextRowText`

`crates/lushtext-core/src/ui/row_text/` (`mod.rs` wrapper, `imp.rs`), a
`GtkWidget` subclass. It is a called presentation surface, cross-cutting, with
no workflow role (it holds no decision logic).

- **CSS name `label`.** Adwaita's class rules apply as they do to the labels
  it replaces: `.title`, `.subtitle`, `.caption`, `.dim-label`, `.heading`,
  `.navigation-sidebar` row labels, high contrast. The font comes from the
  widget's Pango context and the colour from `gtk_widget_get_color`.
  `dim-label` is an opacity on the node and needs nothing.
- **Measure.**
  - Vertically, the height a `GtkLabel` would report for one line of a fixed
    probe string in the widget's font. It is computed once per font
    description (including size and CSS `line-height`) and cached per thread.
    Rows therefore keep the height `main` gives a Latin title, which the
    visual proof checks.
  - Horizontally, the minimum is the width of the ellipsis and the natural
    width equals the minimum, so containers decide the width, as with
    `GtkInscription`.
  - Neither depends on the text. In `natural-width-from-text` mode the
    horizontal natural width is the text's shaped width. Only the palette's
    group headers use it (a few rows), so a separator still fills the rest of
    the header row.
- **Draw.** The Pango layout is created on the first snapshot, so a row that is
  never drawn never lays out text. It is updated on text, width and
  font-context change. It ellipsizes with the configured mode (end, middle or
  none, as the replaced labels did), is single-line with `xalign`, and is
  centred vertically in the allocation.
- **Text changes.** Setting the text queues a redraw only (a resize only in
  `natural-width-from-text` mode) and updates the accessible label. Rebinding
  a realized row therefore costs the next frame only its paint, for the rows
  on screen (B4, A108).
- **Accessibility.** The `label` role, with the accessible label kept equal to
  the text, so AT-SPI clients that read row children see the same text as
  before (see the accessibility spec delta). Rows keep their list-item and row
  accessible names and descriptions unchanged.
- **Rejected:**
  - **`GtkInscription`**: the same measure contract, but an eager layout costs
    15 ms of setup per 205 rows (B4, A109). The cost lands in the publish
    dispatch that creates the rows, pushing it past `B_SETTLE_US`. A109
    records it, and a report is drafted in `docs/upstream-reports.md` (filing
    needs the maintainer).
  - **One two-line label with markup**: still measures, so 11–13 ms at 205
    rows. Its fixed markup alpha and size would also diverge from Adwaita's
    `.dim-label` and `.caption` under high contrast and font scaling.
  - **`GtkLabel` with `width-chars`/`max-width-chars`**: still shapes the
    text to measure height and natural width.
  - **Removing the bookmark row's Open button**: it saves 1.6 ms of setup and
    about 0.5 ms of a frame (B4). It would cost mouse users the one-click open
    of a bookmark, a visible loss. Kept.

### D3. Lighter rows, same trees

- **Notes rows**: title and subtitle become `LushtextRowText` (end-ellipsized).
- **Notes section headers**: become `LushtextRowText` with `heading`.
- **Palette rows**:
  - the name becomes `LushtextRowText`, middle-ellipsized; on headers it uses
    `natural-width-from-text` with no ellipsis, as the label had;
  - the subtitle becomes `LushtextRowText`.
- **Unchanged**: every CSS class, margin, spacing, tooltip, icon and the
  per-row accessibility helpers. The row trees keep their shape, so the
  widget-tree helpers that find the row parts change only the downcast type.

### D4. Sliced realized-row retirement outside input dispatches

The close input dispatch tears the 205 rows down in one call: 13–15 ms (B2,
B3), over `B_INTERACT_US`. The palette's deferred clear does the same in one
`G_PRIORITY_LOW` dispatch (8–12 ms).

- **The mechanism.** Both become a time-sliced retirement that removes rows
  from the end of the model:
  - each turn emits `items-changed(len - k, k, 0)` for units of rows while
    `list_budget::turn_continues` holds;
  - a turn takes at least one unit and stays within `LIST_TURN_SLICE_US`;
  - turns run at `G_PRIORITY_LOW`.
- **The unit** is a named power-of-two constant. It is a cap rounded down, set
  by measurement so one unit's teardown fits `LIST_TURN_SLICE_US` with margin
  (expected 16 rows at about 70 µs a row).
- **A110** pins that removing realized rows from the end tears down only those
  rows, at a cost proportional to them.
- **The notes browser.**
  - The close input dispatch only starts the dialog's close.
  - Whatever currently empties the model or tears the rows down inside that
    dispatch is moved into the retirement, after the dialog's `closed`.
    Implementation identifies the call path first; the profile shows the list
    item manager's teardown inside `close()`.
  - The model's sources, entries and payload ownership are released once the
    model is empty, through the existing disposal paths. `dispose_runtime`
    keeps its generation invalidation immediately, so no stale publish can
    land during the retirement.
- **The palette.** The deferred clear becomes the retirement. Re-entering a
  category before it finishes cancels it and publishes over the remaining rows
  (the retarget path), which is cheaper than today's synchronous clear.
- **Freshness.**
  - A retirement is owned by its model and generation, and stops when the
    model is published again or disposed.
  - Evidence: `category_holds_sources` becomes false only when the retirement
    completes, and the notes evidence gains a `retiring_rows` count. Tests and
    automation wait on these, not on frames.

### D5. Staged first publish, only if the first frame still misses `B_SETTLE_US`

After D2–D3 the first frame with rows is expected near 18–20 ms under GL (B2's
36 ms layout less the text share). That may still exceed `B_SETTLE_US`.

**Decision rule, applied in implementation:** if, after D2–D3, the 4-round
interleaved GL median of the frame that first shows 16,384 rows exceeds
`B_SETTLE_US`, implement the staged first publish. Otherwise do not, and
record the measured figure.

- **How it works.**
  - A source's first publish lists `min(len, R_FIRST)` rows, then grows by
    `R_STEP` rows per frame (a tick callback on the list) until the full
    length.
  - `R_FIRST` is a coverage window rounded up (it must cover the tallest
    viewport the dialog allows: 64). `R_STEP` is a cap rounded down, set by
    measurement so each growth frame fits `B_SETTLE_US`.
- **Scope.**
  - Only the first publish of a source is staged. A query publish retargets
    rows in place and does not need it.
  - Growth is cancelled by any newer publish or disposal.
  - `listed_rows` evidence and "Showing X of Y" report the final length from
    the first publish. The browser evidence gains a `growing_rows` count, and
    tests and automation wait for it to reach zero.
  - Every AT-SPI anchor in `docs/automation-reference.md` for the notes
    browser and palette (role `label`, name equal to the row text) is
    unchanged.
- **The visible cost.** The overlay scrollbar's thumb settles within a few
  frames of opening; it is hidden unless the pointer is over the list. No row
  on screen changes. Any other visible effect found in implementation reverses
  this decision: the first frame then stays a measured exception.

### D6. Measurement protocol and renderer attribution

- **The measuring test.** Verdicts come from a measurement-only test, not
  committed: the B2 harness, appended to the widget suite of a `main` and a
  branch worktree. It measures steady state, rounds 2–3 of 3 dialogs per
  process.
- **The two renderers.** Each measurement records layout and paint phases,
  under `GSK_RENDERER=gl` (the users' renderer class; the verdict) and under
  the harness's cairo (recorded, comparable with the committed bounds).
- **Rounds and conditions.**
  - 4 interleaved `main`/branch rounds on the affected cases only: notes at 8
    and 16,384 rows; palette X1, leave and grouped publish at 131,072 files.
  - A 1-minute load below 4 for 30 s before each run, recorded before and
    after.
  - Runs detached, through `scripts/measure.sh`.
- **The committed bound tests:**
  - `test_browser_dispatches_stay_within_their_measured_bounds` and
    `test_palette_category_dispatches_stay_within_their_budgets` gain phase
    assertions that hold under either renderer;
  - the first frame's and the query frame's **layout phase**, the close input
    dispatch and the leave turns are asserted against `B_SETTLE_US` and
    `B_INTERACT_US`, or against twice a recorded exception's worst;
  - whole-frame worst values are printed, never asserted, under cairo, because
    A111 makes them the harness's paint.

### D7. Exceptions after the change

E1–E3, X1, the deferred clear and DP's grouped-publish figure are
re-measured under D6. Each either fits its budget and its record is closed,
or stays a toolkit-floor exception with its corrected attribution, its axiom
(A1, A90, A108 or A110) and a widget bound at twice its measured worst. The
programme record (`docs/next/long-line-documents.md`) replaces BN's and DP's
"all three come from the about 205 rows" sentences with D1's attribution. The
cairo animation paint is recorded once, against A111, as a property of the
test renderer.

### D8. GTK axioms A108–A111

Each gets a probe in `gtk-lush-axioms`, an interactive/`--check` sample, and a
ledger row "Verified against" GTK 4.24.1 / libadwaita 1.10.0 (the GNOME 51
toolbox). A111 is skipped where no GL renderer is available, with a stated
skip. The probes reuse this stage's measurement fixture
(`.claude/handoffs/lighten-notes-browser-rows/zz_row_cost.rs`): row trees,
205 vs 16 realized rows, frame-phase split.

- **A108** asserts that the frame after a text change of realized `GtkLabel`
  rows grows with the realized rows (layout share over 50 %). The same frame
  with a text-independent measure has no layout share and does not grow.
- **A109** asserts that constructing 205 `GtkInscription`s costs several times
  constructing 205 `GtkLabel`s, measured as the constructor alone.
- **A110** asserts that removing 64 of 205 realized rows from the end tears
  down at most 64 row widgets, at a cost within a factor of 2 of 64/205 of a
  full clear.
- **A111** asserts that under cairo an `AdwDialog`'s open-animation frames
  cost at least several ms of paint with 8 rows and with 205. It records the
  GL figure where available.

### D9. No new Kani harness; `core-list-budget` rerun if touched

The retirement reuses `list_budget::turn_continues`, whose properties
`core-list-budget` already proves. If implementation adds a pure planning
function (the retirement unit, `R_FIRST`/`R_STEP` growth), it goes in
`model/list_budget.rs` with unit tests. The `core-list-budget` shard is rerun
through `make kani KANI_SHARD=core-list-budget`, one CBMC at a time, under the
memory cap. A new harness is added only if a new invariant crosses a budget,
for example "the growth reaches the full length in `ceil((len - R_FIRST) /
R_STEP)` frames". If so, it goes in `scripts/kani-shards.py` `ORACLES` too.

## Risks / Trade-offs

- **Row height parity.**
  - A fixed line height differs from `GtkLabel` when a title needs a taller
    fallback font (emoji, some scripts). Such rows would draw their glyphs
    past the line box instead of growing. Mitigations:
    - the cached probe height is the label's own single-line height in the
      primary font;
    - the visual proof covers Latin, CJK and emoji titles;
    - nothing is clipped.
  - Uniform rows are also how `AdwSidebar` and GNOME lists read. If the proof
    shows a clipped glyph, the probe string gains the fallback's ascent.
- **AT-SPI text interface.** `GtkLabel` implements `GtkAccessibleText` (caret
  and text-range queries). `LushtextRowText` exposes its text as the
  accessible label only. Orca speaks list rows by their row name and
  description, which are unchanged, so row reading is unchanged. Flat review
  of a row's children reads the accessible label. Accessibility smoke and the
  matrix row record this. If a regression appears, implementing
  `GtkAccessibleText` on the widget is the follow-up.
- **The staged publish (D5)** adds a state to the first publish. It is
  conditional, generation-guarded, and reverses if it shows anything.
- **Retirement while the model is reused.** A publish during retirement must
  win. The retirement is owned by the model and its generation and checks
  both every turn. A93/BN's emission rules (every emission matches the
  reported length) apply, and the BN crash-regression test is extended to a
  publish in the middle of a retirement.

## Migration Plan

None: no persisted format, setting, action or automation field changes
meaning. `retiring_rows` is additive in the notes evidence. If it is projected
into the automation snapshot, `docs/automation*.md` are updated and
`make check-automation-docs` runs.

## Open Questions (answered by tasks)

- The retirement unit and its measured per-unit cost (task 1.4).
- Whether D5 is needed: the GL first-frame figure after D2–D3 (task 5).
- Which call path tears the rows down inside the browser's close input
  dispatch (task 1.3).
