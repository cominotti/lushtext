## 1. Red tests that reproduce the defect

- [x] 1.1 Add widget-test helpers in `crates/lushtext/tests/widget/workspace_section.rs` (or `common.rs`): `rendered_row_labels(&gtk4::ListView) -> Vec<String>` walking realized children with non-zero height, `scroll_outer_to_bottom(&LushtextSidebar)`, and `mapped_section(&LushtextSidebar)` that waits for a mapped section after `load_workspaces()` (stale-handle pitfall)
- [x] 1.2 Add `test_large_directory_renders_last_row_after_scroll` (300 synthetic files in `root/nested/`, real `LushtextWindow`, assert last rendered label is the last file and no unrendered band) and confirm it fails on `main` with 205 realized rows
- [x] 1.3 Add boundary tests for 199, 200, 201, 202, 205 entries asserting the last row renders (pre-fix proof: the 300- and 421-row reproductions realized exactly 205 rows, the GTK cap plus extras; the boundary test runs green post-fix)
- [x] 1.4 Extend `test_large_reconciliation_is_batched_supersedable_and_preserves_state` — that fixture has no outer scroller, so it now asserts the list stays virtualized (realized rows in `1..200` for 500 model rows) instead of a scroll-to-bottom

## 2. GTK Lush viewport slice container

- [x] 2.1 Add GTK-free `slice_geometry.rs` to `crates/gtk-lush/widgets/src/` with `Slice` and `slice(viewport_top, viewport_h, content_h, overscan)`; unit tests for the design table values, content-shorter-than-viewport, zero content, negative viewport top
- [x] 2.2 Add bounded proptest properties (inside content, covers intersection, idempotent, small-move coverage) wired into `make test-prop`
- [x] 2.3 Add `viewport_slice_bin/{mod.rs,imp.rs}` following `clip_bin`: single `child` property, `overscan` property, optional `outer-adjustment` property with ancestor `GtkScrolledWindow` discovery at map, `measure` passing the child's natural height and minimum width, `size_allocate` applying the slice and setting the child `vadjustment`, fixed zero-range `hadjustment`, css name `lush-viewport-slice`
- [x] 2.4 Implement outer→inner sync (`value-changed`, `notify::page-size` → `queue_allocate`) and inner→outer sync (post-allocation read-back plus guarded `value-changed`, forwarded against the unclamped viewport top from an idle); GTK Lush family crates may not depend on each other, so plain handler ids are stored and disconnected on unroot/dispose instead of `ViewportObserver`/`SignalBag`
- [x] 2.5 Re-export from `lib.rs`, register the GObject type name `GtkLushViewportSliceBin`, add rustdoc with a `no_run` example
- [x] 2.6 Add proof-harness example under `crates/gtk-lush/widgets/examples/` hosting a 1,000-row `GtkListView` and asserting last-row rendering plus bounded realized count
- [x] 2.7 Add the pattern to `crates/gtk-lush-adoption-lab` as a second consumer and record it in `docs/gtk-lush-adoption/matrix.toml`
- [x] 2.8 Update `crates/gtk-lush/widgets/README.md`, `CHANGELOG.md`; run `make check-gtk-lush-policy`, `make check-gtk-lush-adoption`, `make gtk-lush-doctests`, `make gtk-lush-examples`, `make gtk-lush-public-api-advisory` (snapshots are generated into `target/`, not committed)

## 3. Sidebar adoption

- [x] 3.1 Edit `resources/ui/workspace-section.blp`: replace `ScrolledWindow inner_scrolled_window` with `$GtkLushViewportSliceBin file_tree_slice { child: ListView file_tree_view }`; regenerate with `make blueprint-generate`; `make check-blueprint`
- [x] 3.2 Update `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs`: rename template child, `ensure_type()` the slice bin in `class_init`, remove `inner_scrolled_window` uses
- [x] 3.3 Audit `folder_execution.rs::select_and_scroll_to`, `context_menus.rs` `scroll_to`, `peek_execution.rs` anchoring, and `sidebar/list_execution.rs` header scroll for correct behaviour over the outer adjustment; fix any that assumed the inner scrolled window
- [x] 3.4 Replace test hooks that called `inner_scrolled_window.set_propagate_natural_height(false)` / `set_max_content_height` with window-height bounding
- [x] 3.5 Run tasks 1.x tests and confirm green; run `make test-widget` in full

## 4. Edge-case coverage (specs `workspace-tree-virtualized-rendering`, `workspace-sidebar-shell`)

