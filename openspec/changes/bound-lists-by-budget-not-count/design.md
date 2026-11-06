## Context

`power-of-two-thresholds` (PT, archived 2026-11-06) turned every count that
cuts a user-visible list into a **presentation cap** rounded up from its
former value (design D12), and queued the structural fix as its D13 item 1:
bound lists by retained bytes and per-turn time instead of by count, and
always show what lies past any cut that remains. PT's stage-3 measurements
also found three dispatches that `main` already runs past its budgets, which
`.agents/rules/preexisting-blockers.md` makes part of this work:

| Surface | Dispatch | Measured on `main` | Budget |
|---|---|---|---|
| Workspace search | the first-results reveal of a 16,384-match search | 22–26 ms (PT M2, the column that is `main` today; 20–23 ms at 10,000) | `B_SETTLE_US` 16 ms |
| Notes browser | the sidebar rebuild of 512 rows | 34–40 ms (PT M6; 32–38 ms at 500) | `B_SETTLE_US` |
| Workspace sidebar | the terminal dispatch of a 10,000-entry expansion | 25–30 ms (PT M5) | `B_SETTLE_US` |

Reading PT's raw evidence (`.claude/handoffs/power-of-two-thresholds/amend/widget/evidence.txt`)
for this spec adds one more on a surface this change touches:

| Workspace search | entering Replace Preview over 16,384 matches | 27.8–29.2 ms (PT M3) | `B_SETTLE_US` |

PT's review listed M3 only as "no worse than `main`", so it was never called a
blocker; it is one, on the same panel and the same list, so it is in scope.

The structural item names five surfaces. Their code, read for this spec on
`origin/main` `469e3f84`:

- **Workspace search** (`services/content_search/search.rs`,
  `ui/search_panel/`). The worker stops at `RESULT_CAP` (16,384 matches) and
  sends `ResultCap`; the panel shows `16,384+ results (truncated)`. GTK drains
  at most 256 events per 50 ms poll (`MAX_SEARCH_EVENTS_PER_TICK`) and
  appends **one match at a time**: one `ListStore::append` per match (one
  `items-changed` each), one root append and one `TreeListRow::set_expanded`
  per new file. Every `SearchMatch` carries its own `PathBuf`.
  `JOURNAL_SCAN_MAX_ENTRIES` is asserted `>= RESULT_CAP`.
- **Workspace sidebar** (`ui/sidebar/workspace_section/scan_execution.rs`).
  A child scan keeps the first `MAX_DIR_ENTRIES` (10,000, a recorded
  presentation-cap exception) entries in sort order and appends a placeholder
  row `10000+ items - showing first 10000`. Rows are spliced in 256-row
  batches per idle turn, but `finish_child_reconciliation` then rebuilds the
  directory's path caches from the mirror (`recache_child_rows_from_mirror` →
  `replace_child_cache`: a `PathBuf` clone and three map updates per row) in
  one dispatch. That dispatch is the 25–30 ms. The service already has a
  deterministic, byte-bounded scan
  (`services::file_tree::scan_directory_bounded_with_cancel_and_bytes`) used
  by the palette index.
- **Notes browser** (`ui/window/notes/browser.rs`). Every rebuild constructs
  an `AdwSidebarItem` per row (up to `NOTES_BROWSER_RENDER_LIMIT`, 512), and
  for every bookmark a suffix `GtkButton`, in one dispatch.
- **Command palette** (`ui/command_palette/policy.rs`
  `MAX_RESULTS_PER_SOURCE`, 64): no slow dispatch (M1 6.4–7.9 ms); the change
  is a new interaction (a per-category "N more…" row opening a drill-down).
- **Minimap markers** (`ui/editor_page/minimap/policy.rs`, three caps of
  2,048): no recorded slow dispatch that is the markers' own. PT's M7 figures
  (118–140 ms "long", 741–823 ms "wrapped") include a `set_text` of the whole
  fixture and the text view's relayout, so they are not attributable to the
  markers without a new measurement.

Rules in force: `.agents/rules/rust.md` (thresholds; the presentation-cap
kind), `.agents/rules/workflow-convention.md` (both touched workflows,
`WFR-SEARCH-REPLACE` and the workspace-tree workflow, are migrated: pure
decisions go to `policy.rs`, coordination to named modules, observable state
to `evidence.rs`), `.agents/rules/documentation.md`, the responsiveness budgets
in `model/responsiveness_budget.rs`, and the measurement rules of PT D12
(load below 4, `taskset -c 8-23`, interleaved with `main`, load recorded).

