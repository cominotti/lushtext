Tests: failing-first widget, unit and property tests (groups 2–3), before the code that makes them pass (groups 4–6).
Kani: new scheduled shard `core-sidebar-collapse` (design D9), run once after the code is final, plus its `ORACLES` entry; no existing shard's harness inputs change (`ui/sidebar/policy.rs` is no harness's oracle today, and `model::list_budget` is reused unchanged; check with `scripts/kani-shards.py oracle` at apply time).
Commits: design D16. Nothing visual-sensitive (`crates/lushtext-core/src/ui/`, `crates/lushtext/tests/widget/`, `resources/style/`) is committed before task 7.7, which lands all of it in one commit after the visual-geometry and accessibility smokes ran on that exact tree.
Timing: the sidebar collapse and small-directory expansion cases of design D2, four rounds interleaved `main` → branch → `main` → branch through `scripts/measure.sh`, a round counting only at a recorded 1-minute load below 4.
GTK axioms: the samples of the three new axioms (design D13) and A91's, on GNOME 51 and the GNOME 50 floor.
Lane: none. The responsiveness lane drives editor documents and the restyle of a populated sidebar, not expansion or collapse; the widget verdict test is the measurement vehicle (design D2).

**Stage 3 status (2026-11-15).** Every task that needs no quiet machine is
done. The verdict campaign (tasks 1.2 and 7.2, four rounds interleaved at a
recorded 1-minute load below 4) is queued with the coordinator's quiet-window
runner, which writes `impl/campaign2/slc-v2-r<round>-<side>-c<attempt>.log`
and ends `impl/campaign2/progress.log` with `done (coordinator)`; the machine
stayed at a load of 6–32 through the stage, so no round ran. What waits on it:
the verdict tables (1.2, 7.2), the constants and D10's expansion decision
(1.3, group 6), D12's counter confirmation (1.4), the residual figures (1.5,
printed by the same test's `residual` cases), the design-tables commit (1.6),
and the enforcement of the pass bar (7.1). The constants stay at their
design candidates (32, 32, 8,192, 128 ms) until then.

**Stage 4 status (2026-11-15): verdict failed, back to implementation.** The
coordinator ran the four rounds quiet (`impl/campaign2/`, no round
contaminated). Design "Measured baseline" holds the tables and design
"Stage 4 verdict" the reading: at unit 32 every collapse with rows after the
directory and the folder header's two-root collapse put frames of 16.8–29.9
ms over `B_SETTLE_US`; expansion is over budget on `main` and the branch
alike (D10 is live). Checked from it: 1.2 (cairo only; GL moves to 7.2), 1.5
and 1.6. Still owed: 1.3 (units 8 and 16, the expansion and two-root
profiles), 1.4 (the probe store's identity, and `main`'s unflagged
re-expansion frames), 6.1 or 6.2, 7.1 (it needs a step-time instrument
first) and 7.2 on the final code. The stage 4 review's eleven findings
(design "Stage 4 review findings", one blocker: the key route takes
characters typed into inline rename) are owed by the same rework. Not
merged.

## 1. Quiet baseline and attribution on `main` (before any fix)

- [x] 1.1 Port the stage 1 scratch code (`.claude/handoffs/slice-large-directory-collapse/scratch-measure.patch`) into one verdict test in `crates/lushtext/tests/widget/workspace_tree_virtualization.rs`, `test_directory_collapse_and_small_expansion_dispatches_fit_the_budgets`. It covers the D2 cases (collapse: 0 and 400 rows after the directory × N = 64, 256, 4,096, 16,385, 131,072; the folder header collapsing two 4,096-entry roots; re-expansion in the middle of a 131,072-entry collapse; expansion: 0 and 400 × N = 64, 256, 1,024, cold and right after a collapse) and prints, per case:
  - the input dispatch;
  - every dispatch until collapsed plus 700 ms, with the frames split into layout and paint;
  - time-to-collapsed;
  - the scan terminal-publication counter around each dispatch.

  It enforces nothing yet (`timing_verdicts_enforced()` gates the assertions added in 7.1). Keep the measurement code identical between `main` and the branch: keep it as a patch saved in the handoff directory and apply it to both worktrees. It is not committed before task 7.7 (design D16).
