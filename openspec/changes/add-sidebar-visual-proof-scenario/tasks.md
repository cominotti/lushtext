## 1. Failing-first coverage for the action

- [ ] 1.1 Add widget tests in `crates/lushtext/tests/widget/workspace_tree_virtualization.rs` that activate `win.reveal-workspace-path` on a real window. The cases are: the last of 400 files (`beyond-cap`), entry 250 of a 300-entry directory two levels down (`nested-beyond-cap`), and a row in the second of two tall workspaces. Each case asserts rendered rows, not model rows: the row is selected, mapped, and wholly inside the outer viewport; neighbours are contiguous; `row_placement` is stable across six forced layouts; `correction_count()` stays flat. Run them and record that they fail because the action does not exist.
- [ ] 1.2 Add the `already-visible` widget test: reveal a row that is on screen at rest, then assert the outer scroller value and the header's bounds are unchanged. Guard it with `assert_sidebar_can_scroll` so it cannot pass vacuously. Record the failure.
- [ ] 1.3 Add typed-failure widget tests, each asserting that selection, scroll position, scope, visibility, and `expanded_paths` are unchanged. Cover:
  - `not-in-workspace`;
  - `outside-scope`, with a specific workspace selected and the target in another;
  - `not-found`, for a missing middle component, with no directory expanded beyond the last existing ancestor;
  - `hidden-by-visibility`, with a dot-named component and hidden files off;
  - `beyond-entry-cap`, using a directory above the 10,000-entry cap and a target past it;
  - `superseded`, with a second reveal issued while the first awaits a scan, proving the first selects nothing, via the existing scan-delay test policy.

  Record the failures.
- [ ] 1.4 Add a widget test that disposes the window mid-reveal and asserts the `workspace-path-reveal` blocker clears and no continuation runs.
- [ ] 1.5 Add action-catalog unit tests (`services/action_catalog/tests.rs`) expecting the `reveal-workspace-path` row: window scope, `string` → `none`, `dbus-action`, `exported`, `diagnostic-only`, widget and visual-smoke lanes. Record the failure.

## 2. Slice-bin evidence accessor (GTK Lush)

- [ ] 2.1 Add a failing adoption-lab widget test (`gtk_lush_adoption.rs`) and a LushText widget test. Both assert `ViewportSliceBin::outer_request_pending()` is true between a forwarded `scroll_to` and its idle, false after it, and false at rest across repeated allocations.
- [ ] 2.2 Implement the read-only accessor over the existing `outer_request_scheduled` cell, with rustdoc matching `allocation_count()`/`correction_count()`. Update the widgets crate CHANGELOG and README and the public-API snapshot.
- [ ] 2.3 Run `make check-gtk-lush-policy`, `make gtk-lush-public-api-advisory`, `make gtk-lush-doctests`, and `make gtk-lush-adoption-lab`, and confirm 2.1 passes.

## 3. Reveal resolution and coordination (WFR-WORKSPACE-TREE)

- [ ] 3.1 Add the pure resolution to `crates/lushtext-core/src/ui/sidebar/policy.rs`. It takes the workspaces, the scope, the `WorkspaceEntryVisibility`, and the target, and returns `(workspace, folder, ancestor chain)` or `not-in-workspace` / `outside-scope` / `hidden-by-visibility`. Give it unit tests and a proptest that the ancestor chain always lies inside the chosen folder. Confirm the file imports no toolkit crate.
- [ ] 3.2 Add `crates/lushtext-core/src/ui/sidebar/reveal_execution.rs` (role: coordination `execution`). It implements the stage order: resolve, expand each ancestor via `set_expanded(true)`, await the admitted scan, classify `not-found`/`beyond-entry-cap` from scan results, call `select_and_scroll_to`, then settle on bin evidence within 60 frame ticks, ending `revealed` or `unsettled`. A per-window generation handles supersession and disposal ends the reveal. Every path reaches exactly one terminal.
- [ ] 3.3 Narrate the stage order and its two resumption points in the `ui/sidebar/mod.rs` facade, staying within 370 lines. Re-measure the facade and record the figure.
- [ ] 3.4 Extend `WorkspaceTreeEvidence` in `ui/sidebar/evidence.rs` with the reveal status, generation, and depth. Extend the evidence inertness proof so reads during and after a reveal do not change scan counters or the expansion registries.
- [ ] 3.5 Register `win.reveal-workspace-path` in `ui/window/actions.rs`, delegating to the sidebar facade, and add the catalog row in `services/action_catalog/mod.rs`. Confirm 1.1–1.5 now pass.

## 4. Automation readiness, events, and snapshot

- [ ] 4.1 Add the `workspace-path-reveal` blocker and the `workspace-path-revealed` predicate, which reports `workflow-failure` on a typed failure. Add the blocker to `visual-geometry-settled`, `accessibility-settled`, and `idle` in `model/automation.rs` / `ui/automation.rs`, and derive the `workspace-path-reveal` workflow event. Add unit tests for the predicate/blocker tables.
- [ ] 4.2 Project the reveal status, generation, and depth into `window.workspace` from the evidence surface. Add the `workspace-sidebar-viewport`, `workspace-revealed-row`, and `workspace-revealed-section-header` surfaces, plus at most eight unlabelled neighbour rectangles, to `visual_geometry_snapshot`, reported absent with a reason when no reveal is current. Add a widget test asserting the bounds, the cap of eight, and that no file name appears in the serialized snapshot.
- [ ] 4.3 Add an automation-client self-test case to `scripts/lushtext-automation.py`. It builds the string parameter array for `win.reveal-workspace-path` from a catalog row and parses a `workflow-failure` reply for `workspace-path-revealed`. Run `make automation-client-self-test`.

