## Why

The workspace sidebar silently stops drawing file-tree rows after roughly the two-hundredth row: a nested folder with about 400 entries shows its first ~200 and then blank space, even though every entry is present in the tree model. The sidebar must always show every item the tree model holds, so a user can trust that what they see is the directory.

## What Changes

- **Root cause, proven 2026-09-14 with a widget test against the real window and sidebar.** `GtkListView` caps the row widgets it realizes for one visible range at `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200) plus 2 extra items per tracker (`gtk/gtklistview.c`, GTK 4.22.5). The sidebar's inner `GtkScrolledWindow` runs with `vscrollbar-policy: never` and `propagate-natural-height: true`, so the list view's own viewport *is* the whole list. GTK allocates the full height (11,486 px for 302 rows) but populates only the first 205 rows (ending at 7,790 px). Scrolling the outer scroller never realizes more, because the inner adjustment never moves. A synthetic 300-file directory reproduces it: model rows 302, realized 205, last rendered row `row-0202.txt`. The data path (scan, 10,000-entry cap, 256-row reconciliation batches, byte selector) is complete and innocent.
- Replace the inner "natural-height scrolled window" presentation with a **virtualized viewport slice**: each section's list view keeps advertising its full natural height to the outer scroller, but is allocated only the band of rows that intersects the outer viewport (plus a bounded overscan), with its own vertical adjustment driven from the outer scroller. Row widgets exist only for visible rows, so no row count can exceed the GTK cap, and the outer scroller's range stays exact.
- Keep every existing sidebar contract: one `GtkListView` + `GtkTreeListModel` per workspace section, the fixed workspace-scope row, no horizontal scrollbar, ellipsized labels, DnD, context menus, peek, focus-folder drilldown, the 10,000-entry truncation placeholder, bounded reconciliation, watch, and readiness evidence.
- Add the slice container as a reusable **GTK Lush widget** (`crates/gtk-lush/widgets`), with a GTK-free pure geometry policy, unit and property tests, a proof-harness example, and adoption-lab evidence, following the existing `ClipBin` shape.
- Add **regression and edge-case coverage**: rendered-row assertions (not model-row assertions) at 199/200/201/202/205/300/1,000 rows, multi-section totals, keyboard navigation to the last row, scroll-to-row and refresh while scrolled, window resize, collapse/expand, workspace-filter hide, the truncation placeholder at row 10,001, plus a visual-proof scenario that the last row is rendered and no blank band exists.
- **Register the knowledge**: document the GTK cap and the slice pattern in `.agents/rules/ui.md`, `AGENTS.md`, the `gtk4-libadwaita-internals` and `gtk-testing` skill references, and the GTK Lush README/CHANGELOG/API advisory; retire the "Inner ScrolledWindow pattern" guidance that produced the defect.

## Capabilities

### New Capabilities
- `workspace-tree-virtualized-rendering`: every row present in a workspace section's tree model is rendered and reachable regardless of row count, with row widgets bounded by the visible viewport rather than by the list length.
- `gtk-lush-viewport-slice`: the reusable GTK Lush container that hosts a `GtkScrollable` child at full natural height inside an outer scroller while allocating only the visible slice, including its pure slice-geometry policy.

### Modified Capabilities
- `workspace-sidebar-shell`: ADDED requirement that the scrollable workspace-section area renders every tree row (the shell's dense-row scrolling scenarios currently assert scrolling, not completeness).
- `gtk-lush-workspace`: ADDED requirement that the slice container is a governed `gtk-lush-widgets` API with README, CHANGELOG, snapshot, proof example, and adoption-lab evidence.

## Impact

- **Code**: `resources/ui/workspace-section.blp` (+ generated `.ui`), `crates/lushtext-core/src/ui/sidebar/workspace_section/{imp.rs,mod.rs,folder_execution.rs,context_menus.rs,peek_execution.rs}`, `crates/lushtext-core/src/ui/sidebar/{imp.rs,list_execution.rs}`; new module in `crates/gtk-lush/widgets/src/`; `crates/gtk-lush-adoption-lab`; `workspace-hack` if features change.
- **Tests**: `crates/lushtext/tests/widget/{workspace_section.rs,sidebar.rs}`, `crates/lushtext-core/tests/properties/`, GTK Lush widget unit/property tests and proof-harness example, a visual-proof scenario under `scripts/visual-geometry-scenarios` gated by `scripts/check-visual-proof-policy.py` and `crates/cargo-gtk-proof/src/policy.rs`.
- **Docs and rules**: `AGENTS.md` (widget hierarchy, "Inner ScrolledWindow pattern" decision), `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, `.agents/skills/gtk4-libadwaita-internals/references/containers-lists-and-factories.md`, `.agents/skills/gtk-testing/references/widget-testing.md`, `docs/workflow-readability-matrix.md` (WFR-WORKSPACE-TREE re-derived cells), `docs/accessibility-matrix.md` (A11Y-WORKSPACE-DENSE-DEEP proof mapping), `docs/end-user-coverage.md`, `docs/gtk-lush-adoption/matrix.toml`, `crates/gtk-lush/widgets/{README,CHANGELOG}.md`, `README.md` architecture note.
- **No dependency changes**; GTK 4.22 and gtk4-rs 0.11 already expose everything needed (`GtkScrollable` adjustments, `WidgetImpl::measure`/`size_allocate`).
- **Not affected**: directory scanning, reconciliation batching, watch service, session or workspace persistence, automation snapshot fields.