## Goals / Non-Goals

**Goals**

- Workspace search and the workspace sidebar keep no count cap. Each is
  bounded by a retained-byte budget, and that budget never retains less than
  `main` retained at `main`'s worst case.
- Whatever a budget cuts is stated, with the true total where the worker
  knows it, in the UI, to assistive technologies (announced once) and in the
  automation snapshot.
- Every dispatch of a search delivery, a Replace Preview entry and a
  directory expansion fits `B_SETTLE_US`, at any size up to the budget. The
  work in a turn is bounded by a time slice, not only by a count. Anything
  that cannot fit is a recorded, measured toolkit-floor exception with a GTK
  axiom and a sample.
- One GTK-free, Kani-proved policy decides admission against a byte budget
  and whether a turn continues. Both workflows use it.
- Failing-first widget tests show no visible loss against `main` and each
  budget holding.

**Non-Goals (this change)**

- The notes browser, the palette drill-down and minimap binning. They are
  queued as separate changes (D1).
- Replace All's own safety budget (`MAX_REPLACE_PREVIEW_ROWS` 16,384,
  `MAX_REPLACE_PREVIEW_BYTES` 64 MiB, the per-file and undo caps). It is
  unchanged. `stream-replace-all` (PT D13 item 2) owns it.
- The palette file index (`scale-file-index`, D13 item 3).
- A process-wide budget across expanded sidebar directories. The budget stays
  per directory, as the count was. See Risks.

## Measured baseline

Measured by PT stage 3, at load below 4, interleaved, on the optimized widget
build with the surface presented. Raw lines are in
`.claude/handoffs/power-of-two-thresholds/amend/widget/evidence.txt`, and the
measurement code is in `amend/measure/`. The search and sidebar files have not
changed since those runs except by PT's own commits (`git log fbf0f875..469e3f84`
on those paths shows only PT). So the columns that describe `main` today are:

