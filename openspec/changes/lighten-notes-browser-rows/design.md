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
  - Vertically, the height a `GtkLabel` would report for one line of the text
    in the widget's font, **without shaping the text when the primary font
    covers it**. A single-font line's logical height is the font's ascent plus
    descent (and CSS `line-height`), whatever its characters, so it is
    computed once per font description (including size and `line-height`)
    from a probe string and cached per thread, beside the primary font
    (`pango_context_load_font`). A text with a character the primary font
    does not cover (`pango_font_has_char`, a per-character lookup, no
    shaping) is measured exactly, as a `GtkLabel` would measure it: a
    fallback font changes the line's height. Measured on this host (spec
    review, `.claude/handoffs/lighten-notes-browser-rows/spec-review/line-height-probe.txt`,
    Adwaita Sans 11): Latin 19 px, CJK 21 px, Devanagari 20 px, Arabic 29 px.
    A fixed height would shrink those rows against `main` and draw their
    glyphs over the subtitle below, a visible loss. Rows therefore keep
    exactly the height `main` gives every title, which the visual proof
    checks; only covered texts (the common case) skip shaping.
  - Horizontally, the minimum is the width of the ellipsis and, by default,
    the natural width equals the minimum, so containers decide the width, as
    with `GtkInscription`. That is exact only where the container gives the
    widget the same width whatever its natural width: a vertical box, or a
    box child that is the only one expanding. **Width parity**: where
    siblings share a box's extra width by their natural widths (GtkBox gives
    the space beyond the minimums to children in proportion to their missing
    natural size, then to expanding children), the widget uses
    `natural-width-from-text`, in which the natural width is the text's
    shaped single-line width, computed once per text and font and cached
    until either changes. The palette row needs it on both texts: its name
    (`hexpand`) and its subtitle (`halign: end`) share the row by their
    natural widths, and with min = natural the subtitle would be squeezed to
    its ellipsis. The palette group header needs it too, so its separator
    fills the rest of the header row.
  - So the measure depends on the text only through primary-font coverage
    and, in `natural-width-from-text` mode, through one cached shaping. It
    never lays out the text at the allocated width (no height-for-width
    pass), which is what `GtkLabel` repeats for every realized row (B3).
- **Draw.** The Pango layout is created on the first snapshot, so a row that is
  never drawn never lays out text. It is updated on text, width and
  font-context change. It ellipsizes with the configured mode (end, middle or
  none, as the replaced labels did), is single-line with `xalign`, and is
  centred vertically in the allocation.
- **Text changes.** Setting the text queues a redraw only, and updates the
  accessible label. It queues a resize as well when the new text's coverage
  differs from the old one's (either needs a fallback font) or in
  `natural-width-from-text` mode. Rebinding a realized notes row with covered
  text therefore costs the next frame only its paint, for the rows on screen
  (B4, A108). A palette row still costs one cached shaping per changed text,
  not `GtkLabel`'s two measures; task 1.5 measures what that leaves.
- **Accessibility.** By default the `label` role, settable before the
  accessible context exists, so the notes section header keeps the `heading`
  role it has on `main` (`accessibility::set_role`). The accessible label is
  kept equal to the text, so AT-SPI clients that read row children see the
  same text as before (see the accessibility spec delta). Rows keep their
  list-item and row accessible names and descriptions unchanged.
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
  - the name becomes `LushtextRowText`, middle-ellipsized, with
    `natural-width-from-text` (D2 width parity); on headers it has no
    ellipsis, as the label had;
  - the subtitle becomes `LushtextRowText`, middle-ellipsized, with
    `natural-width-from-text`.
