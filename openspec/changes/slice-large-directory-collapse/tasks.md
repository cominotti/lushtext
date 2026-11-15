Tests: failing-first widget, unit and property tests (groups 2–3), before the code that makes them pass (groups 4–6).
Kani: new scheduled shard `core-sidebar-collapse` (design D9), run once after the code is final; no existing shard's harness inputs change (`ui/sidebar/policy.rs` is no harness's oracle today; check with `scripts/kani-shards.py oracle` at apply time).
Timing: the sidebar collapse and small-directory expansion cases of design D2, four rounds interleaved `main` → branch → `main` → branch through `scripts/measure.sh`, a round counting only at a recorded 1-minute load below 4.
GTK axioms: the samples of the three new axioms (design D13) and A91's, on GNOME 51 and the GNOME 50 floor.
Lane: none. The responsiveness lane drives editor documents and the restyle of a populated sidebar, not expansion or collapse; the widget verdict test is the measurement vehicle (design D2).

## 1. Quiet baseline and attribution on `main` (before any fix)

- [ ] 1.1 Port the stage 1 scratch code (`.claude/handoffs/slice-large-directory-collapse/scratch-measure.patch`) into one verdict test in `crates/lushtext/tests/widget/workspace_tree_virtualization.rs`, `test_directory_collapse_and_small_expansion_dispatches_fit_the_budgets`. It covers the D2 cases (collapse: 0 and 400 rows after the directory × N = 64, 256, 4,096, 16,385, 131,072; expansion: 0 and 400 × N = 64, 256, 1,024) and prints, per case:
  - the input dispatch;
  - every dispatch until collapsed plus 700 ms, with the frames split into layout and paint;
  - time-to-collapsed;
  - the scan terminal-publication counter around each dispatch.

  It enforces nothing yet (`timing_verdicts_enforced()` gates the assertions added in 7.1). Keep the measurement code identical between `main` and the branch: apply it to a `main` worktree as a patch saved in the handoff directory.
- [ ] 1.2 Run it on `main` and on the branch, four rounds interleaved, through `scripts/measure.sh`, at a recorded load below 4. Record cairo and GL (steady state) frame figures. Write the tables into design "Measured baseline" as the verdict baseline, replacing the non-verdict stage 1 figures but keeping them as history.
- [ ] 1.3 Attribute every expansion dispatch over `B_SETTLE_US` with a `perf` profile (run under `scripts/measure.sh`, which uses no systemd scope; pass `--call-graph=fp --user-callchains`), then choose D10's fix, or no fix, from the attribution. Decide the three constants of design D3 from the figures (`DIRECT_COLLAPSE_ROWS`, `COLLAPSE_REALIZED_ROWS_PER_FRAME` with 8, 16 and 32 measured, `COLLAPSE_UNBOUND_CHUNK_ROWS`), each a power of two rounded down. Record both in design "Implementation decisions".
- [ ] 1.4 Confirm D12's attribution:
  - in the verdict test's re-expansion right after a 131,072-entry collapse, show by counter that every dispatch over `B_SETTLE_US` coincides with a scan's terminal publication;
  - record the store that scan published to: it must belong to the collapsed directory's path and be shown by no row, i.e. the `is_expandable()` probe's store.

  If a dispatch does not match, attribute it before going on.
- [ ] 1.5 Measure the residual routes of design D2 once on `main` and record them: a refresh splice removing an expanded 256-entry and an expanded 131,072-entry directory, a section rebuild with one expanded, and a direct collapse while the sidebar is hidden.
- [ ] 1.6 Commit (signed, conventional, staged by name) and push.

## 2. GTK axioms first (design D13)

- [ ] 2.1 Add the next free ids (provisionally A112–A114) to `crates/gtk-lush/axioms/src/catalogue.rs` with probes `src/probes/aNNN.rs`, following the existing `control`/`axiom`/`measure` discipline:
  - A112: a mapped presented `GtkListView` over a `GtkTreeListModel` with a counting `SignalListItemFactory`. Remove 16 / 64 / 256 bound rows, with 0 and 400 rows after them, then remove unbound rows after and before the band. Record setups, teardowns and timings, and that no removed widget is rebound to another item.
  - A113: the expander's gesture with its phase set to `None`; a capture-phase key controller on an ancestor returning `Stop` for each collapse binding; an inserted `listitem` action group; the AT-SPI action table.
  - A114: the `:checked` arrow under a `lushtext-collapsing`-style class rule, in LTR and RTL, under the shipped style.
