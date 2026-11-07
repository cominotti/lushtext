## Why

The maintainer's standard (2026-11-05) is that LushText be extremely
performant, handle large workspaces, and show no user-visible loss.
`bound-lists-by-budget-not-count` (BL) applied it to workspace search and the
workspace sidebar. Its design D1 queued the notes browser as the next slice.

`Browse Notes…` and `Browse Bookmarks…` still cut at counts the user can feel:

- **The rendered list stops at 512 rows** (`NOTES_BROWSER_RENDER_LIMIT`). The
  browser then says "Showing first 512 matches. Refine search to narrow
  results." The user cannot scroll to row 513. That happens even though the
  worker already holds the row in an immutable, byte-bounded source.
- **The source stops at 16,384 rows** (`NOTES_BROWSER_SOURCE_ENTRY_LIMIT`),
  **16,384 sidecars per scan** (`NOTES_BROWSER_SIDECAR_SCAN_LIMIT`), and
  **16,384 open-editor snapshots and bookmarks**
  (`NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT`). Byte ceilings already bound
  each of these: 64 MiB of searchable text, a 64 MiB retained graph, 8 MiB of
  sidecar paths, 96 MiB of construction scratch, and 4 MiB of open-editor
  snapshots. The counts only hide rows.

`main` also misses its dispatch budget here. PT's M6 (interleaved, load below
4) measured the worst dispatch from activating Browse Notes until its rows
are shown at **32–38 ms on `main`** (500 rows) and 34–40 ms at 512 rows.
`B_SETTLE_US` is 16 ms. The cause is in the toolkit. `AdwSidebar` (libadwaita
1.9.3, `adw-sidebar.c`) is a `GtkListBox` bound to a flattened model through
`gtk_list_box_bind_model`. It therefore builds one `GtkListBoxRow` widget tree
for **every** item, including rows that are not on screen, and every query
rebuilds all of them (`remove_all` and then one `SidebarItem` per row). The
browser also wraps the sidebar in a second `GtkScrolledWindow`, which hands
the inner list its full natural height. A count cap was the only thing that
kept this linear rebuild bounded. Under the pre-existing-blockers rule, the
fix is part of this work.

## What Changes

- **Probe first.** A GTK axiom with a sample (A92) pins what the source shows:
  in sidebar mode, `AdwSidebar` builds one row widget per item, including
  off-screen items, and its rebuild cost grows with the item count.
- **A virtualized browser list.** The browser drops `AdwSidebar` for a
  `GtkListView` with section headers. The list view sits on a
  `GtkSingleSelection` over one custom `GListModel` that also implements
  `GtkSectionModel`, and a `header-factory` draws the section titles. It is
  styled `navigation-sidebar` and placed directly in its own
  `GtkScrolledWindow`.
  - The model holds only the published grouped indexes and the section
    bounds (`Arc<[u32]>`). It creates row objects only for rows the view
    asks for.
  - Publishing a query is one `items-changed`, so GTK work no longer grows
    with the result count (A87 and the new A93).
  - The category grouping and section bounds move from GTK into the query
    worker.
  - Everything the browser does today stays: the four sections, both modes,
    selection-driven preview, explicit Open, the bookmark row's Open button,
    keyboard paths, the accessible names, and every generation guard.
- **No count caps in the browser.**
  - `NOTES_BROWSER_RENDER_LIMIT` is removed. Every row the source admitted
    and the query matched is listed.
  - The browser's source entry count and sidecar scan count are removed.
    Its existing byte ceilings govern admission and keep their values and
    skip-and-continue semantics, so the browser never retains fewer rows
    than `main` did.
  - The open-editor snapshot is captured in time-sliced GTK turns
    (`model/list_budget.rs`: `turn_continues`). Its 4 MiB retained-byte
    limit is admitted through `RetainedBudget`, and it has no count cap.
    Each editor is captured whole or restarted when its bookmark generation
    changes between slices.
  - The command palette keeps its own counts. Its per-category cut is
    `drill-down-palette-categories`.