- **Unchanged**: every CSS class, margin, spacing, tooltip, icon and the
  per-row accessibility helpers. The row trees keep their shape, so the
  widget-tree helpers that find the row parts change only the downcast type:
  `notes_row_widgets` and `project_section_header` (`ui/window/notes/browser.rs`),
  the two row-part lookups in `ui/command_palette/row_factory.rs`, and the
  widget-test helpers that read row text by `GtkLabel` (`labels_in` and
  `first_label` in `crates/lushtext/tests/widget/common.rs`, the label
  lookups in `notes_browser.rs`). A test helper that stopped finding row text
  would make a negative assertion (`!labels_in(..)…`) pass vacuously, so the
  helpers read `LushtextRowText` too, and each caller is checked to still
  see rows.

### D4. Sliced realized-row retirement outside input dispatches

The close input dispatch tears the 205 rows down in one call: 13–15 ms (B2,
B3), over `B_INTERACT_US`. The palette's deferred clear does the same in one
`G_PRIORITY_LOW` dispatch (8–12 ms).

- **The mechanism.** Both become a time-sliced retirement:
  - **Unrealized rows first, in one emission each.** The model knows which
    rows GTK holds (its `live` map of row objects). The first turn removes
    every row after the highest live position, then every row before the
    lowest, one `items-changed` each. Removing rows GTK does not hold tears
    nothing down, and doing it first means the realized band is all that is
    left, so GTK has no other row to realize when the band shrinks (a list
    scrolled to row 5,000 would otherwise move its anchor back as its band is
    removed and realize earlier rows, turning the retirement into creation).
    It also avoids thousands of emissions for 131,072 rows.
  - **Then the band from the end, in units.** Each turn emits
    `items-changed(len - k, k, 0)` for units of rows while
    `list_budget::turn_continues` holds: a turn takes at least one unit (Kani
    T1) and starts no unit at or after `LIST_TURN_SLICE_US` (T2), so its row
    work costs at most the slice plus one unit.
  - Turns run at `G_PRIORITY_LOW`.
- **The unit** is a named power-of-two constant. It is a cap rounded down, set
  by measurement so the slice plus one unit's teardown fits `B_INTERACT_US`
  with margin (expected 16 rows at about 70 µs a row).
- **A110** pins that, on a list that is not mapped, removing unrealized rows
  tears nothing down and realizes nothing, and removing realized rows from
  the end of what is left tears down only those rows, at a cost proportional
  to them, and realizes none.
- **The list stays alive until the retirement completes.** Rows are torn down
  one unit at a time only while the `GtkListView` exists and listens to the
  model. Disposing the list (finalizing the dialog) or unsetting its model
  tears every realized row down in that one dispatch. So the retirement owns
  a strong reference to the list view (for the notes browser, through it the
  dialog) and drops it after its last unit. Window disposal or application
  shutdown cancels it and releases everything at once, outside any
  interaction.
