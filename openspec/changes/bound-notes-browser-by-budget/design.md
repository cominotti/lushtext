## Context

`Browse Notes…` (`Ctrl+Alt+A`) and `Browse Bookmarks…` are two modes of one
workflow, `WFR-NOTES-BOOKMARKS` (`ui/window/notes/`). Its two browser stage
orders are:

1. **Source** (`source_execution.rs`). On the GTK thread, the browser
   snapshots open-editor note and bookmark metadata
   (`open_editor_note_snapshots_bounded`). A worker then builds one immutable
   source of `PaletteNoteEntry` rows (`services/palette/notes.rs`,
   `NoteSourceAdmission`). That source is bounded by an entry count, a
   searchable-text budget, a retained-graph budget, sidecar-scan count and
   path-byte budgets, a construction-scratch budget and a diagnostic count.
2. **Query** (`query_execution.rs`). A 150 ms debounce, then a worker match
   over the source (`query_notes_browser_source`) that keeps at most
   `NOTES_BROWSER_RENDER_LIMIT` (512) indexes. On GTK it then runs
   `publish_notes_browser_query`: `sidebar.remove_all()`,
   `append_notes_sidebar_sections` (one `AdwSidebarSection` per category,
   one `AdwSidebarItem` per row, with a suffix `GtkButton` for a bookmark),
   a selection restore and the first preview.

`browser.rs` is a called presentation surface that holds the dialog state.
It places the `AdwSidebar` inside a `GtkScrolledWindow` of its own.

This change is item 1's first queued sub-change in
`docs/next/long-line-documents.md` (BL D1). Its brief: make the browser
incremental and virtualized; bound it by retained bytes, reusing
`model/list_budget.rs`; drop count caps such as `NOTES_BROWSER_RENDER_LIMIT`;
say "showing X of Y" only if a byte budget cuts; and discharge the notes
rebuild blocker. It also says to probe `AdwSidebar` first.

## Is it worth doing as described? (spec-stage decision)

**Yes, refined.** Evidence:

- **The blocker is real and structural.** PT M6 (`.claude/handoffs/power-of-two-thresholds/amend/widget/evidence.txt`;
  optimized widget build, presented, interleaved, load 1.99–3.67 at start)
  measured the worst dispatch from `show-notes` until the sidebar had its
  rows at 32.1–37.7 ms on `main` (500 rows), and 34.3–39.9 ms at 512 rows.
  The open action itself took 6.9–19.3 ms and 9.4–13.4 ms. `B_SETTLE_US` is
  16 ms. Because the cost is linear in rows, no count cap of any useful size
  fits the budget.