- [ ] 2.2 Add the interactive/`--check` samples `crates/gtk-lush/axioms/examples/aNNN_*.rs` (exit 0/1/2, refusing with 77 unless `GTK_LUSH_AXIOMS_HEADLESS=1`). Mark A112's probe slow if it exceeds the slow threshold.
- [ ] 2.3 Add ledger rows to `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`, and correct A91's dependent-design text (design D15). Update `crates/gtk-lush/axioms/README.md` and `CHANGELOG.md`.
- [ ] 2.4 Run `make check-gtk-axioms` and the new samples with `make gtk-axiom-sample AXIOM=<id> CHECK=1` on GNOME 51 and on the GNOME 50 floor (`scripts/run-gtk-axioms.sh runtimes --only <ids>`), headless only. Copy the printed versions into "Verified against". If A114 is refuted on either toolkit, switch design D6 to its fallback and record it.
- [ ] 2.5 Commit and push.

## 3. Failing-first tests

- [ ] 3.1 Policy unit tests in `ui/sidebar/policy.rs` for `next_collapse_step` (design D9):
  - reference cases: empty band, band at the head, band in the middle, band past the end, nested stores, a store shorter than one unit, and exactly `DIRECT_COLLAPSE_ROWS` rows left;
  - a property test that drives the policy against a model of GTK's band (A112) to `CollapseRow`, checking the per-step bounds and the frame count of design D2.

  They must fail to compile (the policy does not exist) before group 4.
- [ ] 3.2 Widget tests in `workspace_tree_virtualization.rs`, presented, each failing on `main` where stated:
  - `test_collapsing_a_directory_with_rows_after_it_bounds_each_step`: 256 entries plus 400 rows after; no collapse step removes more than the unit, and the list view's child count changes by at most the unit per step. Fails on `main`: one call destroys 207 widgets.
  - `test_collapse_shows_in_the_first_frame`: arrow class and accessible `expanded` false after the input dispatch, before any row is removed. Fails on `main`: the class does not exist.
  - `test_collapse_keeps_the_directory_row_still`: rendered top constant at every step, no slice bin correction, `outer_request_pending()` false at the end.
  - `test_collapse_from_inside_the_children_matches_a_direct_collapse`: folder header collapse while scrolled into a 4,096-entry directory; the outer value and rendered placement equal a direct collapse's on a twin tree.
  - `test_collapse_moves_selection_and_focus_to_the_directory`: on the sliced and on the direct path. Fails on `main`, where selection lands on another row.
  - `test_expanding_again_mid_collapse_restores_every_entry`: once by click and once through each LushText expand caller (reveal, new-item flow, restore, folder toggle). A guard test: it passes on `main`, where no collapse can be pending, and must stay green. It asserts that the store lists every entry again and that no caller acts on a partly drained store.
  - `test_a_collapse_nobody_draws_completes`: start a sliced collapse, then hide the sidebar, and separately start one with the section body collapsed; the collapse completes and `refresh_blocks_readiness` becomes false.
  - `test_ancestor_collapse_drains_the_descendant_first`: `outer/inner` with 16,385 entries.
  - `test_collapse_stops_loads_and_refreshes_under_it`: collapse during a 131,072-entry load; no batch is applied afterwards, and re-expansion loads afresh.
  - `test_collapse_routes_reach_the_entry_point`: click, each collapse key, activation, header collapse and toggle, reveal failure, reorder hover. Counted through the evidence's `collapse_direct_total` and `collapse_steps_total`.
  - `test_expand_routes_keep_gtk_behaviour`: click and expand keys expand and scroll as `main` does.
  - `test_collapse_readiness_and_evidence`: `refresh_blocks_readiness` is true while collapsing; the evidence inertness proof covers the new fields.
  - `test_removing_a_collapsing_directory_cancels_the_collapse`.