- **The notes browser.**
  - The close input dispatch only starts the dialog's close.
  - Whatever currently empties the model or tears the rows down inside that
    dispatch is moved into the retirement, after the dialog's `closed`.
    Implementation identifies the call path first; the profile shows the list
    item manager's teardown inside `close()`. The one model emission the
    browser owns there today is `dispose_runtime`'s `self.rows.clear()`
    (`ui/window/notes/source_execution.rs`), called from the `closed`
    handler, which also drops the state that keeps the dialog alive. If task
    1.3 finds the teardown on a path the browser does not own (the dialog
    host, a focus change, the list's unrealize), D4 is revised before task
    5.
  - **Call path, measured (task 1.3, 2026-11-12).** A measurement-only test
    on `main` (`.claude/handoffs/lighten-notes-browser-rows/impl/`, probe
    `zz13_probe.rs`) connected to the list's model and to the dialog's
    `closed` around one `AdwDialog::close()` at 16,384 rows. The one
    `items-changed(0, 16384, 0)` is emitted **inside** `close()`, by the
    browser's own `closed` handler: libadwaita 1.10 emits `closed` in the
    close call itself (before the close animation), and `dispose_runtime`'s
    `self.rows.clear()` tears the realized rows down there. The list is
    unmapped, unrealized and destroyed about 120 ms later, at the end of the
    close animation (the dialog host's last reference), in one dispatch of
    2.5 ms once the rows are gone. So the teardown is a model emission the
    browser owns, and D4 stands as written. The retirement additionally waits
    for the list to be **unmapped** before its first turn: removing rows while
    the dialog is still fading would remove visible rows from it, and A110's
    premise is an unmapped list.
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
    `R_STEP` rows per frame (a tick callback on the list) until it lists
    `min(len, R_BAND)` rows, then lists the rest in one emission. Rows past
    GTK's realized band (A1) are not realized, so listing them costs no row
    widgets; growing 16,384 rows by `R_STEP` would take about a thousand
    frames instead of a few.
  - `R_FIRST` is a coverage window rounded up (it must cover the tallest
    viewport the dialog allows: 64). `R_BAND` is a coverage window rounded up
    over A1's realized band (256). `R_STEP` is a cap rounded down, set by
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
  on screen changes. The rows' set-size accessibility metadata reports the
  final length from the first publish (it is the app's, not the model's), and
  a key press that moves the selection (End, Page Down) during growth
  completes the growth first, in that dispatch's budget, or is shown to land
  where it would on the full list. Any other visible effect found in
  implementation reverses this decision: the first frame then stays a
  measured exception.

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
- **A110** asserts, on an unmapped list of 16,384 rows with about 205
  realized, that removing the unrealized rows on both sides of the band
  disposes and creates no row widget, and that then removing 64 of the
  remaining rows from the end disposes at most 64 row widgets, creates none,
  and costs within a factor of 2 of 64/205 of a full clear. It records,
  without asserting, what happens when the band is removed from the end
  while unrealized rows remain before it (the case D4's order avoids).
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

- **Row height parity.** A fixed line height would differ from `GtkLabel`
  whenever a title needs a fallback font (CJK 21 px, Arabic 29 px against
  Latin 19 px on this host). D2 therefore uses the cached height only for
  texts the primary font covers and measures the others exactly. A workspace
  whose titles are mostly in another script gains less (those rows measure
  as on `main`), but loses nothing. The visual proof covers Latin, CJK,
  Arabic and emoji titles.
- **Width parity.** A natural width equal to the minimum changes how a box
  shares its width between siblings. D2's width-parity rule keeps the
  palette's name/subtitle split exact, at the cost of one cached shaping per
  palette row text; the visual proof checks text origins and ellipsis
  against `main`.
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

## Spec review (2026-11-12)

An independent review checked the artifacts against the code and changed
them in place. Decisions, with their rationale:

- **Height parity by primary-font coverage (D2).** The proposed fixed line
  height contradicted the "rows look the same" scenario: Pango on this host
  gives a CJK title 21 px, Devanagari 20 px and Arabic 29 px against Latin's
  19 px (Adwaita Sans 11; `spec-review/line-height-probe.txt`). A fixed
  height would shrink those rows and draw their glyphs over the subtitle.
  User experience first: texts the primary font covers keep the cached
  height (no shaping), the others are measured exactly.
- **Width parity in the palette (D2, D3).** The palette row's name
  (`hexpand`) and subtitle (`halign: end`) share the row by their natural
  widths (GtkBox gives the space beyond the minimums toward each child's
  natural size first). With natural = minimum the subtitle would collapse to
  its ellipsis. Both palette texts report their shaped width, cached per text;
  task 1.5 measures what the palette still gains. The notes rows need no
  such mode: their texts sit in a vertical box that is the row's only
  expanding child.
- **Band-first retirement and a live list (D4).** Removing 131,072 rows from
  the end in 16-row units would take thousands of emissions, and removing a
  scrolled list's band from the end can move GTK's anchor and realize earlier
  rows. The rows GTK never realized go in one emission each first. The
  retirement also owns the list view: the `closed` handler drops the state
  that keeps the dialog alive, and finalizing the list would tear every row
  down in one dispatch.
- **Turn bound (D4, spec).** `turn_continues` lets a turn finish the unit it
  started (Kani T2), so a turn is bounded by the slice plus one unit, not by
  the slice; the unit is sized so that fits `B_INTERACT_US`.
- **Staged growth stops at the band (D5).** Growing every row by `R_STEP`
  would take about a thousand frames for 16,384 rows; only the rows GTK
  realizes cost anything, so growth stops at `R_BAND` and lists the rest at
  once.
- **Header role (spec).** The notes section header has the `heading` role on
  `main`, not `label`, and keeps it.
- **Test helpers (task 2.6).** `labels_in` and `first_label` find row text by
  `GtkLabel`; without an update a negative assertion would pass vacuously.

## Implementation record (2026-11-12/13)

Same host, toolkit and build as the baseline; branch tree after D2–D5. Every
run behind `scripts/measure.sh` with the 1-minute load below 4 for 30 s,
recorded before and after in `.claude/handoffs/lighten-notes-browser-rows/runs.log`
(0.29–1.25 during the verdict campaign). Evidence: that directory's `impl/`.

### Tasks 1.1, 1.4 and 1.5

- **Palette baseline phases (1.1)**, `main`, 131,072 files, from the red and
  D5 rounds (cairo and GL alike): the first category publish 7.1–7.8 ms; X1
  13.7–16.7 ms of layout and 1.7–2.5 ms of paint; Escape 0.4–1.0 ms; every
  dispatch after the leave until the sources are released, worst 5.1–12.8 ms,
  which is the deferred clear's teardown and the frame that shows the grouped
  view again.
- **Per-row teardown (1.4).** A110's probe removes 64 rows of the realized band
  in 2.3 ms (37 µs a row) against 8.7 ms for a full clear of 206 (42 µs); a
  notes row with its widgets costs about 70 µs in the app. The unit is
  `LIST_RETIREMENT_UNIT_ROWS` = 16 (a cap, rounded down to a power of two):
  about 1.1 ms, so `LIST_TURN_SLICE_US` (4 ms) plus one unit fits
  `B_INTERACT_US` with margin. Measured turns: notes 1.2–1.5 ms, palette
  3.9–5.1 ms.
- **Width parity (1.5).** Measured in the app rather than the isolated probe,
  because the palette row's widths depend on the real `GtkBox` and CSS: with
  `natural-width-from-text` on the name and subtitle and D2–D3 alone, X1's
  layout stayed at 14.0–14.9 ms (red rounds, cairo and GL), because the
  frame after a publish into an empty view still realizes about 205 rows and
  shapes each text once for its natural width. That is what decided D5 for
  the palette as well (below). Width parity itself is proved by
  `row_text::test_palette_row_text_splits_the_row_like_labels` at several row widths and
  by the cross-ref pixel comparison of task 4.3.

### Task 1.3 to 5: what differs from the design

- **D2.** `LushtextRowText` reads its CSS text attributes from a label probe
  per class set and caches font metrics per primary font
  (`FONT_METRICS_CACHE_ENTRIES` = 64, a cap). Coverage (`pango_font_has_char`)
  is checked on every text change, so a query publish that rebinds about 205
  rows grew from 2.2–2.8 to 5.6–5.7 ms (about 17 µs a row), within
  `B_INTERACT_US`, while the frame after it lost 18–19 ms of layout. The
  committed bound for that dispatch moved from a 6 ms constant to
  `B_INTERACT_US` with this figure recorded.
- **D4.** The retirement's first turn removes every range nobody holds, not
  only the two around the band: `list_budget::unheld_ranges` returns the
  ranges after, between and before the held rows (the band, the focus row,
  the selection), last first, so one emission per range tears nothing down
  (A110). Its turns run from a `Priority::LOW` idle after the list is
  unmapped.
- **D5 decided by rule (task 6).** After D2–D3 the GL first frame that shows
  16,384 rows still had 23.7–25.7 ms of layout (4 interleaved rounds,
  `d5-*` logs), over `B_SETTLE_US`, so the staged first publish was
  implemented, for the notes browser and for the palette's category view.
  `R_FIRST` is 32, not the 64 the design named: the tallest viewport shows
  about 20 rows, and 64 rows measured 10.3–10.6 ms of layout, which with a GL
  frame's paint leaves no margin. `R_STEP` = 32 (cap) and `R_BAND` = 256
  (coverage window), all in `docs/thresholds.md`. Growth stops at `R_BAND`
  and lists the rest in one emission, so 16,384 rows are listed in at most
  eight steps. Any key, a far selection or an index past the listed rows
  finishes the growth first; the evidence and "Showing X of Y" report the
  final length from the first publish.
- **Kani (D9, task 7).** `list_budget.rs` gained pure functions, and three
  invariants cross a budget, so the design's "no new harness" did not hold:
  G1 (growth reaches every row in at most eight steps), R1 (the unheld ranges
  keep every held row and with them cover the list) and R2 (a unit removes at
  least one and at most `LIST_RETIREMENT_UNIT_ROWS` rows), all in
  `core-list-budget` and in `ORACLES`. R1 first ran out of memory (14 GiB) on
  the standard library's sort and a growing `Vec` over symbolic input; it now
  proves the allocation-free core `for_each_unheld_range` over zero to two
  strictly ascending held rows and every `u32` length, and the property tests
  cover `unheld_ranges`' filter, sort and dedup. The shard: 12 of 12, 16 s,
  1.5 GiB peak locally (R1 4.7 s). Its dispatched CI budget is the
  maintainer's.

### Measured verdicts (task 8.1)

4 interleaved `main`/branch rounds, optimized widget build, the in-dialog
phase harness (rounds 2–3 of each process, so GL is steady state) for the
notes browser, the committed bound test for the palette. Min–max.

| Case | `main` GL | branch GL | `main` cairo | branch cairo |
|---|---|---|---|---|
| Notes 16,384: first frame with rows, layout | 36.6–37.8 ms | 5.0–5.4 ms | 36.3–37.8 ms | 5.1–5.2 ms |
| Notes 16,384: first frame, whole | 38.6–43.6 ms | 7.3–11.2 ms | 57.4–64.5 ms | 23.7–26.5 ms |
| Notes 16,384: open-animation and growth frames, worst | 5.4 ms (one warm-up 83.6) | 6.9–10.7 ms | 27.7–29.1 ms | 32.7–35.2 ms |
| Notes 16,384: query frame, layout / whole | 18.6–18.9 / 20.8–21.3 ms | 0.14–0.18 / 3.0–4.9 ms | 18.7–19.6 / 20.4–21.4 ms | 0.14–0.21 / 2.6–2.9 ms |
| Notes 16,384: close input dispatch (`close()`) | 8.8–15.5 ms | 0.6–1.5 ms | 8.5–15.4 ms | 0.6–1.4 ms |
| Notes 16,384: frame ending the close animation, update+layout | ≤ 1.1 ms | 6.9–10.0 ms | ≤ 0.8 ms | 6.5–7.2 ms |
| Notes 16,384: retirement turns (bound test) | — | 1.36–1.38 ms | — | 1.43–1.46 ms |
| Notes 8: first frame layout | 3.7–5.0 ms | 3.1–3.9 ms | | |
| Palette 131,072: first category publish | 7.1–7.2 ms | 1.05–1.09 ms | 7.1–7.2 ms | 1.07–1.10 ms |
| Palette: X1 layout / whole | 13.7–14.1 / 15.9–16.2 ms | 3.5–4.0 / 5.7–6.1 ms | 13.8–14.5 / 15.5–16.3 ms | 6.6–8.0 / 6.6–8.1 ms |
| Palette: Escape input | 0.4–1.0 ms | 0.8–1.0 ms | 0.9–1.0 ms | 0.5–0.9 ms |
| Palette: retirement turns | — (deferred clear) | 4.6–5.1 ms | — | 3.9–4.2 ms |
| Palette: dispatches after the leave, worst / layout | 5.1–10.3 / ≤ 9.6 ms | 5.6–7.2 / 4.6–6.4 ms | 10.0–12.5 / 8.8–11.4 ms | 8.4–13.2 / 7.3–12.1 ms |

`main`'s palette bound test fails every run (its X1 and leave are over the
new assertions), as recorded red in task 2.3; the branch's passes. The
committed notes bound test passes under both renderers on the final tree.

### Exceptions after the change (task 8.3)

- **E1, the first frame: closed.** Its layout is 5.0–5.4 ms (was 36–38) and
  the whole GL frame 7.3–11.2 ms, within `B_SETTLE_US`. The cairo frame's
  remaining 18–21 ms is the test renderer's paint (A111). Bound: the layout
  phase against `B_SETTLE_US`.
- **E2, the query frame: closed.** 0.14–0.21 ms of layout, 2.6–4.9 ms whole.
  Its text measurement moved into the publish dispatch (5.6–5.7 ms, within
  `B_INTERACT_US`). Bound: `B_INTERACT_US` for both.
- **E3, closing: closed.** The input dispatch is 0.6–1.5 ms (was 8.5–15.5).
  **New, within budget:** the frame that ends the close animation spends
  6.5–10 ms in its update and layout phases. A profile of the branch
  (`impl/perf-d5.*`) shows libadwaita's animation completion calling
  `gtk_widget_unparent` on the dialog, which recurses `gtk_widget_unroot`
  through `gtk_list_view_unroot` into the about 205 rows the retirement keeps
  until the list is unmapped (A110's premise); on `main` those rows were
  already destroyed inside `close()`. The frame stays within `B_SETTLE_US`,
  and no row on screen changes. Retiring the rows before the dialog unparents
  would remove visible rows from a fading dialog, so this is accepted and
  recorded; A1 bounds it by the realized band.
- **X1: closed under GL** (3.5–4.0 ms of layout, `B_INTERACT_US` met). Under
  the harness's cairo it is 6.6–8.0 ms, at that target's edge, so the
  committed bound stays `B_SETTLE_US` with these figures in the test.
- **The deferred clear: closed.** Replaced by the retirement, whose turns are
  3.9–5.1 ms (`turn_continues` lets a turn finish its unit, Kani T2).
- **The grouped view shown again after a leave** (DP's grouped-publish
  figure): 5.6–7.2 ms worst under GL, 8.4–13.2 ms under cairo, its layout
  4.6–12.1 ms; within `B_SETTLE_US`, asserted on its layout phase. It is the
  grouped list's own rows (A1, A90), outside this change's rows.
- **Cairo animation paint (A111).** Open- and close-animation frames cost
  16–35 ms of paint under cairo with 8 rows as with 16,384, 3–11 ms under GL;
  a property of the test renderer, never asserted. On the branch the growth
  frames add their 32 rows of layout to it (32.7–35.2 ms worst under cairo,
  6.9–10.7 ms under GL).

### Visual parity (task 4.3)

A new visual-geometry scenario type would prove same-session invariants, but
this change's claim is cross-build: the rows must look as they did on `main`.
`scripts/compare-blueprint-visuals.sh --baseline-ref` captures a baseline ref
and the branch through the same visual-smoke state matrix (notes empty, few,
dense, constrained; palette files, commands, notes, dense files, category,
category dark, no results) and compares the screenshots pixel for pixel, so
it is the proof used, with its report in the handoff directory. Height parity
for CJK, Arabic, Devanagari and emoji titles, and width parity at several
row widths, are widget tests (`row_text.rs`), because no fixture of the smoke
matrix carries those scripts.

## Migration Plan

None: no persisted format, setting, action or automation field changes
meaning. `retiring_rows` is additive in the notes evidence. If it is projected
into the automation snapshot, `docs/automation*.md` are updated and
`make check-automation-docs` runs.

## Open Questions (answered by tasks)

- The retirement unit and its measured per-unit cost (task 1.4).
- Whether D5 is needed: the GL first-frame figure after D2–D3 (task 6).
- What the palette row gains with width parity (task 1.5).
- Which call path tears the rows down inside the browser's close input
  dispatch (task 1.3).
