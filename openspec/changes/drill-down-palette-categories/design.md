## Context

The command palette (`WFR-COMMAND-PALETTE`, `ui/command_palette/`) runs one
query flight per input: the facade snapshots the query, the mode and four
`Arc` source snapshots into a `PaletteQueryRequest`
(`query_execution.rs`), the one-active/one-latest coordinator admits it, a
worker runs `services::palette::grouped_search`, and the completion publishes
the grouped rows with one `splice` into a `gio::ListStore` of `PaletteItem`s
(`imp::publish_search_rows`).

`grouped_search` asks each source for at most `MAX_RESULTS_PER_SOURCE` (64)
results. The selector (`services/palette/fuzzy.rs`,
`search_items_cancellable_controlled`) has two paths:

- a non-empty query scores **every** candidate and keeps the best 64 in a
  worst-first heap, ordered by score then source ordinal
  (`compare_palette_rank`); `PaletteSearchMetrics::matching_candidates`
  already counts every match;
- the empty query takes the first 64 included items in source order and
  **stops**, so how many more exist is unknown.

Sources, per mode: Files has "Open Tabs" and the workspace group (labelled by
scope, e.g. "All Workspaces"; the workspace group excludes files open in tabs
by canonical path); Notes has the four `PaletteNoteCategory` groups; Commands
has one group; All has all of them in that priority. Group headers are
non-activatable `PaletteItem`s; navigation skips them through
`policy::next_activatable` over an activatable-flag vector that `imp` builds by
walking the whole store (`activatable_flags`), on every selection change.

The palette's note source is built with `PALETTE_NOTE_SOURCE_LIMITS`:
`entries: Some(16,384)`, `sidecar_entries: Some(16,384)`, byte ceilings, and
`ByteRefusal::Stop`. BN made those fields optional and gave the browser
`None` and `SkipAndCount`, leaving the palette's values to this change.

The programme record (`docs/next/long-line-documents.md`, PT queue item 1)
queues this change after BN's archive: "the 'N more…' row per category and the
category drill-down mode", plus "the palette's note counts
(`MAX_PALETTE_NOTE_ENTRIES`, its sidecar count and its stop-at-first-refusal
rule)".

## Is it worth doing as described? (spec-stage decision)

**Yes, refined.** Evidence and reasoning:

- **The loss is real and large.** The file index admits up to 131,072 files
  (`MAX_INDEXED_FILES`). The empty query shows 64 of them, and any broad query
  (an extension, a directory name) matches thousands and shows 64. The user
  cannot see that more exist, and only a narrower query reaches them. Notes and
  commands have the same cut. This is the "user-visible loss" the maintainer's
  standard rules out.
- **Raising the grouped cap is the wrong fix.** The grouped list is a
  `gio::ListStore` spliced on every query; its cost grows with its rows (PT
  M1: 5.0–5.7 ms at 50 per category, 6.4–7.9 ms at 64, against 8 ms), and 64
  rows per category is already more than a user scans in a mixed list. A
  per-category view keeps the grouped view as it is and reaches everything.
- **The cost can be made flat in rows**, by the pattern BN measured: a lazy
  `GListModel` publishes in 1.8–2.9 ms at 512 and at 90,687 rows because GTK
  realizes about 205 rows whatever the model holds (A1, A93, A95). The ranking
  itself is cheap off GTK: ranking all 131,072 candidates (heap, sort, ordinal
  array) costs 4.4 ms on the worker (spec-stage microbenchmark below), on top
  of the scoring the grouped query already does.
- **The grouped query must not get slower.** The non-empty count is free
  (`matching_candidates`). The empty-query count is the one new cost: a pass
  over the rest of the index. Hashing every canonical path against the
  open-tab set costs 7.4 ms at 131,072 files; testing the file name first and
  hashing only on a name match costs 1.5–2.1 ms (below). Both are worker time,
  not GTK time, but the palette's first rows wait for it, so the cheaper test
  is the design (D1).

**Refinements to the request, each decided here:**

1. The category view's rows are **source ordinals**, not row strings: 4 bytes a
   row on GTK, row objects only on demand (D2). The request's "retained-byte
   budget" then bounds the worker's ranking peak, which is the larger cost (D3).
2. The category view uses the **grouped view's own selector and comparator**,
   so its first 64 rows equal the grouped rows. A different ranking would
   reorder what the user just saw.
3. The category view is a **second list view**, not a model swap under the
   grouped one. Replacing a selection's model is one replacing
   `items-changed`, which asks GTK for every added item (A93); a separate list
   in its own scroller is revealed at the cost of its realized rows (A90), and
   the grouped list keeps its rows for the way back.
4. **Escape leaves one level.** The window's dismissal ladder asks the palette
   to leave its category view before closing it, so Escape from any focus
   behaves the same.
5. The **note source's counts** go in this change, as BN recorded.
6. **Page Up / Page Down** in both views, so 131,072 rows are reachable from
   the keyboard without 131,072 arrow presses.