- [ ] 3.3 Run them on the branch before group 4 and save the failures in `.claude/handoffs/slice-large-directory-collapse/impl/failing-first.log`. BL's collapse tests (`test_collapsing_a_large_directory_retires_its_listing_after_the_collapse`, `test_collapsing_an_ancestor_releases_its_descendants_listings`, `test_a_collapsed_listing_held_elsewhere_is_released_not_emptied`) call `set_expanded(false)` directly. They keep testing the direct path, which the residual routes still take, and stay green unchanged; say so in their doc comments.
- [ ] 3.4 Commit and push.

## 4. Pure step policy and its proofs (design D9)

- [ ] 4.1 Add `CollapseFacts`, `CollapseStep`, `next_collapse_step` and the three constants (D3, documented with their kind and measured basis per `.agents/rules/rust.md`) to `ui/sidebar/policy.rs`. Integer-only, `#![forbid(clippy::float_arithmetic)]` on the policy module, no GTK imports. Add the constants to the threshold inventory (`make check-thresholds`).
- [ ] 4.2 Add `ui/sidebar/policy/kani_proofs.rs` with harnesses C1–C5 and the shard `core-sidebar-collapse` (gate `scheduled`) to `scripts/kani-shards.py`. Run `make check-kani-shards`.
- [ ] 4.3 Task 3.1's tests pass.
- [ ] 4.4 Commit and push.

## 5. The owned collapse (design D5–D8, D11)

- [ ] 5.1 Add `ui/sidebar/workspace_section/collapse_execution.rs`:
  - the entry points `collapse_directory(row, origin)` and `expand_directory(row, origin)`;
  - the collapse state in `imp.rs`, grouped with the scan runtime state;
  - the step source, frame-paced (`after-paint`) while the list view is mapped and a `G_PRIORITY_LOW` timeout while it is not, switched on `map`/`unmap` (design D4);
  - the band read (list-view children → expander → `list_row()` → `position()`).

  The module doc narrates the stage order, its resumption points and its inversions.
- [ ] 5.2 Start of a collapse (D6, D7):
  - the expander class;
  - the accessible state;
  - `expanded_paths` pruned for the subtree;
  - subtree store runtimes cancelled and refresh targets under it dropped;
  - selection and focus moved to the directory row.
- [ ] 5.3 Cancellation paths (D7): re-expansion (refill through `populate_child_store`), an ancestor taking over, a refresh splice, a section rebuild, a model reset, and dispose. Each ends the collapse exactly once, with its reason in the evidence.
- [ ] 5.4 Routes (D5):
  - `row_factory.rs` sets the expander's own `GestureClick` phase to `None` on every row and installs LushText's disclosure gesture. Expanding activates `listitem.scroll-to` plus `set_expanded(true)`; collapsing calls the entry point.
  - Add the capture-phase key controller for the collapse bindings on the list view.
  - Route `activate_file_at`, `folder_execution.rs`, `reveal_execution.rs` and `reorder_execution.rs` through the collapse entry point.
  - Route every `set_expanded(true)` caller through the expand entry point: `reveal_execution.rs` (`advance_expansion`), `file_execution.rs` (new-item flow), `scan_execution.rs` (restore), and `folder_execution.rs` (expand, toggle, load-time auto-expand).
- [ ] 5.5 Add a `make check-workflow-boundaries` rule: no `set_expanded(` call on a directory row outside `collapse_execution.rs`. Audit every `is_expanded()` reader and every `set_expanded(true)` caller in `ui/sidebar/**` for intent-versus-GTK-state during a collapse, and record the audit in `impl/expansion-audit.md`.
- [ ] 5.6 Readiness and evidence (D11): `refresh_blocks_readiness` covers a running collapse. Add the evidence fields and the automation snapshot's `collapse_pending`.
- [ ] 5.7 The application CSS for `treeexpander.lushtext-collapsing`, both text directions, as A114 pinned it.
- [ ] 5.8 Task 3.2's tests pass. Run each new test in isolation as well, and treat any `FLAKY:` as a blocker (`preexisting-blockers.md`).
- [ ] 5.9 Commit and push.