| Case | `main` today | Source |
|---|---|---|
| Search, 16,384 matches: worst dispatch while results stream | 22.2 / 24.8 / 26.3 ms | M2, "branch" column |
| Replace Preview entry over those matches | 27.8 / 28.8 / 29.2 ms | M3, "branch" column |
| Retirement of that generation | 1.5–2.4 ms | M2/M3 |
| Full 256-event receive turn (PT task 5.4) | median 11.6–13.5 ms, max 12.8–14.9 ms | PT runs.log |
| Sidebar expansion of 10,000 entries: worst dispatch | 25.2 / 27.7 / 30.0 ms (17 realized rows) | M5, "main" column |
| Same at 16,384 entries (PT's rejected candidate) | 40.7 / 41.5 / 43.1 ms | M5, "branch" column |
| Notes browser rebuild, 512 rows (for the queued change) | 34.3 / 39.1 / 39.9 ms | M6 |

The sidebar figure is linear in the entries: 25–30 ms at 10,000 and 41–43 ms
at 16,384, about 2.6 µs per row. So any directory above about 6,000 entries
already misses `B_SETTLE_US` on `main`.

**Not re-measured at spec time.** The host's 1-minute load was 19–24 for the
whole of this stage (`/proc/loadavg`, 2026-11-06 19:0x), above the gate of 4.
Task 1.1 re-confirms the table on `origin/main` before any code changes, with
PT's measurement code, and attributes each slow dispatch (task 1.2). If the
attribution contradicts the hypotheses in D4 and D6, the mechanism changes but
the targets do not.

## Decisions

### D1. Scope: this change is the workspace slice. The rest is queued.

D13 item 1 names five surfaces and three blockers. As one change it would put
a search panel rework, a sidebar loading rework, a notes browser widget
replacement, a new palette mode and a minimap marker algorithm into one review.
These share no code beyond the budget policy of D2. The task allows a split,
and review quality decides it. Three properties make the first slice
coherent:

- **Workspace search and the workspace sidebar** are both `GtkListView`s over
  `GtkTreeListModel`s fed by a worker walking the workspace. Both need the same
  two mechanisms: a retained-byte budget in the worker, and time-sliced batch
  splicing on GTK.
- Both carry a pre-existing blocker (the reveal and Replace Preview; the
  expansion terminal), so this slice discharges three of the four blockers.
- Both are covered by the same widget shards and lane fixtures.

Queued in the programme record, in this order, each a separate change with its
own blocker:

1. **`bound-notes-browser-by-budget`**: the notes browser becomes
   incremental and virtualized, bounded by retained bytes, and states "showing
   X of Y". It discharges the 32–40 ms rebuild. It must first answer, with a
   GTK axiom probe, whether `AdwSidebar` builds row widgets for off-screen
   items. If it does, the browser moves to a `GtkListView` with section headers
   (`GtkSectionModel`, a `header-factory`) styled `navigation-sidebar`. That
   is an Adwaita-native look, but it is a widget change that deserves its own
   review.
2. **`drill-down-palette-categories`**: an "N more…" row per category that
   opens that category's full ranked results in a virtualized list bounded by
   a byte budget, with keyboard and accessibility parity.
3. **`bin-minimap-markers-per-row`**: markers binned per pixel row, with no
   count cap and drawing cost O(height). It must first attribute PT's M7
   worst dispatches (which include a whole-buffer `set_text` and relayout)
   before it can claim or discharge any minimap blocker.

Then `stream-replace-all`, `scale-file-index` and the rest of PT D13, unchanged.

*Alternative rejected:* all five surfaces in one change. That is one very large
review, and the minimap and palette parts would wait on unrelated widget work.
*Alternative rejected:* one change per surface, sidebar and search apart.
Their shared policy (D2) would then land without its second consumer, and its
proof would be shaped by one workflow.

### D2. One pure policy: `model/list_budget.rs`

A GTK-free module (`#![forbid(clippy::float_arithmetic)]`, integer only) owns
two decisions.

**Admission against a retained-byte budget.** `RetainedBudget { limit,
charged }` and `admit(charge) -> Admission::{Retained, Limited}`. Charges add
with saturation. An item is retained only if `charged + charge <= limit`. The
first refusal latches `Limited`, and every later `admit` returns `Limited`, so
**what is retained is always a prefix of the delivery order**. No gap is
possible, which keeps "showing X of Y" true and keeps F4 navigation contiguous.
Kani proves, for every `u64` limit and every sequence of up to N charges:

- (B1) `charged <= limit` always;
- (B2) the retained set is a prefix, and once `Limited` stays `Limited`;
- (B3) no loss against a count cap: if `limit >= n * w`, any sequence of at
  most `n` charges each at most `w` is retained in full. This is the
  statement that the budget never shows less than `main`'s count cap at
  `main`'s worst charge;
- (B4) a counter-example harness: a limit one below `n * w` refuses the
  `n`-th worst-case item. It is kept as a `should_panic` style failing proof,
  like the minimap shard's general-`f64` cases.

**Turn continuation.** `turn_continues(elapsed_us, units_done) -> bool` is
true when `units_done == 0` (every turn makes progress) or
`elapsed_us < LIST_TURN_SLICE_US`. `LIST_TURN_SLICE_US` is computed from
`B_INTERACT_US / 2` (4,000 µs; inventory kind `out: time`, the class every
time budget has), so a turn leaves the rest of an interaction
budget for the frame and the input that waits on it. Kani proves (T1) progress
and (T2) that a turn stops at the first unit started at or after the slice, so
the work the check guards costs at most the slice plus one unit. What a
workflow does after its loop (search applies the turn's collected matches as
splices once the drain stops, D4) is **not** covered by T2: it is bounded by
the count caps and by measurement (task 8.2). The unit's cost is what each
workflow measures (D4, D6). The existing count caps stay as internal caps:
`CHILD_APPEND_BATCH_SIZE` (256) bounds a sidebar unit's size, and
`MAX_SEARCH_EVENTS_PER_TICK` (256) bounds how many search events, and so how
many matches the post-drain splices apply, one turn can take. The slice bounds
how many units run per turn below those caps.

The clock is read by the GTK caller (`glib::monotonic_time`) and passed in, so
the policy stays pure. A new Kani shard `core-list-budget` holds B1–B4 and
T1–T2. It is scheduled weekly and gates pull requests, because both workflows
rely on it. Its runner budget is measured as `check-kani-shards` requires.

*Why a prefix latch and not best-fit?* Best-fit would retain a later small
match after refusing a large one. The list would have a hole the user cannot
see, and Replace All could act on matches after one it skipped. A prefix is
honest and simple to state.

### D3. The budgets and their kind

Both budgets are **presentation caps** (`.agents/rules/rust.md`): rounding
one down hides results. Neither predates PT, so its "up from" value is its
**intended value**: at least **`main`'s worst-case retention under the count
cap it replaces**, and for the sidebar the larger of that and a typical-size
floor (below). Each floor is computed by a `derived` constant from named
inputs, and the budget is the power of two at or above the larger floor. B3
and one compile-time assertion per floor (`BUDGET >= MAIN_WORST_RETENTION`,
and for the sidebar `BUDGET >= SIDEBAR_TYPICAL_FLOOR_RETENTION`) tie them
together. The per-item charge
must never under-count what an item retains, so its fixed part is a **coverage
window** (rounded up), measured as described below.

- **`SEARCH_RESULTS_RETAINED_BYTES`** (`ui/search_panel/policy.rs`). Charge
  per match: `line_bytes + SEARCH_MATCH_FIXED_CHARGE`, plus the file's display
  and canonical path bytes once per file, on its first match. Paths are
  interned per file (an `Arc<Path>` shared by the file group, the match items
  and the navigation index); today every `SearchMatch` clones its own
  `PathBuf`. `main`'s worst case: 16,384 matches, each a 4,096-byte line
  (`MAX_SEARCH_MATCH_LINE_BYTES`), each in its own file with a 4,096-byte path
  (Linux `PATH_MAX`, an external fact). The per-file charge counts **both**
  path copies (the canonical path and the display path, which is never longer),
  so that is `16,384 × (4,096 + 2 × 4,096 + fixed)`: 192 MiB plus
  `16,384 × fixed`. With a fixed charge of at most 4 KiB it is at most
  256 MiB, so the budget is **256 MiB**; a measured fixed charge above 4 KiB
  makes it 512 MiB by the same rule. Final after task 2.4 measures the fixed
  charge. For typical matches (a 120-byte line, files of
  about 20 matches, a fixed charge near 1 KiB), 256 MiB retains more than
  200,000 matches. The budget is a ceiling that only pathological input
  reaches. At `main`'s own worst case it retains exactly what `main` did,
  rounded up.
- **`SIDEBAR_DIRECTORY_RETAINED_BYTES`** (`ui/sidebar/policy.rs`, the
  `WFR-WORKSPACE-TREE` workflow's one pure policy; `workspace_section/` has no
  `policy.rs`). Charge per row: the scanner's
  existing graph weight (`directory_entry_graph_bytes`) plus
  `SIDEBAR_ROW_FIXED_CHARGE`, which covers the `FileTreeItem`, the mirror row,
  and the three path caches. Two derived floors decide it, and the budget is
  the power of two at or above the **larger** of them (both asserted at
  compile time):
  - `MAIN_SIDEBAR_WORST_RETENTION`: `main`'s worst case, 10,000 rows with
    255-byte names (`NAME_MAX`) under a parent near `PATH_MAX`;
  - `SIDEBAR_TYPICAL_FLOOR_RETENTION`: 131,072 rows with 32-byte names under a
    parent path of 256 bytes, so that "past 10,000" means far past it.

  The second floor is the user-experience target, and it is not given up when
  the measured fixed charge makes it large. The first remedy is to shrink what
  a row really retains (the duplicate path copies in the caches become
  `Arc<Path>`, then re-measure); the budget then follows the floor. As an
  estimate only: at about 1 KiB per typical row the typical floor is
  128 MiB, and `main`'s worst case (about 10,000 × 5 KiB) is about 50 MiB, so
  the typical floor is expected to decide.

**Measuring the fixed charges (coverage windows).** A widget fixture retains
16,384 matches and 16,384 rows, presented, then reads `VmHWM` growth against
an empty control in a release test process of its own, as PT M9/M12 did. The
per-item growth, rounded up to a power of two, is the fixed charge. A memory
test then checks the bound itself: process `VmHWM` growth at a full budget is
at most `budget × 5/4`.

**What leaves the inventory.** `RESULT_CAP` and `MAX_DIR_ENTRIES` are removed,
and so are their rows in `docs/thresholds.md`. The bench mirror of
`MAX_DIR_ENTRIES` goes too. `JOURNAL_SCAN_MAX_ENTRIES`'s assertion moves from
`RESULT_CAP` to `MAX_REPLACE_PREVIEW_ROWS`. Replace All touches only previewed
rows, so the files one run can journal are at most the preview's rows; that
was always the real bound. The new rows: the two budgets (presentation cap, `up from <the larger derived
floor>`), the two fixed charges (coverage window), the two
`MAIN_*_WORST_RETENTION` values and `SIDEBAR_TYPICAL_FLOOR_RETENTION`
(derived), and `LIST_TURN_SLICE_US` (`out: time`, written
`expr: B_INTERACT_US / 2`, as `REANALYSIS_ALLOWANCE_US` records
`expr: B_SETTLE_US`). `make check-thresholds` must pass
with the new rows and fail first on each of: a budget below its `up from`
value, a missing fixed-charge row, and a `RESULT_CAP` row left behind
(task 5.1's self-test cases).

### D4. Workspace search: stream without a count, show what is past the budget

**Worker.** The worker no longer counts matches against a cap. It charges each
match through `RetainedBudget`. While matches are `Retained`, it sends them as
today. At the first `Limited` it sends one `SearchEvent::BudgetReached {
retained }`, behind the matches it describes (the ordering `ResultCap` has
today), then **keeps walking and counting matches without retaining them**. It
sends a coalesced `SearchEvent::BeyondBudget(total_found)` at most every
250 ms, by a worker-side clock (the existing `Progress` cadence is every 100
files, not a time, so it cannot carry this), best-effort through `try_send`
like `Progress`, and the terminal `Done` carries the final total, so a dropped
`BeyondBudget` loses nothing. Cancellation, the identity-limit `Incomplete`
terminal and errors are unchanged. Reaching the budget must not set the walk's
stop state (`WalkStop`); only the caller's cancellation and `Incomplete` stop
production.

**Admission is serialized with delivery.** The walk is `ignore::WalkParallel`,
so several threads find matches at once, and today's cap is an
`AtomicUsize` that is "approximate under parallel walkers". The prefix
property of D2 is about *delivery order*, so admission and the send of an
admitted match must happen as one step: one `Mutex<RetainedBudget>` per
search, held across `admit` and the `tx.send` of that match (and of
`BudgetReached` on the first refusal). Channel order is then admission order,
`BudgetReached` is always behind every retained match, and the sequential
Kani proofs of D2 apply to what the consumer receives. Holding the lock across
a blocking send costs nothing new: when the bounded channel is full the
walkers wait either way. Once the budget is `Limited`, a walker reads the
latch through one `AtomicBool` and counts with one `fetch_add`, without taking
the lock, so the beyond-budget walk is not serialized. *Alternative rejected:*
a lock-free `fetch_add` reservation. A refused large charge and an admitted
small one can then reach the channel in either order, so a retained match
could arrive after `BudgetReached`, and the proof would not describe the
running code.

*Why keep counting?* "Showing 245,301 matches; 108,229 more not shown" tells
the user how far to narrow the query. "Some results were not shown" does not.
Counting retains nothing per match, and the walk is the same work a
non-limited search would do anyway. The user can still stop it with
Escape/close, as today. *Alternative rejected:* stop at the budget like
`ResultCap`. That is cheaper, but it leaves Y unknown, which the requirement
"always show what is past any remaining cut" exists to prevent.

**GTK consumer.** A turn drains events while `turn_continues` holds and fewer
than 256 events have been received. It collects matches into per-file batches
and applies them at the end of the turn:

- one `splice` per touched child store;
- one `splice` of new file rows on the root store;
- one pass expanding the new file rows, which are the tail of the flattened
  model, so their positions are known without a walk.

Every received event still consumes one unit, as the existing requirement says.
So per turn the panel emits `items-changed` once per touched file plus once on
the root, instead of once per match.

**Labels and announcements.** While limited, the count label reads "Showing X
matches in F files; N more not shown — refine the search to see them", with N
growing until `Done`. It is styled `warning`, like the capped label today.
Reaching the budget raises one polite announcement. `Done` raises the final
count, as today. A limited search is not saved to history, matching the capped
behaviour (a capped search was never saved). The exact wording is settled in
task 3.6 against `docs/accessibility.md`'s bounded-announcement rules.

**Replace All.** It is built from the sealed snapshot of retained matches, as
today. Its own safety budget is unchanged. This is a visible change of
balance: on `main` the search cap equalled the preview row budget (both
16,384), so a preview covered every shown match unless its byte budget cut.
Now a search can show more matches than one preview admits (40,000 shown,
16,384 previewed). The preview already states its omitted count
(`ReplacePreviewOutcome::omitted_eligible`) and applies only previewed rows,
so this is honest, and the user sees more than before. Raising the preview's
own budget is `stream-replace-all`'s (PT D13 item 2). When the search was limited, the
preview summary adds "N matches past the search limit are not included", and
Replace All acts only on previewed rows (the data-safety invariant it already
has). No unseen match is ever replaced.

**The first-results reveal (blocker).** The hypothesis to confirm in task 1.2:
the results list is unrealized (`results_scroll` is `visible=false`) when the
first matches arrive, and an unrealized `GtkListView` walks its whole item list
on every `items-changed`. PT recorded this when its unpresented fixture
measured a quadratic artifact. The 256 per-match appends of that turn then run
against a list that is realized in the same dispatch. The fix:

- (a) the batched splices above remove the per-match emissions;
- (b) the turn that reveals the results applies only the first batch, so the
  first realize carries one batch, and later turns fill;
- (c) the time slice bounds every later turn.

The toolkit behaviour relied on is pinned as new GTK axioms with samples
(D7). If the first realize of the list alone is over `B_SETTLE_US` with one
row, the residual is recorded as a toolkit-floor exception with its
measurement, as C4b and C5 did. It is not hidden.

**Replace Preview entry (blocker).** It is attributed in task 1.2 first. The
preview rows are installed into the same list by time-sliced splices
(`turn_continues`), with the panel in a "Preparing preview…" state until the
terminal. Checked-row identity stays tied to the sealed generation. The
existing stale-safety requirement is unchanged.

### D5. Retirement and backpressure stay as they are

Retirement already detaches a generation immediately and retires 256 references
per turn under a two-generation backpressure
(`SEARCH_RETIREMENT_ROWS_PER_SLICE`). Results larger than 16,384 retire over
more turns, each still bounded. Task 3.8 measures retirement at a full budget.
The per-turn slice is not added there unless a retirement turn exceeds
`B_SETTLE_US`, which M2 says is far off (1.5–2.4 ms).

### D6. Workspace sidebar: load every entry incrementally, state any budget cut

**Scan.** A child scan calls
`scan_directory_bounded_with_cancel_and_bytes(max_entries = usize::MAX,
SIDEBAR_DIRECTORY_RETAINED_BYTES, …)`. That is the existing deterministic,
sort-order, byte-bounded path. The service gains one change: a **single pass**
when the directory fits the budget, and the existing two-pass top-k only when
it does not. The comment beside that function explains that the second pass
doubles directory I/O, which must not become the common case. The single pass
retains every entry while its running charge fits. At the first entry that does
not fit, it drops what it retained and continues **as the first, scalar-only
pass** of the existing two-pass selection (counting and tracking the largest
graph weight), then runs the second pass. An over-budget directory therefore
still costs two passes, never three. The scan keeps its `lookahead_cap` and
visibility arguments unchanged. The scan reports the total count of visible
entries it examined. The placeholder is no longer
`10000+ items - showing first 10000`. It appears only when the budget cuts, and
reads "Showing X of Y entries" with a description that says why and offers the
path to the rest (Open, `Ctrl+O`). Below the budget, which by D3 covers every
directory `main` listed in full and far beyond, there is no placeholder.

**Incremental, time-sliced load.** Batches of 256 rows are spliced while
`turn_continues` holds. They start at the top, so the first screen of rows is
interactive at once. While rows remain, a **loading tail row** ("Loading
entries… X of Y") ends the directory. It has a stable accessible name, and its
label is updated at most once per turn. It is removed by the terminal. Readiness
`workspace-refresh-complete` stays pending until the terminal, as it is during
reconciliation today.

A budget-sized plan is plain data that crosses to GTK (up to the budget, where
`main`'s crossed at most 10,000 rows). When a refresh or collapse supersedes a
loading directory, its remaining rows must not be dropped in one GTK dispatch
over `B_SETTLE_US`: they go to the bounded plain-data disposal lane
(`ui/plain_disposal.rs`, the "Plain-data disposal ownership" rule), or a
measurement at the full budget shows the drop fits. The same holds for a stale
scan result rejected on GTK.

**The terminal dispatch (blocker).** The path caches (`child_paths`,
`item_locations`, `visible_path_counts`) are updated **per applied batch** for
the rows that batch inserted or removed, under the same mirror-generation
guard. The terminal is then O(1) in the directory size: it publishes,
schedules the state restore and syncs only the realized rows (at most about
205, A1). This replaces `recache_child_rows_from_mirror`'s whole-directory
rebuild on the expansion path. The manual-refresh path, which reconciles by
plan, uses the same per-batch maintenance, so no path rebuilds a whole
directory in one dispatch.

**Virtualization.** `ViewportSliceBin` already keeps the realized rows bounded
by the viewport, past A1's cap. Nothing changes there, and the
`widgets-geometry` shard is not touched. The widget tests assert **rendered**
rows (`drawn_labels`, A10) at the end of a 131,072-entry directory, by keyboard
traversal and by `win.reveal-workspace-path`.

**What the toolkit must do.** That a splice at the end of a large `ListStore`,
under a `GtkTreeListModel` and a presented `GtkListView`, costs O(batch) and
not O(rows): new axiom (D7). If the probe finds it O(rows), splicing cannot fit
the slice at large sizes. The fallback, decided now so the implementation does
not stall, is to keep the directory's rows in a custom `GListModel` that grows
by batches. `items-changed` then reports only the appended range, and the store
copy disappears. The probe decides between the two.

### D7. GTK axioms this change relies on (new, with samples)

Each axiom is a probe in `gtk-lush-axioms`, with an interactive/`--check`
sample and a ledger row (`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`),
numbered after A86:

- **An unrealized `GtkListView` walks its item list on every
  `items-changed`; a presented one does not.** Cost against the item count, in
  both states.
- **Appending a batch at the end of a `GListStore` under a `GtkTreeListModel`
  (passthrough off) and a presented `GtkListView` costs O(batch)**, measured
  at 16,384 and 262,144 rows.
- **`TreeListRow::set_expanded(true)` on the last rows of the flattened model**
  costs O(children) and not O(position) (extends A22).
- **The first realize of a `GtkListView` inside a revealing `GtkScrolledWindow`**
  costs as a function of realized rows (at most about 205, A1), not of model
  rows. This bounds what the reveal turn can cost.

`make gtk-axioms` runs first on any of these (the toolkit-update procedure in
`build.md`). The two existing axioms the change leans on, A1 (the realized-row
cap) and A10 (mapped is not drawn), are cited without change.

### D8. Workflow placement

- `model/list_budget.rs`: pure and shared. Like `model/attention_refresh.rs`,
  it has two owning workflows, and a forked copy could drift.
- `ui/search_panel/policy.rs`: the search budget, the charge function, label
  and announcement wording, and the batch plan for a turn (which file batches
  to splice, in what order).
- `ui/search_panel/execution.rs`: the drain loop, the per-turn batch splice,
  and the reveal turn.
- `ui/search_panel/evidence.rs`: `retained_matches`, `matches_found`,
  `result_limited`, `reveal_turn_events`, and the worst turn's duration under
  `test-utils`.
- `ui/sidebar/policy.rs` (the `WFR-WORKSPACE-TREE` workflow's one pure
  policy, at its canonical role home): the sidebar budget, the row charge, the
  placeholder and tail-row wording, the per-batch cache plan. The budget
  override for tests lives in `ui/sidebar/test_policy.rs`.
- `ui/sidebar/workspace_section/scan_execution.rs`: batch application with
  per-batch cache maintenance, and the tail row.
- The workspace-tree evidence surface (`ui/sidebar/evidence.rs`,
  `WorkspaceTreeEvidence` and its per-section `WorkspaceSectionEvidence`):
  `directory_total_entries`, `directory_retained_entries`,
  `directory_limited`, `loading_tail_rows`.

Both rows in `docs/workflow-readability-matrix.md` re-derive their measured
cells (`workflow-convention.md`, "Re-deriving a row's measured cells"), and
`make check-workflow-boundaries` passes. The search facade stays within its 370
lines, and so does the sidebar facade.

### D9. Automation and accessibility contracts

- Snapshot: `content_search.result_capped` and
  `search.workspace_result_capped` keep their names (automation clients depend
  on them). They now mean "the retained-byte budget cut the results". New:
  `content_search.matches_found` (`u32`, saturating, including matches past
  the budget) and `content_search.retained_match_count`.
  `content_search.match_count` and `search.workspace_match_count` stay the
  retained count, so existing assertions keep meaning what they meant. For the
  workspace tree, projected from `WorkspaceTreeEvidence` into the existing
  `AutomationWorkspaceSnapshot`: `workspace.limited_directory_count` and
  `workspace.loading_directory_count`. Everything
  is documented in `docs/automation.md` and `docs/automation-reference.md`,
  with `make check-automation-docs`.
- The AT-SPI anchor `atspi-anchor-workspace-search-status-bar-16-384-results-truncated`
  is replaced by `atspi-anchor-workspace-search-limited` (prefix "Showing").
  The accessibility smoke case `workspace-search-capped` keeps its id. Its
  fixture reaches the budget through a smoke-only override
  (`LUSHTEXT_SEARCH_RETAINED_BYTES`), compiled only under a new Cargo feature
  `smoke-list-budgets`. The accessibility and visual smoke builds enable it
  beside `smoke-read-faults`, and release, Meson, Flatpak and Snap never do
  (the `smoke-read-faults` pattern). A real 256 MiB fixture is not a smoke
  fixture.
- Matrix rows `A11Y-WORKSPACE-SEARCH-DENSE-NORESULTS`,
  `A11Y-WORKSPACE-SEARCH-REPLACE`, `A11Y-WORKSPACE-DENSE-DEEP` and
  `A11Y-WORKSPACE-BUSY-ERROR` are updated (limited state, beyond-budget
  count, the loading tail row and the budget row), and `docs/accessibility.md`
  states the announcements.

## Risks / Trade-offs

- **Memory at the budget is larger than at a count cap.** That is the point:
  `main`'s worst case is the floor, not the ceiling. The budget is still a
  hard bound, and a memory test holds it (`VmHWM` growth at most
  `budget × 5/4`). Typical searches retain far more matches than 16,384 in
  much less than 256 MiB.
- **Many expanded huge directories.** The budget is per directory, as the
  count was, so N expanded directories at the budget retain N budgets. That
  was already the case with N × 10,000 rows. A process-wide sidebar ledger is
  recorded as a follow-up in the programme record, not added silently.
- **Counting past the budget keeps the worker busy.** It costs CPU, not
  memory. Escape and close cancel it as today, and the progress lease keeps
  the status bar honest.
- **A toolkit floor on the first realize** may remain. D4 records it as an
  exception with a probe and a measurement. It is never left unexplained.
- **Smoke-only override.** It must not exist in shipped builds.
  `make check-filesystem-boundary`-style auditing via the default-feature
  `cargo check -p lushtext --bins` proves its absence, the same pattern as
  `smoke-read-faults`.

## Migration

No persisted format changes. Search history, saved searches, the Replace All
journal and `workspaces.json` are untouched. The automation snapshot only adds
fields. The renamed AT-SPI anchor is documented, and the old anchor's text no
longer exists, because the label no longer says "16,384+".

## Open questions (answered by tasks, not by the reviewer)

- The two fixed charges and the final budget values (task 2.4). The rule
  for the outcome is fixed now (D3), so only the numbers wait.
- The reveal's residual, if any, after batching (task 3.5). The rule for the
  outcome is fixed now: fit `B_SETTLE_US` or record a measured toolkit floor.
- Whether the end-of-store splice is O(batch) (task 1.3). The fallback is
  decided in D6.

## Spec review (stage 2, 2026-11-06)

An independent review against `origin/main` `469e3f84` changed these, each
recorded here so the implementation does not re-derive them:

- **Role homes.** The workspace-tree workflow (`WFR-WORKSPACE-TREE`) keeps its
  one pure policy at `ui/sidebar/policy.rs`, its evidence at
  `ui/sidebar/evidence.rs` and its test overrides at `ui/sidebar/test_policy.rs`.
  `ui/sidebar/workspace_section/` has no `policy.rs` or `evidence.rs`; the
  first draft named them there, which `make check-workflow-boundaries` would
  have rejected.
- **Parallel admission (D4).** `WalkParallel` delivers from several threads, so
  the prefix property needed a serialization point. Admission and the send of
  an admitted match are one step under a per-search lock; the beyond-budget
  count is lock-free. Without it the Kani proofs would describe a sequential
  model the running code does not follow.
- **`BeyondBudget` cadence (D4).** The `Progress` cadence is every 100 files,
  not 250 ms, so the 250 ms coalescing is a worker clock, best-effort.
- **Search worst case (D3).** The per-file charge counts two path copies, so
  `main`'s worst case is 192 MiB plus 16,384 fixed charges. 256 MiB holds for
  a fixed charge up to 4 KiB.
- **Sidebar floors (D3).** The 131,072-entry target is a second derived floor
  the budget must cover, not a check that can fail after the budget is fixed.
  At about 1 KiB per typical row it needs 128 MiB, above `main`'s worst case,
  so the typical floor is expected to decide. User experience first: the
  target stays, and what a row retains shrinks before anything else.
- **Replace Preview's own budget (D4).** A search can now show more matches
  than one preview admits. The 40,000-match scenario no longer claims every
  match is "offered to Replace Preview"; the preview states its omitted count,
  as it already does, and a scenario pins that it never replaces an omitted
  match.
- **T2's reach (D2).** T2 bounds the guarded loop only; search's post-drain
  splices are bounded by the 256-event cap and by task 8.2's measurement.
- **`LIST_TURN_SLICE_US` kind.** `out: time`, as every time budget in
  `docs/thresholds.md` (`REANALYSIS_ALLOWANCE_US` is the precedent).
- **Sidebar scan and disposal (D6).** An over-budget directory costs two
  passes, not three, and a superseded budget-sized plan retires off GTK or by
  a measured drop.

The baseline was not re-measured in this review. PT's raw lines
(`amend/widget/evidence.txt`) were checked against the table above and agree,
and task 1.1 re-measures before any fix.