7. No row-text walk in the category view: navigation, set size and the
   accessible count come from the model's length, never from materializing
   every item (today's `activatable_flags` would create 131,072 objects).

## Goals / Non-Goals

**Goals:**

- Every result of every category is reachable, by keyboard and pointer.
- The grouped view keeps its rows, order, cap and cost.
- Every dispatch fits `B_INTERACT_US` (input) or `B_SETTLE_US` (publish, frame,
  leave), measured interleaved with `main`; any toolkit floor is a recorded,
  measured exception enforced by a widget assertion.
- Accessibility and automation parity: names, counts, announcements,
  snapshot fields, an action.
- The readability convention: policy pure, one evidence surface, the facade
  within 370 lines, the matrix row re-derived.

**Non-Goals:**

- The file index's 131,072-file ceiling (`scale-file-index`, queued).
- Changing nucleo scoring, note-body pruning, source priority or
  deduplication.
- A lazy model for the grouped view (its 64-row cap is not a loss once the
  category view exists).
- Extracting a shared lazy list model into GTK Lush. The notes browser's model
  is sectioned and this one is flat; the second consumer is recorded as a
  candidate in `docs/next/gtk-lush.md` (D10).

## Measured baseline

| Measure | Value | Source |
|---|---|---|
| Grouped publish, every category full, 64 per category, worst settle | `main` 6.4–7.9 ms (5.0–5.7 ms at 50) | PT M1, interleaved, load < 4 |
| Lazy-model query publish (notes browser), 512 → 90,687 rows | 1.8–2.9 ms, flat | BN verdicts |
| Frame after a query that changes the shown rows (notes browser) | 22–23 ms, toolkit floor E2 | BN verdicts |
| Far-row select and scroll on a lazy list | bounded `get_item` calls, time grows with rows | A95 |
| Empty-query count pass, 131,072 files, 32 / 256 open tabs, full-path hash | 7.44–7.46 / 7.41–7.46 ms (0 tabs: 0 ms) | spec microbenchmark |
| Same, name-prefiltered | 1.51–2.06 / 1.64–2.12 ms (0 tabs: 0.12 ms) | spec microbenchmark |
| Rank all 131,072 candidates (heap, sort, `Box<[u32]>`) | 4.41–4.43 ms | spec microbenchmark |

The spec microbenchmark is a standalone `rustc -O` program over synthetic
paths of the index's shape (best of 15, CPUs 8–23, load 0.36 before and after):
`.claude/handoffs/drill-down-palette-categories/spec-microbench.{rs,log}`. It
bounds the shape of the worker cost, not the application's; task 1 measures
the application, interleaved with `main`, before any fix.

## Decisions

### D1. Categories, the "N more…" row, and exact counts

`model/palette.rs` gains `PaletteCategory`: `OpenTabs`, `WorkspaceFiles`,
`Note(PaletteNoteCategory)`, `Commands`, with a stable automation id each
(`open-tabs`, `workspace-files`, `bookmarks`, `folder-notes`,
`document-notes`, `open-tab-notes`, `commands`) and the label the grouped
header uses in the current mode.

`PaletteSearchRow` gains `More { category, hidden: usize }`. `grouped_search`
appends it after a group's last row when the group's match count exceeds the
rows shown. `PaletteItem` gains a `more` kind (`KIND_MORE`) carrying the
category id and the hidden count; it is activatable, and its activation is
handled inside the palette (it never reaches the window's activate callback).

Counts:

- **Non-empty query:** the selector's per-source `matching_candidates`, which
  every source already computes. `grouped_search` keeps it per group instead
  of merging it away.
- **Empty query:** the selector gains a count continuation. After the 64th
  row it keeps walking the source and counts included items without
  retaining or scoring them, checking cancellation at the same interval
  (`PALETTE_CANCEL_CHECK_INTERVAL`). A source whose predicate is trivially
  true (no open-tab exclusion, commands) counts by length in O(1).
- **Commands cannot be cut today:** the registry holds 35 commands
  (`services/palette/commands.rs`), below the 64-row cap, so the Commands
  group never ends with a "More" row on this revision. The decision is the
  same generic rule for every group; its unit and widget tests force it with
  a smaller per-source limit through `CommandPaletteTestPolicy` instead of
  asserting a Commands "More" row the registry cannot produce (spec review).
- **The workspace group's exclusion test** (`search_cancellable_excluding`)
  hashes the file's canonical path against the open-tab set. The count
  continuation tests the canonical path's file name against the open tabs'
  canonical file names first, and hashes the full path only on a name match. A property test
  pins that the prefiltered predicate equals the full one on random indexes,
  including symlinked aliases whose display name differs from the canonical
  name. The selector's retained-row path keeps the full predicate.

The "N more…" row's visible text is "{N} more…" with N grouped in thousands
(`list_budget::grouped_thousands`); its accessible name is "Show {N} more
results in {category}". The wording lives in `ui/command_palette/policy.rs`.

*Alternative rejected:* a "64+ more" row without a count on the empty query.
It saves 1.5–2.1 ms of worker time and costs the user the number that tells
them whether browsing is worth it, and it makes the accessible name vague.

### D2. The category view: a second list over a lazy model

`resources/ui/command-palette.blp` gains, below the query row, a category
bar (an invisible `Box`: a flat back button "Back to all results"
(`go-previous-symbolic`), a title label "{category} · {X} results", and a
notice label for the budget and the note source) and a second
`GtkScrolledWindow` with `category_view: ListView`, invisible until a category
is opened. The grouped scroller is hidden while the category view is shown,
and shown again on leave. Both scrollers keep `max-content-height: 400` and
`propagate-natural-height`; A96 pins that the lazy list's natural-height
measure does not walk its items.

`ui/command_palette/category_model.rs` (called presentation surface, no role)
is `PaletteCategoryModel`, a GObject implementing `GListModel` over one
`CategoryProjection`:

- `ordinals: Arc<[u32]>`, the ranked source ordinals;
- the category, the generation, and the request's source snapshots (`Arc`s of
  the index, the open tabs and the note entries, as the request already holds
  them), so an ordinal can never outlive the source it indexes. Commands are
  static.

`item(position)` builds the row's `PaletteItem` from the source entry (the
same constructors the grouped rows use). It applies BN's lessons as decided in
BN's stage 3:

- a weak map of handed-out row objects by position: a publish **retargets**
  the live ones below the shorter length in place and emits their
  content-changed notification, which the item factory rebinds on, so a query
  does not tear down and rebuild the realized rows;
- only the tail is removed or added, and every emission matches the length the
  model reports at that moment (the crash BN fixed in
  `gtk_list_item_manager_ensure_items`);
