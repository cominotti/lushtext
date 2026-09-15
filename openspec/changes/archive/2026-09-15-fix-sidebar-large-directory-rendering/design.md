## Context

**Symptom.** A workspace folder with roughly 400 entries, two levels below the workspace root, shows only its first ~200 rows in the sidebar; the rest is blank space the user can scroll through.

**Proof (2026-09-14, widget test against `LushtextWindow` + `LushtextSidebar`, headless mutter, GTK 4.22.5).** A generic fixture of 300 files in `root/nested/` gave:

| Measure | Value |
| --- | --- |
| Rows in `tree_model` | 302 (root, `nested`, 300 files) |
| Row widgets realized by `GtkListView` | 205 |
| Last realized row | `row-0202.txt` |
| Bottom of last realized row | 7,790 px |
| List view allocated height | 11,486 px |
| Outer scroller upper / page | 11,541 / 665 px |

Scrolling the outer scroller to its lower edge changed nothing: the inner adjustment value stayed 0 and its page size stayed equal to the full content height. The same numbers reproduced with the original ~400-entry name set (421 model rows, 205 realized). A first test that only inspected the tree model found all rows present, which isolates the defect to presentation.

**Mechanism.** `gtk/gtklistview.c` sets `gtk_list_base_set_anchor_max_widgets(GTK_LIST_VIEW_MAX_LIST_ITEMS = 200, GTK_LIST_VIEW_EXTRA_ITEMS = 2)`. `GtkListView` realizes at most that many row widgets around its anchor for one visible range. The sidebar's `workspace-section.blp` wraps each section's list view in `GtkScrolledWindow(vscrollbar-policy: never, propagate-natural-height: true)`, so the list view's viewport is its whole content; anything past ~205 rows is allocated but never populated. The current `AGENTS.md`/`ui.md` "Inner ScrolledWindow pattern" exists to give the list an adjustment and avoid crashes, and it does that, but it also disables virtualization.

**Constraints.** Keep one `GtkListView` + `GtkTreeListModel` per `LushtextWorkspaceSection` (13,599 production lines of WFR-WORKSPACE-TREE depend on it: scan/reconcile runtime, watch, DnD, context menus, peek, accessibility rows, evidence). Keep the fixed workspace-scope row, no horizontal scrollbar, ellipsized labels, `propagate-natural-width: false`. Reusable GTK helpers belong in GTK Lush (`gtk-lush-stewardship`). Rows are uniform height, which makes list-view height estimates exact after one row is measured.

## Goals / Non-Goals

**Goals:**
- Every tree-model row renders and is reachable, for any count up to the 10,000-entry placeholder.
- Realized row widgets bounded by viewport, not by model length.
- No change to section semantics, models, runtime, readiness, or evidence.
- Reusable, property-tested container in GTK Lush with proof-harness coverage.
- Regression and edge-case tests that assert **rendered** rows, plus a visual-proof scenario.
- Rules/skills record the GTK cap so the pattern is not reintroduced.

**Non-Goals:**
- Collapsing all sections into one sidebar-wide list (see Decision 1 alternatives).
- Changing directory caps, batching, watch, persistence, or automation snapshot fields.
- Nested per-section scrollbars.
- Upstream GTK changes.

## Decisions

### Decision 1: Virtualize with a viewport-slice container, keep per-section list views

Introduce `gtk_lush_widgets::ViewportSliceBin` (name provisional; `GtkLushViewportSliceBin` in Blueprint) and make it the section's list host instead of the inner `GtkScrolledWindow`.