- [x] 1.2 Run it on `main` and on the branch, four rounds interleaved, through `scripts/measure.sh`, at a recorded load below 4. Record cairo and GL (steady state) frame figures. Write the tables into design "Measured baseline" as the verdict baseline, replacing the non-verdict stage 1 figures but keeping them as history. (2026-11-15, stage 4: rounds `slc-v2-r1`–`r4`, 1-minute load 2.2–3.3 at both ends of every round, cairo only. GL was not run; the failing frames are layout-phase time, so the renderer cannot change them, and the GL figures move to task 7.2.)
- [ ] 1.3 Attribute every expansion dispatch over `B_SETTLE_US` with a `perf` profile (run under `scripts/measure.sh`, which uses no systemd scope; pass `--call-graph=fp --user-callchains`), then choose D10's fix, or no fix, from the attribution. Decide the constants of design D3 from the figures (`DIRECT_COLLAPSE_ROWS`, `SIDEBAR_REALIZED_ROWS_PER_FRAME` with 8, 16 and 32 measured, `COLLAPSE_UNBOUND_CHUNK_ROWS`, each a power of two rounded down; `COLLAPSE_FRAME_STARVATION_MS`, rounded up). If 8 still misses `B_INTERACT_US`, record the toolkit-floor exception of design D2 with its A118 figures. Record both in design "Implementation decisions".
- [ ] 1.4 Confirm D12's attribution:
  - in the verdict test's re-expansion right after a 131,072-entry collapse, show by counter that every dispatch over `B_SETTLE_US` coincides with a scan's terminal publication;
  - record the store that scan published to: it must belong to the collapsed directory's path and be shown by no row, i.e. the `is_expandable()` probe's store.

  If a dispatch does not match, attribute it before going on.
- [x] 1.5 Measure the residual routes of design D2 once on `main` and record them: a refresh splice removing an expanded 256-entry and an expanded 131,072-entry directory, a section rebuild with one expanded, a direct collapse while the sidebar is hidden, and an AT-SPI-equivalent activation of the expander's `listitem.collapse` on a 256-entry directory. (2026-11-15: measured on both sides in the same campaign; design "Measured baseline", "Reading", last item.)
- [x] 1.6 Commit the design tables only (signed, conventional, staged by name) and push. The hook's visual-proof policy reads the whole working tree, so first reverse-apply the measurement patch (`git apply -R`) and reapply it after the commit; stash is not used (design D16). (2026-11-15, stage 4: the verdict test is committed since task 7.7, so the tables commit touches only `openspec/` and `docs/next/`.)

## 2. GTK axioms first (design D13)

- [x] 2.1 Add the next free ids (provisionally A113–A115 at apply time; A118–A120 since the post-stage-4 design revision, design D13) to `crates/gtk-lush/axioms/src/catalogue.rs` with probes `src/probes/aNNN.rs`, following the existing `control`/`axiom`/`measure` discipline:
  - A118: a mapped presented `GtkListView` over a `GtkTreeListModel` with a counting `SignalListItemFactory`. Remove 16 / 64 / 256 bound rows, with 0 and 400 rows after them, then remove unbound rows after and before the band (the list scrolled into the children) and record that the band does not move. Repeat the head removal with 400 rows after it on an unmapped list. Record setups, teardowns and timings, and that no removed widget is rebound to another item.
  - A119, in a row built like LushText's (`GtkListItem:focusable` `TRUE`, a focusable button inside the expander's child):
    - where the click `GestureClick` lives (on the `expander` icon, not the expander; `observe_controllers()` on the expander lists none), and that a capture-phase `GestureClick` on the expander claiming the sequence on a release inside the icon stops the toggle, also after the icon is re-created, while the row's focus-on-click, selection and double-press activation are unchanged;
    - which of the full key-binding list (design D5) reaches the expander from the row's focus and from the button's, and that a capture-phase key controller on an ancestor returning `Stop` prevents them;
    - that the class actions resolve before an inserted `listitem` action group;
    - the AT-SPI action table through the generic widget vtable, and that `DoAction` on `listitem.collapse` collapses directly;
    - which accessible `expanded` state GTK keeps on the expander and on its icon, at bind and on `notify::expanded`.
  - A120: the `:checked` arrow under a `lushtext-collapsing`-style class rule, in LTR and RTL, under the shipped style.