## 6. Expansion (design D10; only if task 1.3 decided to cap)

- [ ] 6.1 Cap the rows a directory load brings into the realized band per frame at `COLLAPSE_REALIZED_ROWS_PER_FRAME`, through the existing batch path. Add a failing-first test of the 256-entry expansion's worst dispatch first.
- [ ] 6.2 If task 1.3 decided not to cap, mark 6.1 not needed here with the figures that decided it.
- [ ] 6.3 Commit and push.

## 7. Verdicts and gates

- [ ] 7.1 Make the verdict test of task 1.1 enforce design D2's pass bar under `timing_verdicts_enforced()`, excluding the named residual (D12), which it prints. Update BL's `test_directory_expansion_dispatches_fit_the_settle_budget` comments, which call the collapse a recorded exception.
- [ ] 7.2 Rerun task 1.2's campaign, four rounds interleaved, on the final code (detached if long). Record the verdict tables, time-to-collapsed and the residuals in design "Verdicts".
- [ ] 7.3 Gates:
  - `make check`, `make check-workflow-boundaries`, `make check-widget-shards` (update the owning shard's `ci_minutes` if its measured time grows), `make check-kani-shards`, `make check-gtk-axioms`, `make check-thresholds`;
  - the widget shard that owns `workspace_tree_virtualization` (`make test-widget-shard WIDGET_SHARD=<shard>`) and `make test-unit`;
  - the rustdoc lint gate.
- [ ] 7.4 Run `git add -N` on new files, then `make check-accessibility-policy` and `make check-visual-proof-policy`. Run headless `make accessibility-smoke` and `make visual-smoke`, plus `make visual-geometry-smoke` if the policy classifies the touched files as visual-sensitive (the slice bin's rendered-row invariant `workspace-tree-rendered-row-anchors`). Never the live desktop.
- [ ] 7.5 Run the Kani shard `core-sidebar-collapse` once, detached (`setsid nohup scripts/heavy.sh -- make kani KANI_SHARD=core-sidebar-collapse KANI_MEASURE=<json>`). Record its local measurement and mark its budget `PENDING DISPATCH` with this change as owner.
- [ ] 7.6 Run `make mutants-diff` over the new policy code, and triage any survivor.
- [ ] 7.7 Commit and push.

## 8. Documentation (`.agents/rules/documentation.md`)

- [ ] 8.1 Update the `WFR-WORKSPACE-TREE` row in `docs/workflow-readability-matrix.md`: declare `collapse_execution.rs`, the new stage order, resumption point and inversions, the new policy and evidence fields, and the re-derived measured cells. Then run `make check-workflow-boundaries`.
- [ ] 8.2 Update the docs:
  - `AGENTS.md`: the sidebar design decisions (viewport slice pattern, multi-workspace sidebar) and the module layout entry for `workspace_section/`;
  - `crates/lushtext-core/src/ui/sidebar/AGENTS.md`: the entry point is the only collapse caller, and the evidence surface hazard list;
  - `.agents/rules/ui.md`: File Tree.
- [ ] 8.3 Update `docs/accessibility.md` and the workspace-tree rows of `docs/accessibility-matrix.md`: the expanded state flips at intent and focus moves to the directory row. Update `docs/automation.md` and `docs/automation-reference.md` for `collapse_pending` and the readiness change, then run `make check-automation-docs`.
- [ ] 8.4 Update `docs/next/long-line-documents.md`, BL record: re-attribute the exception, resolve the follow-ups (D12), and add this change's status. Add the recommendation that `stop-expander-bind-time-child-scans` be queued next, with the residual it owns. Append a note to the archived BL design's stage 4 decisions 1 and 2. Record the follow-ups:
  - lighter sidebar rows;
  - the upstream-report candidate for `gtk_list_item_change_finish` reusing unclaimed row widgets, drafted with the `upstream-gtk-reports` skill and never filed without per-report approval, listed in `docs/upstream-reports.md` as a candidate only.
- [ ] 8.5 README: no change unless the Features list describes sidebar collapse. Record the check.
- [ ] 8.6 Run `openspec validate slice-large-directory-collapse --strict`.
- [ ] 8.7 Commit and push.