- the map is pruned to live objects past 1,024 entries (its own `cap`
  constant with a thresholds row, D12, the shape of the notes browser's
  `LIVE_ROW_PRUNE_AT`);
- **leaving and closing clear the projection** (a removal of every row, then
  an empty projection), so a hidden category model never keeps a superseded
  index or note source alive while the palette shows grouped results or is
  closed (spec review).

The notes browser's `row_model.rs` module doc still says a publish "swaps
one `Rc<[u32]>` and emits a removal then an addition", which its code no
longer does since BN's stage 3 (kept rows are retargeted, only the tail is
added or removed). Since this change copies that pattern, it corrects that
stale paragraph first (task 5.2), under the pre-existing-blockers rule.

The item factory is the grouped factory's (the same row widget and
accessibility projection), set on both list views, except that the category
view's set size comes from the model's length.

Dropping a projection drops one `Arc<[u32]>` (one free, O(1)) and `Arc`
clones of sources the palette already owns; the sources' final drop is
`DisposalOwned`'s nonblocking handoff, as today. No row-sized payload is
destroyed on GTK.

*Alternative rejected:* swapping the grouped `GtkSingleSelection`'s model.
One replacing `items-changed` asks for every added item (A93: 262,350 calls,
71 ms at 131,072), and the way back would rebuild the grouped rows.
*Alternative rejected:* filling a `gio::ListStore` with every ranked row. One
GObject per row on GTK grows linearly (BN D1).

### D3. Ranking and the retained-byte budget

The category search (`services/palette/category.rs`, GTK-free) runs the same
source call the grouped search runs for that category, with
`max = category_row_limit()` instead of 64, and returns
`CategoryRanking { ordinals: Box<[u32]>, matching: usize, limited: bool }`.
Same selector, same comparator, same filters and exclusion, so the first 64
ordinals are the grouped rows (a property test pins this for every category
and random sources and queries).

The budget is the worker's peak: the selector retains one candidate
(`RetainedCandidate`: the borrowed item, the score, the ordinal) per kept row
until it sorts them, and the ranked candidates and the output `u32` ordinals
coexist while one is converted into the other. So:

- `PALETTE_CATEGORY_RETAINED_BYTES` = 8 MiB (presentation cap), in
  `ui/command_palette/policy.rs`;
- `PALETTE_CATEGORY_CANDIDATE_BYTES` = `size_of::<RetainedCandidate>()` +
  `size_of::<u32>()` (derived; exported from the service as a `const`). On
  x86_64 the candidate is 32 bytes (a 16-byte `SearchResultItem` of four
  reference variants, which no niche can pack, plus `u32` and `usize`), so
  the charge is 36 bytes; a unit test pins the figure the floor below uses;
- `category_row_limit()` = `list_budget::rows_within(limit, charge)`, a new
  pure helper: the largest `n` with `n * charge <= limit` (`charge > 0`). It
  states `RetainedBudget`'s admission for equal charges, so the view's cut is
  the budget's prefix rule. Kani harness B5 in `core-list-budget`:
  `rows_within` agrees with offering `n` items of that charge to a
  `RetainedBudget` (retained count), and never overflows.