Behaviour:
- **Measure.** Vertical: `min = child.min`, `nat = child.nat` (the list view's natural height already equals its full content height, as the proof shows). Horizontal: pass through, with `propagate-natural-width`-equivalent behaviour preserved by simply reporting the child's minimum as natural so deep indentation cannot widen the sidebar.
- **Allocate(width, H).** Compute the container's offset inside the outer scroller's content (translate the container origin to the outer `GtkScrolledWindow` child). Let `viewport_top = outer.value − offset`, `page = outer.page_size`. Apply the pure policy `slice(viewport_top, page, H, overscan)` → `(slice_top, slice_h)`. Allocate the child at `y = slice_top`, height `slice_h`, and set the child's vertical adjustment to `value = slice_top` (`upper = H`, `page_size = slice_h`). The child is a `GtkListView`, which is `GtkScrollable`; the container installs its own `GtkAdjustment` on it (`set_vadjustment`), so no inner `GtkScrolledWindow` is needed. Horizontal adjustment stays a fixed zero-range adjustment.
- **Outer → inner sync.** Observe the outer vertical adjustment (`value-changed`, `notify::page-size`) through `gtk_lush_viewport::ViewportObserver` and `queue_allocate()`.
- **Inner → outer sync.** `GtkListView` applies `scroll_to` and keyboard-focus scrolling *inside its own allocation*, which runs within the container's `size_allocate`. So after `child.allocate(...)` the container reads the child adjustment back; any divergence from the slice offset is a request. The delta is measured against the **unclamped** viewport top, not the clamped slice offset: at the top of the content the slice starts at 0 while the viewport starts above the bin (a section header sits there), and that gap is part of the distance the outer scroller must travel (found during implementation; forwarding `requested − slice_top` undershot by exactly the header height). The delta is accumulated and applied to the outer adjustment from an idle, because moving the outer adjustment from inside a layout pass did not reliably schedule another layout under the harness.
- **Re-entrancy guard.** A `Cell<bool>` marks container-originated adjustment writes so the `value-changed` handler ignores them; the post-allocation read-back is what catches in-allocation child requests. `Adjustment::set_value` clamps and emits nothing for an unchanged value, so a request the outer scroller cannot honour ends there rather than re-queuing allocation.
- **Measurement (two GTK facts found during implementation).** A `GtkListView` outside a scroller reports its whole content as *minimum* height, so the bin must ignore the child's vertical minimum (as `GtkScrolledWindow` does). And `GtkViewport` allocates a non-scrollable child its *minimum* in the scroll direction, so while an outer scroller is present the bin advertises the full content as minimum too (the old layout got this by accident because the list's min equalled its natural); standalone, the minimum is zero so a bare host window is not forced to the content height.
- **Overscan.** Zero by default (a configurable `overscan` property remains). With any positive overscan the child's own page is larger than the real viewport, so `GtkListView` judges a row "visible" that the user cannot see and its `scroll_to`/keyboard-focus logic stops requesting scrolls; zero keeps the child's viewport identical to the visible one, which is what makes forwarding correct.

The outer scroller and the container hand off through the adjustment the container discovers at map time by walking ancestors to the nearest `GtkScrolledWindow` (or an explicit `outer-adjustment` property for non-scrolled-window hosts and tests).

**Alternatives considered:**
- *One sidebar-wide `GtkListView` over a `GtkFlattenListModel` of per-workspace `GtkTreeListModel`s, with `header-factory` rendering workspace headers (GTK ≥ 4.12 `GtkSectionModel`).* Idiomatic and equally robust, but it rehosts every per-section presentation surface (headers with refresh/context menus, drilldown header, zero-folder empty state, per-section action groups, DnD, row factory, peek anchoring, dozens of widget tests on `section.imp().file_tree_view`), and zero-item sections produce no header. Blast radius across the largest migrated workflow is disproportionate to a rendering defect. Rejected for this change; recorded as the long-term option if the sidebar is redesigned.
- *Per-section `GtkScrolledWindow` with automatic policy and a bounded `max-content-height`.* Nested scrollbars inside a scrolling sidebar; rejected on UX grounds.
- *Non-virtualizing `GtkListBox`.* Loses `GtkTreeListModel`/`GtkTreeExpander`, creates up to 10,000 widgets per directory; rejected.
- *Realizing more items via GTK.* The cap is a compile-time constant; not configurable.

### Decision 2: The slice geometry is a pure function in GTK Lush