- [x] 4.1 Nested depth-2 directory of 400 entries: every row reachable, last rendered
- [x] 4.2 1,000 entries: first/middle/last rendered when scrolled into view; `outer.upper == header + rows`; realized count ≤ viewport rows + overscan and < 200
- [x] 4.3 Realized set follows the scroll position (top vs. middle of a 1,000-row section)
- [x] 4.4 Two workspaces each with 250 entries: both last rows render; fixed scope row stays visible
- [x] 4.5 Workspace filter hides a 300-row section (zero height, zero realized rows) and `All workspaces` restores it
- [x] 4.6 Keyboard traversal from row 1 to row 300 driven through the same `scroll_to(FOCUS | SELECT)` primitive the Down binding calls (the headless harness has no key synthesis): focused row rendered and inside the outer viewport at each checked step
- [x] 4.7 `select_and_scroll_to` row 280 of 300: rendered, selected, in viewport
- [x] 4.8 Pending selection at row 290 survives a batched refresh (120 removed, 120 added) while scrolled to the lower edge; outer value clamped, no blank band
- [x] 4.9 Space peek on row 270 anchors to the rendered row without resizing the split layout
- [x] 4.10 Focus Folder on a 300-entry directory scrolls the drilldown header to the top and keeps the last entry reachable
- [x] 4.11 Window heights 800 and 500 (fresh windows, since a presented GTK4 window does not shrink) keep the last row rendered and the outer value clamped
- [x] 4.12 Collapsing an expanded 300-row directory removes the rows and leaves no empty band
- [x] 4.13 Two-second continuous outer drag over 1,000 rows: geometry stays stable, realized rows stay within GTK's anchor window, the last row renders afterwards, and the harness warning classifier reports no GTK warnings
- [x] 4.14 Directory exceeding the 10,000-entry cap: the truncation placeholder row renders at the lower edge
- [x] 4.15 300 entries with 200-character names: labels ellipsize, no horizontal scrollbar, last row reachable
- [x] 4.16 Accessibility parity: row 260 exposes the same label/role/states as row 1 via `AccessibleAudit`; busy state and `workspace-refresh-complete` readiness unchanged by scrolling

## 5. Visual proof

- [x] 5.1 **Re-scoped after an attempt.** A `workspace-large-directory` visual-smoke case (300-file folder, expand, End, AT-SPI tree must hold the last row and not a row far above the viewport) was implemented and withdrawn: the headless capture harness cannot drive it. `atspi-key` events never reach the toplevel under headless Mutter (only the popover-focused Escape in the accessibility lane works), tree rows expose no AT-SPI actions so `atspi-activate-accessible` cannot expand them, the top-level folder context menu has no Focus Folder item, and opening a document does not reveal its row. Rendered-row proof is carried by the widget lane against the real window (17 scenarios) and the adoption lab. **Deferred follow-up:** add an automation-only `reveal-workspace-path` string action (expand ancestors, select, scroll into view) with `docs/automation*.md` updates, then add the screenshot case and its `A11Y-WORKSPACE-DENSE-DEEP` mapping
- [x] 5.2 No new `cargo-gtk-proof` scenario or policy key (see 5.1); `make check-visual-proof-policy` runs against the existing visual-geometry summary for this diff
- [x] 5.3 Run `make accessibility-smoke`, `make visual-smoke`, and `make visual-geometry-smoke` to confirm no regression in sidebar scenarios

## 6. Knowledge registration and documentation

- [x] 6.1 `.agents/rules/ui.md`: replace the "Inner ScrolledWindow pattern" bullet with a "Viewport slice pattern" bullet naming `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200 + 2) and the rule that a `GtkListView` must never receive its whole content as viewport when it may exceed ~200 rows; update the widget hierarchy diagram
- [x] 6.2 `AGENTS.md`: update the "Inner ScrolledWindow pattern" Key Design Decision, the GTK Lush architecture bullet (`widgets` gains the slice bin), and the Rules Index line for `ui.md` if its summary changes
- [x] 6.3 `.agents/skills/gtk4-libadwaita-internals/references/containers-lists-and-factories.md`: add the cap constant, upstream source location, symptom signature (allocated but unrealized band), and the slice pattern under "GtkListView Virtualization"
- [x] 6.4 `.agents/skills/gtk-testing/references/widget-testing.md`: rendered-vs-model assertions, `rendered_row_labels` helper, stale-section-handle pitfall after `load_workspaces()`, scroll-to-bottom helper
- [x] 6.5 `.agents/rules/widget-wiring.md`: adjustment ownership and re-entrancy guard rules for a `GtkScrollable` hosted by a custom container
- [x] 6.6 `docs/workflow-readability-matrix.md`: re-derive WFR-WORKSPACE-TREE size cells (files, production lines, seam counts) and note the changed called presentation surface; `make check-workflow-boundaries`
- [x] 6.7 `docs/accessibility-matrix.md` A11Y-WORKSPACE-DENSE-DEEP: rendered-last-row widget proof recorded and the deferred visual case noted; `docs/end-user-coverage.md` widget lane row names the rendered-row coverage
- [x] 6.8 `README.md`: architecture note that the sidebar virtualizes rows through the GTK Lush slice container; `docs/next/gtk-lush.md` posture update for the new widget
- [x] 6.9 The sidebar-wide single-list redesign is recorded as a rejected alternative in `design.md`; no separate `docs/next/` note (the GTK Lush record gained the widget entry instead)
- [x] 6.10 Run `make check-agent-skills`, `make check-agent-docs`

## 7. Final gates

- [x] 7.1 `make fmt`, `make check` (all policy audits including visual-proof and accessibility policy against fresh smoke summaries), `make test-unit` (1,709), `make test-int` (76), `make test-widget-headless` (all passed, no flaky retries), `make test-prop` (48), `make check-gtk-lush-policy`, `make check-gtk-lush-adoption`, `make check-workflow-boundaries`, rustdoc lint gate, `make visual-geometry-smoke`, `make visual-smoke`, `make accessibility-smoke`
- [x] 7.2 **User-gated.** The live `make run` check needs a person at the desktop (the agent cannot drive the real GNOME session); the automated stand-in is the real-window widget lane above. Suggested manual pass: open the workspace with the >400-entry nested directory, expand it, scroll the sidebar to the bottom, arrow-key to the last row, press Space for peek, click Refresh, and watch stderr for GTK warnings
- [x] 7.3 Commit with conventional message `fix(sidebar): virtualize workspace tree rows past the GtkListView realized-widget cap`