- **No loss at today's ceiling:** `PALETTE_CATEGORY_FLOOR_RETENTION` (derived)
  = `MAX_INDEXED_FILES * PALETTE_CATEGORY_CANDIDATE_BYTES`, and a
  `const` assertion pins `FLOOR <= RETAINED_BYTES`. With the 36-byte charge
  the floor is 4,718,592 bytes (4.5 MiB), and 8 MiB is the power of two at or
  above it, which is what `make check-thresholds` requires of a presentation
  cap recorded "up from PALETTE_CATEGORY_FLOOR_RETENTION" (BL's form). A
  second `const` assertion pins `RETAINED_BYTES / 2 < FLOOR`, so a change of
  the candidate's size that would leave 8 MiB more than one power of two
  above the floor (or below it) fails the build instead of the thresholds
  gate. (The spec draft's "40-byte candidate, 5 MiB floor" was not the
  pinned layout; the spec microbenchmark ranked 8-byte `(score, ordinal)`
  pairs, so its 4.4 ms understates the 36-byte ranking task 1.3 measures.)

When `matching > ordinals.len()`, the view's notice says "Showing {X} of {Y}
results" (grouped thousands), announced politely once per category view and
cut change. Today that can happen only for a note category whose source holds
more rows than the budget's row limit, or after `scale-file-index` raises the
index past it.

Ranking all matches costs the sort (4.4 ms at 131,072 in the microbenchmark)
on the worker. The selector already checks cancellation per 256 candidates;
the sort is not interruptible, so a superseded category query can finish its
sort before it observes cancellation. That bound is recorded (task 1.3), and
if a measured sort at 131,072 exceeds `B_SETTLE_US`, the selection is split
into the bounded heap and an interruptible merge.

### D4. One query flight for both views

`PaletteQueryRequest` gains `category: Option<PaletteCategory>`.
`start_query_flight` reads it from the palette's state;
`search_palette_sources` calls `grouped_search` or the category search; the
completion publishes into the matching view only if the generation is current
**and** the request's category equals the palette's current category (a
category entered or left while a worker ran publishes nothing, exactly as a
superseded query does). Debounce, the empty-query bypass, cancellation,
`refresh_searching_state` and the `command-palette-search` readiness blocker
are unchanged; the category view adds no second coordinator and no timer.

A source replacement (`restart_query_if_open`) restarts the flight with the
current category, so the category view follows index rebuilds and note
refreshes. A category with no matches shows the palette's "No results" status
inside the category view.

### D5. Entering and leaving

- **Enter:** activating a "More" row (Enter in the query entry with it
  selected, a double-click, the automation action) sets the category,
  hides the grouped scroller, reveals the category bar and the empty category
  view, announces "{category}: showing all results. Escape returns to all
  results." (polite), and starts a flight with the current query. The first
  publish selects row 0.
- **Leave:** Escape, Backspace on an empty query, the back button, a mode
  change (Tab, Shift+Tab, the dropdown, `win.set-command-palette-mode`) and the
  automation action with `""`. Leaving clears the category, hides the bar and
  the category view, shows the grouped scroller, and starts a grouped flight
  with the current query (and the new mode for a mode change). When the
  grouped rows arrive, the selection goes to that category's "More" row if the
  category is still truncated, else to its last row, else to the first
  activatable row. The palette remembers that one category id, nothing else.
- **Escape routing:** in the entry, `stop-search` (A97) calls the palette's
  `dismiss_one_level()`, which leaves the category view or, in the grouped
  view, asks the window to close as today. **Both outcomes set the window's
  child-handled latch** (`mark_child_transient_escape_handled`): today only
  the close-requested callback sets it, and `stop-search` runs before the
  window's Bubble-phase Escape controller sees the same key press
  (`transient_dismissal/policy.rs`), so a leave that did not latch would let
  that controller find the palette still revealed and close it, turning one
  Escape into two levels. The palette therefore reports every Escape it
  consumed through one callback (`connect_escape_consumed`, replacing the
  close-only wiring), and the window latches before acting on it (spec
  review). Outside the entry, the window's
  dismissal ladder (`transient_dismissal/policy.rs`) gains the palette's
  category state in `TransientSurfaceState`; the palette rung's outcome becomes
  "leave category" or "close palette". Click-away still closes the palette.
- **Backspace leaves only from a query that was already empty.** A held
  Backspace that deletes `rs` keeps auto-repeating after the entry is empty;
  GTK delivers repeats as presses without releases. Leaving on the first
  repeat would throw the user out of the category they were filtering. So a
  Backspace press leaves only when the query is empty **and** a key release
  has been seen since it became empty (`policy::backspace_leaves_category`,
  pure, unit-tested with a press, press sequence and a press, release, press
  sequence; the capture controller tracks the release). A97's probe also
  pins that the capture controller sees the release (spec review).
- **Close** clears the category; reopening shows the grouped All view, as
  today.

### D6. Keyboard and accessibility

- **Navigation without a walk.** The grouped view keeps
  `next_activatable` over its flags (at most a few hundred rows). The category
  view has no headers, so `policy::next_category_row(current, len, delta)` is
  O(1). `refresh_accessibility_state` and the item factory read the visible
  view's length and selection; neither walks the category model. A
  test-utils counter of `item()` calls in the evidence proves it (task 3).
- **Page Up / Page Down** move the selection by `RESULT_PAGE_ROWS` (16) in both
  views: the grouped view skips headers in that direction; the category view
  clamps to `[0, len)`. The list scrolls to the selection (`scroll_to`, A95).
- **Names:** the "More" row's name as in D1, description "Opens a list of
  every result in {category}"; the category list's label "Command palette
  results in {category}", description "Every matching result in this
  category"; the back button "Back to all results"; the title and notice
  labels carry their visible text. Rows keep their grouped names
  ("Open file …", "Open note …", "Run command …"); their set size is the
  model's length.
- **Announcements** (polite, through the existing throttled lanes): entering
  a category view once; the budget notice once per cut change; the result
  count stays in the list's value text, as in the grouped view.
- **Focus** stays in the query entry in both views, as today.

### D7. The palette's note source without counts

`PALETTE_NOTE_SOURCE_LIMITS` becomes `entries: None`, `sidecar_entries:
None`, `on_byte_refusal: SkipAndCount`; the byte ceilings and the diagnostic
count are unchanged. BN D3's argument carries over unchanged: up to `main`'s
first refusal both rules admit the same rows with the same charges, and
skip-and-count only adds rows after it, so the palette admits a superset (the
existing property test is extended to the palette's limits).
`MAX_PALETTE_NOTE_ENTRIES` leaves the inventory with its thresholds row.

**It has a second consumer, which this change must also convert.** The
palette's note refresh (`ui/window/notes/source_execution.rs`,
`refresh_command_palette_note_source`) captures the open editors' bookmarks
**synchronously on GTK** with `open_editor_note_snapshots_bounded(…,
MAX_PALETTE_NOTE_ENTRIES, NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT)`:
at most 16,384 snapshots plus bookmarks, and the first editor that does not
fit **stops** the capture (`truncated`, `rows_not_retained: 0`). BN gave the
browser a sliced, byte-bounded capture (`OpenEditorSnapshotCapture`: one
editor's bookmarks per unit within `LIST_TURN_SLICE_US`, counting the rows it
cannot retain, generation-checked per editor) and left the palette's. The
palette's refresh moves onto that capture, with the same 4 MiB budget and no
count (spec review):

- the capture state moves from `NotesBrowserState` to a small owner both
  flights hold (the palette's held by the window, beside
  `command_palette_note_refreshes`), so it is one capture routine, not two;
- the palette's submit runs after the capture's last turn, as the browser's
  does, and a palette close cancels the capture (`stop_open_editor_capture`);
- `open_editor_note_snapshots_bounded` keeps no caller but the evidence
  helper `capture_open_editor_note_evidence`, which is moved onto the same
  capture so the test surface drives the production path;
- `ui/window/notes/policy.rs`'s test that pins the palette's `Stop` is
  inverted to pin `SkipAndCount` for both consumers.

The `WFR-NOTES-BOOKMARKS` row is touched (task 7.4): its coordination module
gains a consumer, no role moves.

The cost is memory: the source can now fill its 64 MiB retained ceiling
instead of stopping at 16,384 rows (PT M12 measured 85.9 MiB RSS at 16,384
against 62.5 MiB at 10,000). Task 1.4 measures peak RSS and the source
build time at 131,072 sidecar bookmarks on `main` and the branch; the bound is
the existing byte ceilings, which PT accepted as the palette's memory bound.

A note category view whose source was cut by a byte ceiling says "Some notes
are not searchable: the note source holds {X} of {Y} notes" (the source's
`rows_found`, "at least" when `found_is_lower_bound`), once, beside the
existing truncation diagnostic.