`slice(viewport_top: f64, viewport_h: f64, content_h: f64, overscan: f64) -> Slice { top, height }` lives in a GTK-free module of `gtk-lush-widgets`, unit-tested and property-tested (proptest, bounded by `make test-prop`): slice inside content; slice ⊇ viewport ∩ content; content shorter than viewport → whole content at 0; idempotent; small viewport moves keep coverage. The widget's `size_allocate` only translates coordinates and applies the result. This mirrors the repo's policy-purity convention without making the container a LushText workflow.

### Decision 3: Section template and wiring change is minimal

`workspace-section.blp`: replace `ScrolledWindow inner_scrolled_window { … child: ListView file_tree_view }` with `$GtkLushViewportSliceBin file_tree_slice { child: ListView file_tree_view }`. `imp.rs`: rename the template child, `ensure_type()` the new widget in `class_init`, drop `set_propagate_natural_height`/`set_max_content_height` test hooks (the two tests that bound height for fixtures use the window height instead). `select_and_scroll_to`, context-menu `scroll_to`, and peek anchoring keep calling `file_tree_view.scroll_to`; the container translates it to the outer scroller. `list_execution.rs` (sidebar scrolls a focused header to the top via the outer adjustment) is unchanged.

### Decision 4: Tests assert rendered rows, not model rows

**Visual lane outcome (implementation finding).** A `cargo-gtk-proof`/visual-smoke scenario for this defect was attempted and withdrawn: the headless capture harness cannot expand a folder or scroll the tree. Synthesized keys (`atspi-key`) do not reach the toplevel under headless Mutter (only popover-focused Escape works), tree rows expose no AT-SPI actions (`actions=[]`) so `atspi-activate-accessible` cannot expand them, top-level folder context menus have no Focus Folder entry, and opening a document does not reveal its row. Rendered-row proof therefore lives in the widget lane against the real `LushtextWindow` and its outer scroller (17 scenarios), plus the adoption lab's 1,000-row demo. A screenshot case becomes possible once an automation-only `reveal-workspace-path` target-state action exists; that is recorded as a deferred follow-up in `tasks.md`, not silently dropped.

Add a shared widget-test helper `rendered_row_labels(list) -> Vec<String>` that walks realized children of the list view and reads their bound label, plus `scroll_outer_to_bottom(sidebar)`. Scenarios from the specs run at 199/200/201/202/205/300/1,000 rows, nested depth 2, two sections of 250, filter hide/show, keyboard Down traversal, `select_and_scroll_to` row 280, refresh while scrolled with pending selection, resize 800→500→800, collapse, 10,001-entry placeholder, 200-character names. A `cargo-gtk-proof` visual scenario screenshots the sidebar after scroll-to-bottom and asserts the last row label region is non-blank and the band below the last row is the sidebar background. Existing `test_large_reconciliation_is_batched_supersedable_and_preserves_state` gains a rendered-last-row assertion.

Widget-test pitfall recorded in the gtk-testing skill: `sidebar.load_workspaces()` rebuilds sections after the first `sections` entry appears; take the section handle only after it is mapped, or the geometry read is against a detached instance (0 px, 48 px stale allocations).

### Decision 5: Knowledge registration

- `.agents/rules/ui.md`: replace "Inner ScrolledWindow pattern" with "Viewport slice pattern", stating the 200-item cap and that a `GtkListView` must never be given its whole content as viewport when it can exceed ~200 rows; update the widget-hierarchy diagram.
- `AGENTS.md`: same for the Key Design Decision bullet and module layout (`gtk-lush-widgets` gains the container).
- `.agents/skills/gtk4-libadwaita-internals/references/containers-lists-and-factories.md`: add the cap constant, its source, symptom, and the slice pattern under "GtkListView Virtualization".
- `.agents/skills/gtk-testing/references/widget-testing.md`: rendered-vs-model assertions, the stale section handle pitfall, `LUSHTEXT_WIDGET_CHILD`-safe scroll helpers.
- `.agents/rules/widget-wiring.md`: adjustment ownership when a `GtkScrollable` lives under a custom container.
- `docs/workflow-readability-matrix.md` WFR-WORKSPACE-TREE: re-derive size cells; note `imp.rs`/template as the changed called presentation surface. `docs/accessibility-matrix.md` A11Y-WORKSPACE-DENSE-DEEP: add the rendered-last-row widget proof and visual scenario. `docs/end-user-coverage.md`: visual lane row. `docs/gtk-lush-adoption/matrix.toml`, `crates/gtk-lush/widgets/README.md`, `CHANGELOG.md`, public-API snapshot.

