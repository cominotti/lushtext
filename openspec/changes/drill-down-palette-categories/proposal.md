## Why

The maintainer's standard (2026-11-05) is that LushText be extremely
performant, handle large workspaces, and show no user-visible loss.
`bound-lists-by-budget-not-count` (BL) applied it to workspace search and the
workspace sidebar, and `bound-notes-browser-by-budget` (BN) to Browse Notes.
This is the third slice, queued by BL's D1 and BN's follow-ups: the command
palette.

The palette still hides results the user cannot reach:

- **Each category shows at most 64 results** (`MAX_RESULTS_PER_SOURCE`). A
  workspace with 131,072 indexed files shows 64 of them for the empty query,
  and 64 of the 40,000 that match `rs`. Nothing says that more exist, and
  nothing reaches row 65 except a narrower query. The same holds for open
  tabs, each note category and commands.
- **The palette's note source stops at 16,384 rows and 16,384 sidecars per
  scan** (`MAX_PALETTE_NOTE_ENTRIES`), and at the first byte refusal (`Stop`).
  Its byte ceilings (64 MiB of text, a 64 MiB retained graph, 8 MiB of sidecar
  paths, 96 MiB of construction scratch) already bound it. BN removed the same
  counts from the browser and recorded the palette's as this change's concern.

The cap itself is right for the grouped view: 64 rows per category keep the
mixed list scannable and its splice within `B_INTERACT_US` (PT M1: 6.4–7.9 ms
with every category full). What is missing is a way past it.

## What Changes

- **An "N more…" row ends every truncated category.** It is a normal,
  activatable row (Enter, double-click, the arrow keys reach it) whose
  accessible name states the count ("Show 1,234 more results in All
  Workspaces"). Categories that are not truncated show no such row.
- **Activating it opens the category view.** The palette shows only that
  category, with every ranked result in a virtualized list, a title naming the
  category and its result count, and a back button.
  - The ranking is the grouped view's ranking: the category view's first 64
    rows are exactly the rows the grouped view showed.
  - The list is a second `GtkListView` over a lazy `GListModel` that holds the
    ranked source ordinals (`Box<[u32]>`) and the request's source snapshots,
    and creates row objects only for the rows GTK asks for (BN's pattern,
    A93). Publishing a query costs the same at 64 and at 131,072 rows.
  - The ranked rows are bounded by a retained-byte budget through
    `model/list_budget.rs` (8 MiB, which holds the whole 131,072-file index).
    If the budget cuts, the view says "Showing X of Y results".
  - The query stays live: typing filters within the category through the same
    one-active/one-latest query flight, debounce, generations and cancellation.
  - **Escape**, **Backspace on an empty query**, the back button and a mode
    change return to the grouped view with the query kept, and select the
    category's "N more…" row again. Escape anywhere in the palette leaves the
    category view before it closes the palette.
  - Page Up and Page Down move the selection by a page in both views, so a
    long category is reachable from the keyboard.
- **Counts are exact.** A non-empty query already scores every candidate, so
  the count is free. For the empty query the file group counts the rest of the
  index with a name-prefiltered exclusion test (1.5–2.1 ms on the worker at
  131,072 files, against 7.4 ms for the full-path test; spec-stage
  microbenchmark).
- **The palette's note source loses its counts.** It admits by its byte
  ceilings alone and skips and counts a refused row (`SkipAndCount`), so it
  admits a superset of today's rows (BN D3's argument). A note category view
  says when the source itself was cut.
- **Automation:** `window.command_palette` gains `category`,
  `category_result_count`, `category_total_count` and `category_limited`; a
  new action `win.set-command-palette-category` opens or leaves a category
  view.
- **Toolkit facts:** two new GTK axioms with samples. A96: a `GtkListView` in a
  `GtkScrolledWindow` with `propagate-natural-height` and
  `max-content-height` measures in time flat in its model's item count. A97:
  `GtkSearchEntry` emits `stop-search` on Escape and keeps its text, and a
  capture-phase key controller sees Backspace on an empty entry.

Not changing: the 64-row grouped cap, nucleo scoring, the source priority,
deduplication, the file index's own 131,072-file ceiling
(`scale-file-index`), and the window-side palette shell.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `command-palette-source-groups`: the "N more…" row, the category view, exact
  counts, and the note source without counts.
- `gtk-accessibility-spine`: the "N more…" row's name, the category view's
  list label, title and announcement.
- `dbus-automation-spine`: the category snapshot fields and the category
  action.
- `main-thread-responsiveness`: grouped and category-view dispatches fit the
  interaction budgets.
- `transient-surface-dismissal`: Escape leaves the category view before it
  closes the palette.

## Impact

- `crates/lushtext-core/src/ui/command_palette/` (`WFR-COMMAND-PALETTE`):
  `policy.rs` (more-row, navigation, wording, category ids), `query_execution.rs`
  (the category target in the request), `evidence.rs`, `imp.rs`, `item.rs`,
  the facade, and a new called presentation surface `category_model.rs`.
- `crates/lushtext-core/src/services/palette/` (`grouped.rs` per-group counts,
  a category search, `fuzzy.rs` count continuation, `notes.rs` limits).
- `crates/lushtext-core/src/model/palette.rs` (`PaletteCategory`, the
  `More` row), `model/list_budget.rs` (a rows-for-budget helper with a Kani
  harness).
- `crates/lushtext-core/src/ui/window/transient_dismissal/policy.rs` (the
  palette rung leaves a category first), `ui/window/palette_shell.rs` (the
  action), `ui/automation.rs`, `services/action_catalog`.
- `resources/ui/command-palette.blp` and its generated `.ui`.
- `crates/gtk-lush/axioms` (A96, A97), the axiom ledger.
- Docs: `docs/thresholds.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `docs/accessibility.md`,
  `docs/accessibility-matrix.md`, `docs/accessibility-orca-checklist.md`,
  `docs/workflow-readability-matrix.md`, `docs/next/long-line-documents.md`,
  `README.md`, `AGENTS.md`.