- [x] 2.2 Add the interactive/`--check` samples `crates/gtk-lush/axioms/examples/aNNN_*.rs` (exit 0/1/2, refusing with 77 unless `GTK_LUSH_AXIOMS_HEADLESS=1`). Mark A118's probe slow if it exceeds the slow threshold.
- [x] 2.3 Add ledger rows to `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`, and correct A91's dependent-design text (design D15). Update `crates/gtk-lush/axioms/README.md` and `CHANGELOG.md`.
- [x] 2.4 Run `make check-gtk-axioms` and the new samples with `make gtk-axiom-sample AXIOM=<id> CHECK=1` on GNOME 51 and on the GNOME 50 floor (`scripts/run-gtk-axioms.sh runtimes --only <ids>`), headless only. Copy the printed versions into "Verified against". If A120 is refuted on either toolkit, switch design D6 to its fallback and record it. If A118 shows that removing the rows before the band moves a mapped list's band or realizes rows, switch design D4 to its fallback order and record it. Record A119's reachable key list as design D5's key route.
- [x] 2.5 Commit and push.

## 3. Failing-first tests

- [x] 3.1 Policy unit tests in `ui/sidebar/policy.rs` for `next_collapse_step` (design D9):
  - reference cases: empty band, band at the head, band in the middle, band past the end, nested stores, a store shorter than one unit, and exactly `DIRECT_COLLAPSE_ROWS` rows left;
  - a property test that drives the policy against a model of GTK's band (A118) to `CollapseRow`, checking the per-step bounds and the frame count of design D2.

  They must fail to compile (the policy does not exist) before group 4.
- [x] 3.2 Widget tests in `workspace_tree_virtualization.rs`, presented, each failing on `main` where stated:
  - `test_collapsing_several_directories_takes_one_unit_per_frame`: the folder header collapses two expanded 4,096-entry roots; every frame removes at most one unit across both; both complete. Fails on `main`: one call per root.
  - `test_a_starved_frame_clock_still_completes_the_collapse`: a test policy suppresses the frame-paced source; the starvation fallback completes the collapse and the evidence counts its steps. Then ticks resume: the first tick takes no bound unit, so no frame lays out two units.
  - `test_a_rebound_collapsing_row_stays_collapsed_for_accessibility`: scroll the collapsing row out of the band and back; the overlay's and the expander's accessible `expanded` stay false and the arrow class stays.
  - `test_the_arrow_click_survives_an_icon_recreation`: change the row's expandability so GTK re-creates the icon, then click the arrow; LushText's gesture handles it on release and GTK's toggles nothing.
  - `test_collapsing_a_directory_with_rows_after_it_bounds_each_step`: 256 entries plus 400 rows after; no collapse step removes more than the unit, and the list view's child count changes by at most the unit per step. Fails on `main`: one call destroys 207 widgets.
  - `test_collapse_shows_in_the_first_frame`: arrow class and accessible `expanded` false after the input dispatch, before any row is removed. Fails on `main`: the class does not exist.
  - `test_collapse_keeps_the_directory_row_still`: rendered top constant at every step, no slice bin correction, `outer_request_pending()` false at the end.
  - `test_collapse_from_inside_the_children_matches_a_direct_collapse`: folder header collapse while scrolled into a 4,096-entry directory; the outer value and rendered placement equal a direct collapse's on a twin tree.
  - `test_collapse_moves_selection_and_focus_to_the_directory`: on the sliced and on the direct path. Fails on `main`, where selection lands on another row.
  - `test_expanding_again_mid_collapse_restores_every_entry`: once by click and once through each LushText expand caller (reveal, new-item flow, restore, folder toggle). A guard test: it passes on `main`, where no collapse can be pending, and must stay green. It asserts that the store lists every entry again and that no caller acts on a partly drained store.
  - `test_a_collapse_nobody_draws_completes`: start a sliced collapse, then hide the sidebar, and separately start one with the section body collapsed; the collapse completes and `refresh_blocks_readiness` becomes false.
  - `test_ancestor_collapse_drains_the_descendant_first`: `outer/inner` with 16,385 entries.
  - `test_collapse_stops_loads_and_refreshes_under_it`: collapse during a 131,072-entry load; no batch is applied afterwards, and re-expansion loads afresh.
  - `test_collapse_routes_reach_the_entry_point`: click, each collapse key A119 shows reachable (written once task 2.4 records them), activation, header collapse and toggle, reveal failure, reorder hover. Counted through the evidence's `collapse_direct_total` and `collapse_steps_total`.
  - `test_expand_routes_keep_gtk_behaviour`: a click and each expand key A119 shows reachable expand and scroll as `main` does, and a click keeps the row's focus and selection behaviour.
  - `test_collapse_readiness_and_evidence`: `refresh_blocks_readiness` is true while collapsing; the evidence inertness proof covers the new fields.
  - `test_removing_a_collapsing_directory_cancels_the_collapse`.