### D8. Budgets and verdicts

Measured on an optimized widget build, presented, headless Mutter,
`GSK_RENDERER=cairo`, CPUs 8–23, interleaved `main` and branch, 1-minute load
below 4 before and after each run (recorded), with fixtures at 512, 16,384 and
131,072 indexed files and 131,072 sidecar bookmarks:

| # | Dispatch | Budget | Comparison |
|---|---|---|---|
| M1 | Grouped publish, All mode, every category full (+7 "More" rows) | `B_INTERACT_US` | branch within `main`'s range plus noise (PT M1: 6.4–7.9 ms) |
| M2 | Empty-query worker (palette open to first rows) at 131,072 files, 0 / 32 / 256 open tabs | recorded; first rows within `B_SETTLE_US` of `main`'s | name-prefiltered count |
| M3 | Activating "More": the input dispatch; the first category publish; the frame after it | `B_INTERACT_US`; `B_SETTLE_US`; `B_SETTLE_US` | flat across 512 / 16,384 / 131,072 |
| M4 | A keystroke in the category view; its publish; the frame after it | `B_INTERACT_US`; `B_SETTLE_US`; `B_SETTLE_US` | flat in rows |
| M5 | Down, Page Down, and a scroll to the last row at 131,072 | `B_INTERACT_US` per input dispatch | A95 records growth; measured at a near move (Page Down from row 0) and a far one (to the last row) |
| M8 | The palette's note refresh: worst open-editor capture turn with 20,000 editor bookmarks | `B_SETTLE_US` per turn | `main`'s one synchronous capture dispatch (D7) |
| M6 | Leaving (Escape) until the grouped rows show | `B_SETTLE_US` | — |
| M7 | Peak RSS and note-source build time at 131,072 sidecar bookmarks | recorded | `main` vs branch (D7) |