- **`AdwSidebar` does build off-screen rows.** In libadwaita 1.9.3
  (`src/adw-sidebar.c`, fetched to
  `.claude/handoffs/bound-notes-browser-by-budget/src/adw-sidebar.c`), sidebar
  mode creates a `GtkListBox` (line 1964). It binds the box to the filtered,
  flattened item model with `gtk_list_box_bind_model(…, create_row, …)`
  (line 1973). `create_row` builds a `GtkListBoxRow`, a box, an icon, title
  and subtitle labels, and property bindings for every item (lines
  1334–1400). `GtkListBox` has no recycling, so the answer to the brief's
  question is yes. It also makes the brief's conditional branch, replacing
  the sidebar with a `GtkListView` with section headers, the decided path.
  The axiom probe (A92, task 1.3) pins it on the pinned runtime, with a
  sample. The browser's own outer `GtkScrolledWindow` adds to this: the
  `AdwSidebar` scroller propagates its natural height, so even a
  virtualized inner list would get its whole content as its viewport
  (A1's failure mode).
- **The user-visible loss is real.** At 513 matching rows the user cannot
  scroll further and is told to refine the search. For a large workspace
  with many bookmarks, which is the maintainer's stated target, that is the
  normal case.

**Refinements to the brief, with rationale:**

1. **"Bounded by retained bytes" is already true of what the browser
   retains. The counts are what hide rows.** The rendered list needs
   nothing beyond the source it indexes. The source's retained graph is
   already byte-bounded by `MAX_PALETTE_NOTE_RETAINED_BYTES` (64 MiB) and
   the other ceilings listed in Context. So the change removes the
   browser's counts and keeps those byte ceilings (D3). It does not add a
   result budget that could never cut: a list of `u32` indexes into a
   bounded source is bounded by that source.
2. **The browser source moves from a prefix latch to skip-and-continue.**
   *Corrected at spec review:* `main` does **not** skip and continue. Every
   refusal in `load_note_entries_bounded_for_scope`
   (`services/palette/notes.rs`) ends construction: a refused `admit`
   breaks the sidecar loop and returns `admission.complete()`. Because
   construction runs bookmarks, then folder notes, then document notes,
   then open tabs, a text or retained-byte cut inside the bookmarks hides
   **every** folder note, document note and open-tab row on `main`. BL
   latched its budgets to a prefix because search order is what the user
   navigates (F4) and what Replace All acts on. The notes source is
   regrouped by category before display, so its admission order is never
   user-visible, and a latch here only starves later categories. The
   browser therefore skips a refused row and keeps admitting later rows that
   fit. That retains a superset of `main`'s latch (D3) and is what lets
   construction count the rows it does not retain (D5).
3. **`model/list_budget.rs` is reused where its contract fits: GTK-side
   work.** The open-editor snapshot is the browser's only GTK work that
   grows with data once the list is virtualized. It is captured on GTK
   today, in one dispatch, under a 16,384-row count. Lifting the count
   needs time slicing (`turn_continues`) and a retained-byte admission
   (`RetainedBudget`). The capture stops at its first refusal on `main`
   already, so the prefix latch fits there with no loss (D4).
4. **"Incremental" means rows are created on demand, not spliced in
   batches.** BL spliced batches because its rows come from a streaming
   worker. Here the query result is complete when it reaches GTK. A model
   whose items are created lazily makes publication O(1) on GTK. That is
   strictly better than time-sliced splicing, and it keeps the list
   complete at the first frame instead of growing under the user (D2).

## Goals / Non-Goals

**Goals:**

- Every row the source admitted and the query matched is listed and
  reachable by scrolling and by keyboard, in both modes.
- No dispatch of the browser exceeds `B_SETTLE_US`, and the keystroke input
  dispatch fits `B_INTERACT_US`, at 512, 16,384 and 131,072 rows.
- A byte cut is stated as "Showing X of Y" and announced once. Nothing is
  silently incomplete.
- Keyboard, accessibility, automation, generation-guard and activation
  parity with `main`.

**Non-Goals:**

- The command palette's note counts (`MAX_PALETTE_NOTE_ENTRIES`) and its
  per-category cut. Those belong to `drill-down-palette-categories`.
- The local-history browser's `AdwSidebar`. Its snapshot count is bounded
  by retention (64 per file), so it has no blocker. It is recorded as a
  candidate, not changed.
- Raising the byte ceilings themselves (64 MiB text, 64 MiB retained
  graph, 8 MiB sidecar paths, 96 MiB construction scratch, 4 MiB snapshot).
  They keep their PT values.
- `NOTES_BROWSER_DIAGNOSTIC_LIMIT` (1,024 recovery diagnostics). Past it the
  browser already says that more sidecars failed, so it hides no note.

## Measured baseline

| Measure | `main` | Source |
|---|---|---|
| Worst dispatch, `show-notes` until the rows are shown (500 rows) | 32.1 / 36.9 / 37.7 ms | PT M6, interleaved, load < 4 |
| Same at 512 rows (PT branch) | 34.3 / 39.1 / 39.9 ms | PT M6 |
| `show-notes` action dispatch (snapshot of 17,000 bookmarks, capped at 10,000 then 16,384) | 6.9–19.3 ms; 9.4–13.4 ms | PT M6 |
| Rows reachable | 512 of any larger match set | `NOTES_BROWSER_RENDER_LIMIT` |
| Source rows | ≤ 16,384 | `NOTES_BROWSER_SOURCE_ENTRY_LIMIT` |

**Not re-measured at this stage.** Host load was 22–32 (1-minute) for the
whole spec stage (`/proc/loadavg` at 2026-11-07, recorded in
`.claude/handoffs/bound-notes-browser-by-budget/spec-stage.md`). The rule is
load below 4. PT M6 is a recent, interleaved, load-qualified measurement of
the same code path: `main` has not changed `ui/window/notes/` since. M6's
worst dispatch is not attributed: it spans the dialog build, the first
`AdwSidebar` population and the first preview. Task 1.1 attributes it on
`main`, interleaved and at load below 4, before any fix. Task 1.1 also
measures 16,384 and 131,072 rows on `main` with the render cap raised in a
measurement-only patch, so the growth is known rather than assumed.

## Decisions

### D1. Replace `AdwSidebar` with a `GtkListView` with section headers

The browser rail becomes this widget stack: a `GtkScrolledWindow` holding a
`GtkListView` (`navigation-sidebar` style class, `tab-behavior` item), over
a `GtkSingleSelection` (`autoselect` off, `can-unselect` off), over a
`NotesBrowserRowModel`.

- **Factories.** A `GtkSignalListItemFactory` builds each row the way
  `AdwSidebar`'s `create_row` does: an icon (presentation role), a title
  label, a dimmed subtitle label and the bookmark Open suffix button. The
  layout, CSS classes, tooltip and ellipsizing are the same. A
  `header-factory` builds the section title label, styled as
  `AdwSidebar`'s header (`.heading`, dimmed, the same margins).
- **The outer `GtkScrolledWindow` is the list's own scroller**, so the list
  gets a real viewport (A1). The second scroller goes away.
- **The placeholder** (`AdwSidebar::set_placeholder`) becomes a `GtkStack`
  with two pages, the list and an empty-state status label. The label keeps
  its texts and its AT-SPI anchors (`status bar` "No notes match that
  search" and "No notes yet").
- **Behaviour carried over unchanged:**
  - Selection drives the preview (`selection-changed`, not activation).
  - `activate` (Enter, or the click that `AdwSidebar` reported as
    `activated`) selects and previews, and never opens.
  - The Open button and the bookmark suffix button open.
  - `win.select-notes-browser-row` takes an index into the flat visible
    order, headers excluded, exactly as `filtered_indices` defines it today.
  - The list's accessible role (`list`) and label ("Notes results") are
    unchanged.

*Alternative rejected:* keep `AdwSidebar` and page it, for example 512 rows
with a "more" row. That is a count cap with extra steps. It also keeps the
linear rebuild per page and the user-visible stop.
*Alternative rejected:* a `GtkListBox` with lazy rows. `GtkListBox` cannot
recycle rows.
*Alternative rejected:* `GtkFlattenListModel` over four `GListStore`s of
row objects. It is simpler, but creating one GObject per row on GTK is
linear in rows (A87 bounds the signal cost, not the object creation), and
at 131,072 rows that alone exceeds the budget. D2's lazy model has no
per-row GTK cost.

### D2. A lazy list and section model (`ui/window/notes/row_model.rs`)

`NotesBrowserRowModel` is a GObject subclass that implements `GListModel`
and `GtkSectionModel` (`gtk4::subclass::section_model`, available under the
pinned `gnome_50` feature). It holds one `NotesBrowserProjection`:

- `rows: Arc<[u32]>`, the source indexes in grouped visual order;
- `sections: [u32; 5]`, the start of each `PaletteNoteCategory` section
  plus the end. Empty sections have zero length, and the header factory
  never sees them;
- the source generation and the mode. The model holds **no owning
  reference** to the source payload. The browser state keeps the source in
  its `DisposalOwned` guard, so its final drop stays on a disposal worker
  and never on GTK (CLAUDE.md, "Plain-data disposal ownership"). The row
  object carries only the source index; the item factory's `bind` reads the
  entry through the browser state and checks the generation.

`item(position)` returns a small `NotesBrowserRow` GObject carrying the
source index. Row objects are created only when GTK asks for them. GTK asks
for the realized range (about 205 rows, A1), the selected item and an
activated item, and the probe pins this (A93). If A93 shows that GTK
requires stable item identity, the model keeps a cache of row objects keyed
by position, bounded at twice the A1 cap and cleared on publish. That is a
fallback, decided by the probe and not up front.
`section(position)` searches the five bounds, so it is O(1).

**Publish** swaps the projection and emits one
`items-changed(0, old_len, new_len)` and one `sections-changed`. A87
(presented list: a fixed cost per `items-changed`, not per item) and A93
(no item walk on publish) together make it O(1) in rows on GTK.

**Grouping moves to the worker.** The bounded source is already assembled
in category order (`NoteSourceAdmission::complete` appends bookmarks, folder
notes, document notes and open tabs, the order of
`PaletteNoteCategory::ALL`), and the match walks the source in index order.
The matches are therefore already grouped, and the current GTK-side
`append_notes_sidebar_sections` filter (four passes over the matches, on
GTK) only re-derives that order. `query_notes_browser_source` returns the
matching indexes plus the section bounds, which it records in the same pass
when the category changes. The section-bound rule is pure policy in
`notes/policy.rs` (`group_note_rows`), so the worker and the tests share
it. A property test pins that its output equals the old four-pass grouping
for random sources, so a future change to the source order cannot
silently break the sections.

**Selection restore.** A publish keeps the previously selected source
index if it is still listed. The position lookup is a linear scan over a
`u32` slice (about 0.1 ms at 131,072 rows; task 1.1 measures it). If that
lookup is over 1 ms at the largest case, the worker returns the position
instead: the request carries the selected source index, and GTK still
checks the generation.

### D3. No count caps in the browser source; byte ceilings unchanged

`NoteSourceLimits.entries` and `.sidecar_entries` become `Option<usize>`.
The palette passes `Some(16,384)`. The browser passes `None`.
`NoteSourceLimits` also gains `on_byte_refusal: ByteRefusal`, either
`Stop` (today's latch) or `SkipAndCount`. The palette passes `Stop`, so its
admitted rows, worker cost and tests are unchanged; the palette's own cut is
`drill-down-palette-categories`' concern. The browser passes
`SkipAndCount`. A count refusal (palette only) always stops, because every
later row would be refused too. The byte
ceilings (searchable text, retained graph, sidecar path bytes, construction
scratch) and the diagnostic count stay mandatory for every consumer, so no
consumer can request an unbounded source. `notes_browser_source_limits_for_entries`
is replaced by a test-only byte override (`test_policy.rs`).

**No loss against `main`, by construction.** `main`'s rule is a prefix
latch: rows are admitted in order while each fits the count and every
remaining byte ceiling, and the first refusal ends construction. The
browser's rule drops the count and replaces the latch with skip and
continue: a row is admitted if it fits every remaining byte ceiling, and a
refused row does not stop later rows. Up to `main`'s first refusal both
rules admit the same rows with the same charges, and the browser's rule
only adds rows after it, so it admits a superset. A unit and property test
states this: for random row sequences, charges and limits, the browser's
rule admits a superset of `main`'s latch, and a failing-first widget test
shows `main` hiding every folder note behind a text cut in the bookmarks.
Removing a count or the latch cannot break any byte bound: the retained
graph, text, path and scratch ledgers are the same code, and each refusal
releases its charge as today.

**Counting what is not retained.** `NoteSourceMetrics` gains `rows_found`,
every row the construction parsed, admitted or not, and
`found_is_lower_bound`.

- When a byte ceiling refuses a row, construction keeps parsing the
  remaining sidecars for the count. That costs worker CPU and transient
  parse memory, which the construction ceiling bounds. It does not retain
  the rows.
- When the sidecar path-byte or construction ceiling stops the scan itself,
  `found_is_lower_bound` is set and the label says "at least".
- Cancellation (dialog close, mode switch) stops the count, as it stops
  construction today.

**What leaves the inventory:**

- `NOTES_BROWSER_RENDER_LIMIT`, `NOTES_BROWSER_SOURCE_ENTRY_LIMIT`,
  `NOTES_BROWSER_SIDECAR_SCAN_LIMIT` and
  `NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT`, with their
  `docs/thresholds.md` rows and their pinned unit assertions;
- `notes_browser_limit_messages`'s "Showing first N matches" branch.

`make check-thresholds` must fail first on a leftover row (task 5.1).

### D4. The open-editor snapshot: time-sliced, byte-bounded, no count

`open_editor_note_snapshots_bounded` runs in one GTK dispatch today. It
stops at 16,384 snapshots plus bookmarks or at 4 MiB
(`NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT`), whichever comes first.
For the browser it becomes a sliced capture in `source_execution.rs`:

- **The unit** is up to `OPEN_EDITOR_SNAPSHOT_SLICE_ROWS` (256, an internal
  cap rounded down) bookmark records of one editor, or one editor's path
  snapshot.
- **The turn** takes units while `turn_continues(elapsed_us, units_done)`
  holds (`LIST_TURN_SLICE_US`, 4 ms; Kani T1/T2). It runs from a
  `G_PRIORITY_DEFAULT_IDLE` source owned by the browser state and removed
  on dispose.
- **The bytes** are admitted through one `RetainedBudget` at the unchanged
  4 MiB. `main` already stops at its first refusal, so the latch matches
  `main` and loses nothing (B3: at `main`'s worst per-row charge, the budget
  retains at least `main`'s 16,384 rows wherever `main` did, because `main`
  applied the same 4 MiB).
- **Consistency.** Each editor records `bookmark_change_generation()` at its
  first slice. If the generation differs at any later slice, that editor's
  captured bookmarks are dropped (their charge returned) and the editor is
  captured again from its first slice. There is no whole-editor one-turn
  fallback: it would put an unbounded unit on GTK and break D6's budget.
  *Corrected at spec review:* the stage-1 draft captured an editor whole
  after two restarts. Restarting until consistent terminates in practice
  because the browser is an `AdwDialog`, which blocks input to its window,
  so the user cannot edit bookmarks during the capture. The generation then
  moves only on an async bookmark-sidecar apply
  (`load_bookmarks_if_generation_matches`), at most once per load, or by
  automation. The evidence counts restarts (D8), and a widget test drives a
  change between slices. Live bookmark state (the "freshly changed
  open-editor bookmarks" scenario) is therefore never torn.
- **Sorting.** `bookmark_records_bounded_by_retained_bytes` sorts each
  editor's records by line on GTK today. The sliced capture leaves them in
  capture order and the worker sorts them, so no GTK step is
  `O(n log n)` in one editor's bookmarks.
- **Counting past the cut.** After the byte budget latches, the capture
  keeps walking the remaining editors and bookmarks in the same sliced
  turns. It resolves each mark's line and counts it, but clones nothing, so
  D5's Y includes the open-editor rows the snapshot did not retain. That
  walk is GTK work linear in bookmarks, so it is sliced like the capture.
- **Order with the dialog.** The dialog is presented first, showing the
  existing loading label. The capture then runs in turns. The worker source
  request is submitted when the capture completes. The source generation
  guards it, and a mode switch or close cancels the capture.

The command palette keeps its one-dispatch capture with its own count
(`MAX_PALETTE_NOTE_ENTRIES`). Its source is not shown in a list that can
grow past the palette's per-category cut, so that is
`drill-down-palette-categories`' concern. The capture function takes a
`CapturePlan` (`OneTurn { max_rows }` for the palette, `Sliced` for the
browser), so the two consumers share the per-editor code.

### D5. "Showing X of Y", once

`notes/policy.rs` owns the wording, which is pure and unit-tested:

- Browse Notes: "Showing {X} of {Y} notes and bookmarks. The rest exceed
  the memory budget." ("of at least {Y}" when the count is a lower bound).
- Browse Bookmarks: the same, with "bookmarks".
- With a query, the label keeps stating the source cut. Matching runs over
  the admitted rows only, and the label says so: "Searching {X} of {Y}
  notes and bookmarks."
- An open-editor snapshot cut is folded into the same X and Y. Its rows
  that were counted but not retained (D4, "Counting past the cut") count
  toward Y.

The label is visible and its accessible label carries the same text. One
polite announcement is raised when a source generation publishes as
limited. It is not raised again for queries, selections or re-presents of
the same generation, and the evidence counts announcements so a test can pin
this. A mode switch is a new source generation and may announce once again.
The existing typed truncation reasons stay in the evidence and in recovery
diagnostics. The user-facing text no longer depends on which byte ceiling
bit.

### D6. Budgets and verdicts

The following dispatches are measured interleaved with `main`, presented,
in an optimized widget build at load below 4. Each must fit `B_SETTLE_US`:

- every dispatch from `show-notes` until the first rows and the first
  preview are shown;
- each query publish;
- each snapshot turn;
- each dispatch of dialog close and retirement.

The search field's key-press dispatch must fit `B_INTERACT_US`. The cases
are 512, 16,384 and 131,072 source rows, plus 20,000 open-editor bookmarks
in one editor. The rule for a residual: shrink it (stage the dialog build,
for example present the shell and mount the preview after the first frame,
as window composition does), or record it as a toolkit-floor exception with
its own axiom and figures. A residual is never left unexplained. The
verdicts become enforced widget assertions with a 2× margin over the
measured worst at the reference case (the BL task 8.2 pattern), and the
table goes into this design.

### D7. GTK axioms (new, with samples) and Kani

These axioms are numbered after A91, each a probe in `gtk-lush-axioms` with
an interactive/`--check` sample and a ledger row:

- **A92.** `AdwSidebar` in sidebar mode creates one row widget per item,
  off-screen items included. `remove_all` plus appending N items costs
  O(N), measured at 512 and 16,384. This pins the source reading above.
- **A93.** A presented `GtkListView` with a `header-factory`, over a
  `GtkSingleSelection` over a custom `GListModel` + `GtkSectionModel`:
  - after `items-changed(0, old, new)`, `get_item` and `get_section` are
    called only for the realized range, and the publish time is flat
    between 512 and 131,072 items;
  - whether a fresh object per `get_item` call is accepted (no identity
    requirement), which decides D2's cache fallback.
- **A94.** Section headers from a `header-factory` take no keyboard focus.
  Arrow-key navigation moves between items across section boundaries, with
  single selection following focus. The header widget's accessible role and
  label are recorded, which decides D9's header labelling.
- **A95.** On that list, `GtkSingleSelection::set_selected(p)` plus
  `GtkListView::scroll_to(p, …)` for p far down realizes only around p.
  Its cost is flat in p.

A1 (the realized-row cap), A10 (mapped is not drawn) and A87 are cited
unchanged. `make gtk-axioms` runs first after the probes land.

**Kani.**

- `model/list_budget.rs` is reused unchanged, so `core-list-budget` needs
  no rerun unless the module changes. If it does change, the shard is
  rerun and its runner budget re-measured.
- No new shard. The new pure decisions are proved by exhaustive unit tests
  and property tests:
  - `group_note_rows`, the section lookup and the label wording are
    small, enumerable functions;
  - the admission superset property is checked over random sequences.

  A Kani shard adds a CI runner budget for no additional assurance here.
  The design records this choice so a reviewer can overturn it.

### D8. Workflow placement

- `notes/policy.rs`: `group_note_rows`, the section lookup, the limited
  label and announcement wording, the snapshot slice constant and the
  capture restart rule. All of it is pure.
- `notes/row_model.rs` (new): a called presentation surface, with no role.
  It holds the GObject model and the row object.
- `notes/browser.rs`: the list view, the factories, the empty-state stack
  and the selection wiring. `append_notes_sidebar_sections`,
  `build_notes_sidebar_item` and `sidebar_item_index` are removed.
- `notes/query_execution.rs`: publish through the model, and the
  limited-state announcement.
- `notes/source_execution.rs`: the sliced capture, as a resumption point
  named in the facade.
- `notes/evidence.rs`: `NotesBrowserRuntimeSnapshot` gains:
  - `listed_rows`, `rows_found`, `found_is_lower_bound` and `limited`;
  - `limit_announcements`;
  - `snapshot_turns`, `snapshot_restarts`, `snapshot_rows_counted_not_retained`
    and the worst snapshot turn (µs, `test-utils` only);
  - `realized_row_widgets`.
- `notes/mod.rs`, the facade, names the new resumption point. It stays
  within the 370-line budget.

The `WFR-NOTES-BOOKMARKS` row in `docs/workflow-readability-matrix.md`
re-derives its measured cells, and `make check-workflow-boundaries` passes.

### D9. Automation and accessibility

- **Snapshot** (`window.notes`, from `NotesEvidence`):
  - `notes.browser_open` (bool);
  - `notes.browser_row_count` (u32, the rows listed);
  - `notes.browser_rows_found` (u32, saturating);
  - `notes.browser_limited` (bool).

  All four are documented in `docs/automation.md` and
  `docs/automation-reference.md`, and checked by `make check-automation-docs`.
- **AT-SPI.** Every `atspi-anchor-notes-browser-*` anchor keeps its role and
  name. The rows' accessible names are their visible titles, as today:
  `atspi-anchor-notes-browser-label-bookmark-line-1` keeps "Bookmark ·
  Line 1". Section headers are exposed as headings (role `heading`, label
  the section title) when A94 shows the header widget's role is settable.
  Otherwise the probe records what GTK exposes, and each row's accessible
  description carries its section name. Either way Orca hears the section.
  The limited label is the existing `limit_label`, whose accessible role
  is `Status` (AT-SPI `status bar`). Its accessible name, today the fixed
  "Notes result limit", now tracks its visible text, so a screen reader
  reads the counts. No existing anchor names it. A new anchor,
  `atspi-anchor-notes-browser-status-bar-showing` (role `status bar`, name
  prefix "Showing"), names it. *Corrected at spec review:* the stage-1
  draft gave it role `label`, which the widget does not have.
- **Smoke.** A new accessibility case, `notes-limited`, reaches a byte cut
  through `LUSHTEXT_NOTES_SOURCE_TEXT_BYTES`. That variable is compiled only
  under the existing `smoke-list-budgets` feature, which release, Meson,
  Flatpak and Snap never enable (BL's pattern). The visual smoke cases
  `notes-few`, `notes-dense` and `notes-constrained` are re-captured, and
  `notes-dense` now shows a scrollable full list.
- **Docs.**
  - `docs/accessibility.md` states the announcement, the headers and the
    keyboard paths.
  - `docs/accessibility-matrix.md` updates `A11Y-NOTES-EMPTY` and
    `A11Y-NOTES-POPULATED`, and the "Notes and bookmark rows" surface row.
  - `docs/accessibility-orca-checklist.md` gains the limited announcement
    and section-header expectation.

### D10. Thresholds

- **Removed rows:** the four counts of D3.
- **Added:** `OPEN_EDITOR_SNAPSHOT_SLICE_ROWS` (256, internal cap, rounded
  down; measured by task 1.1/8.1).
- **Unchanged and re-cited:**
  - `NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT`, now "admitted by
    `RetainedBudget`; no count";
  - `NOTES_BROWSER_SOURCE_TEXT_LIMIT`, now "the browser's only text bound";
  - `MAX_PALETTE_NOTE_RETAINED_BYTES`, `…_SIDECAR_PATH_BYTES` and
    `…_CONSTRUCTION_BYTES`, now noting their second consumer.

`scripts/check-threshold-kinds.py`'s self-test fails first on a leftover
`NOTES_BROWSER_RENDER_LIMIT` row and on a missing slice row.

## Risks / Trade-offs

- **A larger source than `main` can retain.** Without the 16,384 count, a
  workspace of small bookmarks can admit up to what 64 MiB of retained graph
  holds. With a row charge of a few hundred bytes, that is on the order of
  100,000–300,000 rows. Memory stays within the same byte bounds as
  `main`, by construction. Worker time for construction and query grows
  with rows, but it is off GTK, cancellable and superseded. Task 8.3
  measures worker query time at 131,072 rows. If it exceeds 100 ms, the
  query reports progress in the existing loading label and does not block.
- **Counting past a byte cut costs worker I/O.** It is bounded by the
  sidecar path-byte ceiling (the candidate list) and cancelled on close.
- **A custom `GListModel` is new to the codebase.** A93/A94/A95 pin the
  toolkit behaviour it relies on before any production code depends on
  it. The widget suite covers selection, activation, headers, empty states
  and generation guards.
- **Look and feel.** `navigation-sidebar` on a `GtkListView` is GTK's own
  sidebar style, the same class `AdwSidebar` applies to its list box.
  `notes-few` and `notes-constrained` are compared side by side with
  `main`, and any visible difference is fixed or recorded.
- **The sliced capture lets edits interleave.** The generation restart
  keeps each editor consistent. Different editors may be captured at
  slightly different moments, which is indistinguishable from the user
  opening the browser a few milliseconds later.

## Migration

No persisted format changes. The automation snapshot only adds fields. All
existing AT-SPI anchors keep their names. The truncation message text
changes from "Showing first 512 matches…" and the per-reason source
messages to the D5 wording. No public anchor named those strings.

## Spec review (stage 2, 2026-11-07)

Reviewed against `main` at `e4ec2624`. The approach stands. Corrections made
in place, each marked *Corrected at spec review*:

1. **Admission on `main` is a prefix latch, not skip-and-continue.** Every
   refusal in `load_note_entries_bounded_for_scope` returns
   `admission.complete()`, so a byte cut inside the bookmarks hides every
   later category. Refinement 2 and D3 now describe the browser's move to
   skip-and-continue as a change. It is an improvement, and the superset
   claim still holds against the latch. The palette keeps `Stop` through
   a per-consumer `ByteRefusal`, so its behaviour and worker cost are
   unchanged. A failing-first test and a new spec scenario pin it.
2. **No whole-editor one-turn snapshot fallback.** That fallback was an
   unbounded GTK unit and contradicted the budget requirement ("whatever
   the number of rows"). An editor now restarts until it is captured
   consistently. That terminates because the `AdwDialog` blocks input to
   its window. The worker sorts the captured records, which removes an
   `O(n log n)` GTK step.
3. **Y for a snapshot cut was unspecified.** The capture now counts the
   rows past the cut in the same sliced turns, without cloning them.
4. **The row model must not own the source.** The source is
   `DisposalOwned`. If the model kept an owning `Arc`, its last drop would
   land on GTK.
5. **The grouping is already in source order.** The worker records the
   section bounds in the match pass. The property test keeps the old
   four-pass grouping as its reference.
6. **The limited anchor has the wrong role.** The label's role is
   `Status`, so the anchor is now
   `atspi-anchor-notes-browser-status-bar-showing`.
7. **Task 1.1's 131,072-row baseline on `main`** also needs the source and
   sidecar counts raised, because the render cap alone stops at 16,384.

Checked and confirmed:

- the libadwaita 1.9.3 citations (lines 1964, 1973 and 1334);
- the 35 `find_adw_sidebar` call sites;
- the scenario names of every MODIFIED requirement (unchanged);
- the `gtk4` 0.11 `SectionModelImpl` under `gnome_50`;
- the `list_budget` API (`turn_continues`, latching `RetainedBudget`);
- the `smoke-list-budgets` feature and the `core-list-budget` shard;
- that the next free axiom number is A92.

No new measurement was taken. The host's 1-minute load was 19–25 for most
of the review and 1.66 at its end. The baseline is task 1.1's job, and it
runs interleaved before any fix.

## Open questions (answered by tasks, not by the reviewer)

- The attribution of `main`'s 32–38 ms (task 1.1), and whether the dialog
  build alone exceeds `B_SETTLE_US`, which would make D6's staging
  necessary. The rule for the outcome is fixed now.
- A93's identity answer, which decides D2's cache fallback.
- A94's header role, which decides D9's header labelling.