- **"Showing X of Y", once.** If a byte ceiling cuts the source, source
  construction keeps counting the rows it parses without retaining them. The
  browser then says "Showing X of Y notes and bookmarks" ("bookmarks" in
  Browse Bookmarks), or "of at least Y" when a scan ceiling stopped the
  count. It announces this once per source generation and never per query.
- **Budgets.** Every dispatch from activation until the first rows and the
  first preview are shown, each query publish, and each snapshot turn fit
  `B_SETTLE_US`. The input dispatch of a keystroke in the search field fits
  `B_INTERACT_US`. These hold at 512, 16,384 and 131,072 rows. Any residual
  is a measured toolkit-floor exception with its own axiom.
- **Contracts.**
  - Automation snapshot: new `notes.browser_open`,
    `notes.browser_row_count`, `notes.browser_rows_found` and
    `notes.browser_limited`.
  - Accessibility: the browser's AT-SPI anchors keep their names. The
    `A11Y-NOTES-*` matrix rows and the Orca checklist gain the limited
    state, its single announcement, and the section headers.
  - A smoke-only `LUSHTEXT_NOTES_SOURCE_TEXT_BYTES` override lives under the
    existing `smoke-list-budgets` feature, for a new accessibility case
    `notes-limited`.
  - `docs/thresholds.md` drops the four count rows and gains the snapshot
    slice row, and `make check-thresholds` fails first on any of them left
    behind.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-notes`: two changes:
  - The bounded source and query requirement drops the render cap, the
    browser's source counts and the snapshot count, and adds the sliced
    capture.
  - The folder-note rail requirement is renamed to describe a virtualized
    rail.
  - New requirements: every admitted row is listed, and a byte cut is stated
    once.
- `document-notes`: the document-note rail requirement is renamed and
  modified in the same way.
- `main-thread-responsiveness`: the notes browser's dispatches fit the
  interaction budgets.
- `dbus-automation-spine`: the snapshot reports the browser's listed and
  found rows.
- `gtk-accessibility-spine`: the browser's limited state and section headers
  are stated and announced once.

## Impact

- **Code:**
  - `ui/window/notes/`: `browser.rs`, `query_execution.rs`,
    `source_execution.rs`, `policy.rs`, `evidence.rs` and `test_policy.rs`.
  - A new called presentation surface, `ui/window/notes/row_model.rs`, for
    the custom list and section model.
  - `services/palette/notes.rs`: optional count limits, a found-row count,
    and the grouped query result.
  - The automation snapshot projection.
  - `gtk-lush-axioms`: A92–A95, each with a probe and a sample.
- **Tests:** about 35 widget tests in `crates/lushtext/tests/widget/window.rs`
  find the browser through `find_adw_sidebar`. They move to a list-view
  helper and keep their assertions. New failing-first widget tests cover no
  visible loss, every budget, the sliced capture, keyboard and accessibility
  parity, and the generation guards.
- **Docs:**
  - `docs/thresholds.md`, `docs/automation.md`,
    `docs/automation-reference.md`, `docs/accessibility.md`,
    `docs/accessibility-matrix.md` and
    `docs/accessibility-orca-checklist.md`.
  - `docs/workflow-readability-matrix.md` (`WFR-NOTES-BOOKMARKS`
    re-derived).
  - `AGENTS.md`/`.claude/CLAUDE.md` (bookmarks and notes) and the GTK axiom
    ledger.
  - `docs/next/long-line-documents.md`.
- **Unchanged:** persisted formats, sidecars, actions, and the
  `win.select-notes-browser-row` index meaning (the flat visible row order,
  headers excluded).
- **Kani:** `core-list-budget` is reused unchanged; it is rerun only if
  `model/list_budget.rs` changes. No new shard (design D7).