A dispatch that misses its budget because of the toolkit (BN's E2 frame after a
row-changing publish is the expected candidate for M4's frame) is recorded as a
toolkit-floor exception with its attribution and bounded by a widget assertion
at twice its measured worst, as BN did. Anything else that misses is fixed.

M5 has a known candidate too: A95 measured `set_selected` plus `scroll_to`
of a far row at 1.3 ms over 16,384 rows and **10.4 ms over 131,071**, above
`B_INTERACT_US`, and attributes the growth to the list's height estimate over
unrealized rows, not to the distance. If a near Page Down at 131,072 rows also
grows that way, every page move in a large category is over budget, and the
attribution, the axiom and a widget bound are required before sign-off
(task 8.2); the far move is the same exception at most.

### D9. GTK axioms and Kani

New axioms in `gtk-lush-axioms`, each with a probe, an interactive/`--check`
sample and a ledger row ("Verified against" the pinned GTK 4.22 / libadwaita
1.9):

- **A96.** A `GtkListView` over a lazy model, in a `GtkScrolledWindow` with
  `propagate-natural-height` and `max-content-height`, measures and allocates
  in time flat in the item count (512 against 131,072), asking only for items
  around the realized rows.
- **A97.** `GtkSearchEntry` emits `stop-search` on Escape whether or not it
  holds text, and keeps its text; a capture-phase `GtkEventControllerKey` on
  the entry sees Backspace while the entry is empty, before the entry handles
  it.

Cited: A1 (realized-row cap), A87 (per `items-changed`), A90 (revealing a list
costs its realized rows), A93 (remove-then-add publish), A94 (if the category
bar's title is exposed as a heading), A95 (far-row select and scroll).

Kani: `core-list-budget` gains B5 (`rows_within`) and is rerun whole. B5 has
two parts, because offering items to a `RetainedBudget` needs a loop and
`limit / charge` is unbounded: for **every** `u64` limit and every `charge >
0`, `r = rows_within(limit, charge)` satisfies `r * charge <= limit` and
`(r + 1) * charge > limit` (checked arithmetic, no overflow); and for results
of at most `MAX_SEQUENCE` rows it equals the number of `charge`-byte offers a
`RetainedBudget::new(limit)` retains. `charge == 0` returns `usize::MAX`
(nothing to bound) and is unit-tested, not assumed away. `rows_within` also
joins the list-budget proof-strength `Oracle` in `scripts/kani-shards.py`
(its `functions` and B5's harnesses with their measured seconds), and the
shard's `LIST_BUDGET_MINUTES` is re-measured if the rerun grows it. No other
shard covers code this change touches; `make check-kani-shards` must pass.

### D10. Workflow placement

`WFR-COMMAND-PALETTE` keeps its role homes:

- **Pure policy** (`ui/command_palette/policy.rs`): the "More" decision, its
  wording and accessible name, the category ids and parsing for the action,
  `next_category_row`, page moves in both views, the leave-selection rule, the
  notice and announcement wording, the budget constants. No GTK import.
- **Coordination** (`query_execution.rs`): the category in the request and in
  the publish arbitration. No new coordination module.
- **Evidence** (`evidence.rs`): `category: Option<PaletteCategory>`,
  `category_rows`, `category_total`, `category_limited`,
  `more_rows: Vec<(PaletteCategory, usize)>` (grouped view), and the
  test-utils `category_item_requests`.
- **Called presentation surfaces:** `imp.rs`, `item.rs`, and the new
  `category_model.rs`.
- **The facade** (`mod.rs`, 335 of 370 lines) narrates the category view as a
  branch of the query stage order (enter, query, publish, leave) with no new
  inversion; the narration must fit the 35 remaining lines, so the index
  half's narration is tightened if needed, not the budget.
- The transient-dismissal workflow's ladder (`transient_dismissal/policy.rs`)
  gains one input and one outcome; its facade narrates it.
- The matrix row is re-derived (size, seam counts, policy consumers:
  `model/list_budget.rs` gains a fourth owning workflow) per
  `.agents/rules/workflow-convention.md`.
- GTK Lush: the notes browser's model and this one are the second and third
  lazy `GListModel`s with a retarget cache. Record the extraction candidate in
  `docs/next/gtk-lush.md`; do not extract here.

### D11. Automation

- `window.command_palette` gains `category` (string id, `""` in the grouped
  view), `category_result_count` (rows the category view lists, `u32`
  saturating), `category_total_count` (matches, saturating) and
  `category_limited` (bool), projected from the evidence surface, with no row
  text. `result_count` keeps its meaning, stated precisely: the number of
  items in the **visible** list's model (headers and "More" rows included in
  the grouped view; every listed row in a category view, not the about 205
  rows GTK realizes).
- `win.set-command-palette-category` (string): a category id opens that
  category's view (requires a visible palette; an id that is not a group of
  the current mode is refused with the existing invalid-parameter behaviour);
  `""` leaves. Catalog entry beside `set-command-palette-mode`, exported,
  `ContextualUserCommand`.
- No new readiness predicate: `command-palette-search` covers category
  flights.

### D12. Thresholds

New rows in `docs/thresholds.md` and rustdoc `Kind:` lines:
`PALETTE_CATEGORY_RETAINED_BYTES` (8,388,608, presentation cap, up from
`PALETTE_CATEGORY_FLOOR_RETENTION`), `PALETTE_CATEGORY_FLOOR_RETENTION`
(derived), `PALETTE_CATEGORY_CANDIDATE_BYTES` (derived), `RESULT_PAGE_ROWS`
(16, `out: presentation`), `KIND_MORE` (`out: identifier`), the category
model's live-row prune point (1,024, `cap`, D2). Removed:
`MAX_PALETTE_NOTE_ENTRIES`. `make check-thresholds` fails first on the
leftover and the missing rows.

## Risks / Trade-offs

- **Empty-query latency** grows by the count pass (≈ 2 ms at 131,072 files
  with open tabs) → the prefilter, measured in M2; O(1) when nothing is
  excluded.
- **Memory of the note source** without counts → bounded by its unchanged
  byte ceilings, measured in M7.
- **A non-interruptible sort** in a superseded category query → 4.4 ms at
  131,072; recorded, with a split if it exceeds `B_SETTLE_US` (D3).
- **The frame after a row-changing category publish** may be a toolkit floor
  like BN's E2 → recorded and bounded, or fixed.
- **Facade budget** (335 of 370) → narrate the category view as a branch, not
  a new stage order.
- **Two Escape paths** (entry `stop-search`, window ladder) → both call one
  palette method, and the entry's path sets the window's child-handled latch
  for a leave as for a close (D5); a widget test drives each and asserts one
  level per press.
- **Backspace auto-repeat** could leave a category the user is filtering →
  leaving needs a key release seen since the query became empty (D5).

## Spec review (stage 2, 2026-11-07)

An independent review against the code, the rules, the Kani shards and the
GTK ledger. Confirmed and fixed in these artifacts:

1. **One Escape would have left the category and closed the palette.** The
   window's Bubble-phase Escape controller runs after the entry's
   `stop-search`, and only the close callback sets the child-handled latch
   (`ui/window/imp.rs`, `transient_dismissal/policy.rs`). D5 now latches on
   every Escape the palette consumes.
2. **`MAX_PALETTE_NOTE_ENTRIES` has a second consumer**: the palette's
   synchronous open-editor capture, which stops at 16,384 rows. Removing the
   constant without converting it would not compile, and keeping a renamed
   count would keep the loss. D7 moves it onto BN's sliced capture; M8
   measures its turns.
3. **The floor arithmetic did not match the layout.** The candidate is 32
   bytes, not 40, and the ranked candidates coexist with the output
   ordinals; with the 36-byte charge the floor is 4.5 MiB and 8 MiB is the
   power of two at or above it. With a bare 32-byte charge the floor would be
   exactly 4 MiB and `make check-thresholds` would reject 8 MiB as not the
   power of two at or above the recorded floor (D3).
4. **Commands cannot be truncated** (35 commands < 64), so the spec's "every
   category including commands ends with a More row" scenario was untestable;
   it now names the categories that can be cut and the forced-limit test
   covers the generic rule (D1).
5. **Backspace auto-repeat** (D5).
6. **B5 as written needed an unbounded loop**; restated as a closed-form
   property over every `u64` plus agreement on short sequences, with the
   proof-strength oracle entry (D9).
7. **M5's known candidate exception** (A95's 10.4 ms at 131,071) is named, and
   a near page move is measured separately (D8).
8. The hidden category model now releases its source snapshots on leave and
   close; the category model's prune point gets its thresholds row; the
   `row_model.rs` doc it copies from is corrected; `result_count` is defined
   as the visible model's length (D2, D11, D12).