- [x] 3.3 Run them on the branch before group 4 and save the failures in `.claude/handoffs/slice-large-directory-collapse/impl/failing-first.log`. Grep every widget test and smoke scenario that collapses through a route this change takes over (`collapse_folders`, `toggle_folders`, row activation, the reveal, the expander) and then reads `is_expanded()` or the row count synchronously. Record each one in the log as direct (at most `DIRECT_COLLAPSE_ROWS`, unchanged) or sliced (to wait on `refresh_blocks_readiness`). BL's collapse tests (`test_collapsing_a_large_directory_retires_its_listing_after_the_collapse`, `test_collapsing_an_ancestor_releases_its_descendants_listings`, `test_a_collapsed_listing_held_elsewhere_is_released_not_emptied`) call `set_expanded(false)` directly. They keep testing the direct path, which the residual routes still take, and stay green unchanged; say so in their doc comments.
- [x] 3.4 No commit (design D16): the tests do not compile yet and are visual-sensitive; they land in task 7.7.

## 4. Pure step policy and its proofs (design D9)

- [x] 4.1 Add `CollapseFacts`, `CollapseStep`, `next_collapse_step` and the four constants (D3, documented with their kind and measured basis per `.agents/rules/rust.md`) to `ui/sidebar/policy.rs`. Integer-only, `#![forbid(clippy::float_arithmetic)]` on the policy module, no GTK imports. Add the constants to the threshold inventory (`make check-thresholds`).
- [x] 4.2 Add `ui/sidebar/policy/kani_proofs.rs` with harnesses C1–C5 over the allocation-free core (design D9). Add the shard `core-sidebar-collapse` (gate `scheduled`, `measured_in` `PENDING DISPATCH (owed by slice-large-directory-collapse)` until task 7.5) and the `ORACLES` entry for `crates/lushtext-core/src/ui/sidebar/policy.rs`, scoped by `functions=` to the collapse core, to `scripts/kani-shards.py`. Run `make check-kani-shards`.
- [x] 4.3 Task 3.1's tests pass.
- [x] 4.4 No commit (design D16).

## 5. The owned collapse (design D5–D8, D11)

- [x] 5.1 Add `ui/sidebar/workspace_section/collapse_execution.rs`:
  - the entry points `collapse_directory(row, origin)` and `expand_directory(row, origin)`;
  - the collapse state in `imp.rs`, grouped with the scan runtime state;
  - the step source (design D4): a tick callback on the list view while it is mapped, as `ui/list_growth.rs` does; a `G_PRIORITY_LOW` timeout while it is not, or when no tick arrived within `COLLAPSE_FRAME_STARVATION_MS`; switched on `map`/`unmap`; with a test-policy switch that suppresses ticks for task 3.2;
  - the window-wide one-unit-per-frame token and the section's collapse queue (design D7);
  - the band read (list-view children → expander → `list_row()` → `position()`).

  The module doc narrates the stage order, its resumption points and its inversions.
- [x] 5.2 Start of a collapse (D6, D7):
  - the expander class;
  - the accessible state on the overlay and on the expander, re-applied by the bind path and the `notify::expanded` hook from the collapse state (design D6);
  - `expanded_paths` pruned for the subtree;
  - subtree store runtimes cancelled and refresh targets under it dropped;
  - selection and focus moved to the directory row.