### Decision 6: Simplification pass outcome

The post-implementation `/simplify` review (reuse, simplification, efficiency, altitude) was applied selectively:

- **Applied.** The one-child parenting contract moved to a crate-private `single_child.rs` shared by `ClipBin` and `ViewportSliceBin`; the bin's redundant `slice_top`/`slice_height` cells and getters were removed (the adjustment already holds the band); `measure` holds the child borrow instead of cloning; the widget tests gained shared `realized_list_rows`/`find_descendant` helpers in `common.rs`, a `LargeTree` fixture struct, `seed_files`, `select_and_scroll_to`, `wait_for_refresh_idle`, f64 comparisons in place of two cast helpers, and the tautological purity property was dropped; the adoption lab lost a compile-time assertion on two literals.
- **Tried and reverted.** Replacing the accumulated outer-scroll delta with a "latest requested offset" applied from the idle regressed the harness traversal scenario (the frame clock froze after the idle-driven scroll and the row never entered the viewport), so the proven accumulated-delta form stays. Skipping `Adjustment::configure` when values are unchanged was also reverted for the same run and is not needed.
- **Kept as specified.** The `overscan` and `outer-scrolled-window` properties (spec'd, unused by LushText), the f64 slice geometry (an integer rewrite would churn the property lane for no behaviour gain), and binding to the nearest `GtkScrolledWindow` rather than any `GtkViewport`.
- **Recorded, not done.** The sidebar-wide single-list redesign (Decision 1 alternatives) remains the long-term root-cause option.

## Risks / Trade-offs

- [Allocation feedback loop between outer and inner adjustments] → Re-entrancy flag on container-originated writes; only `queue_allocate`, never synchronous allocation, from adjustment handlers; widget test counts allocations per frame during a two-second drag and asserts no GTK warnings.
- [List-view natural height is an estimate until rows are measured, so the outer range could jitter] → Rows are uniform height; the estimate becomes exact after the first realized row. The property test fixes coverage, and the 1,000-row test asserts `outer.upper == header + rows` after settle.
- [Offset computation during `size_allocate` reads ancestor geometry] → Ancestors are allocated top-down before the container, so `compute_bounds`/`translate_coordinates` to the outer scroller's child is valid; fall back to the last known offset if translation fails, and re-slice on the next frame.
- [Keyboard traversal relies on `GtkListView` calling `scroll_to` into an adjustment we own] → Covered by the Down-traversal scenario; if GTK adjusts before focus lands, the inner→outer handler still centres the row.
- [Popover anchoring (peek, context menu) to rows whose position moves as the slice re-allocates] → Rows inside the viewport keep stable outer coordinates; popovers already dismiss on scroll.
- [Regression in the focus-folder "scroll header to top" path] → Unchanged code path over the outer adjustment; scenario added.
- [GTK Lush governance cost for a widget with one consumer] → Adoption lab becomes the second consumer, as policy requires; the cap is a general GTK trap worth a reusable fix.

## Migration Plan

Single change: land the widget, then the section template/wiring, then tests and docs, gated by `make check`, `make test`, `make test-prop`, `make check-gtk-lush-policy`, `make check-gtk-lush-adoption`, `make check-workflow-boundaries`, `make check-visual-proof-policy`, `make visual-smoke`, `make accessibility-smoke`. Rollback is reverting the template and `imp.rs` wiring; the widget can stay in GTK Lush unused.

## Open Questions

- Final widget name (`ViewportSliceBin` vs. `VirtualStripBin`); pick during implementation, keep `Bin` suffix for parity with `ClipBin`.
- Overscan default: one viewport height each side keeps keyboard traversal smooth; tune with the 1,000-row test if realized counts exceed ~3× viewport rows.
- Whether to record the sidebar-wide single-list redesign in a `docs/next/` note as a deferred option (recommended: one paragraph, no commitment).