## 5. cargo-gtk-proof scenario type

- [ ] 5.1 Add failing `model.rs` unit tests. A `workspace-tree-reveal` manifest with two sizes, two schemes, and three fixture kinds must expand into twelve stable case IDs, and an unknown fixture kind or an empty `fixture_kinds` must be rejected. Then implement the validation and expansion.
- [ ] 5.2 Add fixture preparation to `live.rs`: a synthetic folder tree per kind (at least 300 entries in the revealed directory except `already-visible`) plus a versioned `workspaces.json`. Add the before-capture wait on `workspace-refresh-complete`, the primary action `reveal-workspace-path` with the fixture's target, and after-capture waits on `workspace-path-revealed` then `visual-geometry-settled`. Add a unit test asserting no fixture file name appears in artifacts outside the fixture directory.
- [ ] 5.3 Add the `runner.rs` relationships `revealed-row-inside-sidebar-viewport`, `neighbour-rows-contiguous`, `revealed-row-at-rest`, `outer-scroll-unchanged`, and `section-header-unchanged`, with replay unit tests over synthetic snapshots. The tests include a 1px-gapped neighbour and a non-`revealed` status, and each failure message must be named.
- [ ] 5.4 Add the `png.rs` detector `workspace-selected-row-top-edge`, with unit tests on synthetic RGBA fixtures for a light and a dark highlight, a focus-ring edge, and a no-highlight crop.
- [ ] 5.5 Add `scripts/visual-geometry-scenarios/workspace-tree-reveal.json` and `workspace-tree-reveal-at-rest.json` per design D6, with `invariant_id` `workspace-tree-rendered-row-anchors`. Confirm `cargo run -p cargo-gtk-proof -- schema` accepts both.
- [ ] 5.6 Run `make visual-geometry-smoke` in a real session and confirm every reveal case passes with zero pixel-anchor delta. Freeze one light and one dark after-crop plus snapshot into the compatibility corpus, and confirm `cargo run -p cargo-gtk-proof -- corpus` replays them.
- [ ] 5.7 Prove the anchor fails against the defect. Temporarily offset the bin's child transform by 2px (revert afterwards), and confirm the pixel anchor fails while every snapshot relationship still passes. Record both results in the task note.

## 6. Visual proof policy

- [ ] 6.1 Add `workspace-tree-rendered-row-anchors` and the path set from the `visual-geometry-invariants` delta to `crates/cargo-gtk-proof/src/policy.rs`. Add unit tests for a required path (`scroll_request.rs`), a required scenario file, and a not-required path (`services/draft_service.rs`).
- [ ] 6.2 Mirror the same constant and path set in `scripts/check-visual-proof-policy.py`, and extend its `--self-test` with the same three cases. Run `make check-visual-proof-policy` against the artifacts from 5.6 and confirm it passes, then again without them and confirm it fails, naming the invariant.

## 7. Documentation sync

- [ ] 7.1 Update `docs/automation.md` and `docs/automation-reference.md`: the action-catalog row, the readiness predicate and blocker rows, the aggregate predicates' blocker lists, the workflow ID list, the `window.workspace` reveal fields and their Evidence Projection Map rows, and the visual-geometry surface names. Run `make check-automation-docs`.
- [ ] 7.2 Update `docs/end-user-coverage.md` to put the sidebar rendered-row invariant in the visual lane. Check `docs/accessibility.md` and `docs/accessibility-matrix.md` for impact, and record "no accessibility surface changed" if none, since the action adds no keyboard path or AT-SPI anchor.
- [ ] 7.3 Update the `WFR-WORKSPACE-TREE` row in `docs/workflow-readability-matrix.md`. Declare `reveal_execution.rs`, re-derive the size and seam counts row-scoped without `#[cfg(test)]`, record the new stage order and its resumption points, and record the facade figure from 3.3. Run `make check-workflow-boundaries`.
- [ ] 7.4 Update the `AGENTS.md` module layout (sidebar reveal coordination) and the viewport-slice design decision (the new evidence accessor and the screenshot proof). Update the `widget-wiring.md` or `ui.md` rule if the reveal introduces a pattern they do not already describe.
- [ ] 7.5 Update `docs/next/formal-verification.md`: close the §4 deferral "`cargo-gtk-proof` has no sidebar or slice-bin scenario". Update `docs/next/formal-verification-next.md`: mark the N10 sidebar/slice-bin bullet done, naming this change.
- [ ] 7.6 Check `README.md` for impact. No user-visible feature is added, so record that none is needed unless its testing section lists visual scenarios.

## 8. Gates

- [ ] 8.1 Run `make check`, `make test`, `make test-widget` (the full widget suite, including the adoption lab and the rendered-row checks), `make check-policy`, and `make visual-geometry-smoke`. Every test must pass with no new GTK/Adwaita warnings in the smoke warning scan.
- [ ] 8.2 Run the new reveal widget tests ten times in isolation and confirm zero flakes. Investigate and fix any intermittent failure at its cause, per `preexisting-blockers.md`.
- [ ] 8.3 Run `openspec validate add-sidebar-visual-proof-scenario --strict`.