- [x] 5.3 Cancellation paths (D7): re-expansion (refill through `populate_child_store`), an ancestor taking over, a refresh splice, a section rebuild, a model reset, and dispose. Each ends the collapse exactly once, with its reason in the evidence.
- [x] 5.4 Routes (D5):
  - `row_factory.rs` installs, at setup, one capture-phase `GestureClick` on the expander that lets the press through and claims the sequence on a release inside its current `expander` icon (design D5). Expanding activates `listitem.scroll-to` and calls the expand entry point; collapsing calls the collapse entry point. Remove the bind-time `observe_controllers()` loop and its comment, which A119 shows inert on 4.24.1, or re-aim it if A119 shows a toolkit where it acts. Confirm file-row double-click activation is unchanged (an existing test, or a new one).
  - Add the capture-phase key controller on the list view for exactly the bindings task 2.4 recorded as reachable, or none if A119 found none reachable (design D5).
  - Route `activate_file_at`, `folder_execution.rs`, `reveal_execution.rs` and `reorder_execution.rs` through the collapse entry point.
  - Route every `set_expanded(true)` caller through the expand entry point: `reveal_execution.rs` (`advance_expansion`), `file_execution.rs` (new-item flow), `scan_execution.rs` (restore), and `folder_execution.rs` (expand, toggle, load-time auto-expand).
- [x] 5.5 Add a `make check-workflow-boundaries` rule: under `ui/sidebar/**`, no `.set_expanded(` method call on a `TreeListRow` outside `collapse_execution.rs`. `accessibility::set_expanded` is not matched, and the search panel's tree rows elsewhere are out of scope. Audit every `is_expanded()` reader and every `set_expanded(true)` caller in `ui/sidebar/**` for intent-versus-GTK-state during a collapse, including `row_accessibility.rs`'s bind projection and notify hook, `collapsed_non_empty_folder_rows` and `toggleable_folder_rows`. Record the audit in `impl/expansion-audit.md`.
- [x] 5.6 Readiness and evidence (D11): `refresh_blocks_readiness` covers a running collapse. Add the evidence fields and the automation snapshot's `collapse_pending`.
- [x] 5.7 The application CSS for `treeexpander.lushtext-collapsing`, both text directions, as A120 pinned it.
- [x] 5.7a Add `crates/lushtext-core/src/ui/sidebar/workspace_section/collapse_execution.rs` to `WORKSPACE_TREE_ROW_ANCHOR_EXACT` in `scripts/check-visual-proof-policy.py` and to its mirror in `crates/cargo-gtk-proof/src/policy.rs`, with both self-tests asserting it (design D14). Run `make check-visual-proof-policy`.
- [x] 5.8 Task 3.2's tests pass. Run each new test in isolation as well, and treat any `FLAKY:` as a blocker (`preexisting-blockers.md`).
- [x] 5.9 No commit (design D16).

## 6. Expansion (design D10; only if task 1.3 decided to cap)

- [ ] 6.1 Cap the rows a directory load brings into the realized band per frame at `SIDEBAR_REALIZED_ROWS_PER_FRAME`, through the existing batch path. Add a failing-first test of the 256-entry expansion's worst dispatch first.
- [ ] 6.2 If task 1.3 decided not to cap, mark 6.1 not needed here with the figures that decided it.
- [ ] 6.3 No commit (design D16).

## 7. Verdicts and gates