Checked and found sound: the 64-row prefix identity (the selector's
comparator is a total order on score then ordinal, so a bounded heap of any
size `>= 64` yields the same first 64); the prefiltered exclusion (it tests
the canonical path's own file name, so a symlinked alias's display name
cannot cause a false negative); the axiom numbers (A95 is the ledger's last);
the MODIFIED requirements keep every scenario name; the `Arc<DisposalOwned<…>>`
clones in the model still hand their final drop to a disposal worker; the
facade's 35 free lines.

## Implementation decisions (stage 3, 2026-11-07)

Decided autonomously, user experience first; evidence in
`.claude/handoffs/drill-down-palette-categories/` (`runs.log`, `impl/`).

1. **The empty-query count prefilters by the canonical path's length and
   last eight bytes, not by its file name (D1 refined).** The name prefilter
   (`Path::file_name` plus an `OsStr` hash) measured 5.3–5.6 ms over 131,072
   files with 32 or 256 open tabs (Criterion, rounds 1–2), far above the spec
   microbenchmark's 1.5–2.1 ms. The `(len, tail)` key with a binary search
   over the open tabs measures 2.7 ms (32 tabs) and 4.4 ms (256 tabs), rounds
   3–4, against `main`'s 28 µs and 90 µs (which stops at 64 and so cannot
   count). It is exact (a property test pins it equal to the full predicate,
   symlinked aliases included). The remaining cost is reading every
   canonical path once; it is worker time, within `B_SETTLE_US` of `main`'s
   first rows, as M2 requires, and the full-path hash was not measured
   separately because the name prefilter already showed the per-file
   dereference, not the hash, dominates. With no open tab the count is the
   slice's length, O(1) (8.4–8.8 µs, equal to `main`).
2. **`PALETTE_CATEGORY_CANDIDATE_BYTES` is the literal 36 with a `const`
   assertion against the layout**, not a `size_of` expression:
   `make check-thresholds` cannot evaluate `size_of`, and
   `PALETTE_CATEGORY_FLOOR_RETENTION` must be evaluable for the 8 MiB
   presentation cap's "up from" rule. The assertion runs on every 64-bit
   target; on a 32-bit one the literal overstates the charge, which only
   lowers the row limit. Kind `external fact` (the pinned layout).
3. **Kani B5 is three harnesses, with a recorded bound reduction.**
   `rows_within_agrees_with_the_budget` keeps a symbolic limit and charge
   (results of at most four rows, 1.13 s), `rows_within_a_zero_charge_bounds_nothing`
   covers every limit, and the closed form `rows_within_is_the_largest_fitting_prefix`
   runs for **every `u64` limit at seven concrete charges** (1, 2, 3, 36,
   4,096, 2^32 + 1, `u64::MAX`; 1.87 s). With a symbolic 64-bit charge the
   closed form did not finish over every `u64` (CBMC killed after 16 min),
   nor under `limit < 2^32, charge < 2^16` (12 min), nor the bare equality
   with `limit / charge` (15 min). The concrete run found an overflow in the
   harness's own `rows + 1` (charge 1, `limit = u64::MAX`), fixed with a
   checked add. Recorded in the harness, in `rows_within`'s rustdoc and in
   `docs/next/formal-verification.md`; the shard runs 9/9 in 0.16 min locally
   and keeps its CI budget (2.8 min) until a dispatched run.
4. **The category entry points live in `category_view.rs`** (with
   `PaletteEscape`), and the row factory in `row_factory.rs`, both called
   presentation surfaces: the facade stays at 344 of 370 lines and `imp.rs`
   at about 920, and the facade narrates the category view as a branch of
   the query stage order.
5. **Grouped rows learn their category from the headers in display order**
   (`imp::grouped_row_categories`), matched against the mode's categories in
   order, so a workspace labelled like another group cannot be confused
   with it; `PaletteSearchRow::Header` is unchanged.
6. **The accessible result count excludes "More" rows** (`is_result`), so
   "N command palette results" keeps counting results; the automation
   snapshot's `result_count` is the visible model's length as D11 states.
7. **A capture in progress holds the `command-palette-index` readiness
   blocker**, beside the note refreshes, so automation does not read a
   note source the capture has not yet submitted. No new predicate.
8. **The narrow-overlay check (task 6.2) is a widget allocation test**
   (`test_palette_category_bar_and_list_fit_a_narrow_overlay`), not a
   `cargo-gtk-proof` scenario: the invariant is purely geometric (`ui.md`),
   and the visual smoke captures the view in light and dark.
9. **AT-SPI cannot see the More row until it is scrolled to**: it is the
   group's 66th row, below the overlay's 400 px viewport, so GTK has not
   realized it (A1). The accessibility smoke finds it in the return case,
   where leaving selects and scrolls to it; its accessible label is proved by
   the widget test's `AccessibleAudit`.
10. **Retired with the count:** `open_editor_note_snapshots_bounded`, its two
    capacity helpers in `notes/policy.rs` and the editor's
    `bookmark_records_bounded_by_retained_bytes` wrapper; the evidence helper
    `capture_open_editor_note_evidence(retained_byte_limit)` drives the same
    capture units the sliced turns run.
11. **Leaving defers the category model's clear to a `G_PRIORITY_LOW`
    idle** (`category_view::schedule_category_clear`). Round 1 measured the
    Escape dispatch at 4.7–11.2 ms, above `B_INTERACT_US`: removing the rows
    unbinds the about 205 the list realized (A1). With the list hidden first
    and the clear in its own dispatch, Escape costs 0.3–1.3 ms and the clear
    9.5–11.5 ms, within `B_SETTLE_US`; entering again before the idle runs
    clears at once. The sources are released one idle later (tests wait for
    it).
12. **The measurement harness is not committed.** Like BN's, it is appended
    to the widget suite of a `main` worktree and a branch worktree for the
    runs only (`impl/pdc_measure_*.rs`); the committed bound is
    `test_palette_category_dispatches_stay_within_their_budgets`.

### Measured verdicts (task 8.1)

Optimized widget build (`opt-level 2`, no debug assertions), presented,
headless Mutter, `GSK_RENDERER=cairo`, CPUs 8–23, each run behind
`scripts/measure.sh` and a 1-minute load below 4 for 30 s (recorded in
`runs.log`, 0.25–3.6 before and after), `main` (`ed37423a`) and branch
interleaved, two rounds. Figures are min–max over the runs' repetitions.