- [ ] 7.1 Make the verdict test of task 1.1 enforce design D2's pass bar under `timing_verdicts_enforced()`, excluding the named residual (D12), which it prints. Update BL's `test_directory_expansion_dispatches_fit_the_settle_budget` comments, which call the collapse a recorded exception.
- [ ] 7.2 Rerun task 1.2's campaign, four rounds interleaved, on the final code (detached if long). Record the verdict tables, time-to-collapsed and the residuals in design "Verdicts".
- [x] 7.3 Gates:
  - `make check`, `make check-workflow-boundaries`, `make check-widget-shards` (update the owning shard's `ci_minutes` if its measured time grows), `make check-kani-shards`, `make check-gtk-axioms`, `make check-thresholds`;
  - the widget shard that owns `workspace_tree_virtualization` (`make test-widget-shard WIDGET_SHARD=<shard>`) and `make test-unit`;
  - the rustdoc lint gate.
- [x] 7.4 After task 8.3 (its docs are in the accessibility smoke's fingerprint) and with every visual-sensitive edit in place: run `git add -N` on new files, then `make check-accessibility-policy` and `make check-visual-proof-policy`. Run headless `make accessibility-smoke`, `make visual-smoke` and `make visual-geometry-smoke`. The last is required: `row_factory.rs`, `folder_execution.rs`, `imp.rs`, `reveal_execution.rs` and now `collapse_execution.rs` are keys of `workspace-tree-rendered-row-anchors`. Never the live desktop. Any later edit to a visual-sensitive file voids the proof, so rerun it before the commit.
- [x] 7.5 Run the Kani shard `core-sidebar-collapse` once, detached (`setsid nohup scripts/heavy.sh -- make kani KANI_SHARD=core-sidebar-collapse KANI_MEASURE=<json>`). Record its local measurement as `PENDING DISPATCH (owed by slice-large-directory-collapse)`. GitHub CI is checked only at the end of the programme. Because `make check-kani-shards` fails an archive that still owes a figure, record in the programme record that this debt must be settled by two dispatched runs, or moved to an active change, before this change is archived.
- [x] 7.6 Run `make mutants-diff` over the new policy code, and triage any survivor. (2026-11-15: 33 mutants in 12 min, 31 caught, 1 unviable, 1 missed. The miss, `<` to `<=` in the `highest` band cut of `next_collapse_step`, is equivalent: a held position equal to the store's end makes `for_each_unheld_range` emit nothing and keep its end. Excluded by column in `.cargo/mutants.toml` with that reason; a relisting drops exactly that mutant and keeps the other 25 of the function. Log: `impl/mutants-diff.log`.)
- [x] 7.7 The landing commit (design D16): every visual-sensitive file plus `scripts/kani-shards.py`, the workflow-boundaries rule and the visual-proof keys, staged by name, through the hook, then push.

## 8. Documentation (`.agents/rules/documentation.md`)

Order (design D16): do 8.2's `crates/lushtext-core/src/ui/sidebar/AGENTS.md` edit and all of 8.3 before task 7.4. They land in the 7.7 commit: the first is visual-sensitive, and 8.3's files are in the accessibility smoke's fingerprint. The rest commit in 8.7.

- [x] 8.1 Update the `WFR-WORKSPACE-TREE` row in `docs/workflow-readability-matrix.md`: declare `collapse_execution.rs`, the new stage order, resumption point and inversions, the new policy and evidence fields, and the re-derived measured cells. Then run `make check-workflow-boundaries`.
- [x] 8.2 Update the docs:
  - `AGENTS.md`: the sidebar design decisions (viewport slice pattern, multi-workspace sidebar) and the module layout entry for `workspace_section/`;
  - `crates/lushtext-core/src/ui/sidebar/AGENTS.md`: the entry point is the only collapse caller, and the evidence surface hazard list;
  - `.agents/rules/ui.md`: File Tree.
- [x] 8.3 Update `docs/accessibility.md` and the workspace-tree rows of `docs/accessibility-matrix.md`: the expanded state of the row and of its expander flips at intent, and focus moves to the directory row. Also state the two routes: row activation (`Enter`) takes the paced collapse, and the expander's AT-SPI actions keep GTK's direct collapse (design D5). Update `docs/automation.md` and `docs/automation-reference.md` for `collapse_pending` and the readiness change, then run `make check-automation-docs`.
- [x] 8.4 Update `docs/next/long-line-documents.md`, BL record: re-attribute the exception, resolve the follow-ups (D12), and add this change's status. Add the recommendation that `stop-expander-bind-time-child-scans` be queued next, with the residual it owns. Append a note to the archived BL design's stage 4 decisions 1 and 2. Record the follow-ups:
  - lighter sidebar rows;
  - the upstream-report candidate for `gtk_list_item_change_finish` reusing unclaimed row widgets, drafted with the `upstream-gtk-reports` skill and never filed without per-report approval, listed in `docs/upstream-reports.md` as a candidate only.
- [x] 8.5 README: no change unless the Features list describes sidebar collapse. Record the check.
- [x] 8.6 Run `openspec validate slice-large-directory-collapse --strict`.
- [x] 8.7 Commit and push.