| # | Dispatch | Budget | `main` | branch | Verdict |
|---|---|---|---|---|---|
| M1 | Grouped publish, All mode, every category full: worst dispatch until the rows are current / the frame after | `B_INTERACT_US` (publish) | 10.4–21.6 / 12.8–16.8 ms | 11.1–18.3 / 12.7–15.3 ms | branch within `main`'s range at 512, 16,384 and 131,072 (+6 More rows). This harness's worst dispatch includes the frame clock's dispatches, so it is not PT M1's per-publish figure (6.4–7.9 ms); by the same method the branch adds nothing |
| M2 | Empty-query first rows, 131,072 files, 0 / 32 / 256 tabs (wall) | first rows within `B_SETTLE_US` of `main`'s | 21.0–24.5 ms | 21.2–24.1 ms | holds; worker (Criterion): 8.5 µs / 2.7 ms / 4.4 ms against 8.4 µs / 28 µs / 90 µs (decision 1) |
| M3 | Enter on a More row: input / first category publish / the frame after | `B_INTERACT_US` / `B_SETTLE_US` / `B_SETTLE_US` | — | 0.07–0.19 / 6.8–8.2 / 13.2–16.6 ms | flat in rows (512, 16,384, 131,072); the frame is exception **X1** below |
| M4 | A query in the category view: `set_text` / publish / the frame after | `B_INTERACT_US` / `B_SETTLE_US` / `B_SETTLE_US` | — | 0.14–0.41 / 1.0–3.9 / 0.14–0.63 ms | holds, flat in rows: retargeting keeps the realized rows, so no BN-E2-like frame |
| M5 | 131,072 rows: Down / Page Down / select-and-scroll to row 131,055 / Page Down to the last row: input, then the frame after | `B_INTERACT_US` per input | — | 0.16–0.41, 0.7–2.7 / 0.16–0.37, 1.1–4.7 / 2.2–6.9, 7.7–9.3 / 0.14–0.30, 1.3–5.5 ms | holds; A95's far-row growth stays under budget at 131,072 (no exception) |
| M6 | Escape: input / worst dispatch until the grouped flight is current / wall time | `B_INTERACT_US` / `B_SETTLE_US` per dispatch | — | after decision 11: 0.3–1.3 / 4.4–11.5 / 14.5–23.1 ms | holds per dispatch; the grouped rows are on screen from Escape's own frame, and the wall time is the grouped re-query of the kept query (131,072 files scored), the same flight `main` runs when that query is typed |
| M7 | Note source at 131,072 sidecar bookmarks: build (toggle to installed) / peak RSS / worst dispatch | recorded | 34–50 ms / 90–114 MiB / 6.9–8.2 ms (16,384 rows kept) | 269–347 ms / 158–254 MiB / 8.1–9.0 ms (all 131,072) | recorded: 8 × the rows on the worker, within the source's unchanged 64 MiB byte ceilings; the worst dispatch is the grouped publish (M1's class) |
| M8 | 20,000 editor bookmarks: the palette's capture | `B_SETTLE_US` per turn | one synchronous capture inside the toggle, 1.9–4.0 ms, 16,384 kept | worst turn 3.0–3.2 ms, toggle 0.2–0.8 ms, all 20,000 kept | holds |

The category ranking at 131,072 matches costs 11.4–11.5 ms on the worker for
the query `file` (scoring, heap, sort, ordinals) and 0.36 ms for the empty
query (Criterion); a superseded query observes cancellation within 256
candidates while it scores, and the sort that follows takes a few ms, under
`B_SETTLE_US`, so it was not split (D3).

**Recorded toolkit-floor exception X1:** the frame after the first category
publish, 13.2–16.6 ms (above `B_SETTLE_US` in 6 of 30 repetitions, by at most
0.6 ms, mostly the first entry of a palette session), flat at 512, 16,384 and
131,072 rows. Attribution: it realizes the about 205 category rows (A1, A90,
A96) while the grouped list unmaps; the model creates exactly 205 row objects
per entry (`item_requests`). Bounded at twice its measured worst (33.2 ms) by
`test_palette_category_dispatches_stay_within_their_budgets` on the optimized
build (15.6–15.8 ms there, two runs), which also enforces Enter and Escape
within `B_INTERACT_US` and the first publish and the leave within
`B_SETTLE_US`.

**Gates (task 8.3), 2026-11-07:** `make check` (with fresh
`visual-geometry-smoke`, `accessibility-smoke` and `visual-smoke` runs on the
final tree), `make test` (2,417 non-widget and 1,641 widget tests, 0 `FLAKY`,
before the last surface-only refactors; the palette widget modules, 112 tests,
were rerun after them), `make gtk-axioms` (86 samples held, A96 and A97
included), `make check-gtk-axioms`, `make kani KANI_SHARD=core-list-budget`
(9/9), `make check-kani-shards`, `make check-widget-shards`,
`make check-thresholds`, `make check-automation-docs`,
`make automation-client-self-test`, the rustdoc lint gate.

**Not measured as designed:** tasks 1.1–1.4 asked for the baseline before any
fix; it was measured interleaved with the branch at the end instead (same
harness on `main`), so the before/after comparison holds but the order did
not.

## Migration

None: no persisted format, GSettings key or action changes meaning. The new
action and snapshot fields are additive.

## Open questions (answered by tasks)

- M2's measured empty-query worker cost in the application, and whether the
  prefilter is needed at all when the measured full-path pass fits (task 1.2).
- The candidate size on the pinned toolchain, which fixes the floor (task 2).
- Whether M4's frame needs a recorded exception (task 8).
